//! 库存调整撤回事务持久化与受阻取消入口。
//!
//! 本模块收 executor 独立执行取消计划落盘、开放任务关闭、通知 outbox 与审计；
//! 入口 [`cancel_approval`] 只做 `with_transaction` 包裹与回放恢复。

use application_core::AuditActor;
use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_audit::{AuditActorLogs, AuditLog};
use erp_core::common::time::Instant;
use erp_core::ids::{ApprovalNotificationOutboxId, StockAdjustmentId};
use erp_identity::SharedRbacService;
use erp_inventory::entity::cancellation::{StockAdjustmentCancellation, StockAdjustmentCancellationData};
use erp_inventory::repository::StockAdjustmentCancellationExt;
use erp_inventory::{InventoryExt, StockAdjustment, StockAdjustmentState};
use erp_workflow::entity::approval_integration::{
    ApprovalNotificationEventKind, ApprovalNotificationOutbox, ApprovalNotificationTemplateParams,
};
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::execution::{PlannedWrites, map_receipt_first_write_error};
use erp_workflow::{ApprovalActionContext, ApprovalIntegrationExt, BpmExt, WorkItemExt};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::adapter::{execute_stock_adjustment_domain_action, require_frozen_binding};
use super::approval_query::load_approval_binding;
use super::cancel_facts::{cancellation_identity, historical_task_facts};
use super::cancel_runtime::{
    CancelAuthority, CancelAuthorization, STOCK_ADJUSTMENT_AUDIT_RESOURCE,
    STOCK_ADJUSTMENT_CANCEL_AUDIT_ACTION, ensure_cancel_authorized_with_executor,
    ensure_cancel_execution_identity, ensure_cancel_instance_binding, ensure_cancel_instance_subject,
    ensure_stock_adjustment_open_task_identity,
};
use crate::audit::persist_log;
use crate::{Error, Result};

/// 库存调整撤回事务写入集合。
pub(crate) struct StockAdjustmentCancelPersistInput {
    pub(crate) rbac: SharedRbacService,
    pub(crate) adjustment: StockAdjustment,
    pub(crate) writes: Box<PlannedWrites>,
    pub(crate) open_tasks: Vec<WorkItem>,
    pub(crate) authorization_instance: ApprovalProcessInstance,
    pub(crate) authorization_execution: ApprovalNodeExecution,
    pub(crate) binding: ApprovalDefinitionBinding,
    pub(crate) actor: AuditActor,
    pub(crate) reason: String,
    pub(crate) current_approver_id: String,
    pub(crate) current_approver_name: String,
    pub(crate) document_no: String,
    pub(crate) now: Instant,
}
/// 在同一事务内应用取消计划、关闭全部开放任务并写回库存调整单。
/// # 参数
/// * `db` - 组合层数据库句柄。
/// * `input` - 已准备的领域变更、审批计划及认证操作人。
/// # 返回
/// 返回全部事实和成功事件共同提交后的调整单。
/// # 错误
/// 事务内授权、版本或终态验证失败，或任一持久化步骤失败。
pub(crate) async fn persist_stock_adjustment_cancel(
    db: &Database,
    input: StockAdjustmentCancelPersistInput,
) -> Result<StockAdjustment> {
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |executor| {
            Box::pin(async move { persist_cancel_step(&db, input, executor).await })
        })
        .await
}

/// 重验原目标并构造不可变取消事实，沿用调用方的唯一事务。
async fn persist_cancel_step(
    db: &Database,
    input: StockAdjustmentCancelPersistInput,
    executor: &mut dyn Executor,
) -> Result<StockAdjustment> {
    let authorization = revalidate_cancel_source(db, &input, executor).await?;
    let open_tasks = revalidate_cancel_open_tasks(
        db,
        &input.authorization_instance,
        &input.authorization_execution,
        &input.open_tasks,
        &authorization,
        executor,
    )
    .await?;
    let closed_tasks = WorkItem::close_all_for_approval_cancellation(
        open_tasks,
        input.actor.id(),
        &input.reason,
        input.now,
    )?;
    let audit = cancel_audit(&input, &authorization)?;
    let fact = prepare_cancellation_fact(db, &input, &closed_tasks, &audit.base.id, executor).await?;
    apply_cancel_writes(db, input, authorization, closed_tasks, fact, audit, executor).await
}

