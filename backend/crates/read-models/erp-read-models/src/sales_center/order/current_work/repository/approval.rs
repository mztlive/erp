//! 当前办理的审批事实重验；历史遗留的 OPEN 标记不能单独证明审批仍在进行。
use std::collections::HashMap;

use bpm::model::types::{ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_workflow::BpmExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemType};
use persistence_core::Executor;

use super::CurrentWorkRepository;
use crate::Result;

impl CurrentWorkRepository<'_> {
    /// 批量校验审批任务的实例、当前节点、轮次、对象版本与实际责任人。
    /// # 参数
    /// `tasks` 为关联本销售单的开放任务，`node_names` 接收仍有效的当前审批节点名称。
    /// # 返回
    /// 返回仍匹配当前审批责任的任务及其他业务任务。
    /// # 错误
    /// 审批实例或节点读取失败时返回错误。
    pub(super) async fn current_approvals(
        &self,
        tasks: Vec<WorkItem>,
        node_names: &mut HashMap<String, String>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        let execution_ids =
            tasks.iter().filter_map(|task| task.approval_node_execution_id.clone()).collect::<Vec<_>>();
        let executions = self.db.bpm_workflow().list_executions_by_ids(&execution_ids, executor).await?;
        let instance_ids =
            executions.iter().map(|execution| execution.process_instance_id.clone()).collect::<Vec<_>>();
        let instances = self
            .db
            .bpm_workflow()
            .list_instances_by_ids(&instance_ids, executor)
            .await?
            .into_iter()
            .map(|instance| (instance.base.id.clone(), instance))
            .collect::<HashMap<_, _>>();
        let executions = executions
            .into_iter()
            .map(|execution| (execution.base.id.clone(), execution))
            .collect::<HashMap<_, _>>();

        Ok(tasks
            .into_iter()
            .filter(|task| match task.work_item_type {
                WorkItemType::DocumentApproval => {
                    let execution =
                        task.approval_node_execution_id.as_ref().and_then(|id| executions.get(id.as_ref()));
                    let instance =
                        execution.and_then(|execution| instances.get(execution.process_instance_id.as_ref()));
                    let current = matches_current_approval(task, execution, instance);
                    if current && let Some(execution) = execution {
                        node_names.insert(task.base.id.clone(), execution.node_name.clone());
                    }
                    current
                },
                // 此类型已由统一审批替代，遗留 OPEN 记录不再代表可办理责任。
                WorkItemType::PurchaseOrderReview => false,
                _ => true,
            })
            .collect())
    }
}

/// 仅展示与正在运行实例的当前活动节点完全匹配的审批责任。
fn matches_current_approval(
    task: &WorkItem,
    execution: Option<&ApprovalNodeExecution>,
    instance: Option<&ApprovalProcessInstance>,
) -> bool {
    let (Some(execution), Some(instance)) = (execution, instance) else {
        return false;
    };
    matches!(
        (instance.status, execution.status),
        (ApprovalProcessInstanceStatus::Running, ApprovalNodeExecutionStatus::Active)
            | (ApprovalProcessInstanceStatus::Blocked, ApprovalNodeExecutionStatus::Blocked)
    ) && instance.current_node_execution_id.as_ref().map(AsRef::as_ref) == Some(execution.base.id.as_str())
        && execution.process_instance_id.as_ref() == instance.base.id
        && execution.round_no == instance.current_round_no
        && task.approval_node_execution_id.as_ref().map(AsRef::as_ref) == Some(execution.base.id.as_str())
        && task.matches_business_object(instance.subject.subject_kind(), instance.subject.subject_id())
        && task.matches_subject_version(&instance.subject_version.to_string())
        && task.owner_user_id.as_deref().is_some_and(|id| execution.has_participant(id))
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{
        NewNodeExecution, NewProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp,
    };
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{DocumentApprovalWorkItemData, WorkItemPriority};

    use super::*;
    fn running() -> (ApprovalProcessInstance, ApprovalNodeExecution, WorkItem) {
        let mut instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance"),
            process_definition_id: ApprovalProcessDefinitionId::new("definition"),
            definition_version: 2,
            process_kind: ProcessKind::SalesChangeOrder,
            subject: SubjectRef::new("sales_change_order", "change").unwrap(),
            subject_version: 3,
            started_by: ParticipantId::new("creator").unwrap(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .unwrap();
        let current = ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("current"),
            process_instance_id: ApprovalProcessInstanceId::new("instance"),
            node_key: "finance".into(),
            node_name: "财务复核".into(),
            round_no: 1,
            execution_no: 20,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("reviewer").unwrap(),
            assignee_name_snapshot: "审批人".into(),
            at: Timestamp::from_unix_secs(11).unwrap(),
        })
        .unwrap();
        instance
            .set_current_execution(
                ApprovalNodeExecutionId::new("current"),
                Timestamp::from_unix_secs(11).unwrap(),
            )
            .unwrap();
        let task = WorkItem::new_document_approval(
            WorkItemId::new("task"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new("current"),
                business_object_type: "sales_change_order".into(),
                business_object_id: "change".into(),
                subject_version: "3".into(),
                owner_role: "approver".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "reviewer".into(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(11),
        )
        .unwrap();
        (instance, current, task)
    }

    #[test]
    fn current_node_requires_exact_owner_subject_version_and_round() {
        let (mut instance, mut execution, mut task) = running();
        assert!(matches_current_approval(&task, Some(&execution), Some(&instance)));
        assert!(!matches_current_approval(&task, None, Some(&instance)));
        assert!(!matches_current_approval(&task, Some(&execution), None));

        task.owner_user_id = Some("another".into());
        assert!(!matches_current_approval(&task, Some(&execution), Some(&instance)));

        task.owner_user_id = Some("reviewer".into());
        instance.subject_version += 1;
        assert!(!matches_current_approval(&task, Some(&execution), Some(&instance)));

        instance.subject_version -= 1;
        execution.round_no += 1;
        assert!(!matches_current_approval(&task, Some(&execution), Some(&instance)));

        execution.round_no -= 1;
        instance.current_node_execution_id = Some(ApprovalNodeExecutionId::new("old"));
        assert!(!matches_current_approval(&task, Some(&execution), Some(&instance)));
    }

    #[test]
    fn historical_open_tasks_do_not_revive_finished_approvals() {
        let (mut instance, mut execution, task) = running();
        for status in [
            ApprovalProcessInstanceStatus::Approved,
            ApprovalProcessInstanceStatus::Rejected,
            ApprovalProcessInstanceStatus::Cancelled,
        ] {
            instance.status = status;
            assert!(!matches_current_approval(&task, Some(&execution), Some(&instance)));
        }

        instance.status = ApprovalProcessInstanceStatus::Running;
        for status in [
            ApprovalNodeExecutionStatus::Approved,
            ApprovalNodeExecutionStatus::Rejected,
            ApprovalNodeExecutionStatus::Cancelled,
            ApprovalNodeExecutionStatus::Superseded,
        ] {
            execution.status = status;
            assert!(!matches_current_approval(&task, Some(&execution), Some(&instance)));
        }

        instance.status = ApprovalProcessInstanceStatus::Blocked;
        execution.status = ApprovalNodeExecutionStatus::Blocked;
        assert!(matches_current_approval(&task, Some(&execution), Some(&instance)));
    }
}
