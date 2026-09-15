//! 受阻取消提交、回放与事务写入。

use std::sync::Arc;

use application_core::AuditActor;
use bpm::engine::CommitRequired;
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalBlockerCode, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{ApprovalProcessInstance, IdempotencyKey, ParticipantId, Timestamp};
use erp_core::common::time::Instant;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::authorization::{converge_eligibility, hidden_forbidden, requires_blocked_cancel};
use super::super::idempotency::{
    CancelBlockedIdentityParams, ReceiptBranch, cancel_blocked_identity, command_may_have_committed,
    command_recovery_delay, map_receipt_first_write_error, normalize_idempotency_key, payload_conflict_error,
};
use super::super::runtime_query::{RuntimeRecoveryAction, recovery_options_for};
use super::super::view::{ApprovalCommandView, map_command_view};
use super::super::{CancelExecutionInput, ExecutionCommandInput, PreparedExecution, prepare_cancel};
use super::notifications::{CancelNotificationFacts, persist_cancel_notifications};
use super::read_auth::runtime_object_readable;
use super::{
    ApprovalRuntimeService, ensure_command_actor, ensure_expected_version, find_receipt_for_identity,
    hidden_not_found, load_exact_runtime_snapshot, persisted_command_view_with_executor,
};
use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::entity::document_registry::DocumentType;
use crate::error::{Error, Result};
use crate::ports::{ApprovalObjectReadPort, PreparedWorkflowAudit};
use crate::repository::{BpmExt, WorkItemExt};
use crate::service::approval::business_adapter::{BindingRevalidationContext, adapter_spec_of};
use crate::service::approval::{
    ApprovalActionContext, ApprovalCancelBlockedCommand, ApprovalDomainActionPort, BlockedCancelActionParams,
    approval_actor_is_active_with_executor, approval_cancel_blocked_scope_with_executor,
    approval_document_read_scope_with_executor, definition_management_visibility_with_executor,
};

/// 已提交受阻取消的不可变终态事实；先证明原操作人，再允许比较请求摘要。
pub(super) struct CancelBlockedTerminalFacts {
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
        let outcome = self.commit_cancel_blocked(actor, command.clone(), idempotency_key.clone()).await;
        match outcome {
            Ok(view) => Ok(view),
            Err(error) if command_may_have_committed(&error) => {
                self.recover_cancel_blocked_after_competing_commit(actor, command, idempotency_key, error)
                    .await
            },
            Err(error) => Err(error),
        }
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
            .with_transaction(move |session| {
                Box::pin(async move {
                    cancel_blocked_in_transaction(
                        &db,
                        &rbac,
                        action_port.as_ref(),
                        object_read.as_ref(),
                        audit_port.as_ref(),
                        &actor,
                        &command,
                        &idempotency_key,
                        session,
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
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let rbac = self.auth.clone();
            let object_read = Arc::clone(&self.object_read);
            let audit_port = Arc::clone(&self.audit);
            let actor = actor.clone();
            let command = command.clone();
            let idempotency_key = idempotency_key.clone();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        replay_cancel_blocked_in_transaction(
                            &db,
                            &rbac,
                            object_read.as_ref(),
                            audit_port.as_ref(),
                            &actor,
                            &command,
                            &idempotency_key,
                            session,
                        )
                        .await
                    })
                })
                .await;
            match recovered {
                Ok(Some(view)) => return Ok(view),
                Ok(None) => {},
                Err(error) if command_may_have_committed(&error) => {},
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }
}

/// 受阻取消先按实例终态证明原操作人并重验当前授权，再允许查询和比较收据。
async fn replay_cancel_blocked_in_transaction(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn ApprovalObjectReadPort,
    audit_port: &dyn crate::ports::WorkflowAuditPort,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    idempotency_key: &IdempotencyKey,
    session: &mut mongodb::ClientSession,
) -> Result<Option<ApprovalCommandView>> {
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(&command.approval_process_instance_id), session)
        .await?
        .ok_or_else(hidden_not_found)?;
    if instance.base.id != command.approval_process_instance_id {
        return Err(hidden_not_found());
    }
    let (document_type, snapshot) = load_exact_runtime_snapshot(db, &instance, session, true).await?;

    let terminal_facts = if instance.status == ApprovalProcessInstanceStatus::Cancelled {
        // 与 Fresh 路径保持相同顺序：失权调用方在任何收据读取或摘要比较前即失败关闭。
        ensure_cancel_blocked_authorized(db, rbac, object_read, actor, document_type, &snapshot, session)
            .await?;
        let facts = load_cancel_blocked_terminal_facts(db, audit_port, &instance, session)
            .await?
            .ok_or_else(hidden_not_found)?;
        if facts.actor_id != actor.id() {
            return match ensure_cancel_blocked_instance_preconditions(&instance, command) {
                Err(error) => Err(error),
                Ok(()) => Err(hidden_forbidden()),
            };
        }
        Some(facts)
    } else {
        None
    };

