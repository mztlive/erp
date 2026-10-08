//! 恢复命令的版本读取、当前资格重验与计划准备。

use application_core::AuditActor;
use bpm::engine::{DefinitionGraph, Eligibility};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{
    ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance, IdempotencyKey, Timestamp,
};
use erp_core::common::time::Instant;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};

use super::super::super::apply_plan::PlannedWrites;
use super::super::super::resume::prepare_resume;
use super::super::super::runtime_query::RuntimeRecoveryAction;
use super::super::super::{ExecutionCommandInput, PreparedExecution, ResumeExecutionInput};
use super::super::query::list_projection_from_writes;
use super::super::read_auth::{
    RevalidateDecisionApproverInput, process_required_separation_policy, revalidate_decision_approver,
};
use super::super::{ApprovalRuntimeService, ensure_expected_version, hidden_not_found};
use super::ensure_resume_approver_recovered;
use crate::entity::approval_integration::{ApprovalSubjectSnapshot, document_type_from_subject_kind};
use crate::entity::document_registry::DocumentType;
use crate::entity::work_item::WorkItemStatus;
use crate::error::{Error, Result};
use crate::ports::{PreparedWorkflowAudit, WorkflowAuthorizationPort};
use crate::repository::bpm::ApprovalInstanceListProjection;
use crate::repository::prelude::*;
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use crate::service::approval::business_adapter::{ApprovalAdapterSpec, adapter_spec_of};
use crate::service::approval::{ApprovalResumeCommand, require_approval_management_with_executor};

/// 旧关闭任务的只读并发守卫；恢复不会修改原任务。
pub(super) struct ClosedTaskGuard {
    pub(super) task_id: String,
    pub(super) execution_id: ApprovalNodeExecutionId,
    pub(super) version: u64,
}

/// 已读取并核对版本的原执行、冻结主体与审批人绑定。
struct ResumeRuntime {
    instance: ApprovalProcessInstance,
    current: ApprovalNodeExecution,
    assignee: ApprovalInstanceAssignee,
    closed_task_guard: Option<ClosedTaskGuard>,
    snapshot: ApprovalSubjectSnapshot,
    document_type: DocumentType,
    spec: ApprovalAdapterSpec,
}

/// 跨越事务提交边界的恢复计划及冻结元数据。
pub(super) struct PreparedResume {
    pub(super) writes: PlannedWrites,
    pub(super) new_task_ids: Vec<String>,
    pub(super) list_projection: ApprovalInstanceListProjection,
    pub(super) audit: PreparedWorkflowAudit,
    pub(super) now: Instant,
    pub(super) ended_execution_id: String,
    pub(super) expected_instance_version: u64,
    pub(super) expected_execution_version: u64,
    pub(super) closed_task_guard: Option<ClosedTaskGuard>,
    pub(super) snapshot: ApprovalSubjectSnapshot,
    pub(super) spec: ApprovalAdapterSpec,
    pub(super) assignee_id: String,
    pub(super) assignee_name: String,
    pub(super) subject_version: String,
    pub(super) document_type_label: String,
}

