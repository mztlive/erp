//! 原审批人恢复入口、当前授权回放与单事务提交。

mod persist;
mod prepare;

use std::future::Future;
use std::sync::Arc;

use application_core::AuditActor;
use bpm::engine::{CommitRequired, Eligibility};
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::types::ApprovalProcessInstanceStatus;
use mongodb::Database;
use persist::persist_resume_writes;
use persistence_core::{Executor, NoTransaction, Transactional};
use prepare::PreparedResume;

use super::super::idempotency::{
    PreparedCommandIdentity, ReceiptBranch, normalize_idempotency_key, payload_conflict_error,
    resume_identity,
};
use super::super::runtime_query::{RuntimeRecoveryAction, recovery_options_for};
use super::super::view::{ApprovalCommandView, map_command_view};
use super::query::first_open_task;
use super::read_auth::{
    RevalidateDecisionApproverInput, process_required_separation_policy, revalidate_decision_approver,
};
use super::{
    ApprovalRuntimeService, commit_or_recover, ensure_command_actor, find_receipt_for_identity,
    hidden_not_found, load_exact_runtime_snapshot, persisted_command_view_with_executor, recover_by_replay,
};
use crate::error::{Error, ErrorCode, Result};
use crate::ports::{ApprovalObjectReadPort, WorkflowAuditPort, WorkflowAuthorizationPort};
use crate::repository::BpmExt;
use crate::service::approval::{ApprovalResumeCommand, require_approval_management_with_executor};

impl<A: WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 在原审批人重新合格后恢复当前受阻执行。
    ///
    /// # 参数
    /// * `actor` - 当前认证且具备恢复权限的审计主体
    /// * `command` - 实例、执行、审批人和已关闭任务的期望版本及幂等键
    ///
    /// # 返回
    /// 返回恢复后的审批命令视图；幂等回放返回已持久化视图。
    ///
    /// # 错误
    /// 主体不一致、实例缺失、权限不足、版本冲突、快照不一致或事务写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 回放重验当前授权；新命令在同一事务执行器内重验管理者和原审批人，
    /// 再按收据、实例、执行、任务、通知 outbox 和审计的顺序写入。
    pub async fn resume_current_approver(
        &self,
        actor: &AuditActor,
        command: ApprovalResumeCommand,
    ) -> Result<ApprovalCommandView> {
        ensure_command_actor(actor, &command.actor_id)?;
        let instance_id = command.approval_process_instance_id.clone();
        let idempotency_key = normalize_idempotency_key(&command.idempotency_key)?;
        let identity = resume_identity(
            idempotency_key.clone(),
            &instance_id,
            command.expected_instance_version,
            command.expected_execution_version,
            command.expected_assignment_version,
            command.expected_closed_task_version,
            actor.id(),
        )?;
        if let Some(view) = self.replay_resume(actor, &instance_id, &identity).await? {
            return Ok(view);
        }
        let Some(prepared) = self.prepare_resume_runtime(actor, &command, idempotency_key).await? else {
            return self.persisted_command_view(&instance_id, CommitRequired::Proceed, true).await;
        };
        let view = self.commit_resume(actor, prepared).await;
        commit_or_recover(
            move || async move { view },
            |error| self.recover_resume_after_competing_commit(actor, instance_id, identity, error),
        )
        .await
    }

    /// 冻结端口与计划的所有权，保证事务回调及其 future 可跨线程发送。
    fn commit_resume(
        &self,
        actor: &AuditActor,
        prepared: PreparedResume,
    ) -> impl Future<Output = Result<ApprovalCommandView>> + Send {
        let db = self.db.clone();
        let client = self.db.client().clone();
        let rbac = self.auth.clone();
        let object_read = Arc::clone(&self.object_read);
        let audit_port = Arc::clone(&self.audit);
        let actor = actor.clone();
        async move {
            client
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        apply_resume(
                            &db,
                            &rbac,
                            object_read.as_ref(),
                            audit_port.as_ref(),
                            &actor,
                            &prepared,
                            executor,
                        )
                        .await
                    })
                })
                .await
        }
    }

    /// 在独立事务快照内按当前权限回放原审批人恢复结果。
    async fn replay_resume(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        identity: &PreparedCommandIdentity,
    ) -> Result<Option<ApprovalCommandView>> {
        let db = self.db.clone();
        let rbac = self.auth.clone();
        let actor = actor.clone();
        let instance_id = instance_id.to_string();
        let identity = identity.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    replay_resume_apply(&db, &rbac, &actor, &instance_id, &identity, executor).await
                })
            })
            .await
    }

    /// 唯一键竞争、瞬态事务错误或提交结果未知后，以新会话有限回读胜者。
    async fn recover_resume_after_competing_commit(
        &self,
        actor: &AuditActor,
        instance_id: String,
        identity: PreparedCommandIdentity,
        original_error: Error,
    ) -> Result<ApprovalCommandView> {
        recover_by_replay(original_error, || self.replay_resume(actor, &instance_id, &identity)).await
    }

    async fn persisted_command_view(
        &self,
        instance_id: &str,
        commit: CommitRequired,
        replay: bool,
    ) -> Result<ApprovalCommandView> {
        persisted_command_view_with_executor(&self.db, instance_id, commit, replay, &mut NoTransaction).await
    }

    /// 实例缺失时隐藏存在性；当前 blocker 不允许该动作时返回冲突。
    async fn require_recovery_action(&self, instance_id: &str, wanted: RuntimeRecoveryAction) -> Result<()> {
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let blocked = instance.status == ApprovalProcessInstanceStatus::Blocked;
        if recovery_options_for(blocked, instance.blocker_code).contains(&wanted) {
            return Ok(());
        }
        Err(Error::ConflictError("当前 blocker 不允许该恢复动作".to_string()))
    }
}