/// 决定写入的单据与实例读取必须仍处于同一事务快照。
async fn revalidate_cancel_source(
    db: &Database,
    input: &StockAdjustmentCancelPersistInput,
    executor: &mut dyn Executor,
) -> Result<CancelAuthorization> {
    let adjustment = db
        .inventory()
        .stock_adjustment(&input.adjustment.base.id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("库存调整单不存在".into()))?;
    if adjustment.base.version != input.adjustment.base.version
        || adjustment.approval_subject_version != input.authorization_instance.subject_version
        || adjustment.status != StockAdjustmentState::InApproval
    {
        return Err(Error::ConflictError("库存调整单事务内版本或审批状态已变化".into()));
    }
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(&input.authorization_instance.base.id), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批实例不存在".into()))?;
    ensure_cancel_instance_subject(
        &instance,
        &input.adjustment.base.id,
        input.authorization_instance.subject_version,
    )?;
    let binding = load_approval_binding(db, &input.adjustment.base.id, executor).await?;
    let binding = require_frozen_binding(binding.as_ref())?;
    ensure_cancel_instance_binding(&instance, binding)?;
    if binding != &input.binding || instance != input.authorization_instance {
        return Err(Error::ConflictError("库存调整撤回事务内运行事实已变化".into()));
    }
    ensure_cancel_authorized_with_executor(db, &input.rbac, &instance, &input.actor, executor).await
}

/// 从原命令和全部历史任务形成库存领域拥有的事实。
async fn prepare_cancellation_fact(
    db: &Database,
    input: &StockAdjustmentCancelPersistInput,
    closed_tasks: &[WorkItem],
    audit_event_id: &str,
    executor: &mut dyn Executor,
) -> Result<StockAdjustmentCancellation> {
    let historical_tasks = db
        .work_items()
        .approval_tasks_for_execution(
            &ApprovalNodeExecutionId::new(&input.authorization_execution.base.id),
            executor,
        )
        .await?;
    Ok(StockAdjustmentCancellation::new(StockAdjustmentCancellationData {
        schema_version: 1,
        audit_event_id: audit_event_id.to_string(),
        stock_adjustment_id: input.adjustment.base.id.clone(),
        instance_id: input.authorization_instance.base.id.clone(),
        execution_id: input.authorization_execution.base.id.clone(),
        subject_version: input.authorization_instance.subject_version,
        actor_id: input.actor.id().to_string(),
        reason: input.reason.clone(),
        command: cancellation_identity(&input.writes.receipt),
        document_version: input.adjustment.base.version,
        instance_version: input.authorization_instance.base.version,
        execution_version: input.authorization_execution.base.version,
        task_id: closed_tasks.first().map(|task| task.base.id.clone()),
        task_version: closed_tasks.first().map(|task| task.base.version),
        cancelled_at: input.now,
        historical_tasks: historical_task_facts(&historical_tasks)?,
        blocker_code: input.authorization_execution.blocker_code.map(|code| code.as_str().to_string()),
    })?)
}

