//! 审批决定提交、回放与事务写入。

mod decision_persist;
mod decision_prepare;

use std::sync::Arc;

use application_core::AuditActor;
use bpm::engine::{CommitRequired, TaskCloseReason};
use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{
    ApprovalDecision, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus, ModelError,
};
use bpm::model::{ApprovalCommandReceipt, ApprovalNodeExecution, IdempotencyKey, ParticipantId};
use decision_persist::{PersistDecisionWrites, persist_decision_writes};
use decision_prepare::{
    OpenDecisionRuntime, PreparedOpenDecision, load_open_decision_runtime, prepare_open_decision,
};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::authorization::hidden_forbidden;
use super::super::idempotency::{
    ReceiptBranch, decision_identity, map_receipt_first_write_error, normalize_idempotency_key,
    payload_conflict_error,
};
use super::super::view::{ApprovalCommandView, map_command_view};
use super::query::first_open_task;
use super::read_auth::{
    RevalidateDecisionApproverInput, process_required_separation_policy, revalidate_decision_approver,
};
use super::{
    ApprovalRuntimeService, commit_or_recover, find_receipt_for_identity, hidden_not_found,
    persisted_command_view_with_executor, recover_by_replay,
};
use crate::entity::work_item::{ApprovalDecisionTaskError, WorkItem, WorkItemStatus};
use crate::error::{Error, ErrorCode, Result};
use crate::repository::prelude::*;
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use crate::service::approval::ApprovalDomainActionPort;
use crate::service::approval::business_adapter::adapter_spec_of;

/// 已规范化的审批决定命令；协议字段保持不变，摘要在进入事务前固定。
#[derive(Debug, Clone)]
pub(super) struct RuntimeDecisionCommand {
    pub(super) work_item_id: String,
    pub(super) decision: ApprovalDecision,
    pub(super) reason: Option<String>,
    pub(super) expected_task_version: u64,
    pub(super) idempotency_key: IdempotencyKey,
}

/// Fresh 决定前置只允许开放任务继续；已终结任务才可能进入收据回放。
#[derive(Debug)]
pub(super) enum DecisionReceiptLookup {
    Fresh,
    Terminal(ApprovalNodeExecutionId),
}

/// 决定事务的提交结果；受阻事实提交后由外层转换为稳定 409。
struct RuntimeDecisionOutcome {
    view: ApprovalCommandView,
    blocked: bool,
}

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 提交当前开放任务的通过或驳回。
    ///
    /// 加载任务、执行、实例与定义图，重验三方责任与写时资格，调用
    /// `prepare_decision` 规划，并在一个 MongoDB 事务中应用：最终通过先登记
    /// 领域动作（单据生效），再写实例推进（CAS）、执行结束/插入、审批人绑定、
    /// 命令收据、任务完成/关闭/新建、通知 outbox 与审计。
    ///
    /// # 参数
    /// * `actor` - 当前决定人
    /// * `work_item_id` - 当前开放单据审批任务 ID
    /// * `decision` - `APPROVE` 或 `REJECT`
    /// * `reason` - 可选决定原因；空白按未提供处理
    /// * `expected_task_version` - 调用方持有的任务版本
    /// * `idempotency_key` - 本次决定命令幂等键
    ///
    /// # 返回
    /// 返回回放或实际应用后的审批命令视图。
    ///
    /// # 错误
    /// 任务不存在、责任不一致、版本冲突或仓储失败时返回错误。
    pub async fn submit_decision(
        &self,
        actor: &AuditActor,
        work_item_id: &str,
        decision: &str,
        reason: Option<&str>,
        expected_task_version: u64,
        idempotency_key: &str,
    ) -> Result<ApprovalCommandView> {
        let decision = match decision {
            "APPROVE" => ApprovalDecision::Approve,
            "REJECT" => ApprovalDecision::Reject,
            other => {
                return Err(Error::ValidationError(format!("决定必须是 APPROVE 或 REJECT，收到 {other}")));
            },
        };
        let reason = reason.map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned);
        let key = normalize_idempotency_key(idempotency_key)?;
        let command = RuntimeDecisionCommand {
            work_item_id: work_item_id.to_string(),
            decision,
            reason: reason.clone(),
            expected_task_version,
            idempotency_key: key,
        };
        let outcome = commit_or_recover(
            || self.commit_decision(actor, command.clone()),
            |error| self.recover_decision_after_competing_commit(actor, command.clone(), error),
        )
        .await?;
        if outcome.blocked {
            return Err(Error::from_approval_code(ErrorCode::ApprovalInstanceBlocked));
        }
        Ok(outcome.view)
    }

    /// 在唯一 MongoDB 事务中先查收据，再执行决定前置、授权与全部写入。
    async fn commit_decision(
        &self,
        actor: &AuditActor,
        command: RuntimeDecisionCommand,
    ) -> Result<RuntimeDecisionOutcome> {
        let db = self.db.clone();
        let rbac = self.auth.clone();
        let action_port = Arc::clone(&self.action_port);
        let object_read = Arc::clone(&self.object_read);
        let audit_port = Arc::clone(&self.audit);
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    submit_decision_apply(
                        &db,
                        &rbac,
                        action_port.as_ref(),
                        object_read.as_ref(),
                        audit_port.as_ref(),
                        &actor,
                        &command,
                        executor,
                    )
                    .await
                })
            })
            .await
    }

    /// 并发唯一键或提交结果未知后，必须使用新的事务会话只读胜者收据。
    async fn recover_decision_after_competing_commit(
        &self,
        actor: &AuditActor,
        command: RuntimeDecisionCommand,
        original_error: Error,
    ) -> Result<RuntimeDecisionOutcome> {
        recover_by_replay(original_error, || async {
            let db = self.db.clone();
            let rbac = self.auth.clone();
            let object_read = Arc::clone(&self.object_read);
            let actor = actor.clone();
            let command = command.clone();
            self.db
                .client()
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        replay_decision(&db, &rbac, object_read.as_ref(), &actor, &command, executor).await
                    })
                })
                .await
        })
        .await
    }
}

