//! 审批启动的共有回放与写入阶段。入口保留授权、领域提交、展示快照和恢复策略。

mod replay;

use async_trait::async_trait;
use bpm::engine::TaskIntent;
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::ApprovalNodeExecution;
use erp_core::common::time::Instant;
use erp_core::ids::{ApprovalSubjectSnapshotId, WorkItemId};
use erp_workflow::entity::approval_integration::{ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload};
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority};
use erp_workflow::repository::bpm::ApprovalInstanceListProjection;
use erp_workflow::service::approval::execution::PlannedWrites;
use erp_workflow::{BpmExt, WorkItemExt};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;
pub(crate) use replay::{StartReplayErrors, replay_start};

use crate::adapters::workflow::approver_directory::revalidate_approvers;
use crate::adapters::{freeze_approval_materials, persist_approval_snapshot};
use crate::{Error, Result};

/// 入口任务沿用业务提交时确定的责任和时间。
pub(crate) struct StartTaskContext<'a> {
    pub document_type: DocumentType,
    pub owner_role: &'a str,
    pub organization_id: &'a str,
    pub now: Instant,
}

/// 复验人员并写入运行态，随后构造尚未冻结附件的提交快照。
///
/// # 参数
/// * `db` - 业务与审批数据库。
/// * `writes` - 已写入启动收据对应的计划。
/// * `document_type` - 入口确定的业务单据类型。
/// * `payload` - 已冻结的业务提交载荷。
/// * `now` - 调用方状态变更时间。
/// * `missing_entry` - 入口既有的缺少执行错误。
/// * `executor` - 业务提交使用的同一执行器。
///
/// # 返回
/// 返回可由入口补充专有展示资料的快照。
///
/// # 错误
/// 缺少入口执行、人员复验、运行态写入或快照构造失败时立即停止。
pub(crate) async fn persist_start_runtime(
    db: &Database,
    writes: &PlannedWrites,
    document_type: DocumentType,
    payload: &ApprovalSubjectSnapshotPayload,
    now: Instant,
    missing_entry: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalSubjectSnapshot> {
    let first =
        writes.created_executions.first().ok_or_else(|| Error::Internal(missing_entry.to_string()))?;
    apply_runtime_steps(&MongoStartRuntime { db, writes, first, now }, executor).await?;
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
    writes: &'a PlannedWrites,
    first: &'a ApprovalNodeExecution,
    now: Instant,
}

#[async_trait]
impl StartRuntimeSteps for MongoStartRuntime<'_> {
    async fn revalidate(&self, executor: &mut dyn Executor) -> Result<()> {
        revalidate_approvers(self.db, self.writes, executor).await
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

/// 冻结附件并保存提交快照，沿用入口的写入顺序。
///
/// # 参数
/// * `db` - 业务与审批数据库。
/// * `snapshot` - 入口已补充专有展示资料的快照。
/// * `executor` - 同一业务提交执行器。
///
/// # 返回
/// 冻结资料和快照保存完成时返回空值。
///
/// # 错误
/// 附件冻结或快照写入失败时立即返回，调用方不得继续创建任务。
pub(crate) async fn persist_start_snapshot(
    db: &Database,
    snapshot: ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<()> {
    let snapshot = freeze_approval_materials(db, snapshot, executor).await?;
    persist_approval_snapshot(db, &snapshot, executor).await
}

/// 依计划顺序创建入口人工任务，并返回首个任务回执。
///
/// # 参数
/// * `db` - 工作项数据库。
/// * `writes` - 启动计划；非人工任务意图跳过。
/// * `context` - 入口确定的单据类型、责任与时间。
/// * `executor` - 同一业务提交执行器。
///
/// # 返回
/// 返回首个任务的 ID 与版本；无人工任务时返回 `None`。
///
/// # 错误
/// 任一任务构造或保存失败时停止，不继续后续任务。
pub(crate) async fn persist_start_tasks(
    db: &Database,
    writes: &PlannedWrites,
    context: StartTaskContext<'_>,
    executor: &mut dyn Executor,
) -> Result<Option<(String, u64)>> {
    let mut first_task = None;
    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested { execution_id, assignee, .. } = intent else {
            continue;
        };
        let item = WorkItem::new_document_approval(
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
        if first_task.is_none() {
            first_task = Some((item.base.id.clone(), item.base.version));
        }
        db.work_items().create(&item, executor).await?;
    }
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