/// 保持审批回执为首笔物理写；业务、事实、通知和审计缺一即整体失败。
async fn apply_cancel_writes(
    db: &Database,
    mut input: StockAdjustmentCancelPersistInput,
    authorization: CancelAuthorization,
    closed_tasks: Vec<WorkItem>,
    fact: StockAdjustmentCancellation,
    audit: AuditLog,
    executor: &mut dyn Executor,
) -> Result<StockAdjustment> {
    db.bpm_workflow()
        .insert_command_receipt(&input.writes.receipt, executor)
        .await
        .map_err(map_receipt_first_write_error)?;
    db.stock_adjustments().update(&mut input.adjustment, executor).await?;
    db.bpm_workflow()
        .persist_cancelled_runtime_after_receipt(
            &input.writes.instance,
            &input.writes.updated_executions,
            executor,
        )
        .await?;
    db.work_items().persist_cancelled_approval_tasks(&closed_tasks, executor).await?;
    db.stock_adjustment_cancellations().create(&fact, executor).await?;
    persist_stock_adjustment_cancel_notifications(
        db,
        StockAdjustmentCancelNotificationInput {
            writes: &input.writes,
            authorization: &authorization,
            actor_id: input.actor.id(),
            current_approver_id: &input.current_approver_id,
            current_approver_name: &input.current_approver_name,
            document_no: &input.document_no,
            now: input.now,
        },
        executor,
    )
    .await?;
    persist_log(db, &audit, executor).await?;
    Ok(input.adjustment)
}

/// 取消成功事件仅记录安全的中文说明；事件 ID 由业务事实保存关联。
fn cancel_audit(
    input: &StockAdjustmentCancelPersistInput,
    authorization: &CancelAuthorization,
) -> Result<AuditLog> {
    input
        .actor
        .clone()
        .resource_log_with_message(
            STOCK_ADJUSTMENT_CANCEL_AUDIT_ACTION,
            STOCK_ADJUSTMENT_AUDIT_RESOURCE,
            input.adjustment.base.id.clone(),
            Some(format!("撤回审批：{}；撤回身份：{}", input.reason, authorization.authority.label())),
        )
        .map_err(Into::into)
}

