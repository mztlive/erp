use std::str::FromStr;

use database::InventoryExt;
use entities::inventory::{
    MovementDirection, ReservationEntryType, StockAdjustment, StockAdjustmentLine, StockMovement,
    StockMovementData, StockReservationEntry, StockReservationEntryData,
};
use erp_audit::AuditExt;
use erp_core::common::source::SourceType;
use erp_core::common::time::Instant;
use erp_core::ids::{StockAdjustmentId, StockMovementId, StockReservationEntryId};
use erp_core::money::Quantity;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::{AssignmentSource, WorkItemStatus, WorkItemType};
use erp_workflow::ApprovalIntegrationExt;
use erp_workflow::BpmExt;
use erp_workflow::WorkItemExt;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::errors::{Error, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_workflow::ApprovalActionContext;

use super::adapter::{require_frozen_binding, stock_adjustment_adapter};
use super::approval_query::load_approval_binding;
use super::dto::StockAdjustmentView;
use super::InventoryService;

impl InventoryService {
    /// 在审批运行时持有的唯一事务内过账库存调整单。
    ///
    /// 本方法是合同 §4.4.4 签署的最终通过端口。调用方必须传入审批决定
    /// 事务的执行器；`NoTransaction` 会失败关闭，防止脱离 BPM 终态写入
    /// 单据、库存流水、余额和审计。
    ///
    /// # 参数
    /// * `context` - 审批运行时冻结的实例、执行、任务、业务对象与主题版本
    /// * `actor` - 已认证的审批决定操作人
    /// * `executor` - 审批运行时持有的事务执行器
    ///
    /// # 返回
    /// 返回事务内已过账的库存调整单视图。
    ///
    /// # 错误
    /// 缺少事务会话、非审批中、明细/余额不完整、库存不足、重复过账或任一
    /// 写入失败时返回错误；事务由审批运行时整体提交或回滚。
    pub async fn post_stock_adjustment(
        &self,
        context: &ApprovalActionContext,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<StockAdjustmentView> {
        let session = executor
            .session()
            .ok_or_else(|| Error::Internal("库存调整审批过账缺少运行时事务会话".to_string()))?;
        post_stock_adjustment_write(&self.db, context, actor, session)
            .await
            .map(Into::into)
    }
}

/// 在审批运行时持有的事务内过账库存调整单。
///
/// # 参数
/// * `db` - 数据库实例
/// * `adjustment_id` - 库存调整单 ID
/// * `actor` - 已认证操作人
/// * `session` - 审批运行时持有的唯一事务会话
///
/// # 返回
/// 返回事务内已推进到过账状态的调整单。
///
/// # 错误
/// 调整单/明细不存在、方向或库存不变量失败、任一写入失败时返回错误。
async fn post_stock_adjustment_write(
    db: &Database,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<StockAdjustment> {
    let adjustment_id = StockAdjustmentId::new(context.business_object_id());
    let mut adjustment = db
        .inventory()
        .stock_adjustment(adjustment_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
    validate_post_runtime_context(db, context, actor, &adjustment, session).await?;
    adjustment
        .ensure_approval_postable()
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    let lines = db
        .inventory()
        .adjustment_lines_by_adjustment_ids(std::slice::from_ref(&adjustment_id), session)
        .await?;
    if lines.is_empty() {
        return Err(Error::ValidationError("库存调整单没有明细，无法过账".to_string()));
    }
    let occurred_at = Instant::now();
    for line in &lines {
        adjustment.reason_type.ensure_direction(line.direction)?;
        post_adjustment_line(db, session, &adjustment, line, &occurred_at, actor).await?;
    }
    adjustment.mark_posted()?;
    db.stock_adjustments().update(&mut adjustment, session).await?;
    let audit = actor.clone().resource_log(
        "stock_adjustment.post",
        "stock_adjustment",
        adjustment_id.to_string(),
    )?;
    db.audit_logs().create(&audit, session).await?;
    Ok(adjustment)
}

/// 逐项验证最终通过动作仍对应当前运行实例、执行、任务和冻结主题版本。
///
/// # 错误
/// 任一运行身份缺失、漂移，或当前处理人与开放任务责任不一致时返回冲突，
/// 禁止写入任何库存事实。
async fn validate_post_runtime_context(
    db: &Database,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    adjustment: &StockAdjustment,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    if context.business_object_type() != DocumentType::StockAdjustment.as_str()
        || context.actor_id() != actor.id()
    {
        return Err(Error::ConflictError("库存调整最终动作上下文不匹配".to_string()));
    }
    let subject_version = context
        .subject_version()
        .parse::<u32>()
        .map_err(|_| Error::ConflictError("库存调整审批主题版本无效".to_string()))?;
    if subject_version != adjustment.approval_subject_version {
        return Err(Error::ConflictError("库存调整审批主题版本已变化".to_string()));
    }
    let execution_id = context
        .approval_node_execution_id()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::ConflictError("库存调整最终动作缺少审批执行".to_string()))?;
    let work_item_id = context
        .work_item_id()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::ConflictError("库存调整最终动作缺少审批任务".to_string()))?;
    let instance_id = bpm::ids::ApprovalProcessInstanceId::new(context.approval_process_instance_id());
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&instance_id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批实例不存在".to_string()))?;
    let binding = load_approval_binding(db, &adjustment.base.id, session).await?;
    let binding = require_frozen_binding(binding.as_ref())?;
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批实例缺少冻结快照".to_string()))?;
    snapshot
        .ensure_matches_runtime_subject(
            DocumentType::StockAdjustment,
            adjustment.base.id.as_str(),
            subject_version,
        )
        .map_err(|_| Error::ConflictError("库存调整审批冻结快照已变化".to_string()))?;
    let current_execution = instance
        .current_node_execution_id
        .as_ref()
        .map(ToString::to_string);
    if instance.status != bpm::model::types::ApprovalProcessInstanceStatus::Running
        || instance.process_kind
            != erp_workflow::service::approval::process_kind::process_kind_of(DocumentType::StockAdjustment)
        || instance.process_definition_id != binding.approval_process_definition_id
        || instance.definition_version != binding.approval_definition_version
        || instance.subject.subject_kind() != DocumentType::StockAdjustment.as_str()
        || instance.subject.subject_id() != adjustment.base.id
        || instance.subject_version != subject_version
        || current_execution.as_deref() != Some(execution_id)
    {
        return Err(Error::ConflictError("库存调整审批实例上下文已变化".to_string()));
    }
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&bpm::ids::ApprovalNodeExecutionId::new(execution_id), session)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批执行不存在".to_string()))?;
    if execution.process_instance_id != instance_id
        || execution.status != bpm::model::types::ApprovalNodeExecutionStatus::Active
        || execution.round_no != instance.current_round_no
        || execution.blocker_code.is_some()
        || execution.assignee_participant_id.as_str() != actor.id()
    {
        return Err(Error::ConflictError("库存调整审批执行上下文已变化".to_string()));
    }
    let task = db
        .work_items()
        .find_document_approval_by_id(work_item_id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批任务不存在".to_string()))?;
    let adapter = stock_adjustment_adapter()?;
    if task.work_item_type != WorkItemType::DocumentApproval
        || task.status != WorkItemStatus::Open
        || task.assignment_source != AssignmentSource::ApprovalRuntime
        || task
            .approval_node_execution_id
            .as_ref()
            .map(ToString::to_string)
            .as_deref()
            != Some(execution_id)
        || task.business_object_type != DocumentType::StockAdjustment.as_str()
        || task.business_object_id != adjustment.base.id
        || task.subject_version != context.subject_version()
        || task.owner_user_id.as_deref() != Some(actor.id())
        || task.owner_role != adapter.owner_role
        || task.owner_organization_id != snapshot.payload.responsible_org_id
    {
        return Err(Error::ConflictError("库存调整审批任务上下文已变化".to_string()));
    }
    Ok(())
}

/// 过账单条调整明细（流水 + 余额 + 适用预占释放，位于调用方事务内）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `adjustment` - 调整单表头
/// * `line` - 调整明细
/// * `occurred_at` - 过账业务时间
/// * `actor` - 审计操作人（记录人身份）
///
/// # 返回
/// 无返回值；流水/余额/预占写入失败时返回错误。
///
/// # 错误
/// 余额缺失、可用量不足或写入失败时返回错误。
async fn post_adjustment_line(
    db: &Database,
    session: &mut mongodb::ClientSession,
    adjustment: &StockAdjustment,
    line: &StockAdjustmentLine,
    occurred_at: &Instant,
    actor: &AuditActor,
) -> Result<()> {
    let movement_type = adjustment.reason_type.movement_type();
    let movement = StockMovement::new(
        StockMovementId::new(next_id()),
        StockMovementData {
            warehouse_id: adjustment.warehouse_id.clone(),
            sku_id: line.sku_id.clone(),
            movement_type,
            direction: line.direction,
            quantity: line.quantity,
            source_document_id: adjustment.base.id.clone(),
            source_line_id: Some(line.base.id.clone()),
            reversal_of_movement_id: None,
            fact_no: next_id(),
            occurred_at: adjustment.occurred_at.unwrap_or(*occurred_at),
            recorded_at: *occurred_at,
            recorded_by: actor.id().to_string(),
            source_type: SourceType::Erp,
            source_reference: None,
            reason_code: Some(adjustment.reason_type.as_str().to_string()),
            reason_text: adjustment.note.clone(),
        },
    )?;
    db.stock_movements().create(&movement, session).await?;

    let balance = db
        .inventory()
        .balance_for_dimensions(&adjustment.warehouse_id, &line.sku_id, session)
        .await?
        .ok_or_else(|| {
            Error::BusinessLogicError(format!(
                "库存余额不存在（仓库 {}，SKU {}），请先建立期初或入库",
                adjustment.warehouse_id.as_ref(),
                line.sku_id.as_ref()
            ))
        })?;
    match line.direction {
        MovementDirection::Increase => {
            if !db
                .stock_balances()
                .increase_on_hand(&balance.base.id, line.quantity, session)
                .await?
            {
                return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
            }
        }
        MovementDirection::Decrease => {
            release_applicable_reservations(
                db,
                session,
                &adjustment.warehouse_id,
                &line.sku_id,
                &balance.base.id,
                line,
            )
            .await?;
            if !db
                .stock_balances()
                .deduct_available(&balance.base.id, line.quantity, session)
                .await?
            {
                return Err(Error::BusinessLogicError(
                    "可用库存不足，无法过账库存调整".to_string(),
                ));
            }
        }
    }
    // 余额记录最后流水（台账「最后变动」列），与数量增减同事务
    if !db
        .stock_balances()
        .apply_last_movement(&balance.base.id, &movement.base.id, session)
        .await?
    {
        return Err(Error::BusinessLogicError("库存余额行不存在".to_string()));
    }
    Ok(())
}

/// 释放调整仓库/SKU 上的适用预占（盘亏/损坏扣减前）。
///
/// 按预占建立时间顺序整体释放（预占释放只支持全额释放，§6.7），释放总量
/// 不超过本明细扣减数量；释放的同时写预占释放流水并同步余额预占。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `warehouse_id` - 调整仓库
/// * `sku_id` - 调整 SKU
/// * `balance_id` - 库存余额主键（同步释放预占）
/// * `line` - 调整明细（来源单据引用）
///
/// # 返回
/// 无返回值。
///
/// # 错误
/// 释放写入失败时返回错误。
async fn release_applicable_reservations(
    db: &Database,
    session: &mut mongodb::ClientSession,
    warehouse_id: &erp_core::ids::WarehouseId,
    sku_id: &erp_core::ids::SkuId,
    balance_id: &str,
    line: &StockAdjustmentLine,
) -> Result<()> {
    let reservations = db
        .inventory()
        .oldest_operable_reservations(warehouse_id, sku_id, session)
        .await?;
    let mut released_total = Quantity::from_str("0").unwrap();
    let target = line.quantity.to_decimal();
    for reservation in reservations {
        if released_total.to_decimal() >= target {
            break;
        }
        let remaining = reservation.reserved_quantity.to_decimal();
        if remaining <= Quantity::from_str("0").unwrap().to_decimal() {
            continue;
        }
        if !db
            .stock_reservations()
            .release_quantity(&reservation.base.id, reservation.reserved_quantity, session)
            .await?
        {
            continue;
        }
        // 同步余额预占（释放量从 reserved 转入 available），否则后续
        // deduct_available 会因可用量不足而误拒盘亏/损坏过账。
        if !db
            .stock_balances()
            .release_reserved(balance_id, reservation.reserved_quantity, session)
            .await?
        {
            return Err(Error::BusinessLogicError(
                "库存余额预占与预占记录不一致，无法过账库存调整".to_string(),
            ));
        }
        db.stock_reservation_entries()
            .create(
                &StockReservationEntry::new(
                    StockReservationEntryId::new(next_id()),
                    StockReservationEntryData {
                        reservation_id: reservation.base.id.clone().into(),
                        entry_type: ReservationEntryType::Release,
                        quantity: reservation.reserved_quantity,
                        source_document_id: line.stock_adjustment_id.to_string(),
                    },
                )?,
                session,
            )
            .await?;
        released_total = Quantity::try_from(released_total.to_decimal() + remaining)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    }
    Ok(())
}
