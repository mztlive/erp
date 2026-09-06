//! 审批决定提交、回放与事务写入。

use std::sync::Arc;

use bpm::engine::{CommitRequired, TaskCloseReason};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{
    ApprovalDecision, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus, ModelError,
};
use bpm::model::{ApprovalCommandReceipt, ApprovalNodeExecution, IdempotencyKey, ParticipantId, Timestamp};
use database::repository::bpm::ApprovalInstanceListProjection;
use database::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use entities::work_item::{ApprovalDecisionTaskError, WorkItem, WorkItemStatus};
use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Transactional;

use super::super::apply_plan::PlannedWrites;
use super::super::authorization::hidden_forbidden;
use super::super::decision::prepare_decision;
use super::super::idempotency::{
    command_may_have_committed, command_recovery_delay, decision_identity, map_receipt_first_write_error,
    normalize_idempotency_key, payload_conflict_error, ReceiptBranch,
};
use super::super::view::{map_command_view, ApprovalCommandView};
use super::super::{DecisionExecutionInput, ExecutionCommandInput, PreparedExecution};
use super::notifications::{
    persist_decision_notifications, runtime_admin_notification_recipients, DecisionNotificationFacts,
};
use super::query::{first_open_task, list_projection_from_writes};
use super::read_auth::{
    process_required_separation_policy, revalidate_decision_approver, task_proves_current_responsibility,
    RevalidateDecisionApproverInput, RuntimeReadSubject,
};
use super::tasks::{
    complete_or_close_tasks, create_open_tasks, CompleteOrCloseTasksInput, CreateOpenTasksInput,
};
use super::{
    find_receipt_for_identity, hidden_not_found, persisted_command_view_with_executor, require_cas_applied,
    ApprovalRuntimeService,
};
use crate::approval::business_adapter::adapter_spec_of;
use crate::approval::process_kind::process_kind_of;
use crate::approval::{ApprovalActionContext, ApprovalDomainActionPort, DecisionActionParams};
use crate::errors::{Error, ErrorCode, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::SharedRbacService;

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

impl ApprovalRuntimeService {
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
                return Err(Error::ValidationError(format!(
                    "决定必须是 APPROVE 或 REJECT，收到 {other}"
                )))
            }
        };
        let reason = reason
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let key = normalize_idempotency_key(idempotency_key)?;
        let command = RuntimeDecisionCommand {
            work_item_id: work_item_id.to_string(),
            decision,
            reason: reason.clone(),
            expected_task_version,
            idempotency_key: key,
        };
        let outcome = self.commit_decision(actor, command.clone()).await;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) if command_may_have_committed(&error) => {
                self.recover_decision_after_competing_commit(actor, command, error)
                    .await?
            }
            Err(error) => return Err(error),
        };
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
        let rbac = self.rbac.clone();
        let action_port = Arc::clone(&self.action_port);
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    submit_decision_in_transaction(
                        &db,
                        &rbac,
                        action_port.as_ref(),
                        &actor,
                        &command,
                        session,
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
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let rbac = self.rbac.clone();
            let actor = actor.clone();
            let command = command.clone();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        replay_decision_in_transaction(&db, &rbac, &actor, &command, session).await
                    })
                })
                .await;
            match recovered {
                Ok(Some(outcome)) => return Ok(outcome),
                Ok(None) => {}
                Err(error) if command_may_have_committed(&error) => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
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
async fn replay_decision_in_transaction(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    session: &mut mongodb::ClientSession,
) -> Result<Option<RuntimeDecisionOutcome>> {
    let item = db
        .work_items()
        .find_document_approval_by_id(&command.work_item_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let execution_id = match decision_receipt_lookup_gate(&item, actor.id(), command.expected_task_version)? {
        DecisionReceiptLookup::Fresh => return Ok(None),
        DecisionReceiptLookup::Terminal(execution_id) => execution_id,
    };
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&execution_id, session)
        .await?
        .ok_or_else(decision_terminal_fresh_error)?;
    let Some(original_actor_id) = decision_terminal_actor(&item, &execution) else {
        return Err(decision_terminal_fresh_error());
    };
    if original_actor_id != actor.id() {
        return Err(decision_terminal_fresh_error());
    }
    match authorize_decision_terminal_replay(db, rbac, actor, &execution, session).await {
        Ok(()) => {}
        Err(Error::Forbidden(_)) => {
            return Err(decision_terminal_fresh_error());
        }
        Err(error) => return Err(error),
    }
    let identity = decision_identity(
        command.idempotency_key.clone(),
        execution_id.as_ref(),
        &command.work_item_id,
        command.decision.as_str(),
        command.reason.as_deref(),
        command.expected_task_version,
        actor.id(),
    )?;
    let Some(receipt) = find_receipt_for_identity(db, &identity, session).await? else {
        return Err(decision_terminal_fresh_error());
    };
    // 历史无版本摘要可能存在分隔符碰撞，必须先证明收据仍指向该任务的冻结运行身份。
    let ended_execution =
        verify_decision_receipt_runtime_identity(db, &receipt, &execution_id, session).await?;
    let is_current_v3 = receipt.scope_id == identity.current().scope().as_str();
    if !is_current_v3 && !legacy_decision_terminal_facts_match(&item, &ended_execution, command, actor.id()) {
        return Err(payload_conflict_error());
    }
    match identity.classify(Some(&receipt)) {
        ReceiptBranch::SamePayload(_) => {}
        ReceiptBranch::Fresh => unreachable!("receipt was loaded"),
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error()),
    }
    let view =
        persisted_command_view_with_executor(db, &receipt.result_ref, CommitRequired::Proceed, true, session)
            .await?;
    Ok(Some(RuntimeDecisionOutcome {
        blocked: view.instance_status == ApprovalProcessInstanceStatus::Blocked.as_str(),
        view,
    }))
}

