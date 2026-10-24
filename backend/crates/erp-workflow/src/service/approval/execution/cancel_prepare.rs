//! 普通单据撤回共用的运行事实读取和命令输入构造。

use async_trait::async_trait;
use bpm::engine::DefinitionGraph;
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{
    ApprovalCancellationTaskPolicy, ApprovalCommandReceipt, ApprovalNodeExecution, ApprovalProcessInstance,
    IdempotencyKey, ParticipantId, SubjectRef, Timestamp,
};
use erp_core::common::time::Instant;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::authorization::converge_eligibility;
use super::{CancelExecutionInput, ExecutionCommandInput, normalize_document_cancel_reason};
use crate::entity::document_registry::business_document::ApprovalDefinitionBinding;
use crate::entity::work_item::WorkItem;
use crate::repository::prelude::*;
use crate::repository::{BpmExt, WorkItemExt};
use crate::service::approval::read::bound_definition_graph;
use crate::{Error, Result};

/// 已通过取消策略及任务数量校验的普通撤回运行事实。
pub struct DocumentCancelRuntime {
    /// 单据冻结绑定对应的定义图。
    pub graph: DefinitionGraph,
    /// 本次撤回的精确实例。
    pub instance: ApprovalProcessInstance,
    /// 当前节点执行。
    pub current: ApprovalNodeExecution,
    /// 实例决定的任务关闭策略。
    pub task_policy: ApprovalCancellationTaskPolicy,
    /// 当前执行的开放审批任务。
    pub open_tasks: Vec<WorkItem>,
}

impl DocumentCancelRuntime {
    /// 使用已加载的版本和任务事实构造普通撤回输入。
    ///
    /// # 参数
    /// * `reason` - 撤回原因，构造时去除首尾空白。
    /// * `actor_id` - 当前撤回人引用。
    /// * `idempotency_key` - 已规范化的幂等键。
    /// * `receipt` - 当前命令的已有收据。
    /// * `now` - 入口确定的操作时间。
    ///
    /// # 返回
    /// 返回沿用实例、执行与首个开放任务版本的普通撤回输入。
    ///
    /// # 错误
    /// 原因、撤回人或当前审批人资格资料非法时返回校验错误。
    pub fn cancel_input(
        &self,
        reason: &str,
        actor_id: &str,
        idempotency_key: &IdempotencyKey,
        receipt: Option<ApprovalCommandReceipt>,
        now: Instant,
    ) -> Result<CancelExecutionInput> {
        let reason = normalize_document_cancel_reason(reason)?;
        let actor =
            ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("撤回人引用无效".to_string()))?;
        let eligibility = converge_eligibility(
            self.current.assignee_participant_id.as_str(),
            &self.current.assignee_name_snapshot,
            None,
        )?;
        Ok(CancelExecutionInput {
            command: ExecutionCommandInput {
                graph: self.graph.clone(),
                current_eligibility: eligibility.clone(),
                next_eligibility: eligibility,
                receipt,
                idempotency_key: idempotency_key.clone(),
                now: Timestamp::from_utc(now.as_utc()),
            },
            instance: self.instance.clone(),
            current: self.current.clone(),
            subject_version: self.instance.subject_version,
            expected_instance_version: self.instance.base.version,
            expected_execution_version: self.current.base.version,
            expected_task_version: self.open_tasks.first().map(|item| item.base.version),
            reason,
            actor,
            close_open_task: self.task_policy.closes_open_task(),
            blocked_port: false,
            receipt_id: ApprovalCommandReceiptId::new(next_id()),
        })
    }
}

/// 在调用方执行器中按主体加载普通撤回运行事实。
///
/// # 参数
/// * `db` - 工作流数据库。
/// * `binding` - 业务单据冻结的定义绑定。
/// * `subject` - 本次撤回的业务主体。
/// * `subject_version` - 本次撤回的冻结提交版本。
/// * `missing_definition` - 入口既有的定义不存在文案。
/// * `executor` - 调用方读取或事务执行器，全程沿用。
///
/// # 返回
/// 按实例、取消策略、当前执行、开放任务、定义图的顺序返回已校验事实。
///
/// # 错误
/// 实例或执行缺失、策略与任务数量不匹配、定义缺失或仓储失败时立即停止。
pub async fn document_cancel_runtime(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    subject: &SubjectRef,
    subject_version: u32,
    missing_definition: &str,
    executor: &mut dyn Executor,
) -> Result<DocumentCancelRuntime> {
    runtime_from_store(db, binding, subject, subject_version, missing_definition, executor).await
}

