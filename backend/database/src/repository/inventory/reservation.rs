use std::str::FromStr;

use chrono::Local;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use entities::ids::{SalesOrderLineId, SkuId, WarehouseId};
use entities::inventory::{ReservationStatus, StockReservation, StockReservationSourceType};
use entities::money::Quantity;

use super::shared::{ids_to_strings, negate_bson, sort_doc, to_bson};
use super::{InventoryRepository, STOCK_RESERVATIONS};
use crate::executor::Executor;
use crate::repository::{PageResult, Pagination, QueryFilter, Repository};
use crate::{mongo_ops, Result};

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
    /// Service 已证明可读取的仓库集合；`None` 表示公司级，空集合表示无范围。
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

impl QueryFilter for StockReservationFilter {
    /// 转换为 MongoDB 查询条件。
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
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(sales_order_line_id) = &self.sales_order_line_id {
            filter.insert("sales_order_line_id", sales_order_line_id.to_string());
        }
        filter
    }
}

impl Pagination for StockReservationFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, StockReservation> {
    /// 分页检索库存预占列表（投影查询）。
    ///
    /// 只返回 [`StockReservationRow`] 所需的列表字段，不加载整文档；排序字段
    /// 走白名单映射（`created_at`/`updated_at`）。
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
    pub async fn search_stock_reservations(
        &self,
        filter: &StockReservationFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<StockReservationRow>> {
        let query = filter.to_doc();
        let options = FindOptions::builder()
            .sort(stock_reservation_sort(filter))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(stock_reservation_projection())
            .build();
        let collection = self.collection().clone_with_type::<StockReservationRow>();
        let items = mongo_ops::find_many(&collection, query.clone(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), query, executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// ★原子条件写★：消耗预占（`reserved -= q`、`consumed += q`）。
    ///
    /// 第一步写条件内联 `reserved_quantity >= q` 且状态为有效/部分消耗
    /// （§6.7 只有仓发消耗可以扣减预占），不足或状态不符时**直接拒绝**：
    /// 返回 `Ok(false)` 且文档不变。第二步是状态收敛写：剩余为 0 迁移到
    /// `CONSUMED`，否则迁移到 `PARTIALLY_CONSUMED`（两笔均为原子写，第一步
    /// 是本方法的原子守卫；极端并发下状态收敛由 P3 事务兜底）。本方法不更新
    /// 内存实体，调用方需要最新预占时重新读取。
    ///
    /// # 参数
    /// * `id` - 预占主键
    /// * `quantity` - 正数消耗数量
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 预占充足且命中时返回 `true`；预占不足、状态不符或预占不存在时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 写入失败时返回错误。
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
    pub async fn consume_quantity(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity = to_bson(quantity)?;
        let active_statuses = [
            ReservationStatus::Active.as_str(),
            ReservationStatus::PartiallyConsumed.as_str(),
        ];
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "reserved_quantity": { "$gte": &quantity },
            "status": { "$in": active_statuses.as_slice() },
        };
        let update = doc! {
            "$inc": {
                "reserved_quantity": negate_bson(&quantity),
                "consumed_quantity": &quantity,
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

    /// ★原子条件写★：整体释放预占（`reserved -> 0`、`released += q`）。
    ///
    /// 实体状态一致性要求 `RELEASED` 时剩余预占必须为 0（§6.7），因此本方法
    /// 只接受**全额释放**：写条件内联 `reserved_quantity == q` 且状态为有效/
    /// 部分消耗，只有释放全部剩余预占时命中并把状态迁移到 `RELEASED`；剩余
    /// 预占大于 `q` 时返回 `Ok(false)` 且文档不变（部分释放不构成合法状态，
    /// 由 P3 按单据语义编排）。本方法不更新内存实体。
    ///
    /// # 参数
    /// * `id` - 预占主键
    /// * `quantity` - 待释放的剩余预占数量（必须等于当前 `reserved_quantity`）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 剩余预占恰好等于 `quantity` 且命中时返回 `true`；数量不符、状态不符或
    /// 预占不存在时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 写入失败时返回错误。
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
    pub async fn release_quantity(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity = to_bson(quantity)?;
        let zero = to_bson(Quantity::from_str("0").expect("字面量 0 必然合法"))?;
        let active_statuses = [
            ReservationStatus::Active.as_str(),
            ReservationStatus::PartiallyConsumed.as_str(),
        ];
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "reserved_quantity": { "$eq": &quantity },
            "status": { "$in": active_statuses.as_slice() },
        };
        let update = doc! {
            "$set": {
                "reserved_quantity": &zero,
                "status": ReservationStatus::Released.as_str(),
                "updated_at": Local::now().timestamp(),
            },
            "$inc": {
                "released_quantity": &quantity,
                "version": 1,
            },
        };
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
    }
}

/// 为库存预占分页追加唯一主键 tie-breaker，避免相同主排序值跨页重复或遗漏。
fn stock_reservation_sort(filter: &StockReservationFilter) -> Document {
    let mut sort = sort_doc(
        filter.sort_by.as_deref(),
        filter.sort_ascending,
        &["created_at", "updated_at"],
    );
    sort.insert("id", if filter.sort_ascending { 1 } else { -1 });
    sort
}

impl<'a> InventoryRepository<'a> {
    /// 批量读取销售稳定行的现有库存预占。
    ///
    /// # 参数
    /// * `sales_order_line_ids` - 销售稳定行主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回由现有库存分配建立的全部未删除预占；采购入库预占不进入结果。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
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
    ///
    /// # 参数
    /// * `warehouse_id` - 仓库主键
    /// * `sku_id` - SKU 主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回有效或部分消耗的预占集合。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
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
    ///
    /// # 参数
    /// * `warehouse_ids` - 仓库主键集合
    /// * `sku_ids` - SKU 主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中任一页内仓库与 SKU 的可操作预占。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
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
    ///
    /// # 参数
    /// * `warehouse_id` - 仓库主键
    /// * `sku_id` - SKU 主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `created_at` 升序排列的可操作预占。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
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

/// 按维度条件读取可操作预占。
///
/// # 参数
/// * `db` - 数据库
/// * `filter` - 仓库与 SKU 维度条件
/// * `sort` - 可选排序条件
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回有效或部分消耗的预占。
///
/// # 错误
/// MongoDB 查询或游标读取失败时返回错误。
async fn reservations_for_dimensions(
    db: &Database,
    mut filter: Document,
    sort: Option<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<StockReservation>> {
    filter.insert(
        "status",
        doc! { "$in": reservation_status_codes(ReservationStatus::operable()) },
    );
    filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    let options = FindOptions::builder().sort(sort).build();
    mongo_ops::find_many(
        &db.collection::<StockReservation>(STOCK_RESERVATIONS),
        filter,
        options,
        executor,
    )
    .await
}

/// 把预占状态集合转换为持久化代码。
///
/// # 参数
/// * `statuses` - 预占状态集合
///
/// # 返回
/// 返回用于 MongoDB `$in` 的稳定代码集合。
fn reservation_status_codes(statuses: &[ReservationStatus]) -> Vec<&'static str> {
    statuses.iter().map(ReservationStatus::as_str).collect()
}

/// 库存预占列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
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
    use super::{stock_reservation_sort, StockReservationFilter};
    use crate::repository::QueryFilter;
    use entities::ids::WarehouseId;
    use mongodb::bson::{doc, Bson};

    fn filter(warehouse_ids: Option<Vec<WarehouseId>>) -> StockReservationFilter {
        StockReservationFilter {
            warehouse_ids,
            sku_id: None,
            status: None,
            sales_order_line_id: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }

    #[test]
    fn authorized_warehouse_filter_preserves_company_empty_and_exact_scope() {
        assert_eq!(
            filter(Some(vec![
                WarehouseId::new("warehouse-1"),
                WarehouseId::new("warehouse-2"),
            ]))
            .to_doc()
            .get_document("warehouse_id")
            .unwrap(),
            &doc! { "$in": ["warehouse-1", "warehouse-2"] }
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
    fn reservation_sort_uses_unique_id_as_same_direction_tie_breaker() {
        let mut value = filter(None);
        value.sort_by = Some("created_at".to_string());
        value.sort_ascending = true;
        assert_eq!(stock_reservation_sort(&value), doc! { "created_at": 1, "id": 1 });
        value.sort_ascending = false;
        assert_eq!(
            stock_reservation_sort(&value),
            doc! { "created_at": -1, "id": -1 }
        );
    }
}
