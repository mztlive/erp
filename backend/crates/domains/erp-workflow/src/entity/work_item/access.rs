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
    /// # 参数
    /// actor_id 为真实账号身份。
    /// # 返回
    /// 返回空权限、空任务管理集合。
    /// # 错误
    /// 无。
    pub fn new(actor_id: String) -> Self {
        Self {
            actor_id,
            executable_task_ids: BTreeSet::new(),
            decidable_task_ids: BTreeSet::new(),
            managed_task_ids: BTreeSet::new(),
        }
    }
    /// 判断当前任务的管理证明和管理动作权限是否同时成立。
    /// # 参数
    /// id 为精确任务 ID。
    /// # 返回
    /// 其他任务、空集合或无功能权限均返回 false。
    /// # 错误
    /// 无。
    pub fn can_manage(&self, id: &str) -> bool {
        self.managed_task_ids.contains(id)
    }
}

impl TaskAccess {
    /// 根据精确任务管理资格、操作权限和实体责任规则计算动作。
    /// # 参数
    /// policy 为任务状态和责任事实，id 为任务 ID，actor_id 为实际账号，managed_queue 表示当前可展示管理动作。
    /// # 返回
    /// 返回满足当前责任和权限的动作；执行入口仍须重验。
    /// # 错误
    /// 无。
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
            && can_control
        {
            actions.push(WorkItemAllowedAction::AbandonRework);
        }
        actions
    }
}
/// 返回执行任务的完整权限；普通任务返回空集，未注册执行对象失败关闭。
///
/// # 参数
/// * `work_item_type` - 工作项类型
/// * `business_object_type` - 业务对象类型稳定码
///
/// # 返回
/// 供应商履约异常、供给异常和实体已注册的执行任务返回权限码；普通任务返回空向量。未注册执行对象返回 `None`。
///
/// # 错误
/// 不返回错误。
pub fn required_execution_permissions(
    work_item_type: WorkItemType,
    business_object_type: &str,
) -> Option<Vec<&'static str>> {
    if business_object_type == "SUPPLIER_FULFILLMENT_ORDER"
        && matches!(work_item_type, WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException)
    {
        return Some(vec![
            "supplier_fulfillment_order:detail",
            "supplier_fulfillment_order:investigate",
            "supplier_fulfillment_order:complete",
        ]);
    }
    if work_item_type == WorkItemType::BusinessException && business_object_type == "SUPPLIER_OFFERING" {
        return Some(vec!["supplier_offering:resolve_supply_exception"]);
    }
    work_item_type.required_execution_permissions(business_object_type).map(|codes| codes.to_vec())
}
