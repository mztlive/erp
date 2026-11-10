//! 已授权工作项的动作策略；权限解析和写入时重验由调用方负责。

use serde::Serialize;

use super::{WorkItemStatus, WorkItemType};

/// 服务端计算的允许动作。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkItemAllowedAction {
    /// 查看对象。
    View,
    /// 进入固定强类型处理器。
    Process,
    /// 原提交人放弃继续处理本次驳回，保留单据和历史。
    AbandonRework,
    /// 审批通过。
    Approve,
    /// 审批驳回。
    Reject,
    /// 受控转交。
    Reassign,
    /// 受控关闭无效任务。
    Close,
}

/// 计算动作所需的工作项责任与生命周期事实。
#[derive(Debug, Clone, Copy)]
pub struct WorkItemActionPolicy<'a> {
    /// 固定工作项类型。
    pub work_item_type: WorkItemType,
    /// 当前任务状态；仅开放审批任务提供审批决定动作。
    pub status: WorkItemStatus,
    /// 任务固定业务对象类型。
    pub business_object_type: &'a str,
    /// 当前具体个人责任人。
    pub owner_user_id: Option<&'a str>,
    /// 是否绑定审批节点执行；通用管理动作不得修改该责任。
    pub has_approval_execution: bool,
}

/// 调用方对已授权工作项解析的访问事实，不授予额外对象读取权。
#[derive(Debug, Clone, Copy)]
pub struct WorkItemActionAccess<'a> {
    /// 当前认证账号 ID。
    pub actor_id: &'a str,
    /// 当前是否位于主管队列。
    pub managed_queue: bool,
    /// 当前账号是否已通过这个具体任务的管理授权。
    pub can_manage: bool,
}

/// 动作策略按需请求的权限事实，具体解析方式由调用方提供。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkItemActionRequirement {
    /// 目标工作面的完整执行权限。
    Execute,
    /// 审批决定权限；提交决定时仍须重验精确任务责任链。
    Decide,
    /// 通用转派权限。
    Reassign,
    /// 通用关闭权限。
    Close,
}

