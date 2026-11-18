//! 受阻取消提交、回放与事务写入。

use std::sync::Arc;

use application_core::AuditActor;
use bpm::engine::{CommitRequired, DefinitionGraph};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalBlockerCode, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, IdempotencyKey, ParticipantId, Timestamp};
use erp_core::common::time::Instant;
use erp_identity::{AccessTarget, AuthorizationService, SharedRbacService};
use erp_permissions::approval_instance as approval_permissions;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::authorization::{converge_eligibility, hidden_forbidden};
use super::super::idempotency::{
    CancelBlockedIdentityParams, ReceiptBranch, cancel_blocked_identity, map_receipt_first_write_error,
    normalize_idempotency_key, payload_conflict_error,
};
use super::super::runtime_query::{RuntimeRecoveryAction, recovery_options_for};
use super::super::view::{ApprovalCommandView, map_command_view};
use super::super::{
    CancelExecutionInput, ExecutionCommandInput, PlannedWrites, PreparedExecution, prepare_cancel,
};
use super::notifications::{CancelNotificationFacts, persist_cancel_notifications};
use super::{
    ApprovalRuntimeService, commit_or_recover, ensure_command_actor, ensure_expected_version,
    find_receipt_for_identity, hidden_not_found, load_exact_runtime_snapshot,
    persisted_command_view_with_executor, recover_by_replay,
};
use crate::entity::approval_cancellation::ApprovalCancellationFact;
use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::entity::document_registry::DocumentType;
use crate::entity::work_item::WorkItem;
use crate::error::{Error, Result};
use crate::ports::PreparedWorkflowAudit;
use crate::repository::prelude::*;
use crate::repository::{ApprovalCancellationExt, BpmExt, WorkItemExt};
use crate::service::approval::business_adapter::adapter_spec_of;
use crate::service::approval::copies::persist_copies;
use crate::service::approval::{
    ApprovalActionContext, ApprovalCancelBlockedCommand, ApprovalDomainActionPort, BlockedCancelActionParams,
};

/// 已提交受阻取消的不可变终态事实；先证明原操作人，再允许比较请求摘要。
pub(super) struct CancelBlockedTerminalFacts {
    pub(super) receipt_id: String,
    pub(super) blocker: ApprovalBlockerCode,
    pub(super) actor_id: String,
    pub(super) reason: String,
    pub(super) execution_version: u64,
    pub(super) task_versions: Vec<u64>,
}

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 对不允许原审批人恢复但允许人工终止的受阻实例提交取消。必须没有开放任务，且冻结快照的单据类型、对象和提交版本须精确匹配。
    ///
    /// 以原写事务核对当前恢复权限、取消原因、期望版本和幂等身份。同命令回放返回持久化结果；资格、状态、快照、版本或写入不满足时拒绝。
    pub async fn cancel_blocked(
        &self,
        actor: &AuditActor,
        mut command: ApprovalCancelBlockedCommand,
    ) -> Result<ApprovalCommandView> {
        ensure_command_actor(actor, &command.actor_id)?;
        command.reason = command.reason.trim().to_string();
        if command.reason.is_empty() {
            return Err(Error::ValidationError("受阻取消原因不能为空".to_string()));
        }
        let idempotency_key = normalize_idempotency_key(&command.idempotency_key)?;
        command.idempotency_key = idempotency_key.as_str().to_string();

        let commit_command = command.clone();
        let commit_key = idempotency_key.clone();
        commit_or_recover(
            || self.commit_cancel_blocked(actor, commit_command, commit_key),
            |error| {
                self.recover_cancel_blocked_after_competing_commit(actor, command, idempotency_key, error)
            },
        )
        .await
    }

    // 在单一事务仲裁受阻取消的收据及回放。新命令再执行强类型领域动作与全部运行副作用，不单独提前提交收据。
    async fn commit_cancel_blocked(
        &self,
        actor: &AuditActor,
        command: ApprovalCancelBlockedCommand,
        idempotency_key: IdempotencyKey,
    ) -> Result<ApprovalCommandView> {
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let identity_service = self.identity.clone();
        let action_port = Arc::clone(&self.action_port);
        let audit_port = Arc::clone(&self.audit);
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    cancel_blocked_apply(
                        &authorization,
                        &identity_service,
                        &db,
                        action_port.as_ref(),
                        audit_port.as_ref(),
                        &actor,
                        &command,
                        &idempotency_key,
                        executor,
                    )
                    .await
                })
            })
            .await
    }

    // 并发冲突或提交结果未知时，在新快照重验并回读可能已提交的取消收据，不再次执行取消写入。
    async fn recover_cancel_blocked_after_competing_commit(
        &self,
        actor: &AuditActor,
        command: ApprovalCancelBlockedCommand,
        idempotency_key: IdempotencyKey,
        original_error: Error,
    ) -> Result<ApprovalCommandView> {
        recover_by_replay(original_error, || async {
            let db = self.db.clone();
            let authorization = self.authorization.clone();

            let actor = actor.clone();
            let command = command.clone();
            let idempotency_key = idempotency_key.clone();
            self.db
                .client()
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        replay_cancel_blocked(
                            &authorization,
                            &db,
                            &actor,
                            &command,
                            &idempotency_key,
                            executor,
                        )
                        .await
                    })
                })
                .await
        })
        .await
    }
}

