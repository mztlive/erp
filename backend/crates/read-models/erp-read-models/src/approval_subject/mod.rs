//! 审批升级主体的跨域只读事实；复用调用方 Executor，不执行业务写入。
mod sales_subject;
mod upgrade_documents;
mod upgrade_shared;
mod upgrade_subject;
pub use sales_subject::{document_type_of_sales_business, subject_ref_for_sales_business};
pub use upgrade_subject::{
    ApprovalUpgradeReadModel, ApprovalUpgradeSubjectFacts,
    ensure_initial_unsubmitted_approval_upgrade_subject, load_approval_upgrade_subject_facts,
};
