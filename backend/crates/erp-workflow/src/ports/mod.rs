//! 消费方拥有的端口：授权、对象读取、升级事实与工作项事实。

mod audit;
mod authorization;
mod data_scope;
mod object_facts;
mod object_read;
mod upgrade_subject;

#[cfg(test)]
pub use audit::WorkflowAuditFact;
pub use audit::{
    FailClosedAuditPort, PreparedWorkflowAudit, WorkflowAuditAttemptResult, WorkflowAuditOperation,
    WorkflowAuditPort,
};
pub use authorization::{
    FailClosedWorkflowAuthorizationPort, RolePermissionSnapshotFact, WorkflowAccountFact,
    WorkflowAuthorizationPort, WorkflowPolicyWrite, WorkflowQueueAccessFact, permission_covers,
};
pub use data_scope::{WorkflowDataScope, WorkflowScopeObject, WorkflowScopeObjects, WorkflowScopePredicate};
pub use object_facts::{
    FailClosedObjectFactPort, ObjectFact, ObjectFactKey, ObjectFactMap, ObjectFactPort, ObjectKind,
    OrderTaskSource, SubjectBrief, W29CloseFact,
};
pub use object_read::{ApprovalObjectReadPort, FailClosedObjectReadPort};
pub use upgrade_subject::{ApprovalUpgradeSubjectFacts, FailClosedUpgradeSubjectPort, UpgradeSubjectPort};