impl<A: WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 读取原执行并按当前权限准备恢复；此阶段不写入正式事实。
    ///
    /// # 参数
    /// * `actor` - 已认证的恢复操作人
    /// * `command` - 客户端期望的实例、执行、绑定及关闭任务版本
    /// * `idempotency_key` - 已规范化的恢复幂等键
    ///
    /// # 返回
    /// 新命令返回冻结的写入计划；规划器要求回放时返回 `None`。
    ///
    /// # 错误
    /// 恢复动作、版本、冻结主体、管理资格、原审批人资格或定义图不合法时返回错误。
    pub(super) async fn prepare_resume_runtime(
        &self,
        actor: &AuditActor,
        command: &ApprovalResumeCommand,
        idempotency_key: IdempotencyKey,
    ) -> Result<Option<PreparedResume>> {
        let runtime = self.load_resume_runtime(actor, command).await?;
        let eligibility = self
            .revalidate_approver(
                runtime.assignee.current_assignee_participant_id.as_str(),
                &runtime.current.assignee_name_snapshot,
                &runtime.snapshot,
                &runtime.spec,
                &mut NoTransaction,
            )
            .await?;
        ensure_resume_approver_recovered(&eligibility)?;
        let graph = self
            .db
            .bpm_workflow()
            .load_definition_graph(&runtime.instance.process_definition_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例绑定的定义不存在".to_string()))?;
        let now = Instant::now();
        let prepared = plan_resume(actor, command, &runtime, graph, eligibility, idempotency_key, now)?;
        let PreparedExecution::Apply(writes) = prepared else {
            return Ok(None);
        };
        finish_prepared_resume(actor, command, runtime, *writes, now).map(Some)
    }

    /// 按原顺序校验恢复动作、版本、历史任务、冻结主体和管理者来源边界。
    async fn load_resume_runtime(
        &self,
        actor: &AuditActor,
        command: &ApprovalResumeCommand,
    ) -> Result<ResumeRuntime> {
        let instance_id = &command.approval_process_instance_id;
        self.require_recovery_action(instance_id, RuntimeRecoveryAction::ResumeCurrentApprover).await?;
        let (instance, current, assignee) = self.load_resume_execution(command).await?;
        let closed_task_guard = self
            .load_resume_task_guard(
                &ApprovalNodeExecutionId::new(current.base.id.clone()),
                command.expected_closed_task_version,
            )
            .await?;
        let (snapshot, document_type) = self.load_resume_subject(&instance, instance_id).await?;
        require_approval_management_with_executor(
            &self.auth,
            actor,
            "approval_instance:resume",
            document_type,
            &snapshot.business_object_id,
            &mut NoTransaction,
        )
        .await?;
        let spec = adapter_spec_of(document_type)?;
        Ok(ResumeRuntime { instance, current, assignee, closed_task_guard, snapshot, document_type, spec })
    }

    /// 分别读取实例、当前执行和原节点绑定，立即核对每项期望版本。
    async fn load_resume_execution(
        &self,
        command: &ApprovalResumeCommand,
    ) -> Result<(ApprovalProcessInstance, ApprovalNodeExecution, ApprovalInstanceAssignee)> {
        let instance_id = ApprovalProcessInstanceId::new(&command.approval_process_instance_id);
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&instance_id, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        ensure_expected_version("审批实例", command.expected_instance_version, instance.base.version)?;
        let current = self
            .db
            .bpm_workflow()
            .find_current_execution(&instance_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例缺少当前受阻执行".to_string()))?;
        ensure_expected_version("审批执行", command.expected_execution_version, current.base.version)?;
        let assignee = self
            .db
            .bpm_workflow()
            .find_assignee_for_node(&instance_id, &current.node_key, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("实例缺少当前节点审批人绑定".to_string()))?;
        ensure_expected_version("审批人绑定", command.expected_assignment_version, assignee.base.version)?;
        Ok((instance, current, assignee))
    }

    /// 冻结快照必须精确匹配原实例的类型、主体 ID 与提交版本。
    async fn load_resume_subject(
        &self,
        instance: &ApprovalProcessInstance,
        instance_id: &str,
    ) -> Result<(ApprovalSubjectSnapshot, DocumentType)> {
        let snapshot = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_id(instance_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例缺少冻结业务快照".to_string()))?;
        let document_type = document_type_from_subject_kind(instance.subject.subject_kind())
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        snapshot
            .ensure_matches_runtime_subject(
                document_type,
                instance.subject.subject_id(),
                instance.subject_version,
            )
            .map_err(|_| Error::ConflictError("审批实例与冻结业务快照不一致".to_string()))?;
        Ok((snapshot, document_type))
    }

    /// 账号启用、审批静态资格和岗位分离由现有领域授权入口统一判定。
    async fn revalidate_approver(
        &self,
        assignee_id: &str,
        assignee_name: &str,
        snapshot: &ApprovalSubjectSnapshot,
        spec: &ApprovalAdapterSpec,
        executor: &mut dyn Executor,
    ) -> Result<Eligibility> {
        revalidate_decision_approver(
            &self.db,
            &self.auth,
            self.object_read.as_ref(),
            RevalidateDecisionApproverInput {
                assignee_id,
                assignee_name,
                authenticated_actor: None,
                snapshot,
                spec,
                separation_policy: process_required_separation_policy(snapshot.document_type)?,
            },
            executor,
        )
        .await
    }

    /// 原任务必须唯一且已关闭；声明了版本却没有任务，或多任务时失败。
    async fn load_resume_task_guard(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        expected_closed_task_version: Option<u64>,
    ) -> Result<Option<ClosedTaskGuard>> {
        let tasks =
            self.db.work_items().approval_tasks_for_execution(execution_id, &mut NoTransaction).await?;
        if tasks.len() > 1 {
            return Err(Error::ConflictError("受阻执行关联多个历史审批任务".to_string()));
        }
        let Some(task) = tasks.into_iter().next() else {
            if expected_closed_task_version.is_some() {
                return Err(Error::ConflictError("调用方声明了关闭任务版本，但历史任务不存在".to_string()));
            }
            return Ok(None);
        };
        if task.status != WorkItemStatus::Closed {
            return Err(Error::ConflictError("人员恢复要求原审批任务已经关闭".to_string()));
        }
        if let Some(expected) = expected_closed_task_version {
            ensure_expected_version("已关闭审批任务", expected, task.base.version)?;
        }
        Ok(Some(ClosedTaskGuard {
            task_id: task.base.id,
            execution_id: execution_id.clone(),
            version: task.base.version,
        }))
    }
}

/// 委托现有 BPM 恢复规划器；此处只分配新身份和组合已验证输入。
fn plan_resume(
    actor: &AuditActor,
    command: &ApprovalResumeCommand,
    runtime: &ResumeRuntime,
    graph: DefinitionGraph,
    eligibility: Eligibility,
    idempotency_key: IdempotencyKey,
    now: Instant,
) -> Result<PreparedExecution> {
    prepare_resume(ResumeExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: eligibility.clone(),
            next_eligibility: eligibility,
            receipt: None,
            idempotency_key,
            now: Timestamp::from_utc(now.as_utc()),
        },
        instance: runtime.instance.clone(),
        current: runtime.current.clone(),
        assignee: runtime.assignee.clone(),
        expected_instance_version: command.expected_instance_version,
        expected_execution_version: command.expected_execution_version,
        expected_assignment_version: command.expected_assignment_version,
        expected_closed_task_version: command.expected_closed_task_version,
        next_execution_id: ApprovalNodeExecutionId::new(next_id()),
        next_execution_no: runtime.current.execution_no.saturating_add(1),
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        actor_id: actor.id().to_string(),
    })
}

