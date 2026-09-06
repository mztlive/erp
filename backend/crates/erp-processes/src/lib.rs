//! Cross-domain processes: approval dispatch, audited transactions and named use cases.

pub mod approval_dispatch;
pub mod audit;
pub mod catalog;
pub mod customer;
pub mod party;
pub mod source_registry;
pub mod supplier;
pub mod warehouse;

pub use approval_dispatch::ApprovalActionRegistry;
pub use audit::run_audited;