// 受阻取消先按实例终态证明原操作人并重验当前授权，再允许查询和比较收据。
//
// 触发 panic 的条件：
// 已加载收据被分类为 `Fresh` 时 `unreachable!`，表示分类器与查询结果矛盾。
async fn replay_cancel_blocked(
    authorization: &AuthorizationService,
    db: &Database,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    idempotency_key: &IdempotencyKey,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandView>> {
    ensure_cancel_blocked_authorized(authorization, actor, &command.approval_process_instance_id, executor)
        .await?;

    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(&command.approval_process_instance_id), executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    if instance.base.id != command.approval_process_instance_id {
        return Err(hidden_not_found());
    }
    let Some(terminal_facts) = replay_terminal_facts(db, actor, command, &instance, executor).await? else {
        return Ok(None);
    };

    let identity = cancel_blocked_identity(CancelBlockedIdentityParams {
        idempotency_key: idempotency_key.clone(),
        instance_id: &command.approval_process_instance_id,
        blocker: terminal_facts.blocker.as_str(),
        expected_instance_version: command.expected_instance_version,
        expected_execution_version: command.expected_execution_version,
        expected_task_version: command.expected_task_version,
        reason: &command.reason,
        actor_id: actor.id(),
    })?;
    let Some(receipt) = find_receipt_for_identity(db, &identity, executor).await? else {
        return Ok(None);
    };
    if receipt.result_ref != command.approval_process_instance_id
        || receipt.base.id != terminal_facts.receipt_id
    {
        return Err(hidden_not_found());
    }
    // 原回执的结果必须由同一组结构化取消终态事实证明。
    if !cancel_blocked_terminal_facts_match(&instance, &terminal_facts, command, actor.id()) {
        return Err(payload_conflict_error());
    }
    match identity.classify(Some(&receipt)) {
        ReceiptBranch::SamePayload(_) => {},
        ReceiptBranch::Fresh => unreachable!("receipt was loaded"),
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error()),
    }

    persisted_command_view_with_executor(db, &receipt.result_ref, CommitRequired::Cancelled, true, executor)
        .await
        .map(Some)
}

// 受阻取消回放必须先证明当前授权和原取消人，才能比较请求摘要。终态事实缺失不是未执行，不准走新写入。
async fn replay_terminal_facts(
    db: &Database,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    instance: &ApprovalProcessInstance,
    executor: &mut dyn Executor,
) -> Result<Option<CancelBlockedTerminalFacts>> {
    if instance.status != ApprovalProcessInstanceStatus::Cancelled {
        return Ok(None);
    }
    load_exact_runtime_snapshot(db, instance, executor, true).await?;
    let facts =
        load_cancel_blocked_terminal_facts(db, instance, executor).await?.ok_or_else(hidden_not_found)?;
    if facts.actor_id != actor.id() {
        return match ensure_cancel_blocked_instance_preconditions(instance, command) {
            Err(error) => Err(error),
            Ok(()) => Err(hidden_forbidden()),
        };
    }
    Ok(Some(facts))
}

