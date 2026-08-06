//! 域 D17 `inventory` 仓储：stock_movement、stock_balance、
//! stock_reservation(+_entry)、stock_adjustment(+_line)（页面：W10）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询、**原子
//! 条件写入口**与跨集合多步骤写入。集合名常量统一从 `InventoryExt` 关联常量导入。
//!
//! ★高并发热点（P2 §5）★：`stock_balance` 与 `stock_reservation` 的扣减/预占
//! 一律实现为**写条件**（filter 内联 `available_quantity >= q` /
//! `reserved_quantity >= q`），禁止「先读后写」；写条件未命中时返回 `Ok(false)`
//! 且文档不变。余额与预占之间的一致性维护属于 P3 过账事务（§8.2），本层只
//! 提供原子写入口。
//!
//! 软删除边界（§4.5）：`stock_movement` 与 `stock_reservation_entry` 是正式
//! 事实，**不可更新或删除**（§6.7），本域不提供其软删除方法；`stock_balance`
//! 由流水驱动重建，同样不设业务软删除。单据类（`stock_adjustment` 草稿/驳回）
//! 可逻辑删除。
//!
//! 筛选/行类型定义在本文件，经 `InventoryExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use std::str::FromStr;

use chrono::Local;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use entities::common::time::Instant;
use entities::ids::{
    SalesOrderLineId, SkuId, StockAdjustmentId, StockMovementId, StockReservationId, WarehouseId,
};
use entities::inventory::{
    AdjustmentReasonType, MovementDirection, MovementType, ReservationStatus, StockAdjustment,
    StockAdjustmentLine, StockAdjustmentState, StockBalance, StockMovement, StockReservation,
    StockReservationEntry,
};
use entities::money::Quantity;

use super::extensions::InventoryExt;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `stock_movement` 集合名（单一来源：`InventoryExt` 关联常量）。
const STOCK_MOVEMENTS: &str = <mongodb::Database as InventoryExt>::STOCK_MOVEMENTS;
/// `stock_reservation_entry` 集合名（单一来源：`InventoryExt` 关联常量）。
const STOCK_RESERVATION_ENTRIES: &str = <mongodb::Database as InventoryExt>::STOCK_RESERVATION_ENTRIES;
/// `stock_adjustment_line` 集合名（单一来源：`InventoryExt` 关联常量）。
const STOCK_ADJUSTMENT_LINES: &str = <mongodb::Database as InventoryExt>::STOCK_ADJUSTMENT_LINES;

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
}

/// 库存流水列表筛选条件（正式事实，恒为未删除）。
#[derive(Debug, Clone)]
pub struct StockMovementFilter {
    /// 仓库；`None` 表示不筛选。
    pub warehouse_id: Option<WarehouseId>,
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
        if let Some(warehouse_id) = &self.warehouse_id {
            filter.insert("warehouse_id", warehouse_id.to_string());
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

/// 库存余额列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockBalanceRow {
    /// 实体主键。
    pub id: String,
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// SKU。
    pub sku_id: SkuId,
    /// 账面现存。
    pub on_hand_quantity: Quantity,
    /// 有效预占。
    pub reserved_quantity: Quantity,
    /// 可用数量。
    pub available_quantity: Quantity,
    /// 已应用最后流水。
    pub last_movement_id: Option<StockMovementId>,
    /// 乐观锁版本。
    pub version: u64,
}

/// 库存余额列表筛选条件。
#[derive(Debug, Clone)]
pub struct StockBalanceFilter {
    /// 仓库；`None` 表示不筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// SKU；`None` 表示不筛选。
    pub sku_id: Option<SkuId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `sku_id`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for StockBalanceFilter {
    /// 转换为 MongoDB 查询条件（余额不设业务软删除，恒为未删除）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(warehouse_id) = &self.warehouse_id {
            filter.insert("warehouse_id", warehouse_id.to_string());
        }
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id.to_string());
        }
        filter
    }
}

