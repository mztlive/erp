//! 已授权工作项的动作响应转换。
use super::{ProcessingBlockerView, ProcessingState, WorkItemAllowedAction, WorkItemFields, WorkItemScope};
use crate::entity::work_item::{TaskAccess, WorkItem, WorkItemStatus};

impl WorkItemScope {
    /// 仅对已证明可访问的任务选择展示队列，不授予对象访问权。
    /// # 参数
    /// item 为已授权任务，actor_id 为当前账号。
    /// # 返回
    /// 终态为历史，本人为我的，其余为管理。
    /// # 错误
    /// 无。
    pub fn for_task(item: &WorkItem, actor_id: &str) -> WorkItemScope {
        if item.is_terminal() {
            WorkItemScope::History
        } else if item.is_owned_by(actor_id) {
            WorkItemScope::Mine
        } else {
            WorkItemScope::Managed
        }
    }
}
pub struct ViewAccess {
    pub processing_state: ProcessingState,
    pub processing_blocker: Option<ProcessingBlockerView>,
    pub allowed_actions: Vec<WorkItemAllowedAction>,
    pub action_blockers: Vec<ProcessingBlockerView>,
}

impl ViewAccess {
    /// 构造可处理的查看结果。
    ///
    /// # 参数
    /// * `allowed_actions` - 当前允许的动作
    ///
    /// # 返回
    /// 返回处理状态为就绪、无阻塞的视图。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn ready(allowed_actions: Vec<WorkItemAllowedAction>) -> Self {
        Self {
            processing_state: ProcessingState::Ready,
            processing_blocker: None,
            allowed_actions,
            action_blockers: Vec::new(),
        }
    }
}

impl WorkItemFields {
    /// 将队列展示范围交给统一的实体动作策略。
    /// # 参数
    /// scope 为展示队列，actor_id 为账号，access 为精确任务授权事实。
    /// # 返回
    /// 返回实体规则计算的允许动作。
    /// # 错误
    /// 无。
    pub fn allowed_actions(
        &self,
        scope: WorkItemScope,
        actor_id: &str,
        access: &TaskAccess,
    ) -> Vec<WorkItemAllowedAction> {
        access.actions(self.action_policy(), &self.id, actor_id, scope != WorkItemScope::History)
    }
}
impl ViewAccess {
    /// 从已授权任务生成处理状态和动作响应。
    /// # 参数
    /// item 为任务字段，scope 为队列，access 为精确任务资格。
    /// # 返回
    /// 历史与终态不提供动作；审批上下文的真实阻塞由后续投影补齐。
    /// # 错误
    /// 无。
    pub fn from_fields(item: &WorkItemFields, scope: WorkItemScope, access: &TaskAccess) -> Self {
        if scope == WorkItemScope::History || item.status != WorkItemStatus::Open {
            return Self::ready(Vec::new());
        }
        Self::ready(item.allowed_actions(scope, &access.actor_id, access))
    }
}
#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;

    use super::*;
    use crate::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority, WorkItemType};

    /// 构造具有相同负责人的不同任务，用于验证管理授权边界。
    fn task(id: &str) -> WorkItemFields {
        WorkItem::new(
            WorkItemId::new(id),
            WorkItemData {
                work_item_type: WorkItemType::BusinessException,
                business_object_type: "integration_error_task".into(),
                business_object_id: "source".into(),
                subject_version: "v1".into(),
                owner_role: "operator".into(),
                owner_organization_id: "company".into(),
                owner_user_id: "owner".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
        )
        .unwrap()
        .into()
    }

    /// identity 管理判定只作用于精确任务。
    #[test]
    fn management_proof_does_not_transfer_to_another_task_with_the_same_owner() {
        let first = task("first");
        let second = task("second");
        let mut access = TaskAccess::new("manager".into());
        access.managed_task_ids.insert(first.id.clone());
        assert!(
            ViewAccess::from_fields(&first, WorkItemScope::Managed, &access)
                .allowed_actions
                .contains(&WorkItemAllowedAction::Reassign)
        );
        assert!(
            !ViewAccess::from_fields(&second, WorkItemScope::Managed, &access)
                .allowed_actions
                .contains(&WorkItemAllowedAction::Reassign)
        );

        access.managed_task_ids.clear();
        assert!(
            !ViewAccess::from_fields(&first, WorkItemScope::Managed, &access)
                .allowed_actions
                .contains(&WorkItemAllowedAction::Reassign)
        );
    }

    /// 历史和终态任务在读取和命令响应中都不提供动作。
    #[test]
    fn history_and_terminal_responses_have_no_actions() {
        let mut fields = task("first");
        let mut access = TaskAccess::new("manager".into());
        access.managed_task_ids.insert(fields.id.clone());
        assert!(ViewAccess::from_fields(&fields, WorkItemScope::History, &access).allowed_actions.is_empty());
        fields.status = WorkItemStatus::Completed;
        assert!(ViewAccess::from_fields(&fields, WorkItemScope::Managed, &access).allowed_actions.is_empty());
    }
    #[test]
    fn execution_result_is_bound_to_exact_task_and_business_owner() {
        let first = task("first");
        let second = task("second");
        let mut access = TaskAccess::new("owner".into());
        access.executable_task_ids.insert(first.id.clone());
        assert!(
            ViewAccess::from_fields(&first, WorkItemScope::Mine, &access)
                .allowed_actions
                .contains(&WorkItemAllowedAction::Process)
        );
        assert!(
            !ViewAccess::from_fields(&second, WorkItemScope::Mine, &access)
                .allowed_actions
                .contains(&WorkItemAllowedAction::Process)
        );

        access.executable_task_ids.clear();
        assert!(
            !ViewAccess::from_fields(&first, WorkItemScope::Mine, &access)
                .allowed_actions
                .contains(&WorkItemAllowedAction::Process)
        );
    }
}