/// 执行与 Fresh 相同的任务前置；只有 `NotOpen` 才可继续证明终态回放。
pub(super) fn decision_receipt_lookup_gate(
    item: &WorkItem,
    actor_id: &str,
    expected_task_version: u64,
) -> Result<DecisionReceiptLookup> {
    match item.approval_execution_for_decision(actor_id, expected_task_version) {
        Ok(_) => Ok(DecisionReceiptLookup::Fresh),
        Err(ApprovalDecisionTaskError::NotOpen) => item
            .approval_node_execution_id
            .clone()
            .map(DecisionReceiptLookup::Terminal)
            .ok_or_else(|| Error::from_approval_code(ErrorCode::ApprovalTaskNotOpen)),
        Err(error) => Err(map_approval_task_error(error)),
    }
}

/// 终态任务的 Fresh 语义固定为稳定 `APPROVAL_TASK_NOT_OPEN`，不得暴露收据或授权差异。
pub(super) fn decision_terminal_fresh_error() -> Error {
    Error::from_approval_code(ErrorCode::ApprovalTaskNotOpen)
}

/// 决定回放先执行与 Fresh 相同的任务前置和当前授权，再允许查询和比较收据。
async fn replay_decision(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn crate::ports::ApprovalObjectReadPort,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    executor: &mut dyn Executor,
) -> Result<Option<RuntimeDecisionOutcome>> {
    let item = db
        .work_items()
        .find_document_approval_by_id(&command.work_item_id, executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    let execution_id = match decision_receipt_lookup_gate(&item, actor.id(), command.expected_task_version)? {
        DecisionReceiptLookup::Fresh => return Ok(None),
        DecisionReceiptLookup::Terminal(execution_id) => execution_id,
    };
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&execution_id, executor)
        .await?
        .ok_or_else(decision_terminal_fresh_error)?;
    let Some(original_actor_id) = decision_terminal_actor(&item, &execution) else {
        return Err(decision_terminal_fresh_error());
    };
    if original_actor_id != actor.id() {
        return Err(decision_terminal_fresh_error());
    }
    match authorize_decision_terminal_replay(db, rbac, object_read, actor, &execution, executor).await {
        Ok(()) => {},
        Err(Error::Forbidden(_)) => {
            return Err(decision_terminal_fresh_error());
        },
        Err(error) => return Err(error),
    }
    replay_terminal_receipt_view(db, actor, command, &item, &execution_id, executor).await
}