#[async_trait]
trait CancelRuntimeStore: Sync {
    async fn instance(
        &self,
        subject: &SubjectRef,
        subject_version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>>;
    async fn current(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>>;
    async fn open_tasks(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>>;
    async fn graph(
        &self,
        binding: &ApprovalDefinitionBinding,
        missing_definition: &str,
        executor: &mut dyn Executor,
    ) -> Result<DefinitionGraph>;
}

#[async_trait]
impl CancelRuntimeStore for Database {
    async fn instance(
        &self,
        subject: &SubjectRef,
        subject_version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>> {
        Ok(self.bpm_workflow().cancellation_instance_by_subject(subject, subject_version, executor).await?)
    }

    async fn current(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>> {
        Ok(self.bpm_workflow().current_execution_for_cancellation(instance_id, executor).await?)
    }

    async fn open_tasks(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        Ok(self.work_items().open_approval_tasks_for_execution(execution_id, executor).await?)
    }

    async fn graph(
        &self,
        binding: &ApprovalDefinitionBinding,
        missing_definition: &str,
        executor: &mut dyn Executor,
    ) -> Result<DefinitionGraph> {
        bound_definition_graph(self, binding, missing_definition, executor).await
    }
}

async fn runtime_from_store(
    store: &impl CancelRuntimeStore,
    binding: &ApprovalDefinitionBinding,
    subject: &SubjectRef,
    subject_version: u32,
    missing_definition: &str,
    executor: &mut dyn Executor,
) -> Result<DocumentCancelRuntime> {
    let instance = store
        .instance(subject, subject_version, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("没有可撤回的审批实例".to_string()))?;
    let task_policy =
        instance.cancellation_task_policy().map_err(|error| Error::ConflictError(error.to_string()))?;
    let current = store
        .current(&ApprovalProcessInstanceId::new(instance.base.id.clone()), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例缺少当前执行".to_string()))?;
    let open_tasks =
        store.open_tasks(&ApprovalNodeExecutionId::new(current.base.id.clone()), executor).await?;
    current
        .ensure_cancellation_tasks(task_policy, open_tasks.len())
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    let graph = store.graph(binding, missing_definition, executor).await?;
    Ok(DocumentCancelRuntime { graph, instance, current, task_policy, open_tasks })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::model::types::{
        ApprovalCommandKind, ApprovalExecutionAssignmentSource, ApprovalProcessInstanceStatus,
    };
    use bpm::model::{ApprovalProcessDefinition, NewNodeExecution, NewProcessInstance, ProcessKind};
    use entity_core::BaseModel;
    use erp_core::ids::WorkItemId;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::entity::work_item::{DocumentApprovalWorkItemData, WorkItemPriority};

    struct RecordingStore {
        runtime: DocumentCancelRuntime,
        calls: Mutex<Vec<(&'static str, usize)>>,
        missing_current: bool,
        fail_at: Option<&'static str>,
    }

    impl RecordingStore {
        fn new() -> Self {
            Self { runtime: runtime(), calls: Mutex::new(Vec::new()), missing_current: false, fail_at: None }
        }

        fn record(&self, phase: &'static str, executor: &mut dyn Executor) -> Result<()> {
            self.calls.lock().unwrap().push((phase, executor as *mut dyn Executor as *mut () as usize));
            if self.fail_at == Some(phase) {
                return Err(Error::Forbidden(format!("{phase} denied")));
            }
            Ok(())
        }

        fn phases(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().iter().map(|(phase, _)| *phase).collect()
        }
    }

    #[async_trait]
    impl CancelRuntimeStore for RecordingStore {
        async fn instance(
            &self,
            subject: &SubjectRef,
            subject_version: u32,
            executor: &mut dyn Executor,
        ) -> Result<Option<ApprovalProcessInstance>> {
            self.record("instance", executor)?;
            assert_eq!(subject, &self.runtime.instance.subject);
            assert_eq!(subject_version, self.runtime.instance.subject_version);
            Ok(Some(self.runtime.instance.clone()))
        }

        async fn current(
            &self,
            instance_id: &ApprovalProcessInstanceId,
            executor: &mut dyn Executor,
        ) -> Result<Option<ApprovalNodeExecution>> {
            self.record("current", executor)?;
            assert_eq!(instance_id.as_ref(), self.runtime.instance.base.id);
            Ok((!self.missing_current).then(|| self.runtime.current.clone()))
        }

        async fn open_tasks(
            &self,
            execution_id: &ApprovalNodeExecutionId,
            executor: &mut dyn Executor,
        ) -> Result<Vec<WorkItem>> {
            self.record("tasks", executor)?;
            assert_eq!(execution_id.as_ref(), self.runtime.current.base.id);
            Ok(self.runtime.open_tasks.clone())
        }

        async fn graph(
            &self,
            binding: &ApprovalDefinitionBinding,
            missing_definition: &str,
            executor: &mut dyn Executor,
        ) -> Result<DefinitionGraph> {
            self.record("graph", executor)?;
            assert_eq!(binding.approval_process_definition_id.as_ref(), "definition");
            assert_eq!(missing_definition, "入口定义不存在");
            Ok(self.runtime.graph.clone())
        }
    }

    async fn load(store: &RecordingStore, executor: &mut dyn Executor) -> Result<DocumentCancelRuntime> {
        let binding = ApprovalDefinitionBinding::new(
            ApprovalProcessDefinitionId::new("definition"),
            1,
            Instant::from_unix_secs(1),
        )
        .unwrap();
        runtime_from_store(
            store,
            &binding,
            &store.runtime.instance.subject,
            store.runtime.instance.subject_version,
            "入口定义不存在",
            executor,
        )
        .await
    }

    #[tokio::test]
    async fn runtime_loads_in_order_with_one_executor_and_preserves_versions() {
        let store = RecordingStore::new();
        let mut executor = NoTransaction;
        let identity = &mut executor as *mut NoTransaction as usize;
        let loaded = load(&store, &mut executor).await.unwrap();
        assert_eq!(
            *store.calls.lock().unwrap(),
            vec![("instance", identity), ("current", identity), ("tasks", identity), ("graph", identity)]
        );
        assert_eq!(loaded.instance.base.version, 13);
        assert_eq!(loaded.current.base.version, 17);
        assert_eq!(loaded.open_tasks[0].base.version, 23);
        assert_eq!(loaded.task_policy, ApprovalCancellationTaskPolicy::CloseOpenTask);
    }

    #[tokio::test]
    async fn missing_current_stops_before_tasks_and_definition() {
        let mut store = RecordingStore::new();
        store.missing_current = true;
        let error = load(&store, &mut NoTransaction).await.err().unwrap();
        assert!(matches!(error, Error::ConflictError(message) if message == "审批实例缺少当前执行"));
        assert_eq!(store.phases(), vec!["instance", "current"]);
    }

    #[tokio::test]
    async fn terminal_policy_stops_before_current_query() {
        let mut store = RecordingStore::new();
        store.runtime.instance.status = ApprovalProcessInstanceStatus::Cancelled;
        let expected = store.runtime.instance.cancellation_task_policy().unwrap_err().to_string();
        let error = load(&store, &mut NoTransaction).await.err().unwrap();
        assert!(matches!(error, Error::ConflictError(message) if message == expected));
        assert_eq!(store.phases(), vec!["instance"]);
    }

    #[tokio::test]
    async fn invalid_task_count_stops_before_definition() {
        let mut store = RecordingStore::new();
        store.runtime.open_tasks.clear();
        let error = load(&store, &mut NoTransaction).await.err().unwrap();
        assert!(matches!(error, Error::ConflictError(_)));
        assert_eq!(store.phases(), vec!["instance", "current", "tasks"]);
    }

    #[tokio::test]
    async fn query_failure_is_preserved_and_stops_later_reads() {
        let mut store = RecordingStore::new();
        store.fail_at = Some("tasks");
        let error = load(&store, &mut NoTransaction).await.err().unwrap();
        assert!(matches!(error, Error::Forbidden(message) if message == "tasks denied"));
        assert_eq!(store.phases(), vec!["instance", "current", "tasks"]);
    }

    #[tokio::test]
    async fn blocked_runtime_without_tasks_builds_no_close_input() {
        let mut store = RecordingStore::new();
        store.runtime.instance.status = ApprovalProcessInstanceStatus::Blocked;
        store.runtime.open_tasks.clear();
        let loaded = load(&store, &mut NoTransaction).await.unwrap();
        let input =
            loaded.cancel_input("撤回", "submitter", &key(), None, Instant::from_unix_secs(30)).unwrap();
        assert_eq!(loaded.task_policy, ApprovalCancellationTaskPolicy::NoOpenTask);
        assert!(!input.close_open_task);
        assert!(!input.blocked_port);
        assert_eq!(input.expected_task_version, None);
    }

    #[test]
    fn input_preserves_receipt_identity_versions_and_time() {
        let loaded = runtime();
        let receipt = ApprovalCommandReceipt {
            base: BaseModel::fake(),
            command_kind: ApprovalCommandKind::CancelApproval,
            scope_id: "original-scope".into(),
            idempotency_key: key(),
            payload_digest: "original-payload".into(),
            result_ref: "original-instance".into(),
        };
        let input = loaded
            .cancel_input(
                "  撤回重改  ",
                "submitter",
                &key(),
                Some(receipt.clone()),
                Instant::from_unix_secs(30),
            )
            .unwrap();
        assert_eq!(input.subject_version, 7);
        assert_eq!(input.expected_instance_version, 13);
        assert_eq!(input.expected_execution_version, 17);
        assert_eq!(input.expected_task_version, Some(23));
        assert_eq!(input.command.receipt, Some(receipt));
        assert_eq!(input.command.idempotency_key, key());
        assert_eq!(input.command.now, at(30));
        assert_eq!(input.actor.as_str(), "submitter");
        assert_eq!(input.reason, "撤回重改");
        assert!(input.close_open_task);
        assert!(!input.blocked_port);
    }

    #[test]
    fn input_preserves_reason_actor_and_eligibility_error_order() {
        let mut loaded = runtime();
        loaded.current.assignee_name_snapshot = "  ".into();
        for (reason, actor, expected) in [
            ("  ", "", "撤回原因不能为空"),
            ("撤回", "", "撤回人引用无效"),
            ("撤回", "submitter", "审批人显示名不能为空"),
        ] {
            let error =
                loaded.cancel_input(reason, actor, &key(), None, Instant::from_unix_secs(30)).unwrap_err();
            assert!(matches!(error, Error::ValidationError(message) if message == expected));
        }
    }

    fn runtime() -> DocumentCancelRuntime {
        let mut instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance"),
            process_definition_id: ApprovalProcessDefinitionId::new("definition"),
            definition_version: 1,
            process_kind: ProcessKind::SalesOrder,
            subject: SubjectRef::new("sales_order", "order").unwrap(),
            subject_version: 7,
            started_by: participant("submitter"),
            at: at(10),
        })
        .unwrap();
        instance.current_node_execution_id = Some(ApprovalNodeExecutionId::new("execution"));
        instance.base.version = 13;
        DocumentCancelRuntime {
            graph: DefinitionGraph {
                definition: ApprovalProcessDefinition::new_draft(
                    ApprovalProcessDefinitionId::new("definition"),
                    ProcessKind::SalesOrder,
                    1,
                    "销售审批",
                    "entry",
                    participant("admin"),
                    at(1),
                )
                .unwrap(),
                nodes: Vec::new(),
                transitions: Vec::new(),
            },
            instance,
            current: execution(),
            task_policy: ApprovalCancellationTaskPolicy::CloseOpenTask,
            open_tasks: vec![open_task()],
        }
    }

    fn execution() -> ApprovalNodeExecution {
        let mut execution = ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("execution"),
            process_instance_id: ApprovalProcessInstanceId::new("instance"),
            node_key: "entry".into(),
            node_name: "入口".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: participant("approver"),
            assignee_name_snapshot: "审批人".into(),
            at: at(10),
        })
        .unwrap();
        execution.base.version = 17;
        execution
    }

    fn open_task() -> WorkItem {
        let mut task = WorkItem::new_document_approval(
            WorkItemId::new("task"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new("execution"),
                business_object_type: "sales_order".into(),
                business_object_id: "order".into(),
                subject_version: "7".into(),
                owner_role: "sales_order_approver".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "approver".into(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(10),
        )
        .unwrap();
        task.base.version = 23;
        task
    }

    fn key() -> IdempotencyKey {
        IdempotencyKey::parse("cancel-key").unwrap()
    }

    fn participant(id: &str) -> ParticipantId {
        ParticipantId::new(id).unwrap()
    }

    fn at(seconds: i64) -> Timestamp {
        Timestamp::from_unix_secs(seconds).unwrap()
    }
}
