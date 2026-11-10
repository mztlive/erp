//! 应用组合根拥有审批动作分派与对象读取策略装配。
mod action_registry;
mod error;
pub use action_registry::ApprovalActionRegistry;

mod payment_restoration;
pub(crate) use payment_restoration::PaymentRequestRestoration;