impl Pagination for StockBalanceFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

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
    /// 仓库；`None` 表示不筛选。
    pub warehouse_id: Option<WarehouseId>,
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
        if let Some(warehouse_id) = &self.warehouse_id {
            filter.insert("warehouse_id", warehouse_id.to_string());
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

/// 库存调整单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockAdjustmentRow {
    /// 实体主键。
    pub id: String,
    /// 调整单号。
    pub adjustment_no: String,
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// 调整原因类型。
    pub reason_type: AdjustmentReasonType,
    /// 当前状态。
    pub status: StockAdjustmentState,
    /// 仓储经办人。
    pub prepared_by: String,
    /// 仓储复核人。
    pub reviewed_by: Option<String>,
    /// 成本影响确认人。
    pub finance_reviewed_by: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 库存调整单列表筛选条件。
#[derive(Debug, Clone)]
pub struct StockAdjustmentFilter {
    /// 仓库；`None` 表示不筛选。
    pub warehouse_id: Option<WarehouseId>,
    /// 单据状态；`None` 表示不筛选。
    pub status: Option<StockAdjustmentState>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for StockAdjustmentFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(warehouse_id) = &self.warehouse_id {
            filter.insert("warehouse_id", warehouse_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for StockAdjustmentFilter {
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
    pub async fn search_stock_movements(
        &self,
        filter: &StockMovementFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<StockMovementRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["occurred_at", "recorded_at", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(stock_movement_projection())
            .build();
        let collection = self.collection().clone_with_type::<StockMovementRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
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

impl<'a> Repository<'a, StockBalance> {
    /// 分页检索库存余额列表（投影查询）。
    ///
    /// 只返回 [`StockBalanceRow`] 所需的列表字段，不加载整文档；排序字段走
    /// 白名单映射（`sku_id`/`created_at`）。
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
    pub async fn search_stock_balances(
        &self,
        filter: &StockBalanceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<StockBalanceRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["sku_id", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(stock_balance_projection())
            .build();
        let collection = self.collection().clone_with_type::<StockBalanceRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按库存维度查找余额（`(warehouse_id, sku_id)` 全局唯一）。
    ///
    /// # 参数
    /// * `warehouse_id` - 仓库
    /// * `sku_id` - SKU
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的余额；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_dimensions(
        &self,
        warehouse_id: &WarehouseId,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<Option<StockBalance>> {
        self.find_one(
            doc! {
                "warehouse_id": warehouse_id.to_string(),
                "sku_id": sku_id.to_string(),
            },
            executor,
        )
        .await
    }

    /// ★原子条件写★：入库增加账面现存（`on_hand += q`、`available += q`）。
    ///
    /// 以 `id` 定位的单文档原子 `$inc`，不读取旧值；写入条件只要求余额行存在。
    /// 维护恒等式 `available = on_hand - reserved`（§6.7）。本方法不更新内存
    /// 实体，调用方需要最新余额时重新读取。
    ///
    /// # 参数
    /// * `id` - 余额主键
    /// * `quantity` - 正数入库数量
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 命中并完成增加返回 `true`；余额行不存在时返回 `false`（文档不变）。
    ///
    /// # 错误
    /// 当 MongoDB 写入失败时返回错误。
    pub async fn increase_on_hand(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let filter = doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        let update = both_inc(quantity, "on_hand_quantity", "available_quantity")?;
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
    }

    /// ★原子条件写★：预占可用量（`reserved += q`、`available -= q`）。
    ///
    /// 写条件内联 `available_quantity >= q`（§6.7 可用量守恒），可用量不足时
    /// **直接拒绝**：返回 `Ok(false)` 且文档不变，不是「先读后写」的判断。
    /// 本方法不更新内存实体，调用方需要最新余额时重新读取。
    ///
    /// # 参数
    /// * `id` - 余额主键
    /// * `quantity` - 正数预占数量
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 可用量充足且命中时返回 `true`；可用量不足或余额行不存在时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 写入失败时返回错误。
    pub async fn reserve_quantity(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity = to_bson(quantity)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "available_quantity": { "$gte": &quantity },
        };
        let update = cross_inc(quantity, "reserved_quantity", "available_quantity")?;
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
    }

    /// ★原子条件写★：扣减可用量（`on_hand -= q`、`available -= q`）。
    ///
    /// 写条件内联 `available_quantity >= q`（§6.7 不产生负库存），可用量不足
    /// 时**直接拒绝**：返回 `Ok(false)` 且文档不变，不是「先读后写」的判断。
    /// 本方法不更新内存实体，调用方需要最新余额时重新读取。
    ///
    /// # 参数
    /// * `id` - 余额主键
    /// * `quantity` - 正数出库数量
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 可用量充足且命中时返回 `true`；可用量不足或余额行不存在时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 写入失败时返回错误。
    pub async fn deduct_available(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity = to_bson(quantity)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "available_quantity": { "$gte": &quantity },
        };
        let update = both_dec(quantity, "on_hand_quantity", "available_quantity")?;
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
    }

    /// ★原子条件写★：释放预占（`reserved -= q`、`available += q`）。
    ///
    /// 写条件内联 `reserved_quantity >= q`（不允许释放超过已预占），不足时
    /// **直接拒绝**：返回 `Ok(false)` 且文档不变。本方法不更新内存实体，
    /// 调用方需要最新余额时重新读取。
    ///
    /// # 参数
    /// * `id` - 余额主键
    /// * `quantity` - 正数释放数量
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 预占充足且命中时返回 `true`；预占不足或余额行不存在时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 写入失败时返回错误。
    pub async fn release_reserved(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity = to_bson(quantity)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "reserved_quantity": { "$gte": &quantity },
        };
        let update = cross_inc(quantity, "available_quantity", "reserved_quantity")?;
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
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
    pub async fn search_stock_reservations(
        &self,
        filter: &StockReservationFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<StockReservationRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "updated_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(stock_reservation_projection())
            .build();
        let collection = self.collection().clone_with_type::<StockReservationRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
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

impl<'a> Repository<'a, StockAdjustment> {
    /// 分页检索库存调整单列表（投影查询）。
    ///
    /// 只返回 [`StockAdjustmentRow`] 所需的列表字段，不加载整文档；排序字段
    /// 走白名单映射（`created_at`/`adjustment_no`）。
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
    pub async fn search_stock_adjustments(
        &self,
        filter: &StockAdjustmentFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<StockAdjustmentRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "adjustment_no"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(stock_adjustment_projection())
            .build();
        let collection = self.collection().clone_with_type::<StockAdjustmentRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按调整单号查找调整单（唯一单号，详情查询）。
    ///
    /// # 参数
    /// * `adjustment_no` - 调整单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除调整单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_adjustment_no(
        &self,
        adjustment_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<StockAdjustment>> {
        self.find_one_by_field("adjustment_no", adjustment_no, executor)
            .await
    }
}

/// D17 域专用仓储：跨集合批量查询与多步骤事务写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型承载按表头批量取行（`$in`
/// 一次取回，禁止 N+1）与依赖事务的跨集合原子写入入口，由
/// `InventoryExt::inventory()` 访问。
pub struct InventoryRepository<'a> {
    db: &'a Database,
}

impl<'a> InventoryRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 批量读取库存流水（`$in` 一次取回）。
    ///
    /// 供余额重建/过账核对一次性加载指定流水，禁止 N+1。
    ///
    /// # 参数
    /// * `movement_ids` - 流水主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配流水。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn movements_by_ids(
        &self,
        movement_ids: &[StockMovementId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockMovement>> {
        let mut movements = find_by_field_in(
            self.db,
            STOCK_MOVEMENTS,
            "id",
            &ids_to_strings(movement_ids),
            executor,
        )
        .await?;
        movements.sort_by(|left: &StockMovement, right: &StockMovement| left.base.id.cmp(&right.base.id));
        Ok(movements)
    }

    /// 批量读取预占流水（`$in` 一次取回）。
    ///
    /// 供预占明细/冲正核对一次性加载指定预占的全部流水，禁止 N+1。
    ///
    /// # 参数
    /// * `reservation_ids` - 预占主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配流水。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn reservation_entries_by_reservation_ids(
        &self,
        reservation_ids: &[StockReservationId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockReservationEntry>> {
        find_by_field_in(
            self.db,
            STOCK_RESERVATION_ENTRIES,
            "reservation_id",
            &ids_to_strings(reservation_ids),
            executor,
        )
        .await
    }

    /// 批量读取库存调整明细（`$in` 一次取回）。
    ///
    /// 供调整单详情/过账计算一次性加载全部明细，禁止 N+1。
    ///
    /// # 参数
    /// * `adjustment_ids` - 调整单主键集合（为空时直接返回空列表）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn adjustment_lines_by_adjustment_ids(
        &self,
        adjustment_ids: &[StockAdjustmentId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockAdjustmentLine>> {
        find_by_field_in(
            self.db,
            STOCK_ADJUSTMENT_LINES,
            "stock_adjustment_id",
            &ids_to_strings(adjustment_ids),
            executor,
        )
        .await
    }

    /// 创建库存调整单及全部明细（跨集合多步骤写入）。
    ///
    /// 依次写入 `stock_adjustments` 与 `stock_adjustment_lines`，保证表头与
    /// 明细原子可见（§6.7）。**必须收到事务执行器**：本方法不构成原子边界，
    /// 传入 `NoTransaction` 时两笔写入各自自动提交，中途失败会留下只有表头
    /// 没有明细的半成品；Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `adjustment` - 待写入的调整单表头
    /// * `lines` - 待写入的调整明细集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_stock_adjustment_with_lines(
        &self,
        adjustment: &StockAdjustment,
        lines: &[StockAdjustmentLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<StockAdjustment>(<mongodb::Database as InventoryExt>::STOCK_ADJUSTMENTS),
            adjustment,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<StockAdjustmentLine>(STOCK_ADJUSTMENT_LINES),
            lines.to_vec(),
            executor,
        )
        .await
    }
}

/// 把 ID newtype 集合转为字符串集合（用于 `$in` 查询）。
///
/// # 参数
/// * `ids` - ID newtype 集合
///
/// # 返回
/// 返回字符串集合。
fn ids_to_strings<T: AsRef<str>>(ids: &[T]) -> Vec<String> {
    ids.iter().map(|id| id.as_ref().to_string()).collect()
}

/// 按给定字段 `$in` 批量读取实体（空集合直接返回空列表）。
async fn find_by_field_in<T>(
    db: &Database,
    collection_name: &str,
    field: &str,
    values: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de> + Serialize + Send + Sync,
{
    if values.is_empty() {
        return Ok(Vec::new());
    }
    let collection = db.collection::<T>(collection_name);
    mongo_ops::find_many(
        &collection,
        doc! {
            field: { "$in": values },
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        },
        FindOptions::default(),
        executor,
    )
    .await
}

/// 把 `Quantity` 转为 BSON `Decimal128`（存储形态由 P0 固化，不做任何舍入/换算）。
///
/// `bson::to_bson` 的人性化序列化器会把数量写成字符串（$numberDecimal 无法
/// 参与 `$inc`/`$gte`），必须走非人性化序列化器产出真正的 `Decimal128` 变体。
/// bson 2.15 对 `SerializerOptions::human_readable(false)` 标记了 deprecated
/// （bson 自身代码也以 `#[allow(deprecated)]` 使用该配置），但官方文档仍以此
/// 为「非人性化序列化」的唯一入口，且 `serde_helpers::HumanReadable` 只提供
/// 反向（强制人性化）；此处按文档保留该配置。
///
/// # 参数
/// * `quantity` - 定点数量
///
/// # 返回
/// 返回 Decimal128 BSON 值。
///
/// # 错误
/// BSON 序列化失败时返回错误。
#[allow(deprecated)]
fn to_bson(quantity: Quantity) -> Result<Bson> {
    Ok(mongodb::bson::to_bson_with_options(
        &quantity,
        mongodb::bson::SerializerOptions::builder()
            .human_readable(false)
            .build(),
    )?)
}

/// 对 `Quantity` 取相反数并转为 Decimal128 BSON（符号翻转，不改变精度与数值语义）。
///
/// 仅用于 `$inc` 的负方向累加；数量类型构造与 BSON 序列化不做任何舍入。
///
/// # 参数
/// * `quantity` - 定点数量
///
/// # 返回
/// 返回相反数的 Decimal128 BSON 值。
///
/// # 错误
/// 数量类型构造或 BSON 序列化失败时返回错误。
fn negate_bson(quantity: &Bson) -> Bson {
    let Bson::Decimal128(decimal) = quantity else {
        return quantity.clone();
    };
    let mut bytes = decimal.bytes();
    bytes[15] ^= 0x80;
    Bson::Decimal128(mongodb::bson::Decimal128::from_bytes(bytes))
}

/// 构建两个字段同向增加的原子 `$inc` 更新（含 `version` 与 `updated_at` 元数据）。
///
/// # 参数
/// * `quantity` - 增加数量（Decimal128）
/// * `field_a` - 增加字段一
/// * `field_b` - 增加字段二
///
/// # 返回
/// 返回更新条件文档。
fn both_inc(quantity: Quantity, field_a: &str, field_b: &str) -> Result<Document> {
    let quantity = to_bson(quantity)?;
    Ok(doc! {
        "$inc": { field_a: &quantity, field_b: &quantity, "version": 1 },
        "$set": { "updated_at": Local::now().timestamp() },
    })
}

/// 构建两个字段同向减少的原子 `$inc` 更新（含 `version` 与 `updated_at` 元数据）。
///
/// # 参数
/// * `quantity` - 减少数量（Decimal128）
/// * `field_a` - 减少字段一
/// * `field_b` - 减少字段二
///
/// # 返回
/// 返回更新条件文档。
fn both_dec(quantity: Bson, field_a: &str, field_b: &str) -> Result<Document> {
    Ok(doc! {
        "$inc": { field_a: negate_bson(&quantity), field_b: negate_bson(&quantity), "version": 1 },
        "$set": { "updated_at": Local::now().timestamp() },
    })
}

/// 构建一个字段增加、另一个字段减少的原子 `$inc` 更新（含 `version` 与 `updated_at` 元数据）。
///
/// # 参数
/// * `quantity` - 增加数量（Decimal128）
/// * `increase_field` - 增加字段
/// * `decrease_field` - 减少字段
///
/// # 返回
/// 返回更新条件文档。
fn cross_inc(quantity: Bson, increase_field: &str, decrease_field: &str) -> Result<Document> {
    Ok(doc! {
        "$inc": { increase_field: &quantity, decrease_field: negate_bson(&quantity), "version": 1 },
        "$set": { "updated_at": Local::now().timestamp() },
    })
}

/// 构建排序文档（字段名白名单映射）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段白名单
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|field| allowed.contains(field))
        .unwrap_or("created_at");
    doc! { field: direction }
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
    }
}

/// 库存余额列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn stock_balance_projection() -> Document {
    doc! {
        "id": 1,
        "warehouse_id": 1,
        "sku_id": 1,
        "on_hand_quantity": 1,
        "reserved_quantity": 1,
        "available_quantity": 1,
        "last_movement_id": 1,
        "version": 1,
    }
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

/// 库存调整单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn stock_adjustment_projection() -> Document {
    doc! {
        "id": 1,
        "adjustment_no": 1,
        "warehouse_id": 1,
        "reason_type": 1,
        "status": 1,
        "prepared_by": 1,
        "reviewed_by": 1,
        "finance_reviewed_by": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{ids_to_strings, negate_bson, sort_doc, QueryFilter, StockMovementFilter};
    use mongodb::bson::{doc, Bson};
    use std::str::FromStr;

    use entities::common::time::Instant;
    use entities::ids::{SkuId, WarehouseId};
    use entities::inventory::{MovementDirection, MovementType};

    #[test]
    fn movement_filter_applies_dimensions_type_range_and_deleted_filter() {
        let filter = StockMovementFilter {
            warehouse_id: Some(WarehouseId::new("wh-1")),
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
        assert_eq!(document.get_str("warehouse_id").unwrap(), "wh-1");
        assert_eq!(document.get_str("sku_id").unwrap(), "sku-1");
        assert_eq!(document.get_str("movement_type").unwrap(), "PURCHASE_RECEIPT_IN");
        assert_eq!(document.get_str("direction").unwrap(), "INCREASE");
        let range = document.get_document("occurred_at").unwrap();
        assert_eq!(range.get_i64("$gte").unwrap(), 1_700_000_000);
        assert_eq!(range.get_i64("$lte").unwrap(), 1_700_000_100);
    }

    #[test]
    fn sort_doc_maps_whitelisted_fields_and_defaults_otherwise() {
        let allowed = ["occurred_at", "recorded_at"];
        assert_eq!(sort_doc(None, false, &allowed), doc! { "created_at": -1 });
        assert_eq!(
            sort_doc(Some("occurred_at"), true, &allowed),
            doc! { "occurred_at": 1 }
        );
        assert_eq!(
            sort_doc(Some("任意字段"), false, &allowed),
            doc! { "created_at": -1 },
            "白名单外的字段名回落默认排序"
        );
    }

    #[test]
    fn negate_bson_flips_sign_without_touching_magnitude() {
        let positive = Bson::Decimal128(mongodb::bson::Decimal128::from_str("12.345").unwrap());
        let negative = negate_bson(&positive);
        assert_eq!(
            negative,
            Bson::Decimal128(mongodb::bson::Decimal128::from_str("-12.345").unwrap())
        );
        assert_eq!(negate_bson(&negative), positive, "两次取反恢复原值");
    }

    #[test]
    fn ids_to_strings_converts_newtype_collection() {
        let ids = vec![WarehouseId::new("wh-1"), WarehouseId::new("wh-2")];
        assert_eq!(ids_to_strings(&ids), vec!["wh-1".to_string(), "wh-2".to_string()]);
        assert!(ids_to_strings::<WarehouseId>(&[]).is_empty());
    }
}
