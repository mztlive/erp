//! 消费方拥有的端口：授权、对象读取、升级事实与工作项事实。
pub mod approver_directory;

mod audit;
mod authorization;
mod data_scope;
mod finance_responsibility;
pub use finance_responsibility::FinanceResponsibilityPort;
mod object_facts;
mod upgrade_subject;

#[cfg(test)]
pub use audit::WorkflowAuditFact;
pub use audit::{
    FailClosedAuditPort, PreparedWorkflowAudit, WorkflowAuditAttemptResult, WorkflowAuditOperation,
    WorkflowAuditPort,
};
pub use authorization::{FailClosedWorkflowAuthorizationPort, WorkflowAuthorizationPort};
pub use data_scope::{WorkflowScopeObject, WorkflowScopeObjects};
pub use object_facts::{
    FailClosedObjectFactPort, ObjectFact, ObjectFactKey, ObjectFactMap, ObjectFactPort, ObjectKind,
    OrderTaskSource, SubjectBrief, W29CloseFact,
};
pub use upgrade_subject::{ApprovalUpgradeSubjectFacts, FailClosedUpgradeSubjectPort, UpgradeSubjectPort};
