//! 确定性状态迁移计划。只含 BPM 状态与中性任务意图。

use crate::ids::ApprovalNodeExecutionId;
use crate::model::{ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance, ParticipantId};

use super::event::BpmEvent;

/// 计划提交类别。`Blocked` 表示本次决定不被接受，但阻塞事实必须提交。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitRequired {
    /// 决定或命令已被接受，按计划提交。
    Proceed,
    /// 当前决定不被接受，但必须提交 BLOCKED 事实。
    Blocked,
    /// 实例进入最终通过。
    TerminalApproved,
    /// 实例进入取消终态。
    Cancelled,
}

/// 中性任务意图。应用层负责映射为人工任务。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskIntent {
    /// 为活动执行创建唯一指定到人的开放任务。
    HumanTaskRequested {
        /// 新执行。
        execution_id: ApprovalNodeExecutionId,
        /// 任务责任人。
        assignee: ParticipantId,
        /// 节点键。
        node_key: String,
        /// 节点名称。
        node_name: String,
        /// 所属轮次。
        round_no: u32,
    },
    /// 将当前任务标记为已完成。
    CompleteTask {
        /// 对应执行。
        execution_id: ApprovalNodeExecutionId,
    },
    /// 关闭当前开放任务，不视为完成决定。
    CloseTask {
        /// 对应执行。
        execution_id: ApprovalNodeExecutionId,
        /// 固定关闭原因。
        reason: TaskCloseReason,
    },
}

/// 任务关闭原因。人员失效与结构阻塞共用同一关闭原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskCloseReason {
    /// 运行受阻，关闭原开放任务。
    ApprovalRuntimeBlocked,
    /// 随实例取消关闭。
    Cancelled,
}

impl TaskCloseReason {
    /// 返回稳定关闭原因代码。
    ///
    /// # 返回
    /// 返回合同固定关闭原因。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApprovalRuntimeBlocked => "APPROVAL_RUNTIME_BLOCKED",
            Self::Cancelled => "CANCELLED",
        }
    }
}

/// 状态迁移计划。同一输入必须产生同一语义结果。
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionPlan {
    /// 更新后的实例。
    pub instance: ApprovalProcessInstance,
    /// 新创建的执行。
    pub created_executions: Vec<ApprovalNodeExecution>,
    /// 就地更新的执行。
    pub updated_executions: Vec<ApprovalNodeExecution>,
    /// 启动时冻结的实例审批人。
    pub created_assignees: Vec<ApprovalInstanceAssignee>,
    /// 改派后的实例审批人。
    pub updated_assignees: Vec<ApprovalInstanceAssignee>,
    /// 中性任务意图。
    pub task_intents: Vec<TaskIntent>,
    /// 中性领域事件。
    pub events: Vec<BpmEvent>,
    /// 提交类别。
    pub commit: CommitRequired,
}

impl TransitionPlan {
    /// 以更新后的实例构造空写入计划。
    ///
    /// # 参数
    /// * `instance` - 已按本命令推进的实例快照
    /// * `commit` - 提交类别
    ///
    /// # 返回
    /// 返回不含执行或任务意图的计划骨架。
    pub fn for_instance(instance: ApprovalProcessInstance, commit: CommitRequired) -> Self {
        Self {
            instance,
            created_executions: Vec::new(),
            updated_executions: Vec::new(),
            created_assignees: Vec::new(),
            updated_assignees: Vec::new(),
            task_intents: Vec::new(),
            events: Vec::new(),
            commit,
        }
    }

    /// 追加另一份进入节点计划的执行、任务与事件，并采用其实例快照。
    ///
    /// 已接受决定后的下一节点受阻不得改写为 `CommitRequired::Blocked`。
    ///
    /// # 参数
    /// * `enter` - `plan_enter_node` 的结果
    /// * `keep_commit` - 为真时保留当前提交类别
    ///
    /// # 返回
    /// 返回合并后的计划。
    pub fn merge_enter(&mut self, enter: TransitionPlan, keep_commit: bool) {
        self.instance = enter.instance;
        self.created_executions.extend(enter.created_executions);
        self.updated_executions.extend(enter.updated_executions);
        self.created_assignees.extend(enter.created_assignees);
        self.updated_assignees.extend(enter.updated_assignees);
        self.task_intents.extend(enter.task_intents);
        self.events.extend(enter.events);
        if !keep_commit {
            self.commit = enter.commit;
        }
    }
}
