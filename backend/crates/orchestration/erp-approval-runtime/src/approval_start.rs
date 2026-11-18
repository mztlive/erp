//! 审批启动的共有回放与写入阶段。入口保留授权、领域提交、展示快照和恢复策略。

use std::sync::Arc;

mod replay;

use async_trait::async_trait;
use bpm::engine::TaskIntent;
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::ApprovalNodeExecution;
use erp_core::common::time::Instant;
use erp_core::ids::{ApprovalSubjectSnapshotId, WorkItemId};
use erp_identity::{AuthorizationService, SharedRbacService};
use erp_read_models::workbench::freeze_approval_materials;
use erp_workflow::entity::approval_integration::{
    ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload, resolve_runtime_document_type,
};
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority};
use erp_workflow::repository::ApprovalSubjectSnapshotRepositoryExt;
use erp_workflow::repository::bpm::ApprovalInstanceListProjection;
use erp_workflow::service::approval::copies::persist_started_copy;
use erp_workflow::service::approval::execution::PlannedWrites;
use erp_workflow::{ApprovalIntegrationExt, BpmExt, WorkItemExt, WorkflowAuthorizationPort};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;
pub use replay::{StartReplayErrors, replay_start};

use crate::approver_directory::revalidate_approvers;
use crate::{Error, Result};

/// 入口任务沿用业务提交时确定的责任和时间。
pub struct StartTaskContext<'a> {
    pub document_type: DocumentType,
    pub owner_role: &'a str,
    pub organization_id: &'a str,
    pub now: Instant,
}

