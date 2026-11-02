//! 审批实际记录的纯展示投影。
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};

use super::{TimelineApproval, TimelineApprovalResult, TimelineCategory, TimelineDocument, TimelineEvent};

pub(super) fn instance_events(
    instance: &ApprovalProcessInstance,
    document: &TimelineDocument,
) -> Vec<TimelineEvent> {
    let mut events = Vec::new();
    if let Ok(at) = u64::try_from(instance.started_at.unix_secs()) {
        let mut event = document.event(
            format!("instance-start:{}", instance.base.id),
            at,
            "审批已启动".into(),
            "未记录姓名".into(),
        );
        event.category = TimelineCategory::Approval;
        event.actor_id = Some(instance.started_by.as_str().to_owned());
        events.push(event);
    }
    let (title, result) = match instance.status.as_str() {
        "APPROVED" => ("审批已通过", TimelineApprovalResult::Approved),
        "REJECTED" => ("审批已驳回", TimelineApprovalResult::Rejected),
        "CANCELLED" => ("审批已取消", TimelineApprovalResult::Cancelled),
        _ => return events,
    };
    if let Some(at) = instance.ended_at.and_then(|at| u64::try_from(at.unix_secs()).ok()) {
        let mut event = document.event(
            format!("instance-end:{}", instance.base.id),
            at,
            title.into(),
            if instance.status.as_str() == "CANCELLED" {
                "未记录操作人".into()
            } else {
                "审批流程".into()
            },
        );
        event.category = TimelineCategory::Approval;
        event.approval =
            Some(TimelineApproval { result, node_name: None, round_no: Some(instance.current_round_no) });
        events.push(event);
    }
    events
}

pub(super) fn decision_event(
    execution: &ApprovalNodeExecution,
    document: &TimelineDocument,
) -> Option<TimelineEvent> {
    let time = u64::try_from(execution.decided_at?.unix_secs()).ok()?;
    let (label, result) = match execution.status.as_str() {
        "APPROVED" => ("通过", TimelineApprovalResult::Approved),
        "REJECTED" => ("驳回", TimelineApprovalResult::Rejected),
        _ => return None,
    };
    let mut event = document.event(
        format!("decision:{}", execution.base.id),
        time,
        format!("{}：{}（第 {} 轮）", execution.node_name, label, execution.round_no),
        execution.assignee_display_name(),
    );
    event.category = TimelineCategory::Approval;
    event.approval = Some(TimelineApproval {
        result,
        node_name: Some(execution.node_name.clone()),
        round_no: Some(execution.round_no),
    });
    if let Some(reason) = &execution.decision_reason {
        event.details.push(reason.clone());
    }
    Some(event)
}

#[cfg(test)]
mod tests {
    use bpm::model::{NewProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp};
    use bpm::{ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use erp_workflow::DocumentType;

    use super::*;

    #[test]
    fn instance_results_require_a_recorded_end_time() {
        let instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance"),
            process_definition_id: ApprovalProcessDefinitionId::new("definition"),
            definition_version: 1,
            process_kind: ProcessKind::SalesOrder,
            subject: SubjectRef::new("sales_order", "sale").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("sales").unwrap(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .unwrap();
        let document = TimelineDocument::new(DocumentType::SalesOrder, "sale".into());
        let events = instance_events(&instance, &document);
        assert_eq!(events.len(), 1);
        assert!(events[0].approval.is_none());
        assert!(serde_json::to_value(&events[0]).unwrap().get("approval").is_none());

        let end = Timestamp::from_unix_secs(20).unwrap();
        let mut approved = instance.clone();
        approved.complete_approved(end).unwrap();
        let mut rejected = instance.clone();
        rejected.complete_rejected(end).unwrap();
        let mut cancelled = instance;
        cancelled.cancel(end).unwrap();

        for (mut instance, expected) in [
            (approved, TimelineApprovalResult::Approved),
            (rejected, TimelineApprovalResult::Rejected),
            (cancelled, TimelineApprovalResult::Cancelled),
        ] {
            let events = instance_events(&instance, &document);
            assert_eq!(events.len(), 2);
            let outcome = events[1].approval.as_ref().unwrap();
            assert_eq!(outcome.result, expected);
            assert!(outcome.node_name.is_none());
            assert_eq!(outcome.round_no, Some(1));
            assert_eq!(events[1].occurred_at, 20);
            instance.ended_at = None;
            assert_eq!(instance_events(&instance, &document).len(), 1);
        }
    }
}
