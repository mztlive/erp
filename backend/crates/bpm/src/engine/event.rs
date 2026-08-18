//! 中性 BPM 领域事件。不得包含 ERP URL、权限名、业务命令或通知模板。

use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use crate::model::types::ApprovalBlockerCode;
use crate::model::ParticipantId;

/// 中性领域事件种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BpmEventKind {
    /// 实例已启动。
    InstanceStarted,
    /// 令牌进入节点。
    NodeEntered,
    /// 当前节点已通过。
    NodeApproved,
    /// 当前节点已驳回。
    NodeRejected,
    /// 驳回后进入下一轮。
    RoundRestarted,
    /// 实例最终通过。
    InstanceApproved,
    /// 实例已取消。
    InstanceCancelled,
    /// 实例受阻。
    InstanceBlocked,
    /// 原审批人恢复。
    AssigneeRecovered,
    /// 管理员改派。
    AssigneeReassigned,
    /// 旧执行已被替换。
    ExecutionSuperseded,
}

impl BpmEventKind {
    /// 返回稳定事件代码。
    ///
    /// # 返回
    /// 返回大写下划线代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InstanceStarted => "INSTANCE_STARTED",
            Self::NodeEntered => "NODE_ENTERED",
            Self::NodeApproved => "NODE_APPROVED",
            Self::NodeRejected => "NODE_REJECTED",
            Self::RoundRestarted => "ROUND_RESTARTED",
            Self::InstanceApproved => "INSTANCE_APPROVED",
            Self::InstanceCancelled => "INSTANCE_CANCELLED",
            Self::InstanceBlocked => "INSTANCE_BLOCKED",
            Self::AssigneeRecovered => "ASSIGNEE_RECOVERED",
            Self::AssigneeReassigned => "ASSIGNEE_REASSIGNED",
            Self::ExecutionSuperseded => "EXECUTION_SUPERSEDED",
        }
    }
}

/// 中性领域事件。
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BpmEvent {
    /// 事件种类。
    pub kind: BpmEventKind,
    /// 所属实例。
    pub instance_id: ApprovalProcessInstanceId,
    /// 相关执行。
    pub execution_id: Option<ApprovalNodeExecutionId>,
    /// 相关节点。
    pub node_key: Option<String>,
    /// 相关轮次。
    pub round_no: u32,
    /// 动作发起人。
    pub actor: Option<ParticipantId>,
    /// 原因摘要。
    pub reason: Option<String>,
    /// 结构化阻塞码。
    pub blocker_code: Option<ApprovalBlockerCode>,
}

impl BpmEvent {
    /// 构造中性领域事件。
    ///
    /// # 参数
    /// * `kind` - 事件种类
    /// * `instance_id` - 所属实例
    /// * `round_no` - 相关轮次
    ///
    /// # 返回
    /// 返回其余字段为空的事件。
    pub fn new(kind: BpmEventKind, instance_id: ApprovalProcessInstanceId, round_no: u32) -> Self {
        Self {
            kind,
            instance_id,
            execution_id: None,
            node_key: None,
            round_no,
            actor: None,
            reason: None,
            blocker_code: None,
        }
    }

    /// 绑定相关执行。
    ///
    /// # 参数
    /// * `execution_id` - 执行主键
    ///
    /// # 返回
    /// 返回更新后的事件。
    pub fn with_execution(mut self, execution_id: ApprovalNodeExecutionId) -> Self {
        self.execution_id = Some(execution_id);
        self
    }

    /// 绑定节点键。
    ///
    /// # 参数
    /// * `node_key` - 节点键
    ///
    /// # 返回
    /// 返回更新后的事件。
    pub fn with_node_key(mut self, node_key: impl Into<String>) -> Self {
        self.node_key = Some(node_key.into());
        self
    }

    /// 绑定动作发起人。
    ///
    /// # 参数
    /// * `actor` - 处理人
    ///
    /// # 返回
    /// 返回更新后的事件。
    pub fn with_actor(mut self, actor: ParticipantId) -> Self {
        self.actor = Some(actor);
        self
    }

    /// 绑定原因摘要。
    ///
    /// # 参数
    /// * `reason` - 已规范化原因
    ///
    /// # 返回
    /// 返回更新后的事件。
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// 绑定结构化阻塞码。
    ///
    /// # 参数
    /// * `code` - 阻塞码
    ///
    /// # 返回
    /// 返回更新后的事件。
    pub fn with_blocker(mut self, code: ApprovalBlockerCode) -> Self {
        self.blocker_code = Some(code);
        self
    }
}
