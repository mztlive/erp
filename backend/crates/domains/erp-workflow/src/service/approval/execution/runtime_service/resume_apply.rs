//! 原审批人恢复入口、当前授权回放与单事务提交。

use erp_permissions::internal as internal_permissions;
mod persist;
mod prepare;

use application_core::AuditActor;
use bpm::engine::{CommitRequired, Eligibility, TaskIntent};
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::IdempotencyKey;
use bpm::model::types::ApprovalProcessInstanceStatus;
use erp_identity::{AccessTarget, AuthorizationService, SharedRbacService};
use mongodb::Database;
use persist::persist_resume_writes;
use persistence_core::{Executor, Transactional};
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
    hidden_not_found, persisted_command_view_with_executor, recover_by_replay,
};
use crate::error::{Error, ErrorCode, Result};
use crate::ports::WorkflowAuthorizationPort;
use crate::repository::BpmExt;
use crate::service::approval::ApprovalResumeCommand;

impl<A: WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 原审批人恢复资格后，对可恢复的受阻执行提交恢复命令。新命令按实例、当前执行、绑定及历史任务的期望版本，在同一事务重验管理者与原审批人。
    ///
    /// 核验通过后依次保存收据、实例、执行、任务、通知 outbox 与审计；同命令回放仍重验当前授权。主体、快照、版本、授权或持久化错误均拒绝。
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

        let service = self.clone();
        let transaction_actor = actor.clone();
        let transaction_identity = identity.clone();
        let view = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service
                        .resume_stage(
                            &transaction_actor,
                            &command,
                            idempotency_key,
                            &transaction_identity,
                            executor,
                        )
                        .await
                })
            })
            .await;

        commit_or_recover(
            move || async move { view },
            |error| self.recover_resume_after_competing_commit(actor, instance_id, identity, error),
        )
        .await
    }

    // 在入口事务内依次判断授权回放、准备新恢复及持久化，不跨快照拼装命令结果。
    async fn resume_stage(
        &self,
        actor: &AuditActor,
        command: &ApprovalResumeCommand,
        idempotency_key: IdempotencyKey,
        identity: &PreparedCommandIdentity,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalCommandView> {
        let instance_id = &command.approval_process_instance_id;
        if let Some(view) =
            replay_resume_apply(&self.authorization, &self.db, actor, instance_id, identity, executor).await?
        {
            return Ok(view);
        }

        let Some(prepared) = self.prepare_resume_runtime(actor, command, idempotency_key, executor).await?
        else {
            return self.persisted_resume_view(actor, instance_id, executor).await;
        };
        apply_resume(self, actor, &prepared, executor).await
    }

    // 提交竞争或结果未知时，在新只读快照用当前授权和原命令查证收据，不重新执行恢复写入。
    async fn prove_resume_commit(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        identity: &PreparedCommandIdentity,
    ) -> Result<Option<ApprovalCommandView>> {
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let actor = actor.clone();
        let instance_id = instance_id.to_string();
        let identity = identity.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    replay_resume_apply(&authorization, &db, &actor, &instance_id, &identity, executor).await
                })
            })
            .await
    }

    // 恢复遇到唯一键竞争、瞬态事务失败或提交结果未知时，在新会话有限回读胜者，不使用失败会话中的事实。
    async fn recover_resume_after_competing_commit(
        &self,
        actor: &AuditActor,
        instance_id: String,
        identity: PreparedCommandIdentity,
        original_error: Error,
    ) -> Result<ApprovalCommandView> {
        recover_by_replay(original_error, || self.prove_resume_commit(actor, &instance_id, &identity)).await
    }

    // 恢复已执行且不需要再写时，仍在入口事务完成当前授权后再交付原恢复结果。
    async fn persisted_resume_view(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalCommandView> {
        self.authorization
            .require(
                actor,
                &internal_permissions::APPROVAL_RECOVER,
                AccessTarget::record(instance_id)?,
                executor,
            )
            .await?;
        persisted_command_view_with_executor(&self.db, instance_id, CommitRequired::Proceed, true, executor)
            .await
    }

    // 恢复准备时检查当前 blocker 允许请求动作；实例不存在使用隐藏结果，状态不接受动作时返回冲突。
    async fn require_recovery_action(
        &self,
        instance_id: &str,
        wanted: RuntimeRecoveryAction,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), executor)
            .await?
            .ok_or_else(hidden_not_found)?;
        let blocked = instance.status == ApprovalProcessInstanceStatus::Blocked;
        if recovery_options_for(blocked, instance.blocker_code).contains(&wanted) {
            return Ok(());
        }
        Err(Error::ConflictError("当前受阻原因不支持此恢复操作，请查看可选处理办法".to_string()))
    }
}

// 恢复任务写入、授权重验和最终命令视图沿用同一执行器，不换事务提交部分结果。
async fn apply_resume(
    service: &ApprovalRuntimeService<impl WorkflowAuthorizationPort>,
    actor: &AuditActor,
    prepared: &PreparedResume,
    executor: &mut dyn Executor,
) -> Result<ApprovalCommandView> {
    service
        .authorization
        .require(
            actor,
            &internal_permissions::APPROVAL_RECOVER,
            AccessTarget::record(prepared.snapshot.approval_process_instance_id.as_ref())?,
            executor,
        )
        .await?;
    for intent in &prepared.writes.create_tasks {
        let TaskIntent::HumanTaskRequested { assignee, .. } = intent else {
            continue;
        };
        revalidate_resume_with_executor(
            &service.authorization,
            &service.db,
            &service.identity,
            &service.auth,
            RevalidateDecisionApproverInput {
                assignee_id: assignee.as_str(),
                assignee_name: &prepared.assignee_name,
                authenticated_actor: None,
                snapshot: &prepared.snapshot,
                spec: &prepared.spec,
                separation_policy: process_required_separation_policy(prepared.snapshot.document_type)?,
            },
            executor,
        )
        .await?;
    }

    persist_resume_writes(&service.db, prepared, service.audit.as_ref(), executor).await?;
    service.auth.append_assignment_grants(&prepared.writes, executor).await?;

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

// 新任务写入之前，在已授权的原恢复事务里重验要重新指派的原审批人。
async fn revalidate_resume_with_executor(
    authorization: &AuthorizationService,
    db: &Database,
    identity_service: &SharedRbacService,
    rbac: &impl WorkflowAuthorizationPort,
    input: RevalidateDecisionApproverInput<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let eligibility =
        revalidate_decision_approver(authorization, db, identity_service, rbac, input, executor).await?;
    ensure_resume_approver_recovered(&eligibility)
}

// 原审批人恢复回放先按当前账号与责任组织授权，再允许读取和比较收据。
//
// 触发 panic 的条件：
// 已加载收据被分类为 `Fresh` 时 `unreachable!`，表示分类器与查询结果矛盾。
async fn replay_resume_apply(
    authorization: &AuthorizationService,
    db: &Database,
    actor: &AuditActor,
    instance_id: &str,
    identity: &PreparedCommandIdentity,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandView>> {
    authorization
        .require(actor, &internal_permissions::APPROVAL_RECOVER, AccessTarget::record(instance_id)?, executor)
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

// 引擎恢复只接收已重新满足全部资格条件的原审批人；仍受阻时不生成替代任务。
fn ensure_resume_approver_recovered(eligibility: &Eligibility) -> Result<()> {
    if eligibility.blocked_code().is_some() {
        return Err(Error::from_approval_code(ErrorCode::ApprovalCurrentApproverNotRecovered));
    }
    Ok(())
}
