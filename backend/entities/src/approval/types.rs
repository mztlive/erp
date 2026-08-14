//! 审批定义、运行实例与步骤共享的固定枚举。

use serde::{Deserialize, Serialize};

use crate::common::state::DocumentState;

/// 审批运行时类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalRuntimeKind {
    /// ERP 内部事务运行时。
    Internal,
    /// 外部 BPM 运行时。
    Bpm,
}

impl ApprovalRuntimeKind {
    /// 返回运行时类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化和查询的固定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Internal => "INTERNAL",
            Self::Bpm => "BPM",
        }
    }
}

/// 审批定义状态。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalDefinitionStatus {
    /// 草稿，可由受控定义发布流程修改。
    #[default]
    Draft,
    /// 已发布，内容永久冻结且允许启动实例。
    Published,
    /// 已退役，不再允许启动新实例。
    Retired,
}

impl ApprovalDefinitionStatus {
    /// 返回定义状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化和查询的固定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Published => "PUBLISHED",
            Self::Retired => "RETIRED",
        }
    }
}

impl DocumentState for ApprovalDefinitionStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::Published],
            Self::Published => &[Self::Retired],
            Self::Retired => &[],
        }
    }
}

/// 人工步骤分派模式。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalAssignmentMode {
    /// 解析并直接指派给唯一用户。
    Direct,
    /// 进入责任池，由符合资格的用户开始处理。
    Pool,
}

impl ApprovalAssignmentMode {
    /// 返回分派模式的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化和查询的固定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "DIRECT",
            Self::Pool => "POOL",
        }
    }
}

/// 审批步骤允许形成的固定决定。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalDecision {
    /// 通过当前步骤。
    Approve,
    /// 驳回申请人并结束本次审批。
    RejectToApplicant,
    /// 终止审批并执行定义绑定的终止动作。
    TerminateApproval,
}

impl ApprovalDecision {
    /// 返回审批决定的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化和查询的固定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "APPROVE",
            Self::RejectToApplicant => "REJECT_TO_APPLICANT",
            Self::TerminateApproval => "TERMINATE_APPROVAL",
        }
    }
}

/// 审批实例状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalInstanceStatus {
    /// 正在运行。
    Running,
    /// 全部步骤已通过。
    Approved,
    /// 已驳回申请人。
    Rejected,
    /// 已终止审批。
    Terminated,
    /// 已由受控撤回流程取消。
    Cancelled,
    /// 因结构化原因无法安全推进。
    Blocked,
}

impl ApprovalInstanceStatus {
    /// 返回实例状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化和查询的固定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
            Self::Terminated => "TERMINATED",
            Self::Cancelled => "CANCELLED",
            Self::Blocked => "BLOCKED",
        }
    }

    /// 判断实例是否已进入不可逆终态。
    ///
    /// # 返回
    /// 已通过、驳回、终止或取消时返回 `true`。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Approved | Self::Rejected | Self::Terminated | Self::Cancelled
        )
    }
}

impl DocumentState for ApprovalInstanceStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Running => &[
                Self::Approved,
                Self::Rejected,
                Self::Terminated,
                Self::Cancelled,
                Self::Blocked,
            ],
            Self::Blocked => &[Self::Running, Self::Cancelled],
            Self::Approved | Self::Rejected | Self::Terminated | Self::Cancelled => &[],
        }
    }
}

/// 审批步骤实例状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalStepStatus {
    /// 等待前置步骤完成，不得创建待办。
    Waiting,
    /// 当前活动步骤，必须有一个开放待办。
    Active,
    /// 已通过。
    Approved,
    /// 已驳回申请人。
    Rejected,
    /// 已终止审批。
    Terminated,
    /// 已取消。
    Cancelled,
    /// 当前步骤因结构化原因无法安全推进。
    Blocked,
}

impl ApprovalStepStatus {
    /// 返回步骤状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化和查询的固定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "WAITING",
            Self::Active => "ACTIVE",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
            Self::Terminated => "TERMINATED",
            Self::Cancelled => "CANCELLED",
            Self::Blocked => "BLOCKED",
        }
    }

    /// 判断步骤是否为当前运行位置。
    ///
    /// # 返回
    /// 活动或阻塞步骤返回 `true`。
    pub fn is_current(self) -> bool {
        matches!(self, Self::Active | Self::Blocked)
    }

    /// 判断步骤是否已进入不可逆终态。
    ///
    /// # 返回
    /// 已通过、驳回、终止或取消时返回 `true`。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Approved | Self::Rejected | Self::Terminated | Self::Cancelled
        )
    }
}

impl DocumentState for ApprovalStepStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Waiting => &[Self::Active, Self::Blocked, Self::Cancelled],
            Self::Active => &[
                Self::Approved,
                Self::Rejected,
                Self::Terminated,
                Self::Cancelled,
                Self::Blocked,
            ],
            Self::Blocked => &[Self::Active, Self::Cancelled],
            Self::Approved | Self::Rejected | Self::Terminated | Self::Cancelled => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApprovalAssignmentMode, ApprovalDecision, ApprovalDefinitionStatus, ApprovalInstanceStatus,
        ApprovalRuntimeKind, ApprovalStepStatus,
    };
    use crate::common::state::ensure_transition;

    #[test]
    fn stable_codes_match_the_approval_contract() {
        assert_eq!(ApprovalRuntimeKind::Internal.as_str(), "INTERNAL");
        assert_eq!(ApprovalDefinitionStatus::Published.as_str(), "PUBLISHED");
        assert_eq!(ApprovalAssignmentMode::Pool.as_str(), "POOL");
        assert_eq!(
            ApprovalDecision::RejectToApplicant.as_str(),
            "REJECT_TO_APPLICANT"
        );
        assert_eq!(ApprovalInstanceStatus::Blocked.as_str(), "BLOCKED");
        assert_eq!(ApprovalStepStatus::Waiting.as_str(), "WAITING");
    }

    #[test]
    fn terminal_instance_and_step_states_cannot_reopen() {
        assert!(ensure_transition(ApprovalInstanceStatus::Running, ApprovalInstanceStatus::Approved).is_ok());
        assert!(ensure_transition(ApprovalInstanceStatus::Blocked, ApprovalInstanceStatus::Running).is_ok());
        assert!(
            ensure_transition(ApprovalInstanceStatus::Approved, ApprovalInstanceStatus::Running).is_err()
        );
        assert!(ensure_transition(ApprovalStepStatus::Waiting, ApprovalStepStatus::Active).is_ok());
        assert!(ensure_transition(ApprovalStepStatus::Blocked, ApprovalStepStatus::Active).is_ok());
        assert!(ensure_transition(ApprovalStepStatus::Rejected, ApprovalStepStatus::Active).is_err());
    }

    #[test]
    fn enums_serialize_to_contract_codes() {
        assert_eq!(
            serde_json::to_string(&ApprovalDecision::TerminateApproval).unwrap(),
            "\"TERMINATE_APPROVAL\""
        );
        assert_eq!(
            serde_json::to_string(&ApprovalRuntimeKind::Bpm).unwrap(),
            "\"BPM\""
        );
    }
}
