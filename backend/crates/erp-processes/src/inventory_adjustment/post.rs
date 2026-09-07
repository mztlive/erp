use erp_audit::AuditExt;
use erp_core::ids::StockAdjustmentId;
use erp_inventory::InventoryExt;
use erp_inventory::StockAdjustment;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::{AssignmentSource, WorkItemStatus, WorkItemType};
use erp_workflow::ApprovalIntegrationExt;
use erp_workflow::BpmExt;
use erp_workflow::WorkItemExt;
use mongodb::Database;
use persistence_core::Executor;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_workflow::ApprovalActionContext;
use services::{Error, Result};

use super::adapter::{require_frozen_binding, stock_adjustment_adapter};
use super::approval_query::load_approval_binding;
use super::InventoryAdjustmentService;
use erp_inventory::StockAdjustmentView;

impl InventoryAdjustmentService {
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
    erp_inventory::apply_posted_adjustment_in_transaction(db, &mut adjustment, &lines, actor, session)
        .await?;
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
