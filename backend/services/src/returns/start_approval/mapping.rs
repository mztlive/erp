use bpm::model::ApprovalNodeExecution;
use database::repository::bpm::ApprovalInstanceListProjection;
use entities::common::time::Instant;

/// 由入口执行构造有界列表投影。
///
/// # 参数
/// * `execution` - 入口执行
/// * `now` - 状态变更时间
///
/// # 返回
/// 返回启动时的列表投影。
pub fn list_projection_from_execution(
    execution: &ApprovalNodeExecution,
    now: Instant,
) -> ApprovalInstanceListProjection {
    ApprovalInstanceListProjection {
        current_node_key: Some(execution.node_key.clone()),
        current_node_name: Some(execution.node_name.clone()),
        current_assignee_participant_id: Some(execution.assignee_participant_id.as_str().to_string()),
        current_assignee_name: Some(execution.assignee_name_snapshot.clone()),
        latest_rejected_execution_id: None,
        latest_rejection_summary: None,
        last_status_changed_at: Some(now.unix_secs()),
    }
}

#[cfg(test)]
mod tests {
    use super::list_projection_from_execution;
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    use bpm::model::{ApprovalNodeExecution, NewNodeExecution, ParticipantId, Timestamp};
    use entities::common::time::Instant;

    fn execution() -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("e1"),
            process_instance_id: ApprovalProcessInstanceId::new("inst-1"),
            node_key: "n1".into(),
            node_name: "财务复核".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: bpm::model::types::ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("u1").unwrap(),
            assignee_name_snapshot: "张三".into(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .expect("入口执行夹具")
    }

    /// 列表投影必须来自入口执行，不得推断未知审批人。
    #[test]
    fn list_projection_copies_entry_assignee() {
        let projection = list_projection_from_execution(&execution(), Instant::from_unix_secs(10));
        assert_eq!(projection.current_node_key.as_deref(), Some("n1"));
        assert_eq!(projection.current_assignee_participant_id.as_deref(), Some("u1"));
        assert_eq!(projection.current_assignee_name.as_deref(), Some("张三"));
        assert_eq!(projection.last_status_changed_at, Some(10));
    }
}
