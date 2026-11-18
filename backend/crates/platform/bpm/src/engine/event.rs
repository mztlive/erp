//! 中性 BPM 领域事件。不得包含 ERP URL、权限名、业务命令或通知模板。
//!
//! 序列化约定：引擎计划类型（`TransitionPlan`、`BpmEventKind`、`CommitRequired`、
//! `TaskIntent`、`TaskCloseReason`、`Eligibility`）不做持久化，只透出模型快照
//! （`ApprovalProcessInstance`/`ApprovalNodeExecution` 由 `model` 负责 serde）；
//! 如确需透出计划，请按需派生并同步更新本约定。

use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use crate::model::ParticipantId;
use crate::model::types::ApprovalBlockerCode;

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
    /// 历史事件：旧规则下驳回后进入下一轮。新命令不再产生。
    RoundRestarted,
    /// 实例最终通过。
    InstanceApproved,
    /// 实例已取消。
    InstanceCancelled,
    /// 实例受阻。
    InstanceBlocked,
    /// 原审批人恢复。
    AssigneeRecovered,
    /// 旧执行已被替换。
    ExecutionSuperseded,
}

impl BpmEventKind {
    /// 将事件种类映射成稳定的大写下划线代码，供调用方识别事件，不依赖 Rust 变体名。
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
    /// 引擎组装迁移计划时，以种类、实例和轮次建立中性事件；只为该事件确有的执行、原因等信息填充可选字段。
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

    /// 为节点进入、决定或执行替换等事件写入执行 ID，便于消费者定位本轮中的具体执行记录。
    pub fn with_execution(mut self, execution_id: ApprovalNodeExecutionId) -> Self {
        self.execution_id = Some(execution_id);
        self
    }

    /// 为节点相关事件附上定义内的稳定键，便于定位审批步骤而不依赖展示名称。
    pub fn with_node_key(mut self, node_key: impl Into<String>) -> Self {
        self.node_key = Some(node_key.into());
        self
    }

    /// 事件涉及明确的启动人、决定人或取消人时，记录引擎收到的参与人身份。
    pub fn with_actor(mut self, actor: ParticipantId) -> Self {
        self.actor = Some(actor);
        self
    }

    /// 取消等动作需要携带事由时附上摘要；原样写入，由调用方完成规范化。
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// 为受阻或取消事件附上结构化阻塞码，使消费方能够分类处理而不必解析事由文本。
    pub fn with_blocker(mut self, code: ApprovalBlockerCode) -> Self {
        self.blocker_code = Some(code);
        self
    }
}
