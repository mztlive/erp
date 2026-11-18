use chrono::Local;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{SalesOrderLineId, SkuId, WarehouseId};
use erp_core::money::Quantity;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Repository, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use super::shared::{
    authorized_projection_search, ids_to_strings, negate_decimal128, scoped_base_filter, sort_doc,
    to_decimal128, with_id_tie_breaker,
};
use super::{InventoryRepository, STOCK_RESERVATIONS};
use crate::Result as InventoryResult;
use crate::entity::inventory::{
    ReservationStatus, StockReservation, StockReservationSourceType, zero_quantity,
};
use crate::ports::InventoryQuery;

/// 库存预占列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockReservationRow {
    /// 实体主键。
    pub id: String,
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// SKU。
    pub sku_id: SkuId,
    /// 唯一归属销售明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 当前有效预占。
    pub reserved_quantity: Quantity,
    /// 已消耗数量。
    pub consumed_quantity: Quantity,
    /// 已释放数量。
    pub released_quantity: Quantity,
    /// 预占状态。
    pub status: ReservationStatus,
    /// 乐观锁版本。
    pub version: u64,
}

/// 库存预占列表筛选条件。
#[derive(Debug, Clone)]
pub struct StockReservationFilter {
    /// Service 解析的搜索条件；只允许服务端构造。
    pub search: super::InventorySearch,
    /// 业务仓库筛选；`None` 不收窄候选，授权由独立凭证限定。
    pub warehouse_ids: Option<Vec<WarehouseId>>,
    /// SKU；`None` 表示不筛选。
    pub sku_id: Option<SkuId>,
    /// 预占状态；`None` 表示不筛选。
    pub status: Option<ReservationStatus>,
    /// 唯一归属销售明细；`None` 表示不筛选。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for StockReservationFilter {
    // 初始化库存预占筛选：第一页、每页 20 条，业务筛选默认留空，默认降序排序。
    fn default() -> Self {
        Self {
            search: super::InventorySearch::default(),
            warehouse_ids: None,
            sku_id: None,
            status: None,
            sales_order_line_id: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for StockReservationFilter {
    // 转换为 MongoDB 查询条件。
    // 基底来自 `scoped_base_filter`（未删除，以及 `warehouse_ids`：`None` 不限仓库，空集合保持空 `$in`）。有值时精确匹配 `sku_id`、`sales_order_line_id`，并按稳定代码匹配 `status`，最后并入 `search`。
    fn to_doc(&self) -> Document {
        let mut filter = scoped_base_filter(self.warehouse_ids.as_deref());
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(sales_order_line_id) = &self.sales_order_line_id {
            filter.insert("sales_order_line_id", sales_order_line_id.to_string());
        }
        self.search.apply(&mut filter);
        filter
    }
}

impl Pagination for StockReservationFilter {
    // 向仓储分页传递页码与单页条数。
    // 原样返回 `page`，并把 `page_size` 从 `u32` 拓宽为 `u64`；不在此处把小于 1 的页码归一。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 库存预占集合上的领域查询与原子条件写。
#[allow(async_fn_in_trait)]
pub trait StockReservationRepositoryExt {
    /// 分页检索库存预占列表（投影查询）。
    /// 只返回 [`StockReservationRow`] 所需的列表字段，不加载整文档；排序字段走白名单映射（`created_at`/`updated_at`）。
    /// 返回当前页投影行与满足筛选条件的总数。
    /// 完整授权条件下的页与总数；缺资格返回空页。
    /// `executor` 为数据访问执行器，由 Service 决定是否位于事务中 filter 为业务筛选，access 为当前动作凭证，executor 为原读取执行器。
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。按统一库存模型凭证分页查询；候选条件只收窄结果。模型错配、复验或数据库失败返回错误。
    async fn search_authorized_stock_reservations(
        &self,
        filter: &StockReservationFilter,
        access: Option<&InventoryQuery>,
        executor: &mut dyn Executor,
    ) -> InventoryResult<PageResult<StockReservationRow>>;

    /// 仓发消耗正数预占时，沿调用方事务先原子校验记录仍有效或部分消耗且剩余量足够，再从预占量转入消耗量。
    /// 写入后按剩余量收敛为 `Consumed` 或 `PartiallyConsumed`；缺记录、状态不符或不足返回 `false` 且不改文档，传播写入错误。调用方需要新实体时重读。
    async fn consume_quantity(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 释放预占时沿用调用方执行器，仅对有效或部分消耗、且剩余量恰等于请求量的记录原子全额释放并切换为 `Released`。
    /// 缺记录、状态不符或请求部分释放返回 `false`，文档不变；传播写入错误，不同步内存实体。
    async fn release_quantity(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
}

impl StockReservationRepositoryExt for Repository<'_, StockReservation> {
    #[tracing::instrument(
        name = "repository.inventory.search_stock_reservations",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_reservations",
            db.operation.name = "search"
        )
    )]
    async fn search_authorized_stock_reservations(
        &self,
        filter: &StockReservationFilter,
        access: Option<&InventoryQuery>,
        executor: &mut dyn Executor,
    ) -> InventoryResult<PageResult<StockReservationRow>> {
        let options = FindOptions::builder()
            .sort(stock_reservation_sort(filter))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(stock_reservation_projection())
            .build();
        authorized_projection_search(&self.collection(), filter.to_doc(), options, access, executor).await
    }

    #[tracing::instrument(
        name = "repository.inventory.consume_reservation",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_reservations",
            db.operation.name = "update"
        )
    )]
    async fn consume_quantity(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity_bson = to_decimal128(quantity)?;
        let active_statuses =
            [ReservationStatus::Active.as_str(), ReservationStatus::PartiallyConsumed.as_str()];
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "reserved_quantity": { "$gte": quantity_bson },
            "status": { "$in": active_statuses.as_slice() },
        };
        let update = doc! {
            "$inc": {
                "reserved_quantity": negate_decimal128(quantity_bson),
                "consumed_quantity": quantity_bson,
                "version": 1,
            },
            "$set": { "updated_at": Local::now().timestamp() },
        };
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        if result.matched_count == 0 {
            return Ok(false);
        }

        let exhausted = mongo_ops::update_one(
            &self.collection(),
            doc! { "id": id, "reserved_quantity": 0 },
            doc! { "$set": { "status": ReservationStatus::Consumed.as_str() } },
            false,
            executor,
        )
        .await?;
        if exhausted.matched_count == 0 {
            mongo_ops::update_one(
                &self.collection(),
                doc! { "id": id, "status": { "$in": active_statuses.as_slice() } },
                doc! { "$set": { "status": ReservationStatus::PartiallyConsumed.as_str() } },
                false,
                executor,
            )
            .await?;
        }
        Ok(true)
    }

    #[tracing::instrument(
        name = "repository.inventory.release_reservation",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_reservations",
            db.operation.name = "update"
        )
    )]
    async fn release_quantity(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity = to_decimal128(quantity)?;
        let zero = to_decimal128(zero_quantity())?;
        let active_statuses =
            [ReservationStatus::Active.as_str(), ReservationStatus::PartiallyConsumed.as_str()];
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "reserved_quantity": { "$eq": quantity },
            "status": { "$in": active_statuses.as_slice() },
        };
        let update = doc! {
            "$set": {
                "reserved_quantity": zero,
                "status": ReservationStatus::Released.as_str(),
                "updated_at": Local::now().timestamp(),
            },
            "$inc": {
                "released_quantity": quantity,
                "version": 1,
            },
        };
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
    }
}