// 回读不可变取消事实，核对持久化收据与精确执行、历史任务均已达到与本次命令匹配的终态。
async fn load_cancel_blocked_terminal_facts(
    db: &Database,
    instance: &ApprovalProcessInstance,
    executor: &mut dyn Executor,
) -> Result<Option<CancelBlockedTerminalFacts>> {
    if instance.status != ApprovalProcessInstanceStatus::Cancelled
        || instance.current_node_execution_id.is_some()
        || instance.blocker_code.is_some()
        || instance.ended_at.is_none()
    {
        return Ok(None);
    }

    let fact = db
        .approval_cancellation_facts()
        .find_by_id(&instance.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("审批取消缺少结构化操作事实".into()))?;
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&ApprovalNodeExecutionId::new(&fact.execution_id), executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    let blocker = execution.blocker_code.ok_or_else(hidden_not_found)?;
    let tasks = db
        .work_items()
        .approval_tasks_for_execution(&ApprovalNodeExecutionId::new(&fact.execution_id), executor)
        .await?;
    fact.ensure_runtime(instance, &execution, &tasks)?;

    let receipt = db
        .bpm_workflow()
        .find_command_receipt(
            fact.receipt.command_kind,
            &fact.receipt.scope_id,
            &fact.receipt.idempotency_key,
            executor,
        )
        .await?;
    if receipt.as_ref() != Some(&fact.receipt) {
        return Err(Error::ConflictError("审批取消事实与原命令回执不一致".into()));
    }
    Ok(Some(CancelBlockedTerminalFacts {
        receipt_id: fact.receipt.base.id,
        blocker,
        actor_id: fact.actor_id,
        reason: fact.reason,
        execution_version: execution.base.version,
        task_versions: tasks.into_iter().map(|task| task.base.version).collect(),
    }))
}

// 只有原操作人、原因、版本与历史任务事实全部相同时，终态才证明当前取消载荷。
// 实例版本、执行版本、操作人、原因和任务版本都匹配时返回 `true`；期望版本溢出时返回 `false`。
//
// `instance`：已取消实例；`facts`：已加载的终态事实。
pub(super) fn cancel_blocked_terminal_facts_match(
    instance: &ApprovalProcessInstance,
    facts: &CancelBlockedTerminalFacts,
    command: &ApprovalCancelBlockedCommand,
    actor_id: &str,
) -> bool {
    let Some(expected_instance_version) = command.expected_instance_version.checked_add(1) else {
        return false;
    };
    let Some(expected_execution_version) = command.expected_execution_version.checked_add(1) else {
        return false;
    };
    let task_identity_matches = match command.expected_task_version {
        Some(expected) => facts.task_versions.as_slice() == [expected],
        None => facts.task_versions.is_empty(),
    };
    instance.base.version == expected_instance_version
        && facts.execution_version == expected_execution_version
        && facts.actor_id == actor_id
        && facts.reason == command.reason
        && task_identity_matches
}

// 复用 Fresh 受阻取消的实例版本与可恢复状态判定，隐藏收据是否存在。
// 版本一致且当前 blocker 只允许受阻取消时无返回值。
//
// 实例版本不一致或当前 blocker 不允许受阻取消时返回冲突。
//
// `instance`：已加载实例。
pub(super) fn ensure_cancel_blocked_instance_preconditions(
    instance: &ApprovalProcessInstance,
    command: &ApprovalCancelBlockedCommand,
) -> Result<()> {
    ensure_expected_version("审批实例", command.expected_instance_version, instance.base.version)?;
    let blocked = instance.status == ApprovalProcessInstanceStatus::Blocked;
    if !recovery_options_for(blocked, instance.blocker_code).contains(&RuntimeRecoveryAction::CancelBlocked) {
        return Err(Error::ConflictError("当前受阻原因不支持此恢复操作，请查看可选处理办法".to_string()));
    }
    Ok(())
}

