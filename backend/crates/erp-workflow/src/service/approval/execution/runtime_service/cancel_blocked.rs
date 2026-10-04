//! 受阻取消提交、回放与事务写入。

use std::sync::Arc;

use application_core::AuditActor;
use bpm::engine::{CommitRequired, DefinitionGraph};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalBlockerCode, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, IdempotencyKey, ParticipantId, Timestamp};
use erp_core::common::time::Instant;
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
use crate::ports::{ApprovalObjectReadPort, PreparedWorkflowAudit};
use crate::repository::prelude::*;
use crate::repository::{ApprovalCancellationExt, BpmExt, WorkItemExt};
use crate::service::approval::business_adapter::adapter_spec_of;
use crate::service::approval::{
    ApprovalActionContext, ApprovalCancelBlockedCommand, ApprovalDomainActionPort, BlockedCancelActionParams,
    require_approval_management_with_executor,
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
    /// 取消不允许原审批人恢复、但允许人工终止的受阻实例。
    ///
    /// # 参数
    /// * `actor` - 当前认证且具备恢复权限的审计主体
    /// * `command` - 受阻实例、执行和任务期望版本、取消原因及幂等键
    ///
    /// # 返回
    /// 返回取消后的审批命令视图；幂等回放返回已持久化视图。
    ///
    /// # 错误
    /// 原审批人恢复前置不满足、权限不足、版本冲突、快照不一致或事务写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 仅允许无开放任务的受阻端口取消，冻结快照三项主体引用必须全部精确匹配。
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

    /// 在唯一事务内先查/写收据，再执行受阻取消的强类型动作与全部副作用。
    async fn commit_cancel_blocked(
        &self,
        actor: &AuditActor,
        command: ApprovalCancelBlockedCommand,
        idempotency_key: IdempotencyKey,
    ) -> Result<ApprovalCommandView> {
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
                    cancel_blocked_apply(
                        &db,
                        &rbac,
                        action_port.as_ref(),
                        object_read.as_ref(),
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

    /// 并发唯一键或提交结果未知后使用新会话只读胜者收据。
    async fn recover_cancel_blocked_after_competing_commit(
        &self,
        actor: &AuditActor,
        command: ApprovalCancelBlockedCommand,
        idempotency_key: IdempotencyKey,
        original_error: Error,
    ) -> Result<ApprovalCommandView> {
        recover_by_replay(original_error, || async {
            let db = self.db.clone();
            let rbac = self.auth.clone();
            let object_read = Arc::clone(&self.object_read);
            let actor = actor.clone();
            let command = command.clone();
            let idempotency_key = idempotency_key.clone();
            self.db
                .client()
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        replay_cancel_blocked(
                            &db,
                            &rbac,
                            object_read.as_ref(),
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

/// 受阻取消先按实例终态证明原操作人并重验当前授权，再允许查询和比较收据。
async fn replay_cancel_blocked(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn ApprovalObjectReadPort,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    idempotency_key: &IdempotencyKey,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandView>> {
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(&command.approval_process_instance_id), executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    if instance.base.id != command.approval_process_instance_id {
        return Err(hidden_not_found());
    }
    let Some(terminal_facts) =
        replay_terminal_facts(db, rbac, object_read, actor, command, &instance, executor).await?
    else {
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

/// 当前授权和原 actor 必须先于请求摘要比较；缺事实不能当作未执行。
async fn replay_terminal_facts(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn ApprovalObjectReadPort,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    instance: &ApprovalProcessInstance,
    executor: &mut dyn Executor,
) -> Result<Option<CancelBlockedTerminalFacts>> {
    if instance.status != ApprovalProcessInstanceStatus::Cancelled {
        return Ok(None);
    }
    let (document_type, snapshot) = load_exact_runtime_snapshot(db, instance, executor, true).await?;
    ensure_cancel_blocked_authorized(db, rbac, object_read, actor, document_type, &snapshot, executor)
        .await?;
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

/// 加载受阻取消的结构化操作事实并证明回执、执行和任务终态。
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

/// 只有原操作人、原因、版本与历史任务事实全部相同时，终态才证明当前取消载荷。
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

/// 复用 Fresh 受阻取消的实例版本与可恢复状态判定，隐藏收据是否存在。
pub(super) fn ensure_cancel_blocked_instance_preconditions(
    instance: &ApprovalProcessInstance,
    command: &ApprovalCancelBlockedCommand,
) -> Result<()> {
    ensure_expected_version("审批实例", command.expected_instance_version, instance.base.version)?;
    let blocked = instance.status == ApprovalProcessInstanceStatus::Blocked;
    if !recovery_options_for(blocked, instance.blocker_code).contains(&RuntimeRecoveryAction::CancelBlocked) {
        return Err(Error::ConflictError("当前 blocker 不允许该恢复动作".to_string()));
    }
    Ok(())
}

/// 在同一事务内完成受阻取消的收据仲裁、授权、动作、运行时、通知与审计。
// 与回放函数同形以便命令入口统一分派，参数顺序由字段名锚定；告警逐项压制。
#[allow(clippy::too_many_arguments)]
async fn cancel_blocked_apply(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    action_port: &dyn ApprovalDomainActionPort,
    object_read: &dyn ApprovalObjectReadPort,
    audit_port: &dyn crate::ports::WorkflowAuditPort,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    idempotency_key: &IdempotencyKey,
    executor: &mut dyn Executor,
) -> Result<ApprovalCommandView> {
    if let Some(replay) =
        replay_cancel_blocked(db, rbac, object_read, actor, command, idempotency_key, executor).await?
    {
        return Ok(replay);
    }
    let source = load_cancel_source(db, rbac, object_read, actor, command, executor).await?;
    let operation = prepare_blocked_cancel(db, source, command, idempotency_key, actor, executor).await?;
    persist_cancel_operation(db, action_port, audit_port, actor, command, operation, executor).await
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

/// 先重验授权，再验证实例及执行版本，保持原首错顺序。
async fn load_cancel_source(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn ApprovalObjectReadPort,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    executor: &mut dyn Executor,
) -> Result<CancelSource> {
    let instance_id = ApprovalProcessInstanceId::new(&command.approval_process_instance_id);
    let instance =
        db.bpm_workflow().find_instance_by_id(&instance_id, executor).await?.ok_or_else(hidden_not_found)?;
    let (document_type, snapshot) = load_exact_runtime_snapshot(db, &instance, executor, false).await?;
    ensure_cancel_blocked_authorized(db, rbac, object_read, actor, document_type, &snapshot, executor)
        .await?;
    ensure_cancel_blocked_instance_preconditions(&instance, command)?;
    let (current, historical_tasks) = load_cancel_execution(db, &instance, command, executor).await?;
    Ok(CancelSource { document_type, snapshot, instance, current, historical_tasks })
}

/// 当前执行、开放任务数量与请求声明的历史任务版本必须同时成立。
async fn load_cancel_execution(
    db: &Database,
    instance: &ApprovalProcessInstance,
    command: &ApprovalCancelBlockedCommand,
    executor: &mut dyn Executor,
) -> Result<(ApprovalNodeExecution, Vec<WorkItem>)> {
    let policy =
        instance.cancellation_task_policy().map_err(|error| Error::ConflictError(error.to_string()))?;
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
    policy
        .ensure_open_task_count(open_tasks.len())
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    validate_cancel_task_version_with_executor(db, &execution_id, command.expected_task_version, executor)
        .await?;
    let tasks = db.work_items().approval_tasks_for_execution(&execution_id, executor).await?;
    Ok((current, tasks))
}

/// 形成强类型计划和不可变事实，不执行任何业务副作用。
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
        .load_definition_graph(&source.instance.process_definition_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例绑定的定义不存在".into()))?;
    match (source.instance.blocker_code, source.current.blocker_code) {
        (Some(instance), Some(execution)) if instance == execution => {},
        _ => return Err(Error::ConflictError("受阻实例与当前执行 blocker 不一致".into())),
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

/// 包装引擎输入，actor 与原版本均来自已授权命令。
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
        actor: ParticipantId::new(actor.id()).map_err(|_| Error::ValidationError("取消人引用无效".into()))?,
        close_open_task: false,
        blocked_port: true,
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
    })
}

/// 保持回执首写、领域动作、终态、事实、通知及成功审计的提交顺序。
async fn persist_cancel_operation(
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
        .persist_cancelled_runtime_after_receipt(&writes.instance, &writes.updated_executions, executor)
        .await?;
    db.approval_cancellation_facts().create(&fact, executor).await?;
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
    Ok(map_command_view(&writes.instance, None, None, Some("DRAFT".into()), None, writes.commit, false))
}

/// 明确领域取消上下文；审计文本不承担此合同。
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

/// 事务内重验受阻取消账号、动作权限、类型级运行管理、对象读取与 DataScopeFact。
async fn ensure_cancel_blocked_authorized(
    _db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    _object_read: &dyn ApprovalObjectReadPort,
    actor: &AuditActor,
    document_type: DocumentType,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<()> {
    require_approval_management_with_executor(
        rbac,
        actor,
        "approval_instance:cancel_blocked",
        document_type,
        &snapshot.business_object_id,
        executor,
    )
    .await
}

/// 在调用方事务内校验受阻取消携带的可空历史任务版本。
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
