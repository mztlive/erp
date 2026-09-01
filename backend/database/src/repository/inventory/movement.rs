use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use entities::common::time::Instant;
use entities::ids::{SkuId, WarehouseId};
use entities::inventory::{MovementDirection, MovementType, StockMovement};
use entities::money::Quantity;

use super::shared::{entities_by_ids, sort_doc};
use super::{InventoryRepository, STOCK_MOVEMENTS};
use crate::executor::Executor;
use crate::repository::{PageResult, Pagination, QueryFilter, Repository};
use crate::{mongo_ops, Result};

/// 库存流水列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockMovementRow {
    /// 实体主键。
    pub id: String,
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// SKU。
    pub sku_id: SkuId,
    /// 流水类型。
    pub movement_type: MovementType,
    /// 流水方向。
    pub direction: MovementDirection,
    /// 正数数量。
    pub quantity: Quantity,
    /// 来源单据标识。
    pub source_document_id: String,
    /// 来源单据行标识。
    pub source_line_id: Option<String>,
    /// 业务实际发生时间。
    pub occurred_at: Instant,
    /// ERP 记录时间。
    pub recorded_at: Instant,
    /// ERP 记录人。
    pub recorded_by: String,
}

/// 库存流水列表筛选条件（正式事实，恒为未删除）。
#[derive(Debug, Clone)]
pub struct StockMovementFilter {
    /// Service 已证明可读取的仓库集合；`None` 表示公司级，空集合表示无范围。
    pub warehouse_ids: Option<Vec<WarehouseId>>,
    /// SKU；`None` 表示不筛选。
    pub sku_id: Option<SkuId>,
    /// 流水类型；`None` 表示不筛选。
    pub movement_type: Option<MovementType>,
    /// 流水方向；`None` 表示不筛选。
    pub direction: Option<MovementDirection>,
    /// 发生时间下界（含）；`None` 表示不筛选。
    pub occurred_from: Option<Instant>,
    /// 发生时间上界（含）；`None` 表示不筛选。
    pub occurred_to: Option<Instant>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `occurred_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for StockMovementFilter {
    /// 转换为 MongoDB 查询条件（正式事实恒为未删除）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(warehouse_ids) = &self.warehouse_ids {
            filter.insert(
                "warehouse_id",
                doc! { "$in": warehouse_ids.iter().map(ToString::to_string).collect::<Vec<_>>() },
            );
        }
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id.to_string());
        }
        if let Some(movement_type) = self.movement_type {
            filter.insert("movement_type", movement_type.as_str());
        }
        if let Some(direction) = self.direction {
            filter.insert("direction", direction.as_str());
        }
        let mut range = Document::new();
        if let Some(from) = self.occurred_from {
            range.insert("$gte", from.unix_secs());
        }
        if let Some(to) = self.occurred_to {
            range.insert("$lte", to.unix_secs());
        }
        if !range.is_empty() {
            filter.insert("occurred_at", range);
        }
        filter
    }
}

impl Pagination for StockMovementFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, StockMovement> {
    /// 分页检索库存流水台账（投影查询）。
    ///
    /// 只返回 [`StockMovementRow`] 所需的列表字段，不加载整文档；排序字段走
    /// 白名单映射（`occurred_at`/`recorded_at`/`created_at`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    #[tracing::instrument(
        name = "repository.inventory.search_stock_movements",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_movements",
            db.operation.name = "search"
        )
    )]
    pub async fn search_stock_movements(
        &self,
        filter: &StockMovementFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<StockMovementRow>> {
        let query = filter.to_doc();
        let options = FindOptions::builder()
            .sort(stock_movement_sort(filter))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(stock_movement_projection())
            .build();
        let collection = self.collection().clone_with_type::<StockMovementRow>();
        let items = mongo_ops::find_many(&collection, query.clone(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), query, executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按唯一来源单据标识查询库存流水（详情查询）。
    ///
    /// 同一来源单据（如一次采购入库）可能产生多行流水。
    ///
    /// # 参数
    /// * `source_document_id` - 唯一来源单据标识
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的流水集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_source_document(
        &self,
        source_document_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockMovement>> {
        self.find_many(doc! { "source_document_id": source_document_id }, executor)
            .await
    }
}

/// 为库存流水分页追加唯一主键 tie-breaker，避免相同主排序值跨页重复或遗漏。
fn stock_movement_sort(filter: &StockMovementFilter) -> Document {
    let mut sort = sort_doc(
        filter.sort_by.as_deref(),
        filter.sort_ascending,
        &["occurred_at", "recorded_at", "created_at"],
    );
    sort.insert("id", if filter.sort_ascending { 1 } else { -1 });
    sort
}

impl<'a> InventoryRepository<'a> {
    /// 按来源单据读取库存流水。
    ///
    /// # 参数
    /// * `source_document_id` - 来源单据主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该来源单据产生的全部未删除流水。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    #[tracing::instrument(
        name = "repository.inventory.movements_for_source_document",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_movements",
            db.operation.name = "find"
        )
    )]
    pub async fn movements_for_source_document(
        &self,
        source_document_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockMovement>> {
        mongo_ops::find_many(
            &self.db.collection::<StockMovement>(STOCK_MOVEMENTS),
            doc! {
                "source_document_id": source_document_id,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await
    }

    /// 按主键集合批量读取库存流水。
    ///
    /// # 参数
    /// * `ids` - 流水主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部命中的未删除流水。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn movements_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockMovement>> {
        entities_by_ids(self.db, STOCK_MOVEMENTS, ids, executor).await
    }
}

/// 库存流水列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn stock_movement_projection() -> Document {
    doc! {
        "id": 1,
        "warehouse_id": 1,
        "sku_id": 1,
        "movement_type": 1,
        "direction": 1,
        "quantity": 1,
        "source_document_id": 1,
        "source_line_id": 1,
        "occurred_at": 1,
        "recorded_at": 1,
        "recorded_by": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{stock_movement_sort, QueryFilter, StockMovementFilter};

    use entities::common::time::Instant;
    use entities::ids::{SkuId, WarehouseId};
    use entities::inventory::{MovementDirection, MovementType};
    use mongodb::bson::{doc, Bson};

    fn filter(warehouse_ids: Option<Vec<WarehouseId>>) -> StockMovementFilter {
        StockMovementFilter {
            warehouse_ids,
            sku_id: None,
            movement_type: None,
            direction: None,
            occurred_from: None,
            occurred_to: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }

    #[test]
    fn movement_filter_applies_dimensions_type_range_and_deleted_filter() {
        let filter = StockMovementFilter {
            warehouse_ids: Some(vec![WarehouseId::new("wh-1")]),
            sku_id: Some(SkuId::new("sku-1")),
            movement_type: Some(MovementType::PurchaseReceiptIn),
            direction: Some(MovementDirection::Increase),
            occurred_from: Some(Instant::from_unix_secs(1_700_000_000)),
            occurred_to: Some(Instant::from_unix_secs(1_700_000_100)),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(
            document.get_document("warehouse_id").unwrap(),
            &doc! { "$in": ["wh-1"] }
        );
        assert_eq!(document.get_str("sku_id").unwrap(), "sku-1");
        assert_eq!(document.get_str("movement_type").unwrap(), "PURCHASE_RECEIPT_IN");
        assert_eq!(document.get_str("direction").unwrap(), "INCREASE");
        let range = document.get_document("occurred_at").unwrap();
        assert_eq!(range.get_i64("$gte").unwrap(), 1_700_000_000);
        assert_eq!(range.get_i64("$lte").unwrap(), 1_700_000_100);
    }

    #[test]
    fn authorized_warehouse_filter_preserves_company_empty_and_exact_scope() {
        assert_eq!(
            filter(Some(vec![WarehouseId::new("warehouse-1")]))
                .to_doc()
                .get_document("warehouse_id")
                .unwrap(),
            &doc! { "$in": ["warehouse-1"] }
        );
        assert_eq!(
            filter(Some(Vec::new()))
                .to_doc()
                .get_document("warehouse_id")
                .unwrap()
                .get_array("$in")
                .unwrap(),
            &Vec::<Bson>::new()
        );
        assert!(!filter(None).to_doc().contains_key("warehouse_id"));
    }

    #[test]
    fn movement_sort_uses_unique_id_as_same_direction_tie_breaker() {
        let mut value = filter(None);
        value.sort_by = Some("occurred_at".to_string());
        value.sort_ascending = true;
        assert_eq!(stock_movement_sort(&value), doc! { "occurred_at": 1, "id": 1 });
        value.sort_ascending = false;
        assert_eq!(stock_movement_sort(&value), doc! { "occurred_at": -1, "id": -1 });
    }
}