async fn replay_terminal_receipt_view(
    db: &Database,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    item: &WorkItem,
    execution_id: &ApprovalNodeExecutionId,
    executor: &mut dyn Executor,
) -> Result<Option<RuntimeDecisionOutcome>> {
    let identity = decision_identity(
        command.idempotency_key.clone(),
        execution_id.as_ref(),
        &command.work_item_id,
        command.decision.as_str(),
        command.reason.as_deref(),
        command.expected_task_version,
        actor.id(),
    )?;
    let Some(receipt) = find_receipt_for_identity(db, &identity, executor).await? else {
        return Err(decision_terminal_fresh_error());
    };
    // 历史无版本摘要可能存在分隔符碰撞，必须先证明收据仍指向该任务的冻结运行身份。
    let ended_execution =
        verify_decision_receipt_runtime_identity(db, &receipt, execution_id, executor).await?;
    let is_current_v3 = receipt.scope_id == identity.current().scope().as_str();
    if !is_current_v3 && !legacy_decision_terminal_facts_match(item, &ended_execution, command, actor.id()) {
        return Err(payload_conflict_error());
    }
    match identity.classify(Some(&receipt)) {
        ReceiptBranch::SamePayload(_) => {},
        ReceiptBranch::Fresh => unreachable!("receipt was loaded"),
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error()),
    }
    let view = persisted_command_view_with_executor(
        db,
        &receipt.result_ref,
        CommitRequired::Proceed,
        true,
        executor,
    )
    .await?;
    Ok(Some(RuntimeDecisionOutcome {
        blocked: view.instance_status == ApprovalProcessInstanceStatus::Blocked.as_str(),
        view,
    }))
}

/// 在同一事务内执行决定的完整前置、授权、领域动作与运行时写入。
// 决定事务 8 参数：与受阻取消/恢复入口同形以便统一分派，顺序由调用点锚定；告警逐项压制。
#[allow(clippy::too_many_arguments)]
async fn submit_decision_apply(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    action_port: &dyn ApprovalDomainActionPort,
    object_read: &dyn crate::ports::ApprovalObjectReadPort,
    audit_port: &dyn crate::ports::WorkflowAuditPort,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    executor: &mut dyn Executor,
) -> Result<RuntimeDecisionOutcome> {
    if let Some(replay) = replay_decision(db, rbac, object_read, actor, command, executor).await? {
        return Ok(replay);
    }
    let loaded = load_open_decision_runtime(db, actor, command, executor).await?;
    let prepared = prepare_open_decision(db, rbac, object_read, actor, command, &loaded, executor).await?;

    // 收据唯一键先于任何领域写入仲裁同键并发；后续失败会随事务整体回滚。
    db.bpm_workflow()
        .insert_command_receipt(&prepared.writes.receipt, executor)
        .await
        .map_err(map_receipt_first_write_error)?;
    if prepared.should_finalize {
        action_port.execute(loaded.spec.on_final_approve, &prepared.action_context, actor, executor).await?;
    }
    persist_decision_writes(db, persist_decision_input(&loaded, &prepared, command, audit_port), executor)
        .await?;
    let blocked = prepared.writes.commit == CommitRequired::Blocked;
    let view = map_command_view(
        &prepared.writes.instance,
        prepared.writes.created_executions.last(),
        command.reason.clone(),
        None,
        first_open_task(&prepared.writes, &prepared.new_task_ids),
        prepared.writes.commit,
        false,
    );
    Ok(RuntimeDecisionOutcome { view, blocked })
}

fn persist_decision_input<'a>(
    loaded: &'a OpenDecisionRuntime,
    prepared: &'a PreparedOpenDecision,
    command: &'a RuntimeDecisionCommand,
    audit_port: &'a dyn crate::ports::WorkflowAuditPort,
) -> PersistDecisionWrites<'a> {
    PersistDecisionWrites {
        writes: &prepared.writes,
        ended_execution_id: loaded.execution_id.as_ref(),
        expected_instance_version: loaded.expected_instance_version,
        expected_execution_version: loaded.expected_execution_version,
        expected_task_version: command.expected_task_version,
        work_item_id: &command.work_item_id,
        new_task_ids: &prepared.new_task_ids,
        list_projection: &prepared.list_projection,
        audit: &prepared.audit,
        audit_port,
        now: prepared.now,
        actor_id: &prepared.actor_id,
        owner_role: &prepared.owner_role,
        owner_organization_id: &prepared.owner_organization_id,
        subject_version: &prepared.subject_version,
        business_object_id: &prepared.business_object_id,
        document_type_label: &prepared.document_type_label,
        document_no: &loaded.snapshot.payload.document_no,
        submitted_by: &loaded.snapshot.payload.submitted_by,
        ended_execution: &loaded.execution,
        reject_reason: command.reason.as_deref(),
        runtime_admin_ids: &prepared.runtime_admin_ids,
    }
}

