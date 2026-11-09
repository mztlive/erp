//! 页面办理资格重验所需的最小任务动作事实。

use erp_workflow::entity::work_item::{WorkItemStatus, WorkItemType};

use super::WorkItemView;
use super::access::ViewAccess;

/// 资格重验仅消费任务身份、责任和动作状态。
pub(super) struct ActionProjection {
    pub work_item_type: WorkItemType,
    pub status: WorkItemStatus,
    pub business_object_type: String,
    pub business_object_id: String,
    pub owner_user_id: Option<String>,
    pub approval_node_execution_id: Option<String>,
    pub access: ViewAccess,
}

impl ActionProjection {
    /// 页面仅复制资格所需的小字段，展示内容保留在原视图。
    ///
    /// # 参数
    /// `view` 为已授权页面视图。
    /// # 返回
    /// 返回与视图身份及动作一致的最小事实。
    /// # 错误
    /// 无。
    pub(super) fn from_view(view: &WorkItemView) -> Self {
        Self {
            work_item_type: view.work_item_type,
            status: view.status,
            business_object_type: view.business_object_type.clone(),
            business_object_id: view.business_object_id.clone(),
            owner_user_id: view.owner_user_id.clone(),
            approval_node_execution_id: view.approval_node_execution_id.clone(),
            access: ViewAccess {
                processing_state: view.processing_state,
                processing_blocker: view.processing_blocker.clone(),
                allowed_actions: view.allowed_actions.clone(),
                action_blockers: view.action_blockers.clone(),
            },
        }
    }

    /// 仅回填资格状态与动作，不覆盖责任、版本和业务展示。
    ///
    /// # 参数
    /// `view` 为同一任务的原始页面视图。
    /// # 返回
    /// 原地写入最终资格状态与动作。
    /// # 错误
    /// 无。
    pub(super) fn apply(self, view: &mut WorkItemView) {
        view.processing_state = self.access.processing_state;
        view.processing_blocker = self.access.processing_blocker;
        view.allowed_actions = self.access.allowed_actions;
        view.action_blockers = self.access.action_blockers;
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority};

    use super::*;
    use crate::workbench::{ProcessingState, WorkItemAllowedAction, dto};

    /// 构造一个正式注册的履约任务，复用实体和视图生产转换。
    fn fields() -> dto::WorkItemFields {
        WorkItem::new_with_responsibility_key(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::FulfillmentOperation,
                business_object_type: "purchase_receipt".into(),
                business_object_id: "receipt".into(),
                subject_version: "1".into(),
                owner_role: "warehouse_inbound_handler".into(),
                owner_organization_id: "warehouse".into(),
                owner_user_id: "actor".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            "purchase_order:order",
        )
        .unwrap()
        .into()
    }

    /// 资格重验回填不覆盖页面身份、展示和版本。
    #[test]
    fn minimal_actions_preserve_view_identity_and_qualification_results() {
        let fields = fields();
        let mut view = WorkItemView::from_fields(fields, "queue".into()).unwrap();
        view.allowed_actions = vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Process];
        let mut projection = ActionProjection::from_view(&view);
        let before = view.clone();
        projection.access.processing_state = ProcessingState::ExecutionBlocked;
        projection.access.allowed_actions = vec![WorkItemAllowedAction::View];
        projection.apply(&mut view);
        assert_eq!(view.id, before.id);
        assert_eq!(view.owner_user_id, before.owner_user_id);
        assert_eq!(view.owner_organization_id, before.owner_organization_id);
        assert_eq!(view.task_version, before.task_version);
        assert_eq!(view.subject_version, before.subject_version);
        assert_eq!(view.summary_sections, before.summary_sections);
        assert_eq!(view.processing_state, ProcessingState::ExecutionBlocked);
        assert_eq!(view.allowed_actions, vec![WorkItemAllowedAction::View]);
    }
}
