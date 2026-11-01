//! 供应商合作条款申请；正式商务版本仅由内部确认流程写入。

#[cfg(test)]
mod tests;

mod application;
mod confirmed;
mod dto;
mod receipt;
mod repository;

pub use application::{CooperationApplication, CooperationDecision, CooperationStatus, FrozenSubmission};
pub use confirmed::{ConfirmedCooperation, plan_confirmed_cooperation};
pub use dto::{CooperationRequest, CooperationResult, CooperationSubmissionView, CooperationView};
pub use receipt::{CooperationReceipt, cooperation_command};
pub use repository::{COOPERATION_APPLICATIONS, COOPERATION_RECEIPTS, CooperationRepository, ensure_indexes};