// 为库存预占分页追加唯一主键 tie-breaker，避免相同主排序值跨页重复或遗漏。
fn stock_reservation_sort(filter: &StockReservationFilter) -> Document {
    with_id_tie_breaker(
        sort_doc(filter.sort_by.as_deref(), filter.sort_ascending, &["created_at", "updated_at"]),
        filter.sort_ascending,
    )
}

impl<'a> InventoryRepository<'a> {
    /// 批量读取销售稳定行的现有库存预占。
    /// 返回由现有库存分配建立的全部未删除预占；采购入库预占不进入结果。
    /// `sales_order_line_ids` 为销售稳定行主键集合；沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询或游标读取错误。
    #[tracing::instrument(
        name = "repository.inventory.existing_reservations_for_sales_lines",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_reservations",
            db.operation.name = "find"
        )
    )]
    pub async fn existing_stock_reservations_for_sales_lines(
        &self,
        sales_order_line_ids: &[SalesOrderLineId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockReservation>> {
        if sales_order_line_ids.is_empty() {
            return Ok(Vec::new());
        }
        mongo_ops::find_many(
            &self.db.collection::<StockReservation>(STOCK_RESERVATIONS),
            doc! {
                "sales_order_line_id": { "$in": ids_to_strings(sales_order_line_ids) },
                "source_type": StockReservationSourceType::ExistingStock.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await
    }

    /// 读取指定余额维度上的可操作预占。
    /// 返回有效或部分消耗的预占集合。
    /// `warehouse_id` 为仓库主键，`sku_id` 为 SKU 主键，沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询或游标读取错误。
    pub async fn operable_reservations_for_balance(
        &self,
        warehouse_id: &WarehouseId,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockReservation>> {
        reservations_for_dimensions(
            self.db,
            doc! {
                "warehouse_id": warehouse_id.to_string(),
                "sku_id": sku_id.to_string(),
            },
            None,
            executor,
        )
        .await
    }

    /// 批量读取页内库存维度上的可操作预占。
    /// 返回命中任一页内仓库与 SKU 的可操作预占。
    /// `warehouse_ids` 为仓库主键集合；`sku_ids` 为 SKU 主键集合。
    /// 沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询或游标读取错误。
    pub async fn operable_reservations_for_dimensions(
        &self,
        warehouse_ids: &[String],
        sku_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockReservation>> {
        if warehouse_ids.is_empty() || sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        reservations_for_dimensions(
            self.db,
            doc! {
                "warehouse_id": { "$in": warehouse_ids },
                "sku_id": { "$in": sku_ids },
            },
            None,
            executor,
        )
        .await
    }

    /// 按建立时间读取指定维度上的可操作预占。
    /// 返回按 `created_at` 升序排列的可操作预占。
    /// `warehouse_id` 为仓库主键，`sku_id` 为 SKU 主键，沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询或游标读取错误。
    #[tracing::instrument(
        name = "repository.inventory.oldest_operable_reservations",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_reservations",
            db.operation.name = "find"
        )
    )]
    pub async fn oldest_operable_reservations(
        &self,
        warehouse_id: &WarehouseId,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockReservation>> {
        reservations_for_dimensions(
            self.db,
            doc! {
                "warehouse_id": warehouse_id.to_string(),
                "sku_id": sku_id.to_string(),
            },
            Some(doc! { "created_at": 1 }),
            executor,
        )
        .await
    }
}

// 按维度条件读取可操作预占。
// 返回有效或部分消耗的预占。
// `filter` 为仓库与 SKU 维度条件，`sort` 为可选排序条件，沿用调用方执行器 `executor`。
// 传播 MongoDB 查询或游标读取错误。
async fn reservations_for_dimensions(
    db: &Database,
    mut filter: Document,
    sort: Option<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<StockReservation>> {
    filter.insert("status", doc! { "$in": reservation_status_codes(ReservationStatus::operable()) });
    filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    let options = FindOptions::builder().sort(sort).build();
    mongo_ops::find_many(&db.collection::<StockReservation>(STOCK_RESERVATIONS), filter, options, executor)
        .await
}

// 把预占状态集合转换为持久化代码。
// 返回用于 MongoDB `$in` 的稳定代码集合。
// `statuses` 为预占状态集合。
fn reservation_status_codes(statuses: &[ReservationStatus]) -> Vec<&'static str> {
    statuses.iter().map(ReservationStatus::as_str).collect()
}

// 库存预占列表投影字段。
fn stock_reservation_projection() -> Document {
    doc! {
        "id": 1,
        "warehouse_id": 1,
        "sku_id": 1,
        "sales_order_line_id": 1,
        "reserved_quantity": 1,
        "consumed_quantity": 1,
        "released_quantity": 1,
        "status": 1,
        "version": 1,
    }
}

#[cfg(test)]
mod filter_tests {
    use erp_core::ids::WarehouseId;
    use mongodb::bson::{Bson, doc};
    use persistence_core::QueryFilter;

    use super::{StockReservationFilter, stock_reservation_sort};

    fn filter(warehouse_ids: Option<Vec<WarehouseId>>) -> StockReservationFilter {
        StockReservationFilter { warehouse_ids, ..Default::default() }
    }

    #[test]
    fn authorized_warehouse_filter_preserves_company_empty_and_exact_scope() {
        assert_eq!(
            filter(Some(vec![WarehouseId::new("warehouse-1"), WarehouseId::new("warehouse-2"),]))
                .to_doc()
                .get_document("warehouse_id")
                .unwrap(),
            &doc! { "$in": ["warehouse-1", "warehouse-2"] }
        );
        assert_eq!(
            filter(Some(Vec::new())).to_doc().get_document("warehouse_id").unwrap().get_array("$in").unwrap(),
            &Vec::<Bson>::new()
        );
        assert!(!filter(None).to_doc().contains_key("warehouse_id"));
    }

    #[test]
    fn reservation_sort_uses_unique_id_as_same_direction_tie_breaker() {
        let mut value = filter(None);
        value.sort_by = Some("created_at".to_string());
        value.sort_ascending = true;
        assert_eq!(stock_reservation_sort(&value), doc! { "created_at": 1, "id": 1 });
        value.sort_ascending = false;
        assert_eq!(stock_reservation_sort(&value), doc! { "created_at": -1, "id": -1 });
    }
}
