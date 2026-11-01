//! 单据详情使用的轻量审批运行事实；不增加冻结绑定或任务一致性策略。

use async_trait::async_trait;
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, SubjectRef};
use mongodb::Database;
use persistence_core::Executor;

use super::execution::{RuntimeHistoryPage, history_item_from_execution, history_page_from};
use crate::{BpmExt, Result};

/// 已持久化的最新实例、当前执行及详情首屏历史。
pub struct ApprovalRuntimeSummary {
    /// 按主体和既有稳定排序读取的最新实例。
    pub instance: ApprovalProcessInstance,
    /// 当前执行；不存在时保持为空。
    pub current: Option<ApprovalNodeExecution>,
    /// 按执行序号升序截断的首屏历史。
    pub history: RuntimeHistoryPage,
}

/// 按已授权主体读取详情所需的轻量运行事实。
///
/// # 参数
/// * `db` - 审批数据库。
/// * `subject` - 调用方已授权的单据主体。
/// * `history_limit` - 首屏历史条数；多读一条判断后续页。
/// * `executor` - 所有读取共用的调用方执行器。
///
/// # 返回
/// 主体尚无实例时返回 `None`；否则返回最新实例、可选当前执行与有界历史。
/// 本入口仅读取事实，不核验业务冻结绑定、不补默认实例、不读取办理任务。
///
/// # 错误
/// 任一仓储读取失败时保留持久化错误分类，立即停止后续读取。
pub async fn latest_runtime_summary(
    db: &Database,
    subject: &SubjectRef,
    history_limit: u32,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalRuntimeSummary>> {
    summary_from_store(db, subject, history_limit, executor).await
}

async fn summary_from_store(
    store: &impl RuntimeSummaryStore,
    subject: &SubjectRef,
    history_limit: u32,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalRuntimeSummary>> {
    let Some(instance) = store.latest(subject, executor).await? else {
        return Ok(None);
    };
    let id = ApprovalProcessInstanceId::new(instance.base.id.clone());
    let current = store.current(&id, executor).await?;
    let rows = store.history(&id, history_limit.saturating_add(1), executor).await?;
    let history = history_page_from(rows.iter().map(history_item_from_execution).collect(), history_limit);
    Ok(Some(ApprovalRuntimeSummary { instance, current, history }))
}

#[async_trait]
trait RuntimeSummaryStore: Sync {
    async fn latest(
        &self,
        subject: &SubjectRef,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>>;
    async fn current(
        &self,
        id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>>;
    async fn history(
        &self,
        id: &ApprovalProcessInstanceId,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeExecution>>;
}

#[async_trait]
impl RuntimeSummaryStore for Database {
    async fn latest(
        &self,
        subject: &SubjectRef,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>> {
        Ok(self.bpm_workflow().find_latest_by_subject(subject, executor).await?)
    }

    async fn current(
        &self,
        id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>> {
        Ok(self.bpm_workflow().find_current_execution(id, executor).await?)
    }

    async fn history(
        &self,
        id: &ApprovalProcessInstanceId,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeExecution>> {
        Ok(self.bpm_workflow().list_execution_history(id, None, limit, executor).await?)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId};
    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{NewNodeExecution, NewProcessInstance, ParticipantId, ProcessKind, Timestamp};
    use mongodb::ClientSession;

    use super::*;
    use crate::Error;

    #[derive(Default)]
    struct TrackingExecutor(usize);

    impl Executor for TrackingExecutor {
        fn session(&mut self) -> Option<&mut ClientSession> {
            self.0 += 1;
            None
        }
    }

    struct Store {
        instance: Option<ApprovalProcessInstance>,
        current: Option<ApprovalNodeExecution>,
        history: Vec<ApprovalNodeExecution>,
        fail_at: Option<&'static str>,
        calls: Mutex<Vec<String>>,
    }

    impl Store {
        fn new(instance: Option<ApprovalProcessInstance>) -> Self {
            Self {
                instance,
                current: None,
                history: Vec::new(),
                fail_at: None,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn record(&self, stage: &'static str, detail: &str, executor: &mut dyn Executor) -> Result<()> {
            let _ = executor.session();
            self.calls.lock().unwrap().push(format!("{stage}:{detail}"));
            if self.fail_at == Some(stage) {
                return Err(Error::Internal(format!("{stage} failed")));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl RuntimeSummaryStore for Store {
        async fn latest(
            &self,
            subject: &SubjectRef,
            executor: &mut dyn Executor,
        ) -> Result<Option<ApprovalProcessInstance>> {
            self.record("latest", subject.subject_id(), executor)?;
            Ok(self.instance.clone())
        }

        async fn current(
            &self,
            id: &ApprovalProcessInstanceId,
            executor: &mut dyn Executor,
        ) -> Result<Option<ApprovalNodeExecution>> {
            self.record("current", id.as_ref(), executor)?;
            Ok(self.current.clone())
        }

        async fn history(
            &self,
            id: &ApprovalProcessInstanceId,
            limit: u32,
            executor: &mut dyn Executor,
        ) -> Result<Vec<ApprovalNodeExecution>> {
            self.record("history", &format!("{}:{limit}", id.as_ref()), executor)?;
            Ok(self.history.clone())
        }
    }

    fn instance() -> ApprovalProcessInstance {
        ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance"),
            process_definition_id: ApprovalProcessDefinitionId::new("definition"),
            definition_version: 3,
            process_kind: ProcessKind::SalesOrder,
            subject: SubjectRef::new("sales_order", "order").unwrap(),
            subject_version: 2,
            started_by: ParticipantId::new("starter").unwrap(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .unwrap()
    }

    fn execution(number: u32) -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new(format!("execution-{number}")),
            process_instance_id: ApprovalProcessInstanceId::new("instance"),
            node_key: "review".into(),
            node_name: "复核".into(),
            round_no: 1,
            execution_no: number,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("reviewer").unwrap(),
            assignee_name_snapshot: "复核人".into(),
            at: Timestamp::from_unix_secs(11).unwrap(),
        })
        .unwrap()
    }

    #[tokio::test]
    async fn missing_instance_stops_before_execution_and_history_reads() {
        let store = Store::new(None);
        let mut executor = TrackingExecutor::default();
        let loaded = summary_from_store(&store, &instance().subject, 2, &mut executor).await.unwrap();
        assert!(loaded.is_none());
        assert_eq!(*store.calls.lock().unwrap(), ["latest:order"]);
        assert_eq!(executor.0, 1);
    }

    #[tokio::test]
    async fn history_keeps_first_page_and_cursor_using_the_callers_executor() {
        let mut store = Store::new(Some(instance()));
        store.current = Some(execution(3));
        store.history = vec![execution(1), execution(2), execution(3)];
        let mut executor = TrackingExecutor::default();
        let loaded =
            summary_from_store(&store, &instance().subject, 2, &mut executor).await.unwrap().unwrap();
        assert_eq!(loaded.instance.base.id, "instance");
        assert_eq!(loaded.current.unwrap().execution_no, 3);
        assert_eq!(loaded.history.items.iter().map(|item| item.execution_no).collect::<Vec<_>>(), [1, 2]);
        assert!(loaded.history.has_more);
        assert_eq!(loaded.history.next_cursor.as_deref(), Some("2"));
        assert_eq!(*store.calls.lock().unwrap(), ["latest:order", "current:instance", "history:instance:3"]);
        assert_eq!(executor.0, 3);
    }

    #[tokio::test]
    async fn missing_current_execution_preserves_empty_facts_without_new_validation() {
        let store = Store::new(Some(instance()));
        let loaded = summary_from_store(&store, &instance().subject, 8, &mut TrackingExecutor::default())
            .await
            .unwrap()
            .unwrap();
        assert!(loaded.current.is_none());
        assert!(loaded.history.items.is_empty());
        assert!(!loaded.history.has_more);
        assert!(loaded.history.next_cursor.is_none());
    }

    #[tokio::test]
    async fn read_errors_preserve_the_error_and_stop_later_reads() {
        for (stage, count) in [("latest", 1), ("current", 2), ("history", 3)] {
            let mut store = Store::new(Some(instance()));
            store.fail_at = Some(stage);
            let result =
                summary_from_store(&store, &instance().subject, 8, &mut TrackingExecutor::default()).await;
            assert!(matches!(result, Err(Error::Internal(message)) if message == format!("{stage} failed")));
            assert_eq!(store.calls.lock().unwrap().len(), count);
        }
    }
}
