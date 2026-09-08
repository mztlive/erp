//! 库存搜索条件在列表查询和计数前求交。

use super::{InventoryRepository, STOCK_ADJUSTMENT_LINES, STOCK_RESERVATIONS};
use crate::{dto::StockAvailability, entity::inventory::ReservationStatus};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::SkuId;
use mongodb::{
    bson::{doc, Document},
    options::FindOptions,
};
use persistence_core::{mongo_ops, Executor, Result};
use serde::Deserialize;

/// 由领域查询编排产生的库存筛选，持久化表达只在仓储内可见。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InventorySearch(Document);

impl InventorySearch {
    /// 关键词 SKU 集合与显式 SKU 求交；Some 空集合表示无命中。
    pub fn skus(ids: Option<Vec<SkuId>>, sku: Option<&SkuId>) -> Self {
        let Some(ids) = ids else {
            return Self(
                sku.map(|id| doc! { "sku_id": id.to_string() })
                    .unwrap_or_default(),
            );
        };
        let ids = ids
            .into_iter()
            .filter(|id| sku.is_none_or(|selected| selected == id))
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        Self(doc! { "sku_id": { "$in": ids } })
    }

    /// 搜索条件作为额外 AND 分支，不覆盖权限仓库和其他显式条件。
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
    /// # 错误
    /// 读取有效预占维度失败。
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
            }
            Some(StockAvailability::Positive) => {
                search.insert("available_quantity", doc! { "$gt": 0 });
            }
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
            }
            _ => {}
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
    /// # 错误
    /// 读取调整单明细失败。
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
            FindOptions::builder()
                .projection(doc! { "_id": 0, "stock_adjustment_id": 1 })
                .build(),
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
        assert_eq!(
            filter.get_document("warehouse_id").unwrap(),
            &doc! { "$in": ["authorized"] }
        );
        assert_eq!(
            filter.get_array("$and").unwrap()[0].as_document().unwrap(),
            &doc! { "sku_id": { "$in": [] } }
        );
        let matched = InventorySearch::skus(Some(vec![SkuId::new("sku-101")]), Some(&SkuId::new("sku-101")));
        assert_eq!(matched.0, doc! { "sku_id": { "$in": ["sku-101"] } });
    }
}