/// 授权重验、全部写入及视图组装共用调用方的同一事务执行器。
async fn apply_resume(
    db: &Database,
    rbac: &impl WorkflowAuthorizationPort,
    object_read: &dyn ApprovalObjectReadPort,
    audit_port: &dyn WorkflowAuditPort,
    actor: &AuditActor,
    prepared: &PreparedResume,
    executor: &mut dyn Executor,
) -> Result<ApprovalCommandView> {
    revalidate_resume_with_executor(
        db,
        rbac,
        object_read,
        actor,
        RevalidateDecisionApproverInput {
            assignee_id: &prepared.assignee_id,
            assignee_name: &prepared.assignee_name,
            authenticated_actor: None,
            snapshot: &prepared.snapshot,
            spec: &prepared.spec,
            separation_policy: process_required_separation_policy(prepared.snapshot.document_type)?,
        },
        executor,
    )
    .await?;
    persist_resume_writes(db, prepared, audit_port, executor).await?;
    Ok(map_command_view(
        &prepared.writes.instance,
        prepared.writes.created_executions.last(),
        None,
        None,
        first_open_task(&prepared.writes, &prepared.new_task_ids),
        prepared.writes.commit,
        false,
    ))
}

/// 在恢复写入的同一事务中重验管理者及原审批人，所有单据类型一致执行。
async fn revalidate_resume_with_executor(
    db: &Database,
    rbac: &impl WorkflowAuthorizationPort,
    object_read: &dyn ApprovalObjectReadPort,
    actor: &AuditActor,
    input: RevalidateDecisionApproverInput<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    require_approval_management_with_executor(
        rbac,
        actor,
        "approval_instance:resume",
        input.snapshot.document_type,
        &input.snapshot.business_object_id,
        executor,
    )
    .await?;
    let eligibility = revalidate_decision_approver(db, rbac, object_read, input, executor).await?;
    ensure_resume_approver_recovered(&eligibility)
}

/// 原审批人恢复回放先按当前账号与责任组织授权，再允许读取和比较收据。
///
/// # Panics
/// 已加载收据被分类为 `Fresh` 时 `unreachable!`，表示分类器与查询结果矛盾。
async fn replay_resume_apply(
    db: &Database,
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    instance_id: &str,
    identity: &PreparedCommandIdentity,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandView>> {
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    let (_, snapshot) = load_exact_runtime_snapshot(db, &instance, executor, true).await?;
    require_approval_management_with_executor(
        rbac,
        actor,
        "approval_instance:resume",
        snapshot.document_type,
        &snapshot.business_object_id,
        executor,
    )
    .await?;
    let Some(receipt) = find_receipt_for_identity(db, identity, executor).await? else {
        return Ok(None);
    };
    if receipt.result_ref != instance_id {
        return Err(Error::ConflictError("恢复收据结果引用与实例不一致".to_string()));
    }
    match identity.classify(Some(&receipt)) {
        ReceiptBranch::SamePayload(_) => {},
        ReceiptBranch::Fresh => unreachable!("receipt was loaded"),
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error()),
    }
    persisted_command_view_with_executor(db, instance_id, CommitRequired::Proceed, true, executor)
        .await
        .map(Some)
}

/// 恢复端口只接受已重新满足全部资格的原审批人。
fn ensure_resume_approver_recovered(eligibility: &Eligibility) -> Result<()> {
    if eligibility.blocked_code().is_some() {
        return Err(Error::from_approval_code(ErrorCode::ApprovalCurrentApproverNotRecovered));
    }
    Ok(())
}
