//! 运行查询合同：列表 view/status、恢复动作与实例摘要映射。
//!
//! 纯函数，供 HTTP 面 Service 与单测共用，不访问 MongoDB。

use bpm::model::types::ApprovalBlockerCode;
use serde::{Deserialize, Serialize};

/// 实例列表固定视图。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeInstanceListView {
    /// 本人当前开放审批任务。
    Mine,
    /// 本人发起。
    Started,
    /// 运行管理范围。
    Managed,
    /// 受阻管理子集。
    Blocked,
}

impl RuntimeInstanceListView {
    /// 返回稳定 view 名。
    ///
    /// # 返回
    /// 返回 `mine` / `started` / `managed` / `blocked`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mine => "mine",
            Self::Started => "started",
            Self::Managed => "managed",
            Self::Blocked => "blocked",
        }
    }
}

/// 合同冻结的实例状态过滤。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeInstanceStatusFilter {
    /// 运行中。
    Running,
    /// 已通过。
    Approved,
    /// 已取消。
    Cancelled,
    /// 受阻。
    Blocked,
}

impl RuntimeInstanceStatusFilter {
    /// 返回稳定状态码。
    ///
    /// # 返回
    /// 返回 `RUNNING` 等。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Approved => "APPROVED",
            Self::Cancelled => "CANCELLED",
            Self::Blocked => "BLOCKED",
        }
    }
}

/// 恢复端口允许的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeRecoveryAction {
    /// 原审批人恢复后继续。
    ResumeCurrentApprover,
    /// 受阻取消。
    CancelBlocked,
}

/// 按当前 blocker 返回唯一合法恢复动作。
///
/// # 参数
/// * `blocked` - 实例是否 BLOCKED
/// * `blocker` - 结构化 blocker
///
/// # 返回
/// 可恢复原审批人的 blocker 只允许恢复；其它 blocker 只返回受阻取消；非 BLOCKED 返回空。
///
/// # 错误
/// 无。
pub fn recovery_options_for(
    blocked: bool,
    blocker: Option<ApprovalBlockerCode>,
) -> Vec<RuntimeRecoveryAction> {
    if !blocked {
        return Vec::new();
    }
    let Some(code) = blocker else {
        return vec![RuntimeRecoveryAction::CancelBlocked];
    };
    if code.allows_assignee_recovery() {
        vec![RuntimeRecoveryAction::ResumeCurrentApprover]
    } else {
        vec![RuntimeRecoveryAction::CancelBlocked]
    }
}

#[cfg(test)]
mod tests {
    use super::{recovery_options_for, RuntimeRecoveryAction};
    use bpm::model::types::ApprovalBlockerCode;

    /// 可恢复原审批人的 blocker 只给恢复；结构 blocker 只给受阻取消。
    #[test]
    fn recovery_options_follow_blocker_kind() {
        assert!(recovery_options_for(false, Some(ApprovalBlockerCode::ApproverAccountInactive)).is_empty());
        assert_eq!(
            recovery_options_for(true, Some(ApprovalBlockerCode::ApproverAccountInactive)),
            vec![RuntimeRecoveryAction::ResumeCurrentApprover]
        );
        assert_eq!(
            recovery_options_for(true, Some(ApprovalBlockerCode::OpenTaskConflict)),
            vec![RuntimeRecoveryAction::CancelBlocked]
        );
    }
}