/// 在 legacy 摘要双读前验证收据、任务执行、实例、流程类型与冻结主体属于同一运行身份。
async fn verify_decision_receipt_runtime_identity(
    db: &Database,
    receipt: &ApprovalCommandReceipt,
    expected_execution_id: &ApprovalNodeExecutionId,
    executor: &mut dyn Executor,
) -> Result<ApprovalNodeExecution> {
    if receipt.scope_id != expected_execution_id.as_ref() {
        return Err(hidden_not_found());
    }
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(expected_execution_id, executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(&receipt.result_ref), executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    if execution.process_instance_id.as_ref() != receipt.result_ref || instance.base.id != receipt.result_ref
    {
        return Err(hidden_not_found());
    }
    let document_type = crate::entity::approval_integration::resolve_runtime_document_type(
        instance.subject.subject_kind(),
        instance.process_kind,
    )
    .map_err(|_| hidden_not_found())?;
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    snapshot
        .ensure_matches_runtime_subject(
            document_type,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| hidden_not_found())?;
    Ok(execution)
}

/// 从任务不可逆终态与执行事实提取原决定人；不得把当前 owner 投影作为回放授权。
pub(super) fn decision_terminal_actor<'a>(
    item: &'a WorkItem,
    execution: &'a ApprovalNodeExecution,
) -> Option<&'a str> {
    if item.approval_node_execution_id.as_ref()
        != Some(&ApprovalNodeExecutionId::new(execution.base.id.clone()))
    {
        return None;
    }
    match item.status {
        WorkItemStatus::Completed => {
            let completed_by = item.completed_by.as_deref()?;
            let decided_by = execution.decided_by.as_ref()?.as_str();
            let terminal_decision_matches = matches!(
                (execution.status, execution.decision),
                (ApprovalNodeExecutionStatus::Approved, Some(ApprovalDecision::Approve))
                    | (ApprovalNodeExecutionStatus::Rejected, Some(ApprovalDecision::Reject))
            );
            (terminal_decision_matches
                && completed_by == decided_by
                && execution.decided_at.is_some()
                && execution.ended_at.is_some())
            .then_some(completed_by)
        },
        WorkItemStatus::Closed => {
            let closed_by = item.closed_by.as_deref()?;
            (item.close_reason.as_deref() == Some(TaskCloseReason::ApprovalRuntimeBlocked.as_str())
                && execution.status == ApprovalNodeExecutionStatus::Blocked
                && execution.blocker_code.is_some()
                && execution.blocked_at.is_some()
                && execution.ended_at.is_none()
                && execution.assignee_participant_id.as_str() == closed_by)
                .then_some(closed_by)
        },
        WorkItemStatus::Open => None,
    }
}

/// legacy 决定摘要只有在不可变终态执行与任务完整证明原命令时才允许回放。
pub(super) fn legacy_decision_terminal_facts_match(
    item: &WorkItem,
    execution: &ApprovalNodeExecution,
    command: &RuntimeDecisionCommand,
    actor_id: &str,
) -> bool {
    let Some(persisted_task_version) = command.expected_task_version.checked_add(1) else {
        return false;
    };
    let expected_execution_status = match command.decision {
        ApprovalDecision::Approve => ApprovalNodeExecutionStatus::Approved,
        ApprovalDecision::Reject => ApprovalNodeExecutionStatus::Rejected,
    };
    item.base.id == command.work_item_id
        && item.approval_node_execution_id.as_ref()
            == Some(&ApprovalNodeExecutionId::new(execution.base.id.clone()))
        && item.status == WorkItemStatus::Completed
        && item.completed_by.as_deref() == Some(actor_id)
        && item.base.version == persisted_task_version
        && execution.status == expected_execution_status
        && execution.decision == Some(command.decision)
        && execution.decision_reason.as_deref() == command.reason.as_deref()
        && execution.decided_by.as_ref().map(ParticipantId::as_str) == Some(actor_id)
        && execution.decided_at.is_some()
        && execution.ended_at.is_some()
}