    let Some(terminal_facts) = terminal_facts else {
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
    let Some(receipt) = find_receipt_for_identity(db, &identity, session).await? else {
        return Ok(None);
    };
    if receipt.result_ref != command.approval_process_instance_id {
        return Err(hidden_not_found());
    }
    // V2 与 legacy 都必须由同一组终态事实证明 receipt result；legacy 不能只依赖旧摘要。
    if !cancel_blocked_terminal_facts_match(&instance, &terminal_facts, command, actor.id()) {
        return Err(payload_conflict_error());
    }
    match identity.classify(Some(&receipt)) {
        ReceiptBranch::SamePayload(_) => {},
        ReceiptBranch::Fresh => unreachable!("receipt was loaded"),
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error()),
    }
    persisted_command_view_with_executor(db, &receipt.result_ref, CommitRequired::Cancelled, true, session)
        .await
        .map(Some)
}

/// 加载受阻取消的结构化终态、唯一成功审计与历史任务事实，不使用当前请求字段筛选。
async fn load_cancel_blocked_terminal_facts(
    db: &Database,
    audit_port: &dyn crate::ports::WorkflowAuditPort,
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

    let audits = audit_port
        .list_successful_resource_audits("approval_process_instance", &instance.base.id, executor)
        .await?;
    let matching_audits = audits
        .iter()
        .filter(|audit| audit.action == "approval.cancel_blocked")
        .filter_map(|audit| {
            let message = audit.message.as_deref()?;
            let (execution_id, reason) = message.strip_prefix("execution=")?.split_once(" reason=")?;
            (!execution_id.is_empty())
                .then(|| (audit.actor_id.clone(), execution_id.to_string(), reason.to_string()))
        })
        .collect::<Vec<_>>();
    let [(actor_id, execution_id, reason)] = matching_audits.as_slice() else {
        return Ok(None);
    };
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&ApprovalNodeExecutionId::new(execution_id), executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    if execution.process_instance_id.as_ref() != instance.base.id
        || execution.status != ApprovalNodeExecutionStatus::Cancelled
        || execution.ended_at != instance.ended_at
    {
        return Ok(None);
    }
    let Some(blocker) = execution.blocker_code else {
        return Ok(None);
    };
    if !requires_blocked_cancel(blocker) {
        return Ok(None);
    }

    let tasks = db
        .work_items()
        .approval_tasks_for_execution(&ApprovalNodeExecutionId::new(execution_id), executor)
        .await?;
    Ok(Some(CancelBlockedTerminalFacts {
        blocker,
        actor_id: actor_id.clone(),
        reason: reason.clone(),
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
async fn cancel_blocked_in_transaction(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    action_port: &dyn ApprovalDomainActionPort,
    object_read: &dyn ApprovalObjectReadPort,
    audit_port: &dyn crate::ports::WorkflowAuditPort,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    idempotency_key: &IdempotencyKey,
    session: &mut mongodb::ClientSession,
) -> Result<ApprovalCommandView> {
    if let Some(replay) = replay_cancel_blocked_in_transaction(
        db,
        rbac,
        object_read,
        audit_port,
        actor,
        command,
        idempotency_key,
        session,
    )
    .await?
    {
        return Ok(replay);
    }
    let instance_id = ApprovalProcessInstanceId::new(&command.approval_process_instance_id);
    let instance =
        db.bpm_workflow().find_instance_by_id(&instance_id, session).await?.ok_or_else(hidden_not_found)?;
    let (document_type, snapshot) = load_exact_runtime_snapshot(db, &instance, session, false).await?;
    ensure_cancel_blocked_authorized(db, rbac, object_read, actor, document_type, &snapshot, session).await?;
    ensure_cancel_blocked_instance_preconditions(&instance, command)?;
    let task_policy =
        instance.cancellation_task_policy().map_err(|error| Error::ConflictError(error.to_string()))?;
    if task_policy.closes_open_task() {
        return Err(Error::ConflictError("受阻取消不得处理运行中审批实例".to_string()));
    }
    let current = db
        .bpm_workflow()
        .find_current_execution(&instance_id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例缺少当前受阻执行".to_string()))?;
    if current.process_instance_id != instance_id
        || instance.current_node_execution_id.as_ref()
            != Some(&ApprovalNodeExecutionId::new(&current.base.id))
        || current.status != ApprovalNodeExecutionStatus::Blocked
    {
        return Err(Error::ConflictError("受阻审批当前执行引用不一致".to_string()));
    }
    ensure_expected_version("审批执行", command.expected_execution_version, current.base.version)?;
    let execution_id = ApprovalNodeExecutionId::new(&current.base.id);
    let open_tasks = db.work_items().open_approval_tasks_for_execution(&execution_id, session).await?;
    task_policy
        .ensure_open_task_count(open_tasks.len())
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    validate_cancel_task_version_with_executor(db, &execution_id, command.expected_task_version, session)
        .await?;
    let spec = adapter_spec_of(document_type)?;
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&instance.process_definition_id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例绑定的定义不存在".to_string()))?;
    match (instance.blocker_code, current.blocker_code) {
        (Some(instance_blocker), Some(execution_blocker)) if instance_blocker == execution_blocker => {},
        _ => return Err(Error::ConflictError("受阻实例与当前执行 blocker 不一致".to_string())),
    };
    let eligibility = converge_eligibility(
        current.assignee_participant_id.as_str(),
        &current.assignee_name_snapshot,
        None,
    )?;
    let now = Instant::now();
    let prepared = prepare_cancel(CancelExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: eligibility.clone(),
            next_eligibility: eligibility,
            receipt: None,
            idempotency_key: idempotency_key.clone(),
            now: Timestamp::from_utc(now.as_utc()),
        },
        instance,
        current: current.clone(),
        subject_version: snapshot.subject_version,
        expected_instance_version: command.expected_instance_version,
        expected_execution_version: command.expected_execution_version,
        expected_task_version: command.expected_task_version,
        reason: command.reason.clone(),
        actor: ParticipantId::new(actor.id())
            .map_err(|_| Error::ValidationError("取消人引用无效".to_string()))?,
        close_open_task: false,
        blocked_port: true,
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
    })?;
    let PreparedExecution::Apply(writes) = prepared else {
        return Err(Error::Internal("新受阻取消命令不得进入幂等回放分支".to_string()));
    };
    let writes = *writes;
    let action_context = ApprovalActionContext::for_blocked_cancel(BlockedCancelActionParams {
        approval_process_instance_id: command.approval_process_instance_id.clone(),
        approval_node_execution_id: current.base.id.clone(),
        work_item_id: None,
        business_object_type: document_type.as_str().to_string(),
        business_object_id: snapshot.business_object_id.clone(),
        subject_version: snapshot.subject_version.to_string(),
        actor_id: actor.id().to_string(),
        reason: command.reason.clone(),
        idempotency_key: command.idempotency_key.clone(),
    })?;
    let audit = PreparedWorkflowAudit::resource_with_message(
        actor.clone(),
        "approval.cancel_blocked",
        "approval_process_instance",
        command.approval_process_instance_id.clone(),
        Some(format!("execution={} reason={}", current.base.id, command.reason)),
    )?;

    db.bpm_workflow()
        .insert_command_receipt(&writes.receipt, session)
        .await
        .map_err(map_receipt_first_write_error)?;
    action_port.execute(spec.cancel_action, &action_context, actor, session).await?;
    db.bpm_workflow()
        .persist_cancelled_runtime_after_receipt(&writes.instance, &writes.updated_executions, session)
        .await?;
    persist_cancel_notifications(
        db,
        &writes,
        CancelNotificationFacts {
            submitted_by: &snapshot.payload.submitted_by,
            actor_id: actor.id(),
            document_type_label: document_type.label(),
            document_no: &snapshot.payload.document_no,
            current_node_name: &current.node_name,
            current_approver_display_name: &current.assignee_name_snapshot,
        },
        now,
        session,
    )
    .await?;
    audit_port.persist(&audit, session).await?;
    Ok(map_command_view(&writes.instance, None, None, Some("DRAFT".to_string()), None, writes.commit, false))
}

/// 事务内重验受阻取消账号、动作权限、类型级运行管理、对象读取与 DataScopeFact。
async fn ensure_cancel_blocked_authorized(
    _db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn ApprovalObjectReadPort,
    actor: &AuditActor,
    document_type: DocumentType,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !approval_actor_is_active_with_executor(rbac, actor, executor).await? {
        return Err(hidden_forbidden());
    }
    let action_scope = approval_cancel_blocked_scope_with_executor(rbac, actor, executor).await?;
    let read_scope = approval_document_read_scope_with_executor(rbac, actor, document_type, executor).await?;
    let visibility = definition_management_visibility_with_executor(rbac, actor, executor).await?;
    let spec = adapter_spec_of(document_type)?;
    let context = BindingRevalidationContext {
        order_source: None,
        customer_id: None,
        business_org_unit_id: None,
        scope_owner_user_id: None,
        organization_id: snapshot.payload.responsible_org_id.clone(),
        creator_id: snapshot.payload.submitted_by.clone(),
    };
    let read_scope_covers = !read_scope.is_empty()
        && read_scope.covers_object(
            &rbac
                .approval_scope_object(snapshot.document_type, &snapshot.business_object_id, executor)
                .await?,
        );
    let object_readable =
        runtime_object_readable(&spec, &context, actor.id(), read_scope_covers, object_read)?;
    if action_scope.is_empty()
        || !action_scope.covers_object(
            &rbac
                .approval_scope_object(snapshot.document_type, &snapshot.business_object_id, executor)
                .await?,
        )
        || !read_scope_covers
        || !visibility.runtime_admin_types().contains(&document_type)
        || !object_readable
    {
        return Err(hidden_forbidden());
    }
    Ok(())
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