// 在一个事务中完成受阻取消的收据仲裁、授权、动作、运行时、通知与审计。
// 与回放函数同形以便命令入口统一分派，参数顺序由字段名锚定；告警逐项压制。
#[allow(clippy::too_many_arguments)]
async fn cancel_blocked_apply(
    authorization: &AuthorizationService,
    identity_service: &SharedRbacService,
    db: &Database,
    action_port: &dyn ApprovalDomainActionPort,
    audit_port: &dyn crate::ports::WorkflowAuditPort,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    idempotency_key: &IdempotencyKey,
    executor: &mut dyn Executor,
) -> Result<ApprovalCommandView> {
    if let Some(replay) =
        replay_cancel_blocked(authorization, db, actor, command, idempotency_key, executor).await?
    {
        return Ok(replay);
    }

    let source = load_cancel_source(authorization, db, actor, command, executor).await?;
    let operation = prepare_blocked_cancel(db, source, command, idempotency_key, actor, executor).await?;

    persist_cancel_operation(
        identity_service,
        db,
        action_port,
        audit_port,
        actor,
        command,
        operation,
        executor,
    )
    .await
}

/// 事务内授权和运行事实，版本均为取消前版本。
struct CancelSource {
    document_type: DocumentType,
    snapshot: ApprovalSubjectSnapshot,
    instance: ApprovalProcessInstance,
    current: ApprovalNodeExecution,
    historical_tasks: Vec<WorkItem>,
}

/// 已验证且待原子提交的取消计划。
struct CancelOperation {
    source: CancelSource,
    writes: PlannedWrites,
    now: Instant,
    fact: ApprovalCancellationFact,
    audit: PreparedWorkflowAudit,
}

// 在取消受阻实例前先重验当前授权，再核对实例和执行期望版本，避免把版本或存在性暴露给未授权调用方。
async fn load_cancel_source(
    authorization: &AuthorizationService,
    db: &Database,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    executor: &mut dyn Executor,
) -> Result<CancelSource> {
    ensure_cancel_blocked_authorized(authorization, actor, &command.approval_process_instance_id, executor)
        .await?;

    let instance_id = ApprovalProcessInstanceId::new(&command.approval_process_instance_id);
    let instance =
        db.bpm_workflow().find_instance_by_id(&instance_id, executor).await?.ok_or_else(hidden_not_found)?;
    let (document_type, snapshot) = load_exact_runtime_snapshot(db, &instance, executor, false).await?;
    ensure_cancel_blocked_instance_preconditions(&instance, command)?;
    let (current, historical_tasks) = load_cancel_execution(db, &instance, command, executor).await?;
    Ok(CancelSource { document_type, snapshot, instance, current, historical_tasks })
}