/// 计划成功后分配任务身份并冻结投影、审计及事务重验事实。
fn finish_prepared_resume(
    actor: &AuditActor,
    command: &ApprovalResumeCommand,
    runtime: ResumeRuntime,
    writes: PlannedWrites,
    now: Instant,
) -> Result<PreparedResume> {
    let new_task_ids = writes.create_tasks.iter().map(|_| next_id()).collect();
    let list_projection = list_projection_from_writes(&writes, &runtime.current.base.id, None, now);
    let audit = PreparedWorkflowAudit::resource_with_message(
        actor.clone(),
        "approval.resume_current_approver",
        "approval_process_instance",
        command.approval_process_instance_id.clone(),
        Some(format!("execution={}", runtime.current.base.id)),
    )?;
    Ok(PreparedResume {
        writes,
        new_task_ids,
        list_projection,
        audit,
        now,
        expected_instance_version: command.expected_instance_version,
        expected_execution_version: command.expected_execution_version,
        subject_version: runtime.snapshot.subject_version.to_string(),
        document_type_label: runtime.document_type.label().to_string(),
        assignee_id: runtime.assignee.current_assignee_participant_id.as_str().to_string(),
        assignee_name: runtime.current.assignee_name_snapshot,
        ended_execution_id: runtime.current.base.id,
        closed_task_guard: runtime.closed_task_guard,
        snapshot: runtime.snapshot,
        spec: runtime.spec,
    })
}
