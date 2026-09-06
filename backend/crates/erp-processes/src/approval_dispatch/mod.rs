//! Approval domain-action dispatch.

mod action_registry;
mod object_read;
mod sales_subject;
mod upgrade_subject;

pub use action_registry::ApprovalActionRegistry;
pub use object_read::{adapter_object_read_decision, require_wired_object_read, ProcessObjectRead};
pub use sales_subject::{document_type_of_sales_business, subject_ref_for_sales_business};
pub use upgrade_subject::{
    ensure_initial_unsubmitted_approval_upgrade_subject, load_approval_upgrade_subject_facts,
    ProcessUpgradeSubject,
};
