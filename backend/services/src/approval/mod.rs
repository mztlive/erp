//! D03 审批政策、定义管理与目标运行编排。
//!
//! 旧 bootstrap、编译期 registry、resolver 与 runtime 已删除。
//! 目标运行只通过 `execution` 进入 BPM，不得回退旧步骤模型。

mod action;
pub mod binding;
pub mod business_adapter;
pub mod definition;
pub mod definition_assignees;
pub mod definition_dto;
mod dto;
pub mod execution;
pub mod policy;
pub mod process_kind;
mod scope;

pub use action::{
    ApprovalActionContext, ApprovalActionFuture, ApprovalDomainActionPort, FailClosedApprovalActionPort,
};
pub use dto::{
    ApprovalCancelBlockedCommand, ApprovalCancelCommand, ApprovalDecisionCommand, ApprovalReassignCommand,
    ApprovalRecoveryAuthorization, ApprovalResumeCommand, ApprovalStartCommand,
};
pub use scope::{
    approval_management_scope, approval_recovery_authorization, approval_recovery_authorization_scope,
    approval_recovery_scope, ApprovalManagementScope,
};
