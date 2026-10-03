//! 真实实例、当前执行、开放任务与历史使用同一调用方执行器读取。

use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::execution::authorization::requires_blocked_cancel;
use erp_workflow::service::approval::execution::{history_item_from_execution, history_page_from};
use erp_workflow::service::approval::process_kind::process_kind_of;
use erp_workflow::{BpmExt, WorkItemExt};
use mongodb::Database;
use persistence_core::Executor;

use super::{DocumentApprovalInstanceView, DocumentRuntime};
use crate::{Error, Result};

/// 在已授权业务快照内取得最新实例；缺失事实不补造审批对象。
pub(crate) async fn load_document_runtime(
    db: &Database,
    kind: DocumentType,
    id: &str,
    binding: Option<&ApprovalDefinitionBinding>,
    executor: &mut dyn Executor,
) -> Result<Option<DocumentRuntime>> {
    let subject = erp_workflow::entity::approval_integration::subject_ref_for(kind, id)
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    let Some(instance) = db.bpm_workflow().find_latest_by_subject(&subject, executor).await? else {
        return Ok(None);
    };
    if instance.subject != subject
        || instance.process_kind != process_kind_of(kind)
        || binding.is_none_or(|binding| {
            instance.process_definition_id != binding.approval_process_definition_id
                || instance.definition_version != binding.approval_definition_version
        })
    {
        return Err(Error::ConflictError("单据审批实例与冻结绑定不一致".into()));
    }
    load_runtime_facts(db, instance, executor).await.map(Some)
}

/// 当前执行链、任务数和最新驳回均来自持久化事实。
async fn load_runtime_facts(
    db: &Database,
    instance: ApprovalProcessInstance,
    executor: &mut dyn Executor,
) -> Result<DocumentRuntime> {
    let instance_id = ApprovalProcessInstanceId::new(instance.base.id.clone());
    let current = db.bpm_workflow().find_current_execution(&instance_id, executor).await?;
    ensure_current_execution(&instance, current.as_ref())?;
    let tasks = match current.as_ref() {
        Some(execution) => {
            db.work_items()
                .open_approval_tasks_for_execution(
                    &ApprovalNodeExecutionId::new(execution.base.id.clone()),
                    executor,
                )
                .await?
        },
        None => Vec::new(),
    };
    ensure_runtime_tasks(&instance, current.as_ref(), &tasks)?;
    let latest_rejected = db.bpm_workflow().find_latest_rejected_execution(&instance_id, executor).await?;
    let rows = db.bpm_workflow().list_execution_history(&instance_id, None, 9, executor).await?;
    let history = history_page_from(rows.iter().map(history_item_from_execution).collect(), 8);
    Ok(DocumentRuntime {
        instance: instance_view(&instance, current.as_ref(), tasks.first(), latest_rejected.as_ref()),
        history,
        cancellable: runtime_cancellable(&instance, current.as_ref(), tasks.len()),
    })
}

/// 当前实例令牌必须与同实例的开放执行一一对应。
fn ensure_current_execution(
    instance: &ApprovalProcessInstance,
    current: Option<&ApprovalNodeExecution>,
) -> Result<()> {
    match (&instance.current_node_execution_id, current) {
        (None, None)
            if matches!(
                instance.status,
                ApprovalProcessInstanceStatus::Approved | ApprovalProcessInstanceStatus::Cancelled
            ) =>
        {
            Ok(())
        },
        (Some(expected), Some(execution))
            if expected.as_ref() == execution.base.id
                && execution.process_instance_id.as_ref() == instance.base.id
                && execution.round_no == instance.current_round_no
                && matches!(
                    (instance.status, execution.status),
                    (ApprovalProcessInstanceStatus::Running, ApprovalNodeExecutionStatus::Active)
                        | (ApprovalProcessInstanceStatus::Blocked, ApprovalNodeExecutionStatus::Blocked)
                ) =>
        {
            Ok(())
        },
        _ => Err(Error::ConflictError("审批当前执行与实例令牌不一致".into())),
    }
}

/// 开放任务不得引用其他业务对象、主题版本或当前审批人。
fn ensure_runtime_tasks(
    instance: &ApprovalProcessInstance,
    current: Option<&ApprovalNodeExecution>,
    tasks: &[WorkItem],
) -> Result<()> {
    if tasks.len() > 1 {
        return Err(Error::ConflictError("当前审批执行存在多个开放任务".into()));
    }
    for task in tasks {
        if current.is_none_or(|execution| {
            task.approval_node_execution_id.as_ref().map(AsRef::as_ref) != Some(execution.base.id.as_str())
                || task.owner_user_id.as_deref() != Some(execution.assignee_participant_id.as_str())
        }) || !task.matches_business_object(instance.subject.subject_kind(), instance.subject.subject_id())
            || !task.matches_subject_version(&instance.subject_version.to_string())
        {
            return Err(Error::ConflictError("审批开放任务与当前单据、版本或责任人不一致".into()));
        }
    }
    Ok(())
}

/// 单据撤回不能接管一致性受阻或缺失、多重开放任务。
fn runtime_cancellable(
    instance: &ApprovalProcessInstance,
    current: Option<&ApprovalNodeExecution>,
    task_count: usize,
) -> bool {
    current.is_some()
        && !instance.blocker_code.is_some_and(requires_blocked_cancel)
        && !current.and_then(|execution| execution.blocker_code).is_some_and(requires_blocked_cancel)
        && instance
            .cancellation_task_policy()
            .is_ok_and(|policy| policy.ensure_open_task_count(task_count).is_ok())
}

