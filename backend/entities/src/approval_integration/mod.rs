//! ERP 审批集成实体：业务对象快照与通知 outbox。
//!
//! 本模块不得重新定义流程定义、实例、执行、审批人或命令收据。

pub mod notification_outbox;
pub mod subject_snapshot;

pub use notification_outbox::{
    ApprovalNotificationDeliveryStatus, ApprovalNotificationEventKind, ApprovalNotificationOutbox,
    ApprovalNotificationTemplateParams, MAX_DELIVERY_ATTEMPTS, RETRY_BACKOFF_SECS,
};
pub use subject_snapshot::{
    ApprovalSubjectCounterparty, ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload,
};
