//! ERP 审批集成仓储访问器。

use entities::approval_integration::{ApprovalNotificationOutbox, ApprovalSubjectSnapshot};
use mongodb::Database;

use crate::Repository;

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
    /// 返回 `Repository<'_, ApprovalSubjectSnapshot>`。
    fn approval_subject_snapshots(&self) -> Repository<'_, ApprovalSubjectSnapshot>;

    /// 返回通知 outbox 仓储。
    ///
    /// # 返回
    /// 返回 `Repository<'_, ApprovalNotificationOutbox>`。
    fn approval_notification_outbox(&self) -> Repository<'_, ApprovalNotificationOutbox>;
}

impl ApprovalIntegrationExt for Database {
    fn approval_subject_snapshots(&self) -> Repository<'_, ApprovalSubjectSnapshot> {
        Repository::new(self, Self::APPROVAL_SUBJECT_SNAPSHOTS)
    }

    fn approval_notification_outbox(&self) -> Repository<'_, ApprovalNotificationOutbox> {
        Repository::new(self, Self::APPROVAL_NOTIFICATION_OUTBOX)
    }
}
