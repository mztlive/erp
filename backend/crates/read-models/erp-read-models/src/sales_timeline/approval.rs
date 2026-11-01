//! 审批实际记录的纯展示投影。
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};

use super::{TimelineCategory, TimelineDocument, TimelineEvent};

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
    let title = match instance.status.as_str() {
        "APPROVED" => "审批已通过",
        "REJECTED" => "审批已驳回",
        "CANCELLED" => "审批已取消",
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
        events.push(event);
    }
    events
}

pub(super) fn decision_event(
    execution: &ApprovalNodeExecution,
    document: &TimelineDocument,
) -> Option<TimelineEvent> {
    let time = u64::try_from(execution.decided_at?.unix_secs()).ok()?;
    let label = match execution.status.as_str() {
        "APPROVED" => "通过",
        "REJECTED" => "驳回",
        _ => return None,
    };
    let mut event = document.event(
        format!("decision:{}", execution.base.id),
        time,
        format!("{}：{}（第 {} 轮）", execution.node_name, label, execution.round_no),
        execution.assignee_display_name(),
    );
    event.category = TimelineCategory::Approval;
    if let Some(reason) = &execution.decision_reason {
        event.details.push(reason.clone());
    }
    Some(event)
}
