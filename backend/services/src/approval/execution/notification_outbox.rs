//! 将中性 BPM 事件映射为通知 outbox 草稿。事务内只追加。

use bpm::engine::{BpmEvent, BpmEventKind};
use entities::approval_integration::ApprovalNotificationEventKind;

/// 事务内待追加的通知意图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationIntent {
    /// 业务去重键。
    pub dedup_key: String,
    /// 事件种类。
    pub event_kind: ApprovalNotificationEventKind,
}

/// 按合同 §16.5 把中性事件映射为通知意图。
///
/// # 参数
/// * `events` - BPM 事件
///
/// # 返回
/// 返回去重键已固定的通知意图；不包含模板敏感字段。
pub fn map_notification_intents(events: &[BpmEvent]) -> Vec<NotificationIntent> {
    events.iter().filter_map(map_one).collect()
}

/// 映射单个事件。未知组合不产生通知。
fn map_one(event: &BpmEvent) -> Option<NotificationIntent> {
    let instance_id = event.instance_id.as_ref();
    let execution_id = event.execution_id.as_ref().map(|id| id.as_ref());
    let (event_kind, dedup_key) = match event.kind {
        BpmEventKind::InstanceStarted => (
            ApprovalNotificationEventKind::Started,
            format!("started:{instance_id}"),
        ),
        BpmEventKind::NodeEntered => (
            ApprovalNotificationEventKind::Entered,
            format!("entered:{}", execution_id?),
        ),
        BpmEventKind::NodeApproved => (
            ApprovalNotificationEventKind::NodeApproved,
            format!("approved:{}", execution_id?),
        ),
        BpmEventKind::NodeRejected => (
            ApprovalNotificationEventKind::NodeRejected,
            format!("rejected:{}", execution_id?),
        ),
        BpmEventKind::InstanceBlocked => (
            ApprovalNotificationEventKind::Blocked,
            format!("blocked:{}", execution_id?),
        ),
        BpmEventKind::AssigneeRecovered => (
            ApprovalNotificationEventKind::Resumed,
            format!("resumed:{}", execution_id.unwrap_or(instance_id)),
        ),
        BpmEventKind::AssigneeReassigned => (
            ApprovalNotificationEventKind::Reassigned,
            format!("reassigned:{}", execution_id.unwrap_or(instance_id)),
        ),
        BpmEventKind::InstanceCancelled if event.blocker_code.is_some() => (
            ApprovalNotificationEventKind::BlockedCancelled,
            format!("blocked_cancelled:{instance_id}"),
        ),
        BpmEventKind::InstanceCancelled => (
            ApprovalNotificationEventKind::Cancelled,
            format!("cancelled:{instance_id}:{}", event.round_no),
        ),
        BpmEventKind::InstanceApproved => (
            ApprovalNotificationEventKind::Completed,
            format!("completed:{instance_id}"),
        ),
        BpmEventKind::RoundRestarted | BpmEventKind::ExecutionSuperseded => return None,
    };
    Some(NotificationIntent {
        dedup_key,
        event_kind,
    })
}

#[cfg(test)]
mod tests {
    use super::map_notification_intents;
    use bpm::engine::{BpmEvent, BpmEventKind};
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    use entities::approval_integration::ApprovalNotificationEventKind;

    /// 去重键遵循合同固定格式。
    #[test]
    fn execution_outbox_dedup_keys_are_stable() {
        let started = BpmEvent::new(
            BpmEventKind::InstanceStarted,
            ApprovalProcessInstanceId::new("inst"),
            1,
        );
        let entered = BpmEvent::new(
            BpmEventKind::NodeEntered,
            ApprovalProcessInstanceId::new("inst"),
            1,
        )
        .with_execution(ApprovalNodeExecutionId::new("e1"));
        let intents = map_notification_intents(&[started, entered]);
        assert_eq!(intents[0].dedup_key, "started:inst");
        assert_eq!(intents[1].dedup_key, "entered:e1");
        assert_eq!(intents[0].event_kind, ApprovalNotificationEventKind::Started);
    }
}
