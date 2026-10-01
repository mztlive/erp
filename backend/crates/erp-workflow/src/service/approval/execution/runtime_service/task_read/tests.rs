//! 单任务与批量任务共用的冻结责任链行为测试。
#![cfg(test)]

use std::slice::from_ref;

use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalExecutionAssignmentSource, ApprovalProcessInstanceStatus};
use bpm::model::{
    ApprovalNodeExecution, ApprovalProcessInstance, NewNodeExecution, NewProcessInstance, ParticipantId,
    Timestamp,
};
use bpm::{ProcessKind, SubjectRef};
use erp_core::common::time::Instant;
use erp_core::ids::{ApprovalSubjectSnapshotId, WorkItemId};
use erp_core::money::Quantity;

use super::{RuntimeReadSubject, task_chain_matches_subject};
use crate::entity::approval_integration::{ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload};
use crate::entity::document_registry::DocumentType;
use crate::entity::work_item::{
    AssignmentSource, DocumentApprovalWorkItemData, WorkItem, WorkItemPriority, WorkItemStatus,
};

/// 构造一条正式开放审批任务及其精确冻结运行主体。
pub(super) fn fixture() -> (RuntimeReadSubject, WorkItem) {
    let at = Timestamp::from_unix_secs(10).unwrap();
    let instance_id = ApprovalProcessInstanceId::new("instance-1");
    let execution_id = ApprovalNodeExecutionId::new("execution-1");
    let mut instance = ApprovalProcessInstance::start_running(NewProcessInstance {
        id: instance_id.clone(),
        process_definition_id: ApprovalProcessDefinitionId::new("definition-1"),
        definition_version: 1,
        process_kind: ProcessKind::StockAdjustment,
        subject: SubjectRef::new("stock_adjustment", "adjustment-1").unwrap(),
        subject_version: 1,
        started_by: ParticipantId::new("starter").unwrap(),
        at,
    })
    .unwrap();
    instance.set_current_execution(execution_id.clone(), at).unwrap();
    let execution = ApprovalNodeExecution::new_active(NewNodeExecution {
        id: execution_id.clone(),
        process_instance_id: instance_id.clone(),
        node_key: "review".into(),
        node_name: "仓储复核".into(),
        round_no: 1,
        execution_no: 1,
        assignment_source: ApprovalExecutionAssignmentSource::Definition,
        replaces_execution_id: None,
        assignee_participant_id: ParticipantId::new("owner-1").unwrap(),
        assignee_name_snapshot: "审批人".into(),
        at,
    })
    .unwrap();
    let snapshot = ApprovalSubjectSnapshot::new(
        ApprovalSubjectSnapshotId::new("snapshot-1"),
        instance_id,
        DocumentType::StockAdjustment,
        "adjustment-1",
        1,
        ApprovalSubjectSnapshotPayload {
            document_no: "ADJ-1".into(),
            responsible_org_id: "org-1".into(),
            submitted_by: "starter".into(),
            submitted_at: Instant::from_unix_secs(10),
            counterparty: None,
            total_amount: None,
            total_quantity: Some("1".parse::<Quantity>().unwrap()),
            line_count: 1,
        },
    )
    .unwrap();
    let task = WorkItem::new_document_approval(
        WorkItemId::new("task-1"),
        DocumentApprovalWorkItemData {
            approval_node_execution_id: execution_id,
            business_object_type: "stock_adjustment".into(),
            business_object_id: "adjustment-1".into(),
            subject_version: "1".into(),
            owner_role: "stock_adjustment_approver".into(),
            owner_organization_id: "org-1".into(),
            owner_user_id: "owner-1".into(),
            priority: WorkItemPriority::Normal,
            due_at: None,
        },
        Instant::from_unix_secs(10),
    )
    .unwrap();
    (
        RuntimeReadSubject {
            instance,
            current_execution: Some(execution),
            snapshot,
            document_type: DocumentType::StockAdjustment,
        },
        task,
    )
}

/// 开放任务必须是当前执行的唯一开放任务，缺失或重复均不可读。
#[test]
fn approval_task_batch_chain_requires_unique_exact_open_task() {
    let (subject, task) = fixture();
    assert!(task_chain_matches_subject(&task, &subject, from_ref(&task)).unwrap());
    assert!(!task_chain_matches_subject(&task, &subject, &[]).unwrap());
    let mut other = task.clone();
    other.base.id = "other-task".into();
    assert!(!task_chain_matches_subject(&task, &subject, from_ref(&other)).unwrap());
    assert!(!task_chain_matches_subject(&task, &subject, &[task.clone(), other]).unwrap());
}

/// 批量判定沿用节点、岗位、组织、责任人和精确提交版本全部约束。
#[test]
fn approval_task_batch_chain_rejects_frozen_responsibility_drift() {
    let (subject, task) = fixture();
    let changes: &[fn(&mut WorkItem)] = &[
        |item| item.assignment_source = AssignmentSource::SystemRule,
        |item| item.owner_role = "other-role".into(),
        |item| item.owner_organization_id = "other-org".into(),
        |item| item.owner_user_id = Some("other-owner".into()),
        |item| item.business_object_type = "sales_order".into(),
        |item| item.business_object_id = "other-adjustment".into(),
        |item| item.subject_version = "01".into(),
        |item| item.approval_node_execution_id = Some(ApprovalNodeExecutionId::new("other-execution")),
        |item| item.status = WorkItemStatus::Closed,
    ];
    for change in changes {
        let mut changed = task.clone();
        change(&mut changed);
        assert!(!task_chain_matches_subject(&changed, &subject, from_ref(&changed)).unwrap());
    }
    let (mut subject, task) = fixture();
    subject.instance.status = ApprovalProcessInstanceStatus::Approved;
    assert!(!task_chain_matches_subject(&task, &subject, from_ref(&task)).unwrap());
    let (mut subject, task) = fixture();
    subject.current_execution.as_mut().unwrap().node_key = "wrong ".into();
    assert!(!task_chain_matches_subject(&task, &subject, from_ref(&task)).unwrap());
}

/// 已完成任务只承认实际执行人的同一次决定，不依赖实例当前节点仍指向旧执行。
#[test]
fn approval_task_batch_terminal_chain_requires_original_completed_decision() {
    let (mut subject, mut task) = fixture();
    let execution = subject.current_execution.as_mut().unwrap();
    execution
        .record_approve(ParticipantId::new("owner-1").unwrap(), None, Timestamp::from_unix_secs(20).unwrap())
        .unwrap();
    subject.instance.current_node_execution_id = Some(ApprovalNodeExecutionId::new("next-execution"));
    task.status = WorkItemStatus::Completed;
    task.completed_by = Some("owner-1".into());
    assert!(task_chain_matches_subject(&task, &subject, &[]).unwrap());
    let mut changed = task.clone();
    changed.completed_by = Some("someone-else".into());
    assert!(!task_chain_matches_subject(&changed, &subject, &[]).unwrap());
    let mut changed = task.clone();
    changed.owner_role = "other-role".into();
    assert!(!task_chain_matches_subject(&changed, &subject, &[]).unwrap());
    subject.current_execution.as_mut().unwrap().decided_at = None;
    assert!(!task_chain_matches_subject(&task, &subject, &[]).unwrap());
}
