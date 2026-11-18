//! 终态驳回关闭精确实例全部执行下的遗留开放审批任务。

use async_trait::async_trait;
use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, ApprovalProcessInstanceStatus};
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::work_item::{ApprovalRuntimeTaskEnding, WorkItem};
use crate::repository::prelude::*;
use crate::repository::{BpmExt, WorkItemExt};
use crate::{Error, Result};

/// 在驳回原事务中关闭本实例各执行的所有剩余开放任务。
/// # 参数
/// 数据库、已驳回实例、决定人、时间和原事务执行器。
/// # 返回
/// 当前已完成任务保持不变，其余开放审批任务全部关闭。
/// # 错误
/// 实例非驳回终态、执行归属损坏、任务身份冲突或 CAS 失败时回滚。
pub(in crate::service::approval::execution::runtime_service) async fn close_rejected_tasks(
    db: &Database,
    instance: &ApprovalProcessInstance,
    actor_id: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    if instance.status != ApprovalProcessInstanceStatus::Rejected {
        return Err(Error::ConflictError("仅已驳回实例可以关闭残留审批任务".into()));
    }
    close_instance_tasks(&MongoRejectionTasks(db), &instance.typed_id(), actor_id, now, executor).await
}

/// 只暴露精确实例执行分页、按执行读取任务和 CAS 终结的事务端口。
#[async_trait]
trait RejectionTasks: Sync {
    async fn execution_page(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        cursor: Option<u32>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeExecution>>;
    async fn open_tasks(
        &self,
        execution_ids: &[ApprovalNodeExecutionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;
    async fn persist(&self, tasks: &[WorkItem], executor: &mut dyn Executor) -> Result<()>;
}

struct MongoRejectionTasks<'a>(&'a Database);

#[async_trait]
impl RejectionTasks for MongoRejectionTasks<'_> {
    async fn execution_page(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        cursor: Option<u32>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeExecution>> {
        Ok(self.0.bpm_workflow().list_execution_history(instance_id, cursor, 50, executor).await?)
    }

    async fn open_tasks(
        &self,
        execution_ids: &[ApprovalNodeExecutionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        Ok(self.0.work_items().open_approval_tasks_for_executions(execution_ids, executor).await?)
    }

    async fn persist(&self, tasks: &[WorkItem], executor: &mut dyn Executor) -> Result<()> {
        Ok(self.0.work_items().persist_ended_approval_tasks(tasks, executor).await?)
    }
}

/// 逐页处理完整实例历史，不能用单页上限截断残留任务。
async fn close_instance_tasks(
    store: &impl RejectionTasks,
    instance_id: &ApprovalProcessInstanceId,
    actor_id: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut cursor = None;
    loop {
        let executions = store.execution_page(instance_id, cursor, executor).await?;
        if executions.is_empty() {
            return Ok(());
        }
        let ids = page_execution_ids(&executions, instance_id, cursor)?;

        let tasks = store.open_tasks(&ids, executor).await?;
        let ended = close_page_tasks(tasks, &ids, actor_id, now)?;
        if !ended.is_empty() {
            store.persist(&ended, executor).await?;
        }

        cursor = executions.last().map(|execution| execution.execution_no);
    }
}

/// 证明本页执行归属与游标严格递增，防止跨实例关闭或坏数据无限分页。
fn page_execution_ids(
    executions: &[ApprovalNodeExecution],
    instance_id: &ApprovalProcessInstanceId,
    cursor: Option<u32>,
) -> Result<Vec<ApprovalNodeExecutionId>> {
    let mut previous = cursor.unwrap_or(0);
    let mut ids = Vec::with_capacity(executions.len());
    for execution in executions {
        if &execution.process_instance_id != instance_id || execution.execution_no <= previous {
            return Err(Error::ConflictError("驳回清理执行归属或分页序号不一致".into()));
        }
        previous = execution.execution_no;
        ids.push(execution.typed_id());
    }
    Ok(ids)
}

/// 按精确执行分组并调用 WorkItem 生命周期，不接收页外任务。
fn close_page_tasks(
    mut tasks: Vec<WorkItem>,
    execution_ids: &[ApprovalNodeExecutionId],
    actor_id: &str,
    now: Instant,
) -> Result<Vec<WorkItem>> {
    let ending = ApprovalRuntimeTaskEnding::Close { reason: "APPROVAL_INSTANCE_REJECTED".into() };
    let mut ended = Vec::with_capacity(tasks.len());
    for execution_id in execution_ids {
        let (matching, remaining) = tasks
            .into_iter()
            .partition(|task: &WorkItem| task.approval_node_execution_id.as_ref() == Some(execution_id));
        tasks = remaining;
        ended.extend(WorkItem::end_all_for_approval_execution(
            matching,
            execution_id,
            actor_id,
            &ending,
            now,
        )?);
    }
    if !tasks.is_empty() {
        return Err(Error::ConflictError("驳回清理任务不属于本实例执行".into()));
    }
    Ok(ended)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{NewNodeExecution, ParticipantId, Timestamp};
    use erp_core::ids::WorkItemId;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::entity::work_item::{
        DocumentApprovalWorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
    };

    struct RecordingTasks {
        executions: Vec<ApprovalNodeExecution>,
        tasks: Mutex<Vec<WorkItem>>,
        calls: Mutex<Vec<(&'static str, usize)>>,
        fail_write: bool,
    }

    impl RecordingTasks {
        fn record(&self, name: &'static str, executor: &mut dyn Executor) {
            self.calls.lock().unwrap().push((name, executor as *mut dyn Executor as *mut () as usize));
        }
    }

    #[async_trait]
    impl RejectionTasks for RecordingTasks {
        async fn execution_page(
            &self,
            instance_id: &ApprovalProcessInstanceId,
            cursor: Option<u32>,
            executor: &mut dyn Executor,
        ) -> Result<Vec<ApprovalNodeExecution>> {
            self.record("page", executor);
            Ok(self
                .executions
                .iter()
                .filter(|execution| {
                    &execution.process_instance_id == instance_id
                        && execution.execution_no > cursor.unwrap_or(0)
                })
                .take(2)
                .cloned()
                .collect())
        }

        async fn open_tasks(
            &self,
            execution_ids: &[ApprovalNodeExecutionId],
            executor: &mut dyn Executor,
        ) -> Result<Vec<WorkItem>> {
            self.record("read", executor);
            Ok(self
                .tasks
                .lock()
                .unwrap()
                .iter()
                .filter(|task| {
                    task.work_item_type == WorkItemType::DocumentApproval
                        && task.status == WorkItemStatus::Open
                        && task
                            .approval_node_execution_id
                            .as_ref()
                            .is_some_and(|id| execution_ids.contains(id))
                })
                .cloned()
                .collect())
        }

        async fn persist(&self, tasks: &[WorkItem], executor: &mut dyn Executor) -> Result<()> {
            self.record("persist", executor);
            if self.fail_write {
                return Err(Error::ConflictError("task CAS failed".into()));
            }
            let mut stored = self.tasks.lock().unwrap();
            for task in tasks {
                let original = stored.iter_mut().find(|row| row.base.id == task.base.id).unwrap();
                assert_eq!(original.status, WorkItemStatus::Open);
                assert_eq!(original.base.version, task.base.version);
                *original = task.clone();
            }
            Ok(())
        }
    }

    fn execution(id: &str, instance: &str, no: u32) -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new(id),
            process_instance_id: ApprovalProcessInstanceId::new(instance),
            node_key: "review".into(),
            node_name: "审批".into(),
            round_no: no,
            execution_no: no,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("reviewer").unwrap(),
            assignee_name_snapshot: "审批人".into(),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap()
    }

    fn task(id: &str, execution_id: &str) -> WorkItem {
        WorkItem::new_document_approval(
            WorkItemId::new(id),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new(execution_id),
                business_object_type: "sales_order".into(),
                business_object_id: "same-document".into(),
                subject_version: "1".into(),
                owner_role: "sales_order_approver".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "reviewer".into(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(1),
        )
        .unwrap()
    }

    fn store(fail_write: bool) -> RecordingTasks {
        let mut current = task("current-task", "current");
        current.complete_by_approval_runtime("reviewer", Instant::from_unix_secs(2)).unwrap();
        let mut other_type = task("other-type", "old-1");
        other_type.work_item_type = WorkItemType::ApprovalRework;
        RecordingTasks {
            executions: vec![
                execution("current", "rejected", 1),
                execution("old-1", "rejected", 2),
                execution("old-2", "rejected", 3),
                execution("sibling", "other-instance", 1),
            ],
            tasks: Mutex::new(vec![
                current,
                task("old-task-1", "old-1"),
                task("old-task-2", "old-2"),
                task("sibling-task", "sibling"),
                other_type,
            ]),
            calls: Mutex::new(Vec::new()),
            fail_write,
        }
    }

    #[tokio::test]
    async fn rejection_closes_all_history_pages_and_preserves_completed_and_sibling_tasks() {
        let store = store(false);
        let mut executor = NoTransaction;
        let executor_id = &mut executor as *mut NoTransaction as usize;
        close_instance_tasks(
            &store,
            &ApprovalProcessInstanceId::new("rejected"),
            "reviewer",
            Instant::from_unix_secs(3),
            &mut executor,
        )
        .await
        .unwrap();

        let tasks = store.tasks.lock().unwrap().clone();
        assert_eq!(tasks[0].status, WorkItemStatus::Completed);
        for task in &tasks[1..3] {
            assert_eq!(task.status, WorkItemStatus::Closed);
            assert_eq!(task.closed_by.as_deref(), Some("reviewer"));
            assert_eq!(task.close_reason.as_deref(), Some("APPROVAL_INSTANCE_REJECTED"));
        }
        assert_eq!(tasks[3].status, WorkItemStatus::Open, "同单据的兄弟实例不能关闭");
        assert_eq!(tasks[4].status, WorkItemStatus::Open, "非审批任务不能关闭");

        let calls = store.calls.lock().unwrap().clone();
        assert_eq!(
            calls.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["page", "read", "persist", "page", "read", "persist", "page"]
        );
        assert!(calls.iter().all(|(_, identity)| *identity == executor_id));

        close_instance_tasks(
            &store,
            &ApprovalProcessInstanceId::new("rejected"),
            "reviewer",
            Instant::from_unix_secs(4),
            &mut executor,
        )
        .await
        .unwrap();
        assert_eq!(store.calls.lock().unwrap().iter().filter(|(name, _)| *name == "persist").count(), 2);
    }

    #[tokio::test]
    async fn rejection_cleanup_stops_at_cas_failure() {
        let store = store(true);
        let error = close_instance_tasks(
            &store,
            &ApprovalProcessInstanceId::new("rejected"),
            "reviewer",
            Instant::from_unix_secs(3),
            &mut NoTransaction,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::ConflictError(message) if message == "task CAS failed"));
        assert_eq!(
            store.calls.lock().unwrap().iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ["page", "read", "persist"]
        );
    }

    #[test]
    fn rejection_cleanup_refuses_foreign_execution_task_and_non_advancing_cursor() {
        let instance_id = ApprovalProcessInstanceId::new("rejected");
        assert!(
            page_execution_ids(&[execution("foreign", "other-instance", 1)], &instance_id, None).is_err()
        );
        assert!(page_execution_ids(&[execution("old", "rejected", 1)], &instance_id, Some(1)).is_err());
        assert!(
            close_page_tasks(
                vec![task("foreign-task", "foreign")],
                &[ApprovalNodeExecutionId::new("old")],
                "reviewer",
                Instant::from_unix_secs(3)
            )
            .is_err()
        );
    }
}