/// 在同一事务内执行决定的完整前置、授权、领域动作与运行时写入。
async fn submit_decision_in_transaction(
    db: &Database,
    rbac: &SharedRbacService,
    action_port: &dyn ApprovalDomainActionPort,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    session: &mut mongodb::ClientSession,
) -> Result<RuntimeDecisionOutcome> {
    if let Some(replay) = replay_decision_in_transaction(db, rbac, actor, command, session).await? {
        return Ok(replay);
    }

    let item = db
        .work_items()
        .find_document_approval_by_id(&command.work_item_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let execution_id = item
        .approval_execution_for_decision(actor.id(), command.expected_task_version)
        .map_err(map_approval_task_error)?
        .clone();
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&execution_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let expected_execution_version = execution.base.version;
    let instance_id = execution.process_instance_id.clone();
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&instance_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let expected_instance_version = instance.base.version;
    let document_type =
        entities::approval_integration::document_type_from_subject_kind(instance.subject.subject_kind())
            .map_err(|error| Error::ValidationError(error.to_string()))?;
    if instance.process_kind != process_kind_of(document_type) {
        return Err(Error::ConflictError(
            "审批实例流程种类与单据类型不一致".to_string(),
        ));
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(instance_id.as_ref(), session)
        .await?
        .ok_or_else(hidden_not_found)?;
    snapshot
        .ensure_matches_runtime_subject(
            document_type,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| Error::ConflictError("审批实例与冻结业务快照不一致".to_string()))?;
    let spec = adapter_spec_of(document_type)?;
    let separation_policy = process_required_separation_policy(document_type)?;
    let subject = RuntimeReadSubject {
        instance: instance.clone(),
        current_execution: Some(execution.clone()),
        snapshot: snapshot.clone(),
        document_type,
    };
    if !task_proves_current_responsibility(&item, &execution, &subject, actor.id(), spec.owner_role.as_str())
    {
        return Err(Error::ConflictError(
            "APPROVAL_RESPONSIBILITY_CONFLICT".to_string(),
        ));
    }
    let open_tasks = db
        .work_items()
        .open_approval_tasks_for_execution(&execution_id, session)
        .await?;
    if open_tasks.is_empty() || !open_tasks.iter().any(|task| task.base.id == command.work_item_id) {
        return Err(Error::from_approval_code(ErrorCode::ApprovalTaskNotOpen));
    }
    let instance_assignee = db
        .bpm_workflow()
        .find_assignee_for_node(&instance_id, &execution.node_key, session)
        .await?
        .ok_or_else(|| Error::ConflictError("实例缺少节点审批人绑定".to_string()))?;
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&instance.process_definition_id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例绑定的定义不存在".to_string()))?;
    let current_eligibility = revalidate_decision_approver(
        db,
        rbac,
        RevalidateDecisionApproverInput {
            assignee_id: actor.id(),
            assignee_name: &execution.assignee_name_snapshot,
            authenticated_actor: Some(actor),
            snapshot: &snapshot,
            spec: &spec,
            separation_policy,
        },
        session,
    )
    .await?;
    let decision_target = graph
        .decision_target_node_key(&execution.node_key, command.decision)
        .map_err(map_runtime_graph_error)?;
    let next_eligibility = match decision_target {
        Some(node_key) => match graph.node(&node_key) {
            Some(node) => {
                revalidate_decision_approver(
                    db,
                    rbac,
                    RevalidateDecisionApproverInput {
                        assignee_id: node.assignee_participant_id.as_str(),
                        assignee_name: &node.assignee_label_snapshot,
                        authenticated_actor: None,
                        snapshot: &snapshot,
                        spec: &spec,
                        separation_policy,
                    },
                    session,
                )
                .await?
            }
            None => return Err(Error::ConflictError("审批定义缺少目标节点".to_string())),
        },
        None => current_eligibility.clone(),
    };
    let now = Instant::now();
    let prepared = prepare_decision(DecisionExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility,
            next_eligibility,
            receipt: None,
            idempotency_key: command.idempotency_key.clone(),
            now: Timestamp::from_utc(now.as_utc()),
        },
        instance,
        current: execution.clone(),
        work_item_id: command.work_item_id.clone(),
        task_owner_id: item.owner_user_id.clone().unwrap_or_default(),
        instance_assignee_id: instance_assignee
            .current_assignee_participant_id
            .as_str()
            .to_string(),
        decision: command.decision,
        reason: command.reason.clone(),
        expected_task_version: command.expected_task_version,
        actor: ParticipantId::new(actor.id())
            .map_err(|_| Error::ValidationError("决定人引用无效".to_string()))?,
        next_execution_id: ApprovalNodeExecutionId::new(next_id()),
        next_execution_no: execution
            .execution_no
            .checked_add(1)
            .ok_or_else(|| Error::ConflictError("审批执行序号溢出".to_string()))?,
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        open_task_count: open_tasks.len(),
    })?;
    let PreparedExecution::Apply(writes) = prepared else {
        return Err(Error::Internal("新决定命令不得进入幂等回放分支".to_string()));
    };
    let writes = *writes;
    let actor_id = actor.id().to_string();
    let owner_role = spec.owner_role.as_str().to_string();
    let owner_organization_id = snapshot.payload.responsible_org_id.clone();
    let subject_version = writes.instance.subject_version.to_string();
    let business_object_id = writes.instance.subject.subject_id().to_string();
    let document_type_label = document_type.label().to_string();
    let runtime_admin_ids = if writes.notifications.iter().any(|intent| {
        intent.event_kind == entities::approval_integration::ApprovalNotificationEventKind::Blocked
    }) {
        runtime_admin_notification_recipients(db, rbac, document_type, &snapshot, session).await?
    } else {
        Vec::new()
    };
    let should_finalize = writes.commit == CommitRequired::TerminalApproved;
    let action_context = ApprovalActionContext::for_decision(DecisionActionParams {
        approval_process_instance_id: instance_id.to_string(),
        approval_node_execution_id: execution_id.to_string(),
        work_item_id: command.work_item_id.clone(),
        business_object_type: document_type.as_str().to_string(),
        business_object_id: business_object_id.clone(),
        subject_version: subject_version.clone(),
        actor_id: actor_id.clone(),
        reason: command.reason.clone(),
        idempotency_key: command.idempotency_key.as_str().to_string(),
    })?;
    let new_task_ids: Vec<String> = writes.create_tasks.iter().map(|_| next_id()).collect();
    let list_projection =
        list_projection_from_writes(&writes, execution_id.as_ref(), command.reason.clone(), now);
    let audit = actor.clone().resource_log_with_message(
        "approval.decide",
        "approval_process_instance",
        instance_id.to_string(),
        Some(format!(
            "decision={} reason={:?} work_item={}",
            command.decision.as_str(),
            command.reason,
            command.work_item_id
        )),
    )?;

    // 收据唯一键先于任何领域写入仲裁同键并发；后续失败会随事务整体回滚。
    db.bpm_workflow()
        .insert_command_receipt(&writes.receipt, session)
        .await
        .map_err(map_receipt_first_write_error)?;
    if should_finalize {
        action_port
            .execute(spec.on_final_approve, &action_context, actor, session)
            .await?;
    }
    persist_decision_writes(
        db,
        PersistDecisionWrites {
            writes: &writes,
            ended_execution_id: execution_id.as_ref(),
            expected_instance_version,
            expected_execution_version,
            expected_task_version: command.expected_task_version,
            work_item_id: &command.work_item_id,
            new_task_ids: &new_task_ids,
            list_projection: &list_projection,
            audit: &audit,
            now,
            actor_id: &actor_id,
            owner_role: &owner_role,
            owner_organization_id: &owner_organization_id,
            subject_version: &subject_version,
            business_object_id: &business_object_id,
            document_type_label: &document_type_label,
            document_no: &snapshot.payload.document_no,
            submitted_by: &snapshot.payload.submitted_by,
            ended_execution: &execution,
            reject_reason: command.reason.as_deref(),
            runtime_admin_ids: &runtime_admin_ids,
        },
        session,
    )
    .await?;
    let blocked = writes.commit == CommitRequired::Blocked;
    let view = map_command_view(
        &writes.instance,
        writes.created_executions.last(),
        command.reason.clone(),
        None,
        first_open_task(&writes, &new_task_ids),
        writes.commit,
        false,
    );
    Ok(RuntimeDecisionOutcome { view, blocked })
}

