//! 事务提交后由持久化事实映射的最新视图。

use bpm::engine::CommitRequired;
use bpm::model::types::ApprovalProcessInstanceStatus;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use serde::{Deserialize, Serialize};

/// 命令结果类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalCommandOutcome {
    /// 命令已接受并提交。
    Applied,
    /// 当前决定未接受，但已提交 BLOCKED 事实。
    Blocked,
    /// 同载荷幂等回读。
    IdempotentReplay,
}

/// 下一开放任务摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenTaskSummary {
    /// 任务 ID。
    pub work_item_id: String,
    /// 任务版本。
    pub task_version: u64,
    /// 责任人。
    pub owner_user_id: String,
}

/// 启动、决定、取消、恢复、改派和受阻取消的统一响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalCommandView {
    /// 实例 ID。
    pub instance_id: String,
    /// 实例状态。
    pub instance_status: String,
    /// 当前轮次。
    pub current_round_no: u32,
    /// 当前节点。
    pub current_node_key: Option<String>,
    /// 当前节点名称。
    pub current_node_name: Option<String>,
    /// 当前审批人。
    pub current_assignee_participant_id: Option<String>,
    /// 当前审批人显示名。
    pub current_assignee_name: Option<String>,
    /// 单据状态，由调用方从持久化事实填入。
    pub subject_status: Option<String>,
    /// 最近驳回原因。
    pub latest_rejection_reason: Option<String>,
    /// 存在时的下一开放任务。
    pub next_open_task: Option<OpenTaskSummary>,
    /// 命令结果。
    pub outcome: ApprovalCommandOutcome,
}

/// 由提交后的实例、当前执行和可选任务构造视图。
///
/// # 参数
/// * `instance` - 持久化实例
/// * `current_execution` - 当前执行
/// * `latest_rejection_reason` - 最近驳回
/// * `subject_status` - 单据状态
/// * `next_open_task` - 下一开放任务
/// * `commit` - 本次提交类别
/// * `replay` - 是否幂等回读
///
/// # 返回
/// 返回不得用命令输入拼装的最新视图。
#[allow(clippy::too_many_arguments)]
pub fn map_command_view(
    instance: &ApprovalProcessInstance,
    current_execution: Option<&ApprovalNodeExecution>,
    latest_rejection_reason: Option<String>,
    subject_status: Option<String>,
    next_open_task: Option<OpenTaskSummary>,
    commit: CommitRequired,
    replay: bool,
) -> ApprovalCommandView {
    ApprovalCommandView {
        instance_id: instance.base.id.clone(),
        instance_status: instance.status.as_str().to_string(),
        current_round_no: instance.current_round_no,
        current_node_key: current_execution.map(|item| item.node_key.clone()),
        current_node_name: current_execution.map(|item| item.node_name.clone()),
        current_assignee_participant_id: current_execution
            .map(|item| item.assignee_participant_id.as_str().to_string()),
        current_assignee_name: current_execution.map(|item| item.assignee_name_snapshot.clone()),
        subject_status,
        latest_rejection_reason,
        next_open_task,
        outcome: outcome_of(instance.status, commit, replay),
    }
}

/// 由实例状态和提交类别计算对外结果。
fn outcome_of(
    status: ApprovalProcessInstanceStatus,
    commit: CommitRequired,
    replay: bool,
) -> ApprovalCommandOutcome {
    if replay {
        return ApprovalCommandOutcome::IdempotentReplay;
    }
    if commit == CommitRequired::Blocked || status == ApprovalProcessInstanceStatus::Blocked {
        return ApprovalCommandOutcome::Blocked;
    }
    ApprovalCommandOutcome::Applied
}

#[cfg(test)]
mod tests {
    use super::{map_command_view, ApprovalCommandOutcome, OpenTaskSummary};
    use bpm::engine::CommitRequired;
    use bpm::ids::{ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::{ApprovalProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp};

    /// 视图取自持久化事实，幂等回读不重放可变快照承诺。
    #[test]
    fn execution_view_maps_persisted_facts() {
        let instance = ApprovalProcessInstance::start_running(
            ApprovalProcessInstanceId::new("inst"),
            ApprovalProcessDefinitionId::new("def"),
            1,
            ProcessKind::StockAdjustment,
            SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
            1,
            ParticipantId::new("u1").unwrap(),
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap();
        let view = map_command_view(
            &instance,
            None,
            Some("资料不全".into()),
            Some("IN_APPROVAL".into()),
            Some(OpenTaskSummary {
                work_item_id: "wi-1".into(),
                task_version: 1,
                owner_user_id: "u1".into(),
            }),
            CommitRequired::Proceed,
            false,
        );
        assert_eq!(view.instance_status, "RUNNING");
        assert_eq!(view.latest_rejection_reason.as_deref(), Some("资料不全"));
        assert_eq!(view.outcome, ApprovalCommandOutcome::Applied);
        let replay = map_command_view(&instance, None, None, None, None, CommitRequired::Proceed, true);
        assert_eq!(replay.outcome, ApprovalCommandOutcome::IdempotentReplay);
    }
}
