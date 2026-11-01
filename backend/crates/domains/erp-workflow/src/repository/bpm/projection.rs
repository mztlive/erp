//! 启动时的实例列表投影，严格取自被冻结的入口执行。

use bpm::model::ApprovalNodeExecution;
use erp_core::common::time::Instant;

use super::ApprovalInstanceListProjection;

impl ApprovalInstanceListProjection {
    /// 由入口执行构造启动列表投影，不读取当前账号或定义资料。
    ///
    /// # 参数
    /// * `execution` - 本次启动创建的入口执行及其冻结人员。
    /// * `now` - 调用方的状态变更时间。
    ///
    /// # 返回
    /// 返回入口节点和冻结审批人的有界投影；启动时不含驳回记录。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn from_entry_execution(execution: &ApprovalNodeExecution, now: Instant) -> Self {
        Self {
            current_node_key: Some(execution.node_key.clone()),
            current_node_name: Some(execution.node_name.clone()),
            current_assignee_participant_id: Some(execution.assignee_participant_id.as_str().to_string()),
            current_assignee_name: Some(execution.assignee_display_name()),
            latest_rejected_execution_id: None,
            latest_rejection_summary: None,
            last_status_changed_at: Some(now.unix_secs()),
        }
    }
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{ApprovalNodeExecution, NewNodeExecution, ParticipantId, Timestamp};
    use erp_core::common::time::Instant;

    use super::ApprovalInstanceListProjection;

    #[test]
    fn entry_projection_preserves_frozen_identity_and_initial_state() {
        let execution = ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("execution"),
            process_instance_id: ApprovalProcessInstanceId::new("instance"),
            node_key: "finance".into(),
            node_name: "财务复核".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("reviewer").unwrap(),
            assignee_name_snapshot: "张三".into(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .unwrap();
        assert_eq!(
            ApprovalInstanceListProjection::from_entry_execution(&execution, Instant::from_unix_secs(20)),
            ApprovalInstanceListProjection {
                current_node_key: Some("finance".into()),
                current_node_name: Some("财务复核".into()),
                current_assignee_participant_id: Some("reviewer".into()),
                current_assignee_name: Some("张三".into()),
                latest_rejected_execution_id: None,
                latest_rejection_summary: None,
                last_status_changed_at: Some(20),
            }
        );
    }
}
