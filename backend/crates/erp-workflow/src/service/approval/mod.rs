//! D03 审批政策、定义管理与目标运行编排。
//!
//! 旧 bootstrap、编译期 registry、resolver 与 runtime 已删除。
//! 目标运行只通过 `execution` 进入 BPM，不得回退旧步骤模型。

pub mod action;
pub mod binding;
pub mod business_adapter;
pub mod definition;
pub mod definition_assignees;
pub mod definition_dto;
use crate::dto::approval as dto;
pub mod execution;
pub mod policy;
pub mod process_kind;
mod scope;
pub mod upgrade_subject;

pub use action::{
    ApprovalActionContext, ApprovalActionFuture, ApprovalDomainActionPort, BlockedCancelActionParams,
    DecisionActionParams, FailClosedApprovalActionPort,
};
pub use dto::{
    ApprovalCancelBlockedCommand, ApprovalCancelCommand, ApprovalDecisionCommand,
    ApprovalRecoveryAuthorization, ApprovalResumeCommand, ApprovalStartCommand,
};
pub use scope::{
    approval_actor_is_active, approval_actor_is_active_with_executor,
    approval_cancel_blocked_scope_with_executor, approval_cancel_scope_with_executor,
    approval_decide_scope_with_executor, approval_document_action_scope_with_executor,
    approval_document_read_scope, approval_document_read_scope_with_executor,
    definition_management_visibility_with_executor,
};
pub use scope::{
    approval_recovery_authorization, approval_recovery_authorization_scope, approval_recovery_scope,
    ApprovalManagementScope,
};
pub use upgrade_subject::{
    ensure_initial_unsubmitted_approval_upgrade_subject, load_approval_upgrade_subject_facts,
    ApprovalUpgradeSubjectFacts,
};
