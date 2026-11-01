//! 导入确认任务的冻结身份与已完成事实。

use erp_core::common::time::Instant;

use super::{WorkItem, WorkItemStatus, WorkItemType};

/// 调用方从导入确认合同取得的任务身份，不携带开放状态资格。
#[derive(Debug, Clone, Copy)]
pub struct ImportConfirmationTaskIdentity<'a> {
    /// 导入流程锁定的批次对象类型。
    pub business_object_type: &'a str,
    /// 批次稳定 ID。
    pub batch_id: &'a str,
    /// 固定注册表中的确认范围。
    pub confirmation_scope: &'a str,
    /// 范围对应的责任角色。
    pub owner_role: &'a str,
    /// 本次确认对应的责任组织。
    pub owner_organization_id: &'a str,
}

impl WorkItem {
    /// 判断正式任务是否绑定指定导入批次和责任范围。
    ///
    /// # 参数
    /// * `identity` - 调用方锁定的批次、责任范围、角色与组织。
    /// # 返回
    /// 类型、对象和责任字段一致时返回 `true`，不限制生命周期状态。
    /// # 错误
    /// 不返回错误；开放任务的当前责任资格须单独校验。
    pub fn matches_import_confirmation(&self, identity: ImportConfirmationTaskIdentity<'_>) -> bool {
        self.work_item_type == WorkItemType::ImportBusinessConfirmation
            && self.matches_business_object(identity.business_object_type, identity.batch_id)
            && self.responsibility_key() == Some(identity.confirmation_scope)
            && self.owner_role == identity.owner_role
            && self.owner_organization_id == identity.owner_organization_id
    }

    /// 判断任务是否保留指定导入确认的正式完成证明。
    ///
    /// # 参数
    /// * `identity` - 原确认冻结的任务身份。
    /// * `actor_id` - 原决定人与当前认证操作人。
    /// * `decided_at` - 回执冻结的原决定时间。
    /// # 返回
    /// 未删除任务已由原责任人在原时间完成且身份一致时返回 `true`。
    /// # 错误
    /// 不返回错误；本方法接受完成终态，不复用开放任务的 `is_owned_by`。
    pub fn matches_completed_import_confirmation(
        &self,
        identity: ImportConfirmationTaskIdentity<'_>,
        actor_id: &str,
        decided_at: Instant,
    ) -> bool {
        self.matches_import_confirmation(identity)
            && !self.base.is_deleted()
            && self.status == WorkItemStatus::Completed
            && self.completed_by.as_deref() == Some(actor_id)
            && self.completed_at == Some(decided_at)
            && self.owner_user_id.as_deref() == Some(actor_id)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;

    use super::super::direct_data;
    use super::*;

    fn identity() -> ImportConfirmationTaskIdentity<'static> {
        ImportConfirmationTaskIdentity {
            business_object_type: "LEGACY_IMPORT_BATCH",
            batch_id: "batch-1",
            confirmation_scope: "SALES",
            owner_role: "sales",
            owner_organization_id: "org-1",
        }
    }

    fn task() -> WorkItem {
        WorkItem::new_with_responsibility_key(WorkItemId::new("task-1"), direct_data(), "SALES").unwrap()
    }

    #[test]
    fn import_identity_rejects_foreign_objects_and_responsibilities() {
        let task = task();
        assert!(task.matches_import_confirmation(identity()));
        for wrong in [
            ImportConfirmationTaskIdentity { business_object_type: "OTHER", ..identity() },
            ImportConfirmationTaskIdentity { batch_id: "other", ..identity() },
            ImportConfirmationTaskIdentity { confirmation_scope: "FINANCE", ..identity() },
            ImportConfirmationTaskIdentity { owner_role: "finance", ..identity() },
            ImportConfirmationTaskIdentity { owner_organization_id: "other", ..identity() },
        ] {
            assert!(!task.matches_import_confirmation(wrong));
        }
    }

    #[test]
    fn completed_import_proof_keeps_terminal_owner_separate_from_open_eligibility() {
        let mut task = task();
        let at = Instant::from_unix_secs(20);
        assert!(task.is_owned_by("alice"));
        assert!(!task.matches_completed_import_confirmation(identity(), "alice", at));
        task.complete_by_domain_command("alice", at).unwrap();
        assert!(task.matches_completed_import_confirmation(identity(), "alice", at));
        assert!(!task.is_owned_by("alice"));
        assert!(!task.matches_completed_import_confirmation(identity(), "other", at));
        assert!(!task.matches_completed_import_confirmation(
            identity(),
            "alice",
            Instant::from_unix_secs(21)
        ));
        task.owner_user_id = None;
        assert!(!task.matches_completed_import_confirmation(identity(), "alice", at));
    }
}
