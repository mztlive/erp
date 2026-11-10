//! 消费方拥有的端口：授权、对象读取、升级事实与工作项事实。
pub mod approver_directory;

mod audit;
mod authorization;
mod binding_facts;
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
#[cfg(test)]
pub(crate) use authorization::FailClosedWorkflowAuthorizationPort;
pub use authorization::{TaskReadAction, WorkflowAuthorizationPort};
pub use binding_facts::ApprovalBindingFacts;
pub use object_facts::{
    FailClosedObjectFactPort, ObjectFact, ObjectFactKey, ObjectFactMap, ObjectFactPort, ObjectKind,
    OrderTaskSource, SubjectBrief, W29CloseFact,
};
pub use upgrade_subject::{ApprovalUpgradeSubjectFacts, FailClosedUpgradeSubjectPort, UpgradeSubjectPort};