/// 在业务提交的执行器上重验动态审批人，按已登记回执的计划写入审批实例、入口执行和列表投影。
/// 返回尚未冻结附件的快照，供业务入口补充专有展示资料。
/// 缺少入口执行时使用调用方提供的错误；人员复验、运行态写入或快照构造失败时停止。
pub async fn persist_start_runtime(
    db: &Database,
    identity: &Arc<AuthorizationService>,
    writes: &PlannedWrites,
    payload: &ApprovalSubjectSnapshotPayload,
    now: Instant,
    missing_entry: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalSubjectSnapshot> {
    let document_type =
        resolve_runtime_document_type(writes.instance.subject.subject_kind(), writes.instance.process_kind)?;
    let first =
        writes.created_executions.first().ok_or_else(|| Error::Internal(missing_entry.to_string()))?;

    apply_runtime_steps(&MongoStartRuntime { db, identity, writes, first, now }, executor).await?;

    ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new(next_id()),
        ApprovalProcessInstanceId::new(writes.instance.base.id.clone()),
        document_type,
        writes.instance.subject.subject_id(),
        writes.instance.subject_version,
        payload.clone(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

#[async_trait]
trait StartRuntimeSteps: Sync {
    async fn revalidate(&self, executor: &mut dyn Executor) -> Result<()>;
    async fn create_runtime(&self, executor: &mut dyn Executor) -> Result<()>;
}

struct MongoStartRuntime<'a> {
    db: &'a Database,
    identity: &'a Arc<AuthorizationService>,
    writes: &'a PlannedWrites,
    first: &'a ApprovalNodeExecution,
    now: Instant,
}

#[async_trait]
impl StartRuntimeSteps for MongoStartRuntime<'_> {
    async fn revalidate(&self, executor: &mut dyn Executor) -> Result<()> {
        revalidate_approvers(self.db, self.identity, self.writes, executor).await
    }

    async fn create_runtime(&self, executor: &mut dyn Executor) -> Result<()> {
        self.db
            .bpm_workflow()
            .create_bpm_runtime_after_receipt(
                &self.writes.instance,
                &self.writes.created_assignees,
                self.first,
                &ApprovalInstanceListProjection::from_entry_execution(self.first, self.now),
                executor,
            )
            .await?;
        Ok(())
    }
}

async fn apply_runtime_steps(steps: &impl StartRuntimeSteps, executor: &mut dyn Executor) -> Result<()> {
    steps.revalidate(executor).await?;
    steps.create_runtime(executor).await
}

/// 在原提交事务内复验附件转授资格并冻结附件与重提展示资料。
/// 先保存不可变快照，再保存启动抄送，供后续任务引用同一份提交资料；任一步失败都不得继续创建任务。
pub async fn persist_start_snapshot<A: WorkflowAuthorizationPort>(
    identity_service: &Arc<AuthorizationService>,
    rbac: &SharedRbacService,
    db: &Database,
    snapshot: ApprovalSubjectSnapshot,
    auth: &A,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut snapshot = freeze_approval_materials(identity_service, db, auth, snapshot, executor).await?;
    snapshot.freeze_resubmission_display()?;

    db.approval_subject_snapshots().create_immutable_snapshot(&snapshot, executor).await?;
    persist_started_copy(rbac, db, &snapshot, executor).await?;
    Ok(())
}

/// 从同事务已保存的快照继承审批关联，按计划创建人工入口任务，再追加对应的订单读取授权事件。
/// 跟随计划顺序返回首项任务的 ID 和版本；无人工任务返回 `None`，任务构造或保存失败即停止。
pub async fn persist_start_tasks(
    db: &Database,
    writes: &PlannedWrites,
    context: StartTaskContext<'_>,
    executor: &mut dyn Executor,
) -> Result<Option<(String, u64)>> {
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&writes.instance.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("找不到该审批任务提交时的单据资料，请联系管理员核对".into()))?;

    let mut first_task = None;
    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested { execution_id, assignee, .. } = intent else {
            continue;
        };
        let mut item = WorkItem::new_document_approval(
            WorkItemId::new(next_id()),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: context.document_type.as_str().to_string(),
                business_object_id: writes.instance.subject.subject_id().to_string(),
                subject_version: writes.instance.subject_version.to_string(),
                owner_role: context.owner_role.to_string(),
                owner_organization_id: context.organization_id.to_string(),
                owner_user_id: assignee.as_str().to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            context.now,
        )?;
        item.inherit_approval_links(&snapshot)?;
        if first_task.is_none() {
            first_task = Some((item.base.id.clone(), item.base.version));
        }
        db.work_items().create(&item, executor).await?;
    }

    crate::resource_grants::append_assignments(db, writes, executor).await?;
    Ok(first_task)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use persistence_core::NoTransaction;

    use super::*;

    struct RecordingSteps {
        calls: Mutex<Vec<(&'static str, usize)>>,
        fail_at: Option<&'static str>,
    }

    impl RecordingSteps {
        fn record(&self, phase: &'static str, executor: &mut dyn Executor) -> Result<()> {
            self.calls.lock().unwrap().push((phase, executor as *mut dyn Executor as *mut () as usize));
            if self.fail_at == Some(phase) {
                return Err(Error::ConflictError(phase.to_string()));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl StartRuntimeSteps for RecordingSteps {
        async fn revalidate(&self, executor: &mut dyn Executor) -> Result<()> {
            self.record("revalidate", executor)
        }
        async fn create_runtime(&self, executor: &mut dyn Executor) -> Result<()> {
            self.record("runtime", executor)
        }
    }

    #[tokio::test]
    async fn runtime_revalidates_before_write_with_the_same_executor() {
        let steps = RecordingSteps { calls: Mutex::new(Vec::new()), fail_at: None };
        let mut executor = NoTransaction;
        let identity = &mut executor as *mut NoTransaction as usize;
        apply_runtime_steps(&steps, &mut executor).await.unwrap();
        assert_eq!(*steps.calls.lock().unwrap(), vec![("revalidate", identity), ("runtime", identity)]);
    }

    #[tokio::test]
    async fn runtime_stops_on_revalidation_failure() {
        let steps = RecordingSteps { calls: Mutex::new(Vec::new()), fail_at: Some("revalidate") };
        let error = apply_runtime_steps(&steps, &mut NoTransaction).await.unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "revalidate"));
        assert_eq!(
            steps.calls.lock().unwrap().iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            vec!["revalidate"]
        );
    }

    #[tokio::test]
    async fn runtime_preserves_write_failure() {
        let steps = RecordingSteps { calls: Mutex::new(Vec::new()), fail_at: Some("runtime") };
        let error = apply_runtime_steps(&steps, &mut NoTransaction).await.unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "runtime"));
        assert_eq!(steps.calls.lock().unwrap().len(), 2);
    }
}