// 取消计划前沿原事务校验当前执行、开放任务数量及请求中的可选历史任务版本；任一不符不能进入取消。
async fn load_cancel_execution(
    db: &Database,
    instance: &ApprovalProcessInstance,
    command: &ApprovalCancelBlockedCommand,
    executor: &mut dyn Executor,
) -> Result<(ApprovalNodeExecution, Vec<WorkItem>)> {
    let policy = instance.cancellation_task_policy().map_err(|error| {
        Error::ConflictError(crate::service::approval::execution::bpm_model_message(&error).into())
    })?;
    if policy.closes_open_task() {
        return Err(Error::ConflictError("受阻取消不得处理运行中审批实例".into()));
    }

    let instance_id = ApprovalProcessInstanceId::new(&instance.base.id);
    let current = db
        .bpm_workflow()
        .find_current_execution(&instance_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例缺少当前受阻执行".into()))?;
    if current.process_instance_id != instance_id
        || instance.current_node_execution_id.as_ref()
            != Some(&ApprovalNodeExecutionId::new(&current.base.id))
        || current.status != ApprovalNodeExecutionStatus::Blocked
    {
        return Err(Error::ConflictError("受阻审批当前执行引用不一致".into()));
    }
    ensure_expected_version("审批执行", command.expected_execution_version, current.base.version)?;

    let execution_id = ApprovalNodeExecutionId::new(&current.base.id);
    let open_tasks = db.work_items().open_approval_tasks_for_execution(&execution_id, executor).await?;
    policy.ensure_open_task_count(open_tasks.len()).map_err(|error| {
        Error::ConflictError(crate::service::approval::execution::bpm_model_message(&error).into())
    })?;
    validate_cancel_task_version_with_executor(db, &execution_id, command.expected_task_version, executor)
        .await?;
    let tasks = db.work_items().approval_tasks_for_execution(&execution_id, executor).await?;
    Ok((current, tasks))
}

// 把已核验的受阻运行事实转成引擎计划和不可变取消证明；准备阶段不调用领域写入或通知。
async fn prepare_blocked_cancel(
    db: &Database,
    source: CancelSource,
    command: &ApprovalCancelBlockedCommand,
    key: &IdempotencyKey,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<CancelOperation> {
    adapter_spec_of(source.document_type)?;
    let graph = db
        .bpm_workflow()
        .load_instance_graph(&source.instance, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("找不到该审批使用的流程，请联系审批管理员核对".into()))?;
    match (source.instance.blocker_code, source.current.blocker_code) {
        (Some(instance), Some(execution)) if instance == execution => {},
        _ => {
            tracing::debug!(instance_reason = ?source.instance.blocker_code, node_reason = ?source.current.blocker_code, "受阻原因不一致");
            return Err(Error::ConflictError(
                "审批记录与当前环节的受阻原因不一致，请联系审批管理员核对".into(),
            ));
        },
    }

    let now = Instant::now();
    let prepared = prepare_cancel(cancel_input(graph, &source, command, key, actor, now)?)?;
    let PreparedExecution::Apply(writes) = prepared else {
        return Err(Error::Internal("新受阻取消命令不得进入幂等回放分支".into()));
    };
    let [execution] = writes.updated_executions.as_slice() else {
        return Err(Error::Internal("受阻取消必须产生唯一执行终态".into()));
    };

    let audit = PreparedWorkflowAudit::resource_with_message(
        actor.clone(),
        "approval.cancel_blocked",
        "approval_process_instance",
        command.approval_process_instance_id.clone(),
        Some(format!("审批已取消；原因：{}", command.reason)),
    )?;
    let fact = ApprovalCancellationFact::new(
        writes.receipt.clone(),
        &writes.instance,
        execution,
        actor.id().to_string(),
        command.reason.clone(),
        &source.historical_tasks,
        audit.id.clone(),
    )?;
    Ok(CancelOperation { source, writes: *writes, now, fact, audit })
}

// 取消受阻实例前，将已授权命令中的操作人和原版本传入引擎；不从客户端展示视图重新推断责任或版本。
fn cancel_input(
    graph: DefinitionGraph,
    source: &CancelSource,
    command: &ApprovalCancelBlockedCommand,
    key: &IdempotencyKey,
    actor: &AuditActor,
    now: Instant,
) -> Result<CancelExecutionInput> {
    let eligibility = converge_eligibility(
        source.current.assignee_participant_id.as_str(),
        &source.current.assignee_name_snapshot,
        None,
    )?;
    Ok(CancelExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: eligibility.clone(),
            next_eligibility: eligibility,
            receipt: None,
            idempotency_key: key.clone(),
            now: Timestamp::from_utc(now.as_utc()),
        },
        instance: source.instance.clone(),
        current: source.current.clone(),
        subject_version: source.snapshot.subject_version,
        expected_instance_version: command.expected_instance_version,
        expected_execution_version: command.expected_execution_version,
        expected_task_version: command.expected_task_version,
        reason: command.reason.clone(),
        actor: ParticipantId::new(actor.id())
            .map_err(|_| Error::ValidationError("无法确认取消账号，请重新登录后再操作".into()))?,
        close_open_task: false,
        blocked_port: true,
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
    })
}

