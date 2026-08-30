use chrono::Local;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use entities::common::time::Instant;
use entities::ids::{StockAdjustmentId, WarehouseId};
use entities::inventory::{
    AdjustmentReasonType, MovementDirection, StockAdjustment, StockAdjustmentLine, StockAdjustmentState,
};
use entities::money::Quantity;

use super::shared::{
    active_entity_by_id, entities_by_ids, find_by_field_in, ids_to_strings, sort_doc, to_bson,
};
use super::{InventoryRepository, STOCK_ADJUSTMENTS, STOCK_ADJUSTMENT_LINES};
use crate::executor::Executor;
use crate::repository::extensions::InventoryExt;
use crate::repository::{PageResult, Pagination, QueryFilter, Repository};
use crate::{mongo_ops, Result};

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
    /// 原因说明。
    pub note: Option<String>,
    /// 业务发生时间。
    pub occurred_at: Option<Instant>,
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
    #[tracing::instrument(
        name = "repository.inventory.search_stock_adjustments",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_adjustments",
            db.operation.name = "search"
        )
    )]
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
}

impl<'a> InventoryRepository<'a> {
    /// 按主键读取未删除的库存调整单。
    ///
    /// # 参数
    /// * `id` - 调整单主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配调整单；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn stock_adjustment(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<StockAdjustment>> {
        active_entity_by_id(self.db, STOCK_ADJUSTMENTS, id, executor).await
    }

    /// 读取指定仓库尚未过账的库存调整单。
    ///
    /// # 参数
    /// * `warehouse_id` - 仓库主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回草稿与审批中的调整单。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    #[tracing::instrument(
        name = "repository.inventory.pending_adjustments_for_warehouse",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_adjustments",
            db.operation.name = "find"
        )
    )]
    pub async fn pending_adjustments_for_warehouse(
        &self,
        warehouse_id: &WarehouseId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockAdjustment>> {
        let statuses = stock_adjustment_state_codes(StockAdjustmentState::pending_posting());
        mongo_ops::find_many(
            &self.db.collection::<StockAdjustment>(STOCK_ADJUSTMENTS),
            doc! {
                "warehouse_id": warehouse_id.to_string(),
                "status": { "$in": statuses },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await
    }

    /// 按主键集合批量读取库存调整单。
    ///
    /// # 参数
    /// * `ids` - 调整单主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部命中的未删除调整单。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn stock_adjustments_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockAdjustment>> {
        entities_by_ids(self.db, STOCK_ADJUSTMENTS, ids, executor).await
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
    #[tracing::instrument(
        name = "repository.inventory.create_stock_adjustment_with_lines",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.operation.name = "create_stock_adjustment_with_lines"
        )
    )]
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

    /// 更新调整明细数量与方向（草稿/驳回编辑；数量必须为正）。
    ///
    /// 行命中即 `$set quantity`（方向非空时一并更新）并推进行版本；
    /// 行不存在或已软删除时返回 `false`。
    ///
    /// # 参数
    /// * `id` - 明细行主键
    /// * `quantity` - 正数调整数量
    /// * `direction` - 调整方向（`None` 表示保持现状）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 命中并更新返回 `true`；行不存在时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 写入失败时返回错误。
    pub async fn update_adjustment_line(
        &self,
        id: &str,
        quantity: Quantity,
        direction: Option<MovementDirection>,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity = to_bson(quantity)?;
        let mut set = doc! {
            "quantity": quantity,
            "updated_at": Local::now().timestamp(),
        };
        if let Some(direction) = direction {
            set.insert("direction", mongodb::bson::serialize_to_bson(&direction)?);
        }
        let result = mongo_ops::update_one(
            &self.db.collection::<StockAdjustmentLine>(STOCK_ADJUSTMENT_LINES),
            doc! {
                "id": id,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            doc! {
                "$set": set,
                "$inc": { "version": 1 },
            },
            false,
            executor,
        )
        .await?;
        Ok(result.matched_count > 0)
    }

    /// 持久化已通过实体规则校验的调整明细值。
    ///
    /// # 参数
    /// * `line` - 已更新的调整明细实体
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 命中并更新返回 `true`；行不存在或已删除时返回 `false`。
    ///
    /// # 错误
    /// MongoDB 写入或字段序列化失败时返回错误。
    pub async fn persist_adjustment_line(
        &self,
        line: &StockAdjustmentLine,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.update_adjustment_line(&line.base.id, line.quantity, Some(line.direction), executor)
            .await
    }
}

/// 把调整单状态集合转换为持久化代码。
///
/// # 参数
/// * `statuses` - 调整单状态集合
///
/// # 返回
/// 返回用于 MongoDB `$in` 的稳定代码集合。
fn stock_adjustment_state_codes(statuses: &[StockAdjustmentState]) -> Vec<&'static str> {
    statuses.iter().map(StockAdjustmentState::as_str).collect()
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
        "note": 1,
        "occurred_at": 1,
        "version": 1,
        "created_at": 1,
    }
}
