//! 供给分配的当前任务授权，不授予普通销售单读取范围。

use super::{WorkItem, WorkItemStatus, WorkItemType};

impl WorkItem {
    /// 当任务具有有效的开放供给分配责任范围时返回 `true`。
    /// 未删除、开放、对象为销售单且冻结责任完整时返回 `true`。
    ///
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
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;

    use super::*;
    use crate::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority};

    #[test]
    fn procurement_task_requires_open_status_and_frozen_scope() {
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
        assert!(item.is_open_procurement_task());

        for status in [WorkItemStatus::Completed, WorkItemStatus::Closed] {
            let mut ended = item.clone();
            ended.status = status;
            assert!(!ended.is_open_procurement_task());
        }

        let mut invalid = item.clone();
        invalid.business_object_type = "purchase_order".into();
        assert!(!invalid.is_open_procurement_task());
        let mut invalid = item;
        invalid.responsibility_scope_ids.clear();
        assert!(!invalid.is_open_procurement_task());
    }
}
