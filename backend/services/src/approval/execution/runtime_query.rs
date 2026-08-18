//! 运行查询合同：列表 view/status、恢复动作与实例摘要映射。
//!
//! 纯函数，供 HTTP 面 Service 与单测共用，不访问 MongoDB。

use bpm::model::types::ApprovalBlockerCode;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};

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
    /// 改派当前节点。
    ReassignCurrentApprover,
    /// 受阻取消。
    CancelBlocked,
}

/// 规范化列表 view 与 status 组合。
///
/// # 参数
/// * `view` - 固定视图
/// * `status` - 可选状态
///
/// # 错误
/// 非法组合返回 422 语义的校验错误，不得返回伪空列表。
pub fn ensure_list_view_status(
    view: RuntimeInstanceListView,
    status: Option<RuntimeInstanceStatusFilter>,
) -> Result<()> {
    match (view, status) {
        (RuntimeInstanceListView::Mine, None | Some(RuntimeInstanceStatusFilter::Running)) => Ok(()),
        (RuntimeInstanceListView::Blocked, None | Some(RuntimeInstanceStatusFilter::Blocked)) => Ok(()),
        (RuntimeInstanceListView::Started | RuntimeInstanceListView::Managed, _) => Ok(()),
        (RuntimeInstanceListView::Mine, _) | (RuntimeInstanceListView::Blocked, _) => Err(
            Error::ValidationError("当前 view 与 status 组合不合法".to_string()),
        ),
    }
}

/// 按当前 blocker 返回唯一合法恢复动作。
///
/// # 参数
/// * `blocked` - 实例是否 BLOCKED
/// * `blocker` - 结构化 blocker
///
/// # 返回
/// 人员失效返回恢复与改派；其它 blocker 只返回受阻取消；非 BLOCKED 返回空。
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
    if code.allows_personnel_reassign() {
        vec![
            RuntimeRecoveryAction::ResumeCurrentApprover,
            RuntimeRecoveryAction::ReassignCurrentApprover,
        ]
    } else {
        vec![RuntimeRecoveryAction::CancelBlocked]
    }
}

/// 定义期候选人静态过滤。
///
/// # 参数
/// * `name` - 账号显示名
/// * `account` - 登录账号
/// * `search` - 检索串
///
/// # 返回
/// 空检索匹配全部；否则名称或账号包含检索串（忽略大小写）。
///
/// # 错误
/// 无。
pub fn definition_assignee_matches(name: &str, account: &str, search: &str) -> bool {
    let needle = search.trim();
    if needle.is_empty() {
        return true;
    }
    let needle = needle.to_lowercase();
    name.to_lowercase().contains(&needle) || account.to_lowercase().contains(&needle)
}

#[cfg(test)]
mod tests {
    use super::{
        definition_assignee_matches, ensure_list_view_status, recovery_options_for, RuntimeInstanceListView,
        RuntimeInstanceStatusFilter, RuntimeRecoveryAction,
    };
    use bpm::model::types::ApprovalBlockerCode;

    /// mine 只接受省略或 RUNNING；blocked 只接受省略或 BLOCKED。
    #[test]
    fn list_view_status_contract() {
        assert!(ensure_list_view_status(RuntimeInstanceListView::Mine, None).is_ok());
        assert!(ensure_list_view_status(
            RuntimeInstanceListView::Mine,
            Some(RuntimeInstanceStatusFilter::Running)
        )
        .is_ok());
        assert!(ensure_list_view_status(
            RuntimeInstanceListView::Mine,
            Some(RuntimeInstanceStatusFilter::Blocked)
        )
        .is_err());
        assert!(ensure_list_view_status(RuntimeInstanceListView::Blocked, None).is_ok());
        assert!(ensure_list_view_status(
            RuntimeInstanceListView::Blocked,
            Some(RuntimeInstanceStatusFilter::Running)
        )
        .is_err());
        assert!(ensure_list_view_status(
            RuntimeInstanceListView::Started,
            Some(RuntimeInstanceStatusFilter::Cancelled)
        )
        .is_ok());
    }

    /// 人员失效给恢复/改派；结构 blocker 只给受阻取消。
    #[test]
    fn recovery_options_follow_blocker_kind() {
        assert!(recovery_options_for(false, Some(ApprovalBlockerCode::ApproverAccountInactive)).is_empty());
        assert_eq!(
            recovery_options_for(true, Some(ApprovalBlockerCode::ApproverAccountInactive)),
            vec![
                RuntimeRecoveryAction::ResumeCurrentApprover,
                RuntimeRecoveryAction::ReassignCurrentApprover
            ]
        );
        assert_eq!(
            recovery_options_for(true, Some(ApprovalBlockerCode::OpenTaskConflict)),
            vec![RuntimeRecoveryAction::CancelBlocked]
        );
    }

    /// 定义期候选人按姓名或账号包含匹配。
    #[test]
    fn definition_assignee_search_is_case_insensitive() {
        assert!(definition_assignee_matches("张三", "zhangsan", ""));
        assert!(definition_assignee_matches("张三", "zhangsan", "张"));
        assert!(definition_assignee_matches("张三", "ZhangSan", "zhang"));
        assert!(!definition_assignee_matches("张三", "zhangsan", "lisi"));
    }
}
