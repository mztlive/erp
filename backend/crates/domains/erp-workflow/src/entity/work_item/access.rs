//! 工作项精确管理授权事实及统一动作权限。
use std::collections::BTreeSet;

use super::{
    WorkItemActionAccess, WorkItemActionPolicy, WorkItemActionRequirement, WorkItemAllowedAction,
    WorkItemStatus, WorkItemType,
};

/// 已完成授权后供动作计算使用的事实；管理资格只绑定精确任务 ID。
#[derive(Clone)]
pub struct TaskAccess {
    pub actor_id: String,
    pub executable_task_ids: BTreeSet<String>,
    pub decidable_task_ids: BTreeSet<String>,
    pub managed_task_ids: BTreeSet<String>,
}
impl TaskAccess {
    /// 创建尚未授予任何动作的账号事实。
    /// 返回空权限、空任务管理集合。
    ///
    /// actor_id 为真实账号身份。
    pub fn new(actor_id: String) -> Self {
        Self {
            actor_id,
            executable_task_ids: BTreeSet::new(),
            decidable_task_ids: BTreeSet::new(),
            managed_task_ids: BTreeSet::new(),
        }
    }
    /// 当当前任务的管理证明和管理动作权限同时成立时返回 `true`。
    /// 其他任务、空集合或无功能权限均返回 false。
    ///
    /// id 为精确任务 ID。
    pub fn can_manage(&self, id: &str) -> bool {
        self.managed_task_ids.contains(id)
    }
}

impl TaskAccess {
    /// 根据精确任务管理资格、操作权限和实体责任规则计算动作。
    /// 返回满足当前责任和权限的动作；执行入口仍须重验。
    ///
    /// policy 为任务状态和责任事实，id 为任务 ID，actor_id 为实际账号，managed_queue 表示当前可展示管理动作。
    pub fn actions(
        &self,
        policy: WorkItemActionPolicy<'_>,
        id: &str,
        actor_id: &str,
        managed_queue: bool,
    ) -> Vec<WorkItemAllowedAction> {
        let can_control = self.can_manage(id);
        let mut actions = policy.allowed_actions(
            WorkItemActionAccess { actor_id, managed_queue, can_manage: can_control },
            |requirement| match requirement {
                WorkItemActionRequirement::Execute => self.executable_task_ids.contains(id),
                WorkItemActionRequirement::Decide => self.decidable_task_ids.contains(id),
                WorkItemActionRequirement::Reassign | WorkItemActionRequirement::Close => can_control,
            },
        );

        actions.retain(|action| *action != WorkItemAllowedAction::AbandonRework);
        if policy.work_item_type == WorkItemType::ApprovalRework
            && policy.status == WorkItemStatus::Open
            && policy.owner_user_id == Some(actor_id)
            && (self.executable_task_ids.contains(id) || can_control)
        {
            actions.push(WorkItemAllowedAction::AbandonRework);
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rework_policy(status: WorkItemStatus) -> WorkItemActionPolicy<'static> {
        WorkItemActionPolicy {
            work_item_type: WorkItemType::ApprovalRework,
            status,
            business_object_type: "purchase_payment_request",
            owner_user_id: Some("submitter"),
            has_approval_execution: false,
        }
    }

    #[test]
    fn original_submitter_can_abandon_open_rework_without_manage_permission() {
        let mut access = TaskAccess::new("submitter".into());
        access.executable_task_ids.insert("rework-1".into());
        let actions = access.actions(rework_policy(WorkItemStatus::Open), "rework-1", "submitter", false);
        assert!(actions.contains(&WorkItemAllowedAction::Process));
        assert!(actions.contains(&WorkItemAllowedAction::AbandonRework));
    }

    #[test]
    fn non_owner_manager_or_closed_rework_cannot_abandon() {
        let mut manager = TaskAccess::new("manager".into());
        manager.managed_task_ids.insert("rework-1".into());
        manager.executable_task_ids.insert("rework-1".into());
        assert!(!manager.actions(rework_policy(WorkItemStatus::Open), "rework-1", "manager", true).contains(
            &WorkItemAllowedAction::AbandonRework
        ));

        let mut owner = TaskAccess::new("submitter".into());
        owner.executable_task_ids.insert("rework-1".into());
        assert!(!owner.actions(rework_policy(WorkItemStatus::Closed), "rework-1", "submitter", false).contains(
            &WorkItemAllowedAction::AbandonRework
        ));
    }
}