// 应用受阻取消时先以收据仲裁，再执行领域动作、写终态与不可变事实，最后在原事务记录通知和成功审计。
#[allow(clippy::too_many_arguments)]
async fn persist_cancel_operation(
    identity_service: &SharedRbacService,
    db: &Database,
    action_port: &dyn ApprovalDomainActionPort,
    audit_port: &dyn crate::ports::WorkflowAuditPort,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    operation: CancelOperation,
    executor: &mut dyn Executor,
) -> Result<ApprovalCommandView> {
    let CancelOperation { source, writes, now, fact, audit } = operation;
    let context = cancel_action_context(&source, command, actor)?;

    db.bpm_workflow()
        .insert_command_receipt(&writes.receipt, executor)
        .await
        .map_err(map_receipt_first_write_error)?;

    action_port
        .execute(adapter_spec_of(source.document_type)?.cancel_action, &context, actor, executor)
        .await?;

    db.bpm_workflow()
        .persist_cancelled_runtime_after_receipt(
            &writes.instance,
            &writes.updated_executions,
            actor.id(),
            executor,
        )
        .await?;
    db.approval_cancellation_facts().create(&fact, executor).await?;

    persist_copies(identity_service, db, &writes.instance, &writes.events, executor).await?;
    persist_cancel_notifications(
        db,
        &writes,
        CancelNotificationFacts {
            submitted_by: &source.snapshot.payload.submitted_by,
            actor_id: actor.id(),
            document_type_label: source.document_type.label(),
            document_no: &source.snapshot.payload.document_no,
            current_node_name: &source.current.node_name,
            current_approver_display_name: &source.current.assignee_name_snapshot,
        },
        now,
        executor,
    )
    .await?;
    audit_port.persist(&audit, executor).await?;

    let subject_status = match source.document_type {
        DocumentType::PurchasePaymentRequest => "withdrawn",
        _ => "DRAFT",
    };
    Ok(map_command_view(
        &writes.instance,
        None,
        None,
        Some(subject_status.into()),
        None,
        writes.commit,
        false,
    ))
}

// 将已经核验的取消身份和运行事实封装为领域端口输入，不能用自由审计文本代替该合同。
fn cancel_action_context(
    source: &CancelSource,
    command: &ApprovalCancelBlockedCommand,
    actor: &AuditActor,
) -> Result<ApprovalActionContext> {
    ApprovalActionContext::for_blocked_cancel(BlockedCancelActionParams {
        approval_process_instance_id: command.approval_process_instance_id.clone(),
        approval_node_execution_id: source.current.base.id.clone(),
        work_item_id: None,
        business_object_type: source.document_type.as_str().into(),
        business_object_id: source.snapshot.business_object_id.clone(),
        subject_version: source.snapshot.subject_version.to_string(),
        actor_id: actor.id().into(),
        reason: command.reason.clone(),
        idempotency_key: command.idempotency_key.clone(),
    })
}

// 受阻取消在写事务重新检查当前账号、恢复动作功能、真实类型运行管理和来源对象范围，不用旧展示资格。
async fn ensure_cancel_blocked_authorized(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    instance_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization
        .require(actor, &approval_permissions::CANCEL_BLOCKED, AccessTarget::record(instance_id)?, executor)
        .await?;
    Ok(())
}

// 沿取消写事务确认允许为空的历史任务版本是否与原执行的持久化任务相符，不能单凭请求声明无任务。
async fn validate_cancel_task_version_with_executor(
    db: &Database,
    execution_id: &ApprovalNodeExecutionId,
    expected_task_version: Option<u64>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(expected) = expected_task_version else {
        return Ok(());
    };
    let tasks = db.work_items().approval_tasks_for_execution(execution_id, executor).await?;
    if tasks.len() != 1 {
        return Err(Error::ConflictError(
            "调用方声明了审批任务版本，但受阻执行未关联唯一历史任务".to_string(),
        ));
    }
    ensure_expected_version("审批任务", expected, tasks[0].base.version)
}

#[cfg(test)]
mod authorization_tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::service::approval::definition::test_support::unavailable_service;

    #[tokio::test]
    async fn blocked_cancel_and_replay_stop_before_instance_reads_when_authorization_fails() {
        let (service, actor) = unavailable_service().await;
        let command = ApprovalCancelBlockedCommand {
            approval_process_instance_id: "unread-instance".into(),
            expected_instance_version: 7,
            expected_execution_version: 8,
            expected_task_version: Some(9),
            reason: "终止流程".into(),
            idempotency_key: "cancel-key".into(),
            actor_id: actor.id().into(),
        };
        let key = IdempotencyKey::parse(&command.idempotency_key).unwrap();
        let results = [
            load_cancel_source(
                &service.authorization,
                &service.db,
                &actor,
                &command,
                &mut SnapshotExecutor::default(),
            )
            .await
            .map(|_| ()),
            replay_cancel_blocked(
                &service.authorization,
                &service.db,
                &actor,
                &command,
                &key,
                &mut SnapshotExecutor::default(),
            )
            .await
            .map(|_| ()),
        ];
        for result in results {
            assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
        }
    }
}