/// 回放只使用冻结运行事实证明原决定人当前仍具备动作权限，不信任收据或 WorkItem owner 投影。
async fn authorize_decision_terminal_replay(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn crate::ports::ApprovalObjectReadPort,
    actor: &AuditActor,
    execution: &ApprovalNodeExecution,
    executor: &mut dyn Executor,
) -> Result<()> {
    let (snapshot, spec, assignee, document_type) =
        load_decision_terminal_replay_facts(db, execution, executor).await?;
    if execution.assignee_participant_id.as_str() != actor.id()
        || assignee.current_assignee_participant_id.as_str() != actor.id()
    {
        return Err(hidden_forbidden());
    }
    let eligibility = revalidate_decision_approver(
        db,
        rbac,
        object_read,
        RevalidateDecisionApproverInput {
            assignee_id: actor.id(),
            assignee_name: &execution.assignee_name_snapshot,
            authenticated_actor: Some(actor),
            snapshot: &snapshot,
            spec: &spec,
            separation_policy: process_required_separation_policy(document_type)?,
        },
        executor,
    )
    .await?;
    if eligibility.blocked_code().is_some() {
        return Err(hidden_forbidden());
    }
    Ok(())
}

async fn load_decision_terminal_replay_facts(
    db: &Database,
    execution: &ApprovalNodeExecution,
    executor: &mut dyn Executor,
) -> Result<(
    crate::entity::approval_integration::ApprovalSubjectSnapshot,
    crate::service::approval::business_adapter::ApprovalAdapterSpec,
    bpm::model::ApprovalInstanceAssignee,
    crate::entity::document_registry::DocumentType,
)> {
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&execution.process_instance_id, executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    if execution.process_instance_id.as_ref() != instance.base.id {
        return Err(hidden_not_found());
    }
    let document_type = crate::entity::approval_integration::resolve_runtime_document_type(
        instance.subject.subject_kind(),
        instance.process_kind,
    )
    .map_err(|_| hidden_not_found())?;
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    snapshot
        .ensure_matches_runtime_subject(
            document_type,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| hidden_not_found())?;
    let assignee = db
        .bpm_workflow()
        .find_assignee_for_node(
            &ApprovalProcessInstanceId::new(&instance.base.id),
            &execution.node_key,
            executor,
        )
        .await?
        .ok_or_else(hidden_not_found)?;
    Ok((snapshot, adapter_spec_of(document_type)?, assignee, document_type))
}

/// 将单据审批任务前置校验映射为稳定的 Service 错误。
///
/// # 参数
/// * `error` - WorkItem 返回的决定前置失败原因
///
/// # 返回
/// 返回权限拒绝或稳定冲突错误。
///
/// # 错误
/// 无；本方法只执行错误分类转换。
///
/// # 关键业务约束
/// 非当前责任人必须保持禁止语义，任务状态、版本和执行引用失败必须保持冲突语义。
pub(super) fn map_approval_task_error(error: ApprovalDecisionTaskError) -> Error {
    match error {
        ApprovalDecisionTaskError::NotCurrentOwner => Error::Forbidden("无权执行该审批动作".to_string()),
        ApprovalDecisionTaskError::VersionConflict => Error::version_conflict("任务"),
        ApprovalDecisionTaskError::NotDocumentApproval
        | ApprovalDecisionTaskError::NotOpen
        | ApprovalDecisionTaskError::MissingExecution => {
            Error::from_approval_code(ErrorCode::ApprovalTaskNotOpen)
        },
    }
}

/// 将 BPM 决定连线错误映射为运行时冲突。
///
/// # 参数
/// * `error` - BPM 图模型返回的决定连线错误
///
/// # 返回
/// 返回失败关闭的运行时冲突错误。
///
/// # 错误
/// 无；本方法只执行错误分类转换。
///
/// # 关键业务约束
/// 持久化图缺失或重复决定连线时不得继续推导下一审批人。
pub(super) fn map_runtime_graph_error(error: ModelError) -> Error {
    Error::ConflictError(error.to_string())
}