/// 在同一事务快照内重读并校验普通撤回需要关闭的开放任务。
///
/// # 参数
/// * `db` - 执行与任务所在数据库。
/// * `instance` - 待撤回的审批实例。
/// * `expected_execution` - 事务外加载的当前执行。
/// * `expected_tasks` - 事务外加载的开放任务。
/// * `authorization` - 已证明的撤回授权，提供责任组织。
/// * `executor` - 当前事务执行器。
///
/// # 返回
/// 返回事务内重读且与预期身份、版本一致的开放任务。
///
/// # 错误
/// 取消策略、当前执行、任务数量或任务身份变化，或适配器登记不完整时返回冲突或下层错误。仓储读取失败时返回对应错误。
pub(crate) async fn revalidate_cancel_open_tasks(
    db: &Database,
    instance: &ApprovalProcessInstance,
    expected_execution: &ApprovalNodeExecution,
    expected_tasks: &[WorkItem],
    authorization: &CancelAuthorization,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItem>> {
    let policy =
        instance.cancellation_task_policy().map_err(|error| Error::ConflictError(error.to_string()))?;
    let execution_id = instance
        .current_node_execution_id
        .as_ref()
        .ok_or_else(|| Error::ConflictError("审批实例缺少当前执行".to_string()))?;
    let current_execution = db
        .bpm_workflow()
        .find_execution_by_id(execution_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批执行不存在".to_string()))?;
    if &current_execution != expected_execution {
        return Err(Error::ConflictError("库存调整审批执行已变化".to_string()));
    }
    ensure_cancel_execution_identity(instance, &current_execution, policy)?;
    let tasks = db.work_items().open_approval_tasks_for_execution(execution_id, executor).await?;
    policy.ensure_open_task_count(tasks.len()).map_err(|error| Error::ConflictError(error.to_string()))?;
    if tasks.len() != expected_tasks.len()
        || tasks.iter().zip(expected_tasks).any(|(current, expected)| {
            current.base.id != expected.base.id || current.base.version != expected.base.version
        })
    {
        return Err(Error::ConflictError("库存调整开放审批任务已变化".to_string()));
    }
    for task in &tasks {
        ensure_stock_adjustment_open_task_identity(
            task,
            instance,
            &current_execution,
            &authorization.responsible_org_id,
        )?;
    }
    Ok(tasks)
}

/// 库存调整普通撤回通知持久化入参。
pub(crate) struct StockAdjustmentCancelNotificationInput<'a> {
    /// 取消编排写出的计划写入集。
    writes: &'a PlannedWrites,
    /// 事务外冻结的撤回授权快照。
    authorization: &'a CancelAuthorization,
    /// 撤回操作人账号 ID。
    actor_id: &'a str,
    /// 当前节点审批人账号 ID。
    pub(crate) current_approver_id: &'a str,
    /// 当前节点审批人展示名。
    pub(crate) current_approver_name: &'a str,
    /// 调整单单据号。
    pub(crate) document_no: &'a str,
    /// 入队时间。
    now: Instant,
}

/// 在普通撤回事务内追加唯一通知 outbox。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `input` - 取消通知意图、授权与收件人上下文
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 唯一取消通知写入 outbox 时返回 `Ok(())`。
///
/// # 错误
/// 通知意图不唯一或与持久化合同不一致，或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 普通撤回必须产生唯一 `Cancelled` 通知；运行时管理员撤回时操作人也在收件人内。
pub(crate) async fn persist_stock_adjustment_cancel_notifications(
    db: &Database,
    input: StockAdjustmentCancelNotificationInput<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let [intent] = input.writes.notifications.as_slice() else {
        return Err(Error::Internal("库存调整普通撤回必须产生唯一取消通知意图".to_string()));
    };
    let mut recipients =
        vec![input.current_approver_id.to_string(), input.authorization.submitted_by.clone()];
    if input.authorization.authority == CancelAuthority::RuntimeAdmin {
        recipients.push(input.actor_id.to_string());
    }
    recipients.sort();
    recipients.dedup();
    let expected_dedup_key =
        format!("cancelled:{}:{}", input.writes.instance.base.id, input.writes.instance.current_round_no);
    if intent.event_kind != ApprovalNotificationEventKind::Cancelled || intent.dedup_key != expected_dedup_key
    {
        return Err(Error::Internal("库存调整普通撤回通知意图与持久化合同不一致".to_string()));
    }
    let record = ApprovalNotificationOutbox::enqueue(
        ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
        intent.dedup_key.clone(),
        intent.event_kind,
        recipients,
        ApprovalNotificationTemplateParams {
            document_type_label: DocumentType::StockAdjustment.label().to_string(),
            document_no: input.document_no.to_string(),
            current_node_name: input
                .writes
                .updated_executions
                .first()
                .map(|execution| execution.node_name.clone())
                .unwrap_or_default(),
            current_approver_display_name: input.current_approver_name.to_string(),
            round_no: input.writes.instance.current_round_no,
            reject_reason_summary: None,
        },
        input.now,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.approval_notification_outbox().create(&record, executor).await?;
    Ok(())
}

/// 在审批运行时持有的事务内撤回库存调整审批。
///
/// # 参数
/// * `db` - 调整单与运行事实所在数据库。
/// * `context` - 审批运行时冻结的受阻取消上下文。
/// * `action` - 合同强类型领域动作。
/// * `actor` - 已认证的运行时操作人。
/// * `executor` - 审批运行时持有的事务执行器。
///
/// # 返回
/// 调整单已撤回并写入审计时无返回值。
///
/// # 错误
/// 缺少事务会话、上下文或受阻运行事实不匹配、调整单不存在、主题版本变化、动作不允许，或单据与审计写入失败时返回对应错误。
pub async fn cancel_stock_adjustment_approval_apply(
    db: &Database,
    context: &ApprovalActionContext,
    action: erp_workflow::service::approval::policy::ApprovalDomainAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = executor
        .session()
        .ok_or_else(|| Error::Internal("库存调整受阻取消缺少运行时事务会话".to_string()))?;
    if context.business_object_type() != DocumentType::StockAdjustment.as_str()
        || context.actor_id() != actor.id()
        || context.work_item_id().is_some()
    {
        return Err(Error::ConflictError("库存调整受阻取消上下文不匹配".to_string()));
    }
    let subject_version = context
        .subject_version()
        .parse::<u32>()
        .map_err(|_| Error::ConflictError("库存调整审批主题版本无效".to_string()))?;
    let adjustment_id = StockAdjustmentId::new(context.business_object_id());
    let mut adjustment = db
        .inventory()
        .stock_adjustment(adjustment_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("库存调整单不存在".to_string()))?;
    if adjustment.approval_subject_version != subject_version {
        return Err(Error::ConflictError("库存调整审批主题版本已变化".to_string()));
    }
    validate_blocked_cancel_runtime_context(db, context, actor, subject_version, executor).await?;
    execute_stock_adjustment_domain_action(&mut adjustment, action)?;
    db.stock_adjustments().update(&mut adjustment, executor).await?;
    let audit = actor.clone().resource_log(
        "stock_adjustment.cancel_approval",
        "stock_adjustment",
        adjustment_id.to_string(),
    )?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

/// 校验受阻取消动作仍精确指向同一 BLOCKED 实例、执行和无任务事实。
///
/// # 参数
/// * `db` - 实例、绑定、快照与任务所在数据库。
/// * `context` - 审批运行时冻结的受阻取消上下文。
/// * `actor` - 已认证的运行时操作人。
/// * `subject_version` - 已从上下文解析的主题版本。
/// * `executor` - 当前事务执行器。
///
/// # 返回
/// 实例、执行、绑定、快照一致且没有开放任务时无返回值。
///
/// # 错误
/// 执行缺失、实例或执行不存在、绑定或快照不一致、状态不是阻塞、操作人不一致，或仍有开放任务时返回 `ConflictError`。仓储读取失败时返回对应错误。
pub(crate) async fn validate_blocked_cancel_runtime_context(
    db: &Database,
    context: &ApprovalActionContext,
    actor: &AuditActor,
    subject_version: u32,
    executor: &mut dyn Executor,
) -> Result<()> {
    let execution_id = context
        .approval_node_execution_id()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::ConflictError("库存调整受阻取消缺少审批执行".to_string()))?;
    let instance_id = bpm::ids::ApprovalProcessInstanceId::new(context.approval_process_instance_id());
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&instance_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整审批实例不存在".to_string()))?;
    let binding = load_approval_binding(db, context.business_object_id(), executor).await?;
    let binding = require_frozen_binding(binding.as_ref())?;
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整受阻实例缺少冻结快照".to_string()))?;
    snapshot
        .ensure_matches_runtime_subject(
            DocumentType::StockAdjustment,
            context.business_object_id(),
            subject_version,
        )
        .map_err(|_| Error::ConflictError("库存调整受阻实例冻结快照已变化".to_string()))?;
    if instance.status != bpm::model::types::ApprovalProcessInstanceStatus::Blocked
        || instance.process_kind
            != erp_workflow::service::approval::process_kind::process_kind_of(DocumentType::StockAdjustment)
        || instance.process_definition_id != binding.approval_process_definition_id
        || instance.definition_version != binding.approval_definition_version
        || instance.subject.subject_kind() != DocumentType::StockAdjustment.as_str()
        || instance.subject.subject_id() != context.business_object_id()
        || instance.subject_version != subject_version
        || instance.current_node_execution_id.as_ref().map(AsRef::as_ref) != Some(execution_id)
    {
        return Err(Error::ConflictError("库存调整受阻实例上下文已变化".to_string()));
    }
    let execution_id = bpm::ids::ApprovalNodeExecutionId::new(execution_id);
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&execution_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("库存调整受阻执行不存在".to_string()))?;
    if execution.process_instance_id != instance_id
        || execution.status != bpm::model::types::ApprovalNodeExecutionStatus::Blocked
        || execution.round_no != instance.current_round_no
        || execution.blocker_code.is_none()
        || execution.blocker_code != instance.blocker_code
        || context.actor_id() != actor.id()
    {
        return Err(Error::ConflictError("库存调整受阻执行上下文已变化".to_string()));
    }
    if !db.work_items().open_approval_tasks_for_execution(&execution_id, executor).await?.is_empty() {
        return Err(Error::ConflictError("受阻库存调整执行不得存在开放审批任务".to_string()));
    }
    Ok(())
}
