//! 应用组合根拥有审批动作分派与对象读取策略装配。
mod action_registry;
mod error;
mod object_read;
pub use action_registry::ApprovalActionRegistry;
pub use object_read::{ApprovalObjectRead, adapter_object_read_decision, require_wired_object_read};

mod payment_restoration;
pub(crate) use payment_restoration::PaymentRequestRestoration;