/// 在 legacy 摘要双读前验证收据、任务执行、实例、流程类型与冻结主体属于同一运行身份。
async fn verify_decision_receipt_runtime_identity(
    db: &Database,
    receipt: &ApprovalCommandReceipt,
    expected_execution_id: &ApprovalNodeExecutionId,
    session: &mut mongodb::ClientSession,
) -> Result<ApprovalNodeExecution> {
    if receipt.scope_id != expected_execution_id.as_ref() {
        return Err(hidden_not_found());
    }
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(expected_execution_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(&receipt.result_ref), session)
        .await?
        .ok_or_else(hidden_not_found)?;
    if execution.process_instance_id.as_ref() != receipt.result_ref || instance.base.id != receipt.result_ref
    {
        return Err(hidden_not_found());
    }
    let document_type =
        entities::approval_integration::document_type_from_subject_kind(instance.subject.subject_kind())
            .map_err(|_| hidden_not_found())?;
    if instance.process_kind != process_kind_of(document_type) {
        return Err(hidden_not_found());
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, session)
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
                (
                    ApprovalNodeExecutionStatus::Approved,
                    Some(ApprovalDecision::Approve)
                ) | (
                    ApprovalNodeExecutionStatus::Rejected,
                    Some(ApprovalDecision::Reject)
                )
            );
            (terminal_decision_matches
                && completed_by == decided_by
                && execution.decided_at.is_some()
                && execution.ended_at.is_some())
            .then_some(completed_by)
        }
        WorkItemStatus::Closed => {
            let closed_by = item.closed_by.as_deref()?;
            (item.close_reason.as_deref() == Some(TaskCloseReason::ApprovalRuntimeBlocked.as_str())
                && execution.status == ApprovalNodeExecutionStatus::Blocked
                && execution.blocker_code.is_some()
                && execution.blocked_at.is_some()
                && execution.ended_at.is_none()
                && execution.assignee_participant_id.as_str() == closed_by)
                .then_some(closed_by)
        }
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
    rbac: &SharedRbacService,
    actor: &AuditActor,
    execution: &ApprovalNodeExecution,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&execution.process_instance_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    if execution.process_instance_id.as_ref() != instance.base.id {
        return Err(hidden_not_found());
    }
    let document_type =
        entities::approval_integration::document_type_from_subject_kind(instance.subject.subject_kind())
            .map_err(|_| hidden_not_found())?;
    if instance.process_kind != process_kind_of(document_type) {
        return Err(hidden_not_found());
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, session)
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
            session,
        )
        .await?
        .ok_or_else(hidden_not_found)?;
    if execution.assignee_participant_id.as_str() != actor.id()
        || assignee.current_assignee_participant_id.as_str() != actor.id()
    {
        return Err(hidden_forbidden());
    }
    let spec = adapter_spec_of(document_type)?;
    let eligibility = revalidate_decision_approver(
        db,
        rbac,
        RevalidateDecisionApproverInput {
            assignee_id: actor.id(),
            assignee_name: &execution.assignee_name_snapshot,
            authenticated_actor: Some(actor),
            snapshot: &snapshot,
            spec: &spec,
            separation_policy: process_required_separation_policy(document_type)?,
        },
        session,
    )
    .await?;
    if eligibility.blocked_code().is_some() {
        return Err(hidden_forbidden());
    }
    Ok(())
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
        ApprovalDecisionTaskError::VersionConflict => {
            Error::ConflictError("任务版本已变化，请刷新后重试".to_string())
        }
        ApprovalDecisionTaskError::NotDocumentApproval
        | ApprovalDecisionTaskError::NotOpen
        | ApprovalDecisionTaskError::MissingExecution => {
            Error::from_approval_code(ErrorCode::ApprovalTaskNotOpen)
        }
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
fn map_runtime_graph_error(error: ModelError) -> Error {
    Error::ConflictError(error.to_string())
}