/// 使用权威字段映射版本、节点、任务和最后驳回；不做 JSON 转换。
fn instance_view(
    instance: &ApprovalProcessInstance,
    current: Option<&ApprovalNodeExecution>,
    task: Option<&WorkItem>,
    rejected: Option<&ApprovalNodeExecution>,
) -> DocumentApprovalInstanceView {
    DocumentApprovalInstanceView {
        id: instance.base.id.clone(),
        status: instance.status.as_str().into(),
        current_round_no: instance.current_round_no,
        current_node: current.map(|execution| execution.node_key.clone()),
        current_node_name: current.map(|execution| execution.node_name.clone()),
        current_assignee: current.map(|execution| execution.assignee_participant_id.as_str().to_owned()),
        current_assignee_name: current.and_then(|execution| optional_text(&execution.assignee_name_snapshot)),
        latest_rejection: rejected
            .and_then(|execution| execution.decision_reason.as_deref())
            .and_then(optional_text),
        latest_rejection_by: rejected
            .and_then(|execution| execution.decided_by.as_ref())
            .map(|actor| actor.as_str().to_owned()),
        subject_version: Some(instance.subject_version.to_string()),
        instance_version: Some(instance.base.version.to_string()),
        current_execution_id: current.map(|execution| execution.base.id.clone()),
        current_execution_version: current.map(|execution| execution.base.version.to_string()),
        current_task_id: task.map(|task| task.base.id.clone()),
        current_task_version: task.map(|task| task.base.version.to_string()),
        process_version: Some(instance.definition_version),
        blocker_code: instance.blocker_code.map(|code| code.as_str().into()),
        started_by: Some(instance.started_by.as_str().to_owned()),
    }
}

/// 空白文本不作为有效驳回原因或显示名。
fn optional_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::model::types::{ApprovalBlockerCode, ApprovalExecutionAssignmentSource};
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

    /// 真实实例、执行和任务大版本均原样传输，驳回不依赖首屏八条历史。
    #[test]
    fn runtime_projection_preserves_authoritative_versions_and_rejection() {
        let (mut instance, mut current, mut task) = running();
        instance.base.version = 9_007_199_254_740_993;
        current.base.version = 9_007_199_254_740_995;
        task.base.version = 9_007_199_254_740_997;
        let mut rejected = current.clone();
        rejected.base.id = "rejected".into();
        rejected
            .record_reject(
                ParticipantId::new("reviewer").unwrap(),
                "请修正合同",
                Timestamp::from_unix_secs(12).unwrap(),
            )
            .unwrap();
        let view = instance_view(&instance, Some(&current), Some(&task), Some(&rejected));
        assert_eq!(view.instance_version.as_deref(), Some("9007199254740993"));
        assert_eq!(view.current_execution_version.as_deref(), Some("9007199254740995"));
        assert_eq!(view.current_task_version.as_deref(), Some("9007199254740997"));
        assert_eq!(view.latest_rejection.as_deref(), Some("请修正合同"));
        assert_eq!(view.current_node_name.as_deref(), Some("财务复核"));
        assert_eq!(view.started_by.as_deref(), Some("creator"));
    }

    /// 其他执行、来源单、提交版本、审批人和多任务均无法提供操作版本。
    #[test]
    fn runtime_read_fails_closed_for_mismatched_execution_and_tasks() {
        let (instance, current, task) = running();
        ensure_current_execution(&instance, Some(&current)).unwrap();
        ensure_runtime_tasks(&instance, Some(&current), std::slice::from_ref(&task)).unwrap();
        assert!(ensure_current_execution(&instance, None).is_err());
        let mut other_execution = current.clone();
        other_execution.base.id = "other".into();
        assert!(ensure_current_execution(&instance, Some(&other_execution)).is_err());
        let mut other_source = task.clone();
        other_source.business_object_id = "other".into();
        let mut other_version = task.clone();
        other_version.subject_version = "2".into();
        let mut other_owner = task.clone();
        other_owner.owner_user_id = Some("other".into());
        for invalid in [other_source, other_version, other_owner] {
            assert!(ensure_runtime_tasks(&instance, Some(&current), &[invalid]).is_err());
        }
        assert!(ensure_runtime_tasks(&instance, Some(&current), &[task.clone(), task]).is_err());
    }

    /// 普通取消只适用于匹配任务数的运行事实，结构受阻和终态不带取消动作。
    #[test]
    fn runtime_cancel_requires_valid_tasks_and_recoverable_state() {
        let (mut instance, mut current, _) = running();
        assert!(runtime_cancellable(&instance, Some(&current), 1));
        assert!(!runtime_cancellable(&instance, Some(&current), 0));
        current
            .block(ApprovalBlockerCode::ApproverAccountInactive, Timestamp::from_unix_secs(12).unwrap())
            .unwrap();
        instance
            .enter_blocked(
                ApprovalBlockerCode::ApproverAccountInactive,
                Timestamp::from_unix_secs(12).unwrap(),
            )
            .unwrap();
        assert!(runtime_cancellable(&instance, Some(&current), 0));
        instance.blocker_code = Some(ApprovalBlockerCode::InternalInvariantBroken);
        assert!(!runtime_cancellable(&instance, Some(&current), 0));
        instance.cancel(Timestamp::from_unix_secs(13).unwrap()).unwrap();
        assert!(!runtime_cancellable(&instance, None, 0));
    }
}
