//! 库存搜索条件在列表查询和计数前求交。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::SkuId;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};
use serde::Deserialize;

use super::{InventoryRepository, STOCK_ADJUSTMENT_LINES, STOCK_RESERVATIONS};
use crate::dto::StockAvailability;
use crate::entity::inventory::ReservationStatus;

/// 由领域查询编排产生的库存筛选，持久化表达只在仓储内可见。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InventorySearch(Document);

impl InventorySearch {
    /// 关键词 SKU 集合与显式 SKU 求交；`Some` 空集合表示无命中。
    ///
    /// # 参数
    /// * `ids` - 关键词解析出的 SKU；`None` 表示没有关键词条件。
    /// * `sku` - 调用方显式指定的 SKU；`None` 表示未指定。
    ///
    /// # 返回
    /// 无关键词时只保留显式 SKU，两者都缺省时过滤为空文档。
    /// 有关键词时返回二者交集的 `sku_id` `$in` 条件；交集为空时 `$in` 为空数组。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn skus(ids: Option<Vec<SkuId>>, sku: Option<&SkuId>) -> Self {
        let Some(ids) = ids else {
            return Self(sku.map(|id| doc! { "sku_id": id.to_string() }).unwrap_or_default());
        };
        let ids = ids
            .into_iter()
            .filter(|id| sku.is_none_or(|selected| selected == id))
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        Self(doc! { "sku_id": { "$in": ids } })
    }

    /// 搜索条件作为额外 AND 分支，不覆盖权限仓库和其他显式条件。
    ///
    /// # 参数
    /// * `filter` - 待追加条件的查询文档。
    ///
    /// # 返回
    /// 无返回值。搜索文档非空时向 `filter` 写入 `$and`。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn apply(&self, filter: &mut Document) {
        if !self.0.is_empty() {
            filter.insert("$and", vec![self.0.clone()]);
        }
    }
}

#[derive(Deserialize)]
struct ReservationDimension {
    warehouse_id: String,
    sku_id: String,
}
#[derive(Deserialize)]
struct AdjustmentReference {
    stock_adjustment_id: String,
}

impl InventoryRepository<'_> {
    /// 解析余额定位与可用量条件，保留已存在的 SKU 条件。
    ///
    /// # 参数
    /// * `search` - 已求交的 SKU 搜索条件。
    /// * `id` - 精确余额主键；`None` 表示不按主键收窄。
    /// * `availability` - 可用量条件；`None` 或 `All` 不追加数量条件。
    /// * `executor` - 调用方选择的数据访问执行器。
    ///
    /// # 返回
    /// 返回追加了主键和可用量条件的搜索。`Reserved` 且没有可操作预占维度时，把 `id` 写成空 `$in` 表示无命中，并覆盖此前写入的精确主键。
    ///
    /// # 错误
    /// 读取有效预占维度失败时返回仓储错误。
    pub async fn balance_search(
        &self,
        search: InventorySearch,
        id: Option<&str>,
        availability: Option<StockAvailability>,
        executor: &mut dyn Executor,
    ) -> Result<InventorySearch> {
        let mut search = search.0;
        if let Some(id) = id {
            search.insert("id", id);
        }
        match availability {
            Some(StockAvailability::Zero) => {
                search.insert("available_quantity", doc! { "$eq": 0 });
            },
            Some(StockAvailability::Positive) => {
                search.insert("available_quantity", doc! { "$gt": 0 });
            },
            Some(StockAvailability::Reserved) => {
                let dimensions = self.reservation_dimensions(executor).await?;
                let clauses = dimensions
                    .into_iter()
                    .map(|r| doc! { "warehouse_id": r.warehouse_id, "sku_id": r.sku_id })
                    .collect::<Vec<_>>();
                if clauses.is_empty() {
                    search.insert("id", doc! { "$in": Vec::<String>::new() });
                } else {
                    search.insert("$or", clauses);
                }
            },
            _ => {},
        }
        Ok(InventorySearch(search))
    }

    /// 只投影有效预占的维度，不加载预占业务全文。
    async fn reservation_dimensions(&self, executor: &mut dyn Executor) -> Result<Vec<ReservationDimension>> {
        mongo_ops::find_many(&self.db.collection::<ReservationDimension>(STOCK_RESERVATIONS),
            doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "status": { "$in": ReservationStatus::operable().iter().map(ReservationStatus::as_str).collect::<Vec<_>>() } },
            FindOptions::builder().projection(doc! { "_id": 0, "warehouse_id": 1, "sku_id": 1 }).build(), executor).await
    }

    /// 将 SKU 条件解析为包含匹配明细的调整单集合。
    ///
    /// # 参数
    /// * `search` - 已求交的 SKU 搜索条件。
    /// * `id` - 精确调整单主键；`None` 表示不按主键收窄。
    /// * `executor` - 调用方选择的数据访问执行器。
    ///
    /// # 返回
    /// SKU 条件为空时只保留主键过滤。否则返回主键过滤与命中明细所属调整单 `id` `$in` 的交集。
    ///
    /// # 错误
    /// 读取调整单明细失败时返回仓储错误。
    pub async fn adjustment_search(
        &self,
        search: InventorySearch,
        id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<InventorySearch> {
        let search = search.0;
        let mut filter = Document::new();
        if let Some(id) = id {
            filter.insert("id", id);
        }
        if search.is_empty() {
            return Ok(InventorySearch(filter));
        }
        let mut line_filter = search;
        line_filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
        let rows = mongo_ops::find_many(
            &self.db.collection::<AdjustmentReference>(STOCK_ADJUSTMENT_LINES),
            line_filter,
            FindOptions::builder().projection(doc! { "_id": 0, "stock_adjustment_id": 1 }).build(),
            executor,
        )
        .await?;
        let ids = rows
            .into_iter()
            .map(|r| r.stock_adjustment_id)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        filter.insert("$and", vec![doc! { "id": { "$in": ids } }]);
        Ok(InventorySearch(filter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 搜索无命中与显式 SKU 冲突都必须保留空集合，不能覆盖仓库范围。
    #[test]
    fn sku_search_intersects_and_preserves_authorized_filter() {
        let search = InventorySearch::skus(Some(vec![SkuId::new("sku-101")]), Some(&SkuId::new("other")));
        let mut filter =
            doc! { "warehouse_id": { "$in": ["authorized"] }, "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        search.apply(&mut filter);
        assert_eq!(filter.get_document("warehouse_id").unwrap(), &doc! { "$in": ["authorized"] });
        assert_eq!(
            filter.get_array("$and").unwrap()[0].as_document().unwrap(),
            &doc! { "sku_id": { "$in": [] } }
        );
        let matched = InventorySearch::skus(Some(vec![SkuId::new("sku-101")]), Some(&SkuId::new("sku-101")));
        assert_eq!(matched.0, doc! { "sku_id": { "$in": ["sku-101"] } });
    }
}