/// 决定写库事务所需的计划、版本守卫与通知事实。
///
/// # 用途
/// 打包 [`persist_decision_writes`] 的非基础设施参数。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 调用方必须先写入命令收据，再传入本结构执行领域写。
struct PersistDecisionWrites<'a> {
    writes: &'a PlannedWrites,
    ended_execution_id: &'a str,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: u64,
    work_item_id: &'a str,
    new_task_ids: &'a [String],
    list_projection: &'a ApprovalInstanceListProjection,
    audit: &'a erp_audit::AuditLog,
    now: Instant,
    actor_id: &'a str,
    owner_role: &'a str,
    owner_organization_id: &'a str,
    subject_version: &'a str,
    business_object_id: &'a str,
    document_type_label: &'a str,
    document_no: &'a str,
    submitted_by: &'a str,
    ended_execution: &'a ApprovalNodeExecution,
    reject_reason: Option<&'a str>,
    runtime_admin_ids: &'a [String],
}

/// 单事务应用决定写入：实例 CAS 推进、执行结束/插入、审批人绑定、任务
/// 完成/关闭/新建、通知 outbox 与审计。
///
/// # 用途
/// 在调用方已写入命令收据后，持久化决定计划的全部领域副作用。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 计划、版本守卫、任务与通知事实
/// * `session` - 事务会话
///
/// # 返回
/// 全部写入成功时返回 `Ok(())`。
///
/// # 错误
/// CAS 冲突、任务缺失、通知意图非法或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 命令收据必须由调用方先写入以仲裁并发；本函数不再重复写收据。
async fn persist_decision_writes(
    db: &Database,
    input: PersistDecisionWrites<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let PersistDecisionWrites {
        writes,
        ended_execution_id,
        expected_instance_version,
        expected_execution_version,
        expected_task_version,
        work_item_id,
        new_task_ids,
        list_projection,
        audit,
        now,
        actor_id,
        owner_role,
        owner_organization_id,
        subject_version,
        business_object_id,
        document_type_label,
        document_no,
        submitted_by,
        ended_execution,
        reject_reason,
        runtime_admin_ids,
    } = input;
    // 实例 CAS 推进（RUNNING|BLOCKED + 当前执行不变式）。期望版本为加载时的
    // 持久化版本（引擎计划内的版本已在快照上自增），未应用视为并发冲突。
    let expected_current_execution_id = ApprovalNodeExecutionId::new(ended_execution_id);
    require_cas_applied(
        db.bpm_workflow()
            .advance_instance(
                &writes.instance,
                expected_instance_version,
                &expected_current_execution_id,
                list_projection,
                session,
            )
            .await?,
        "审批实例",
    )?;
    // 当前执行结束（APPROVED/REJECTED/BLOCKED）。
    for execution in &writes.updated_executions {
        let expected = if execution.base.id.as_str() == ended_execution_id {
            expected_execution_version
        } else {
            execution
                .base
                .version
                .checked_sub(1)
                .ok_or_else(|| Error::Internal("决定后执行版本非法".to_string()))?
        };
        require_cas_applied(
            db.bpm_workflow()
                .end_active_execution(execution, expected, session)
                .await?,
            "审批执行",
        )?;
    }
    // 新执行与审批人绑定。
    for execution in &writes.created_executions {
        db.bpm_workflow().insert_execution(execution, session).await?;
    }
    if !writes.created_assignees.is_empty() {
        db.bpm_workflow()
            .insert_assignees(&writes.created_assignees, session)
            .await?;
    }
    // 任务：完成当前、按原因关闭、为下一节点新建。
    complete_or_close_tasks(
        db,
        CompleteOrCloseTasksInput {
            complete_tasks: &writes.complete_tasks,
            close_tasks: &writes.close_tasks,
            work_item_id,
            expected_task_version,
            ended_execution_id,
            actor_id,
            now,
        },
        session,
    )
    .await?;
    create_open_tasks(
        db,
        CreateOpenTasksInput {
            writes,
            new_task_ids,
            owner_role,
            owner_organization_id,
            subject_version,
            business_object_id,
            now,
        },
        session,
    )
    .await?;
    persist_decision_notifications(
        db,
        writes,
        DecisionNotificationFacts {
            document_type_label,
            document_no,
            submitted_by,
            ended_execution,
            reject_reason,
            runtime_admin_ids,
        },
        now,
        session,
    )
    .await?;
    db.audit_logs().create(audit, session).await?;
    Ok(())
}
