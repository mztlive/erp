//! 供给分配的当前任务授权，不授予普通销售单读取范围。

use super::{WorkItem, WorkItemStatus, WorkItemType};

impl WorkItem {
    /// 判断任务是否具有有效的开放供给分配责任范围。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 未删除、开放、对象为销售单且冻结责任完整时返回 `true`。
    /// # 错误
    /// 不返回错误；不完整任务返回 `false`。
    pub fn is_open_procurement_task(&self) -> bool {
        !self.base.is_deleted()
            && self.work_item_type == WorkItemType::ProcurementOrderCreation
            && self.business_object_type == "sales_order"
            && !self.business_object_id.is_empty()
            && self.status == WorkItemStatus::Open
            && self.responsibility_key().is_some_and(|key| !key.trim().is_empty())
            && !self.responsibility_scope_ids().is_empty()
            && self.responsibility_scope_ids().iter().all(|id| !id.trim().is_empty())
    }

    /// 判断操作人是否拥有当前供给分配任务的必要资料读取资格。
    ///
    /// # 参数
    /// * `actor_id` - 已认证账号；账号启用状态与采购执行权限由调用方复验。
    /// # 返回
    /// 有效开放任务的当前负责人返回 `true`。
    /// # 错误
    /// 不返回错误；历史负责人、终态或责任范围损坏时返回 `false`。
    pub fn owns_procurement_task(&self, actor_id: &str) -> bool {
        self.is_open_procurement_task() && self.owner_user_id.as_deref() == Some(actor_id)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;

    use super::*;
    use crate::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority};

    #[test]
    fn procurement_read_grant_requires_current_open_owner_and_frozen_scope() {
        let item = WorkItem::new_with_responsibility_scope(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                business_object_type: "sales_order".into(),
                business_object_id: "sales".into(),
                subject_version: "revision".into(),
                owner_role: "role-procurement".into(),
                owner_organization_id: "company".into(),
                owner_user_id: "buyer".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            "scope",
            vec!["line".into()],
        )
        .unwrap();
        assert!(item.owns_procurement_task("buyer"));
        assert!(!item.owns_procurement_task("other"));
        for status in [WorkItemStatus::Completed, WorkItemStatus::Closed] {
            let mut ended = item.clone();
            ended.status = status;
            assert!(!ended.owns_procurement_task("buyer"));
        }
        let mut reassigned = item.clone();
        reassigned.owner_user_id = Some("new-buyer".into());
        assert!(!reassigned.owns_procurement_task("buyer"));
        assert!(reassigned.owns_procurement_task("new-buyer"));
        let mut invalid = item.clone();
        invalid.business_object_type = "purchase_order".into();
        assert!(!invalid.is_open_procurement_task());
        let mut invalid = item;
        invalid.responsibility_scope_ids.clear();
        assert!(!invalid.is_open_procurement_task());
    }
}
