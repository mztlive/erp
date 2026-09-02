//! ERP 审批集成实体：业务对象快照、身份映射与通知 outbox。
//!
//! 本模块不得重新定义流程定义、实例、执行、审批人或命令收据。

mod action_policy;
mod identity;
pub mod notification_outbox;
pub mod subject_snapshot;

pub use action_policy::ApprovalDomainAction;
pub use identity::{
    document_type_from_subject_kind, document_type_of, document_type_of_sales_business, process_kind_of,
    subject_ref_for, subject_ref_for_sales_business,
};
pub use notification_outbox::{
    ApprovalNotificationDeliveryStatus, ApprovalNotificationEventKind, ApprovalNotificationOutbox,
    ApprovalNotificationTemplateParams, MAX_DELIVERY_ATTEMPTS, RETRY_BACKOFF_SECS,
};
pub use subject_snapshot::{
    ApprovalSubjectCounterparty, ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload,
};
