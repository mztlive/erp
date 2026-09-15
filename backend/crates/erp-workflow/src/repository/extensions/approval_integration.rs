//! ERP 审批集成仓储访问器。

use mongodb::Database;

use crate::repository::owned::{ApprovalNotificationOutboxRepository, ApprovalSubjectSnapshotRepository};

/// 审批集成仓储访问器。只暴露业务对象快照与通知 outbox。
pub trait ApprovalIntegrationExt {
    /// `approval_subject_snapshots` 集合名。
    const APPROVAL_SUBJECT_SNAPSHOTS: &'static str = "approval_subject_snapshots";
    /// `approval_notification_outbox` 集合名。
    const APPROVAL_NOTIFICATION_OUTBOX: &'static str = "approval_notification_outbox";
    /// `approval_command_receipts` 集合名；与 BPM 聚合共用同一收据集合。
    const APPROVAL_COMMAND_RECEIPTS: &'static str = "approval_command_receipts";

    /// 返回业务对象快照仓储。
    ///
    /// # 返回
    /// 返回 `ApprovalSubjectSnapshotRepository<'_>`。
    fn approval_subject_snapshots(&self) -> ApprovalSubjectSnapshotRepository<'_>;

    /// 返回通知 outbox 仓储。
    ///
    /// # 返回
    /// 返回 `ApprovalNotificationOutboxRepository<'_>`。
    fn approval_notification_outbox(&self) -> ApprovalNotificationOutboxRepository<'_>;
}

impl ApprovalIntegrationExt for Database {
    fn approval_subject_snapshots(&self) -> ApprovalSubjectSnapshotRepository<'_> {
        ApprovalSubjectSnapshotRepository::new(self, Self::APPROVAL_SUBJECT_SNAPSHOTS)
    }

    fn approval_notification_outbox(&self) -> ApprovalNotificationOutboxRepository<'_> {
        ApprovalNotificationOutboxRepository::new(self, Self::APPROVAL_NOTIFICATION_OUTBOX)
    }
}

#[cfg(test)]
mod tests {
    use super::ApprovalIntegrationExt;

    #[test]
    fn integration_collection_names_match_target_runtime() {
        assert_eq!(mongodb::Database::APPROVAL_SUBJECT_SNAPSHOTS, "approval_subject_snapshots");
        assert_eq!(mongodb::Database::APPROVAL_NOTIFICATION_OUTBOX, "approval_notification_outbox");
        assert_eq!(mongodb::Database::APPROVAL_COMMAND_RECEIPTS, "approval_command_receipts");
        assert_ne!(mongodb::Database::APPROVAL_SUBJECT_SNAPSHOTS, "approval_definitions");
        assert_ne!(mongodb::Database::APPROVAL_NOTIFICATION_OUTBOX, "approval_step_instances");
    }
}
