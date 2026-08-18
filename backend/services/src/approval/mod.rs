//! D03 审批定义 bootstrap、稳定运行时端口与阻塞恢复应用层。

mod action;
pub mod binding;
mod bootstrap;
pub mod business_adapter;
pub mod definition;
pub mod definition_assignees;
pub mod definition_dto;
mod dto;
pub mod execution;
pub mod policy;
pub mod process_kind;
mod registry;
mod resolver;
mod runtime;
mod scope;

pub use action::{
    ApprovalActionContext, ApprovalActionFuture, ApprovalDomainActionPort, FailClosedApprovalActionPort,
};
pub use bootstrap::ensure_approval_definitions;
pub use dto::{
    ApprovalInstanceView, ApprovalRecoveryAction, ApprovalRecoveryAuthorization, ApprovalRuntimeView,
    ApprovalStepInstanceView, ApprovalWorkItemView, BlockedApprovalListParams, BlockedApprovalPage,
    BlockedApprovalView, CancelApprovalCommand, RecoverApprovalCommand, StartApprovalCommand,
    SubmitDecisionCommand,
};
pub use registry::{
    ApprovalBusinessAction, CARD_SALES_APPROVAL, CARD_SALES_APPROVAL_VERSION, OPERATIONS_APPROVAL,
    SALES_MANAGER_APPROVAL,
};
pub use resolver::ApprovalAssigneeResolver;
pub use runtime::{ApprovalRuntimeFuture, ApprovalRuntimePort, InternalApprovalRuntime};
pub use scope::{
    approval_management_scope, approval_recovery_authorization, approval_recovery_authorization_scope,
    approval_recovery_scope, ApprovalManagementScope,
};
