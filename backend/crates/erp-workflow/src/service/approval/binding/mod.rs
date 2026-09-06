//! 单据与当前已发布定义的统一绑定端口。
//!
//! 端口接收调用方 `Executor`，不得自行开嵌套事务，也不得把执行器传入 BPM。

mod bind;
mod revalidate;
mod types;
mod upgrade;

use crate::error::{Error, ErrorCode, Result};

use super::policy::ApprovalRequirement;

/// 绑定审计动作。
pub const DEFINITION_BOUND_AUDIT_ACTION: &str = "approval.definition.bound";
/// 无审批政策事实审计动作。
pub const DEFINITION_POLICY_AUDIT_ACTION: &str = "approval.definition.policy";
/// 未提交升级审计动作。
pub const DEFINITION_UPGRADED_AUDIT_ACTION: &str = "approval.definition.upgraded";

pub use bind::{
    attach_published_binding, bind_published_definition_on_document_create, binding_from_published,
};
pub use types::{
    BindPublishedDefinitionCommand, BindingDecision, UpgradeBindingOutcome, UpgradeBindingResultView,
    UpgradeBindingView, UpgradeUnsubmittedDefinitionCommand,
};
pub use upgrade::{replay_unsubmitted_document_definition_upgrade, upgrade_unsubmitted_document_definition};

/// 按政策决定是否查询发布定义。
///
/// # 返回
/// `NO_APPROVAL` 跳过；`PROCESS_REQUIRED` 必须绑定。
pub fn binding_decision(requirement: ApprovalRequirement) -> BindingDecision {
    match requirement {
        ApprovalRequirement::NoApproval => BindingDecision::SkipNoApproval,
        ApprovalRequirement::ProcessRequired => BindingDecision::RequirePublished,
    }
}

/// 必须审批且缺失发布定义时失败关闭。
///
/// # 错误
/// 返回 `APPROVAL_PROCESS_NOT_CONFIGURED`。
pub fn published_definition_or_not_configured<T>(published: Option<T>) -> Result<T> {
    published.ok_or_else(process_not_configured)
}

/// 构造未配置流程的稳定冲突。
///
/// # 返回
/// 返回 `APPROVAL_PROCESS_NOT_CONFIGURED`。
pub fn process_not_configured() -> Error {
    Error::from_approval_code(ErrorCode::ApprovalProcessNotConfigured)
}

#[cfg(test)]
mod tests;
