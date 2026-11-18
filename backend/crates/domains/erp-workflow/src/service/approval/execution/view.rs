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
    /// 任务版本十进制字符串，避免 JSON/JavaScript 安全整数精度损失。
    pub task_version: String,
    /// 责任人。
    pub owner_user_id: String,
}

/// 审批运行命令的统一响应。
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
/// 返回不得用命令输入拼装的最新视图。
///
/// `current_execution`：当前执行。
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
        current_assignee_name: current_execution.map(|item| item.assignee_display_name()),
        subject_status,
        latest_rejection_reason,
        next_open_task,
        outcome: outcome_of(instance.status, commit, replay),
    }
}

// 组合实例状态和已应用或回放的提交类别，生成命令结果对外使用的状态。
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
    use bpm::engine::CommitRequired;
    use bpm::ids::{ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::{
        ApprovalProcessInstance, NewProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp,
    };

    use super::{ApprovalCommandOutcome, OpenTaskSummary, map_command_view};

    // 验证：视图取自持久化事实，幂等回读不重放可变快照承诺。
    #[test]
    fn execution_view_maps_persisted_facts() {
        let instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("inst"),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            definition_version: 1,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("u1").unwrap(),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap();
        let view = map_command_view(
            &instance,
            None,
            Some("资料不全".into()),
            Some("IN_APPROVAL".into()),
            Some(OpenTaskSummary {
                work_item_id: "wi-1".into(),
                task_version: "9007199254740993".into(),
                owner_user_id: "u1".into(),
            }),
            CommitRequired::Proceed,
            false,
        );

        assert_eq!(view.instance_status, "RUNNING");
        assert_eq!(
            view.next_open_task.as_ref().map(|task| task.task_version.as_str()),
            Some("9007199254740993")
        );
        assert_eq!(view.latest_rejection_reason.as_deref(), Some("资料不全"));
        assert_eq!(view.outcome, ApprovalCommandOutcome::Applied);

        let replay = map_command_view(&instance, None, None, None, None, CommitRequired::Proceed, true);
        assert_eq!(replay.outcome, ApprovalCommandOutcome::IdempotentReplay);
    }
}