impl WorkItemActionPolicy<'_> {
    /// 依据已授权任务及独立权限事实计算允许动作。
    ///
    /// # 参数
    /// * `access` - 当前账号的责任与管理范围；调用方已完成对象读取授权。
    /// * `permits` - 按需解析独立权限事实；仅在前置责任、状态和范围条件满足时调用。
    ///
    /// # 返回
    /// 按查看、处理、通过、驳回、转派、关闭的固定顺序返回适用动作。
    ///
    /// # 错误
    /// 不返回错误。该结果不替代写入入口的授权与版本重验。
    pub fn allowed_actions(
        &self,
        access: WorkItemActionAccess<'_>,
        mut permits: impl FnMut(WorkItemActionRequirement) -> bool,
    ) -> Vec<WorkItemAllowedAction> {
        let mut actions = vec![WorkItemAllowedAction::View];
        let owned = self.owner_user_id == Some(access.actor_id);
        if self.work_item_type == WorkItemType::ApprovalRework {
            if owned && self.status == WorkItemStatus::Open && permits(WorkItemActionRequirement::Execute) {
                actions.extend([WorkItemAllowedAction::Process, WorkItemAllowedAction::AbandonRework]);
            }
            return actions;
        }
        if owned && permits(WorkItemActionRequirement::Execute) {
            actions.push(WorkItemAllowedAction::Process);
        }
        if self.work_item_type.is_document_approval()
            && self.status == WorkItemStatus::Open
            && self.has_approval_execution
            && owned
            && permits(WorkItemActionRequirement::Decide)
        {
            actions.push(WorkItemAllowedAction::Approve);
            actions.push(WorkItemAllowedAction::Reject);
        }
        let approval_responsibility =
            self.work_item_type.is_document_approval() || self.has_approval_execution;
        if access.can_manage
            && access.managed_queue
            && !approval_responsibility
            && self.work_item_type != WorkItemType::SalesInquiryProcessing
        {
            if permits(WorkItemActionRequirement::Reassign) {
                actions.push(WorkItemAllowedAction::Reassign);
            }
            if self.work_item_type.is_w29_closable(self.business_object_type, self.has_approval_execution)
                && permits(WorkItemActionRequirement::Close)
            {
                actions.push(WorkItemAllowedAction::Close);
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exception_policy() -> WorkItemActionPolicy<'static> {
        WorkItemActionPolicy {
            work_item_type: WorkItemType::IntegrationResultUnknown,
            status: WorkItemStatus::Open,
            business_object_type: "integration_error_task",
            owner_user_id: Some("owner"),
            has_approval_execution: false,
        }
    }

    fn manager_access() -> WorkItemActionAccess<'static> {
        WorkItemActionAccess { actor_id: "owner", managed_queue: true, can_manage: true }
    }

    fn granted(_: WorkItemActionRequirement) -> bool {
        true
    }

    #[test]
    fn owner_and_manager_actions_have_stable_order_and_independent_permissions() {
        let policy = exception_policy();
        let access = manager_access();
        assert_eq!(
            policy.allowed_actions(access, granted),
            vec![
                WorkItemAllowedAction::View,
                WorkItemAllowedAction::Process,
                WorkItemAllowedAction::Reassign,
                WorkItemAllowedAction::Close,
            ]
        );
        assert_eq!(
            policy.allowed_actions(access, |required| required == WorkItemActionRequirement::Close),
            vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Close]
        );
        assert_eq!(
            policy.allowed_actions(access, |required| required != WorkItemActionRequirement::Close),
            vec![
                WorkItemAllowedAction::View,
                WorkItemAllowedAction::Process,
                WorkItemAllowedAction::Reassign,
            ]
        );
    }

    #[test]
    fn approval_decisions_require_open_owned_execution_and_decision_permission() {
        let policy = WorkItemActionPolicy {
            work_item_type: WorkItemType::DocumentApproval,
            has_approval_execution: true,
            ..exception_policy()
        };
        let access = manager_access();
        let decision_only = |required| required == WorkItemActionRequirement::Decide;
        assert_eq!(
            policy.allowed_actions(access, decision_only),
            vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Approve, WorkItemAllowedAction::Reject]
        );
        for status in [WorkItemStatus::Completed, WorkItemStatus::Closed] {
            assert_eq!(
                WorkItemActionPolicy { status, ..policy }.allowed_actions(access, decision_only),
                vec![WorkItemAllowedAction::View]
            );
        }
        assert_eq!(
            WorkItemActionPolicy { has_approval_execution: false, ..policy }
                .allowed_actions(access, decision_only),
            vec![WorkItemAllowedAction::View]
        );
        assert_eq!(
            policy.allowed_actions(WorkItemActionAccess { actor_id: "other", ..access }, decision_only),
            vec![WorkItemAllowedAction::View]
        );
        assert_eq!(policy.allowed_actions(access, |_| false), vec![WorkItemAllowedAction::View]);
    }

    #[test]
    fn rework_actions_require_open_original_owner_and_exclude_generic_management() {
        let policy =
            WorkItemActionPolicy { work_item_type: WorkItemType::ApprovalRework, ..exception_policy() };
        let access = manager_access();
        assert_eq!(
            policy.allowed_actions(access, granted),
            vec![
                WorkItemAllowedAction::View,
                WorkItemAllowedAction::Process,
                WorkItemAllowedAction::AbandonRework
            ]
        );
        assert_eq!(
            policy.allowed_actions(WorkItemActionAccess { actor_id: "manager", ..access }, granted),
            vec![WorkItemAllowedAction::View]
        );
        for status in [WorkItemStatus::Completed, WorkItemStatus::Closed] {
            assert_eq!(
                WorkItemActionPolicy { status, ..policy }.allowed_actions(access, granted),
                vec![WorkItemAllowedAction::View]
            );
        }
        assert_eq!(policy.allowed_actions(access, |_| false), vec![WorkItemAllowedAction::View]);
    }

    #[test]
    fn approval_and_inquiry_responsibilities_exclude_generic_management() {
        let access = WorkItemActionAccess { actor_id: "manager", ..manager_access() };
        for policy in [
            WorkItemActionPolicy { work_item_type: WorkItemType::DocumentApproval, ..exception_policy() },
            WorkItemActionPolicy { has_approval_execution: true, ..exception_policy() },
            WorkItemActionPolicy {
                work_item_type: WorkItemType::SalesInquiryProcessing,
                business_object_type: "sales_inquiry",
                ..exception_policy()
            },
        ] {
            assert_eq!(policy.allowed_actions(access, granted), vec![WorkItemAllowedAction::View]);
        }
    }

    /// 管理动作使用外层精确任务证明，不再重复解释负责人名单。
    #[test]
    fn management_requires_queue_and_exact_task_qualification() {
        let policy = exception_policy();
        let access = WorkItemActionAccess { actor_id: "manager", ..manager_access() };
        for restricted in [
            WorkItemActionAccess { managed_queue: false, ..access },
            WorkItemActionAccess { can_manage: false, ..access },
        ] {
            assert_eq!(policy.allowed_actions(restricted, granted), vec![WorkItemAllowedAction::View]);
        }
        assert_eq!(
            policy.allowed_actions(access, granted),
            vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Reassign, WorkItemAllowedAction::Close]
        );
    }

    #[test]
    fn close_permission_does_not_close_unregistered_exception_objects() {
        let policy = WorkItemActionPolicy {
            business_object_type: "SUPPLIER_OFFERING",
            work_item_type: WorkItemType::BusinessException,
            ..exception_policy()
        };
        let access = WorkItemActionAccess { actor_id: "manager", ..manager_access() };
        assert_eq!(
            policy.allowed_actions(access, granted),
            vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Reassign]
        );
        assert_eq!(
            WorkItemActionPolicy { business_object_type: "reconciliation_difference", ..policy }
                .allowed_actions(access, granted),
            vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Reassign, WorkItemAllowedAction::Close]
        );
    }

    #[test]
    fn execution_permission_requires_current_owner_without_extra_status_filter() {
        let policy = exception_policy();
        let access = WorkItemActionAccess { managed_queue: false, ..manager_access() };
        assert_eq!(
            policy.allowed_actions(WorkItemActionAccess { actor_id: "other", ..access }, granted),
            vec![WorkItemAllowedAction::View]
        );
        assert_eq!(
            WorkItemActionPolicy { status: WorkItemStatus::Completed, ..policy }
                .allowed_actions(access, granted),
            vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Process]
        );
    }

    #[test]
    fn permission_resolution_follows_responsibility_and_state_short_circuit() {
        let ordinary = exception_policy();
        let owner = WorkItemActionAccess { can_manage: false, ..manager_access() };
        let outsider = WorkItemActionAccess { actor_id: "other", ..owner };
        let approval = WorkItemActionPolicy {
            work_item_type: WorkItemType::DocumentApproval,
            has_approval_execution: true,
            ..ordinary
        };
        for (policy, access, expected) in [
            (ordinary, outsider, vec![]),
            (ordinary, owner, vec![WorkItemActionRequirement::Execute]),
            (approval, owner, vec![WorkItemActionRequirement::Execute, WorkItemActionRequirement::Decide]),
            (
                WorkItemActionPolicy { status: WorkItemStatus::Closed, ..approval },
                owner,
                vec![WorkItemActionRequirement::Execute],
            ),
            (
                ordinary,
                WorkItemActionAccess { actor_id: "manager", ..manager_access() },
                vec![WorkItemActionRequirement::Reassign, WorkItemActionRequirement::Close],
            ),
            (
                WorkItemActionPolicy { business_object_type: "unregistered", ..ordinary },
                WorkItemActionAccess { actor_id: "manager", ..manager_access() },
                vec![WorkItemActionRequirement::Reassign],
            ),
        ] {
            let mut requested = Vec::new();
            policy.allowed_actions(access, |requirement| {
                requested.push(requirement);
                true
            });
            assert_eq!(requested, expected);
        }
    }

    #[test]
    fn action_serialization_preserves_public_codes() {
        assert_eq!(
            serde_json::to_value([
                WorkItemAllowedAction::View,
                WorkItemAllowedAction::Process,
                WorkItemAllowedAction::Approve,
                WorkItemAllowedAction::Reject,
                WorkItemAllowedAction::Reassign,
                WorkItemAllowedAction::Close,
            ])
            .unwrap(),
            serde_json::json!(["VIEW", "PROCESS", "APPROVE", "REJECT", "REASSIGN", "CLOSE"])
        );
    }
}
