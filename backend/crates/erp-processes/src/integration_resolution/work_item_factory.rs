//! W29 责任 Spec 到真实 WorkItem 的唯一装配。
use erp_core::Result;
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_integration::entity::integration_ops::{
    IntegrationErrorTask, IntegrationResponsibilityKind, IntegrationResponsibilityPriority,
    IntegrationResponsibilitySpec, ReconciliationDifference, difference_responsibility, error_responsibility,
};
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};

/// 为集成错误任务构造指定到人的正式任务。
///
/// # 参数
/// * `id` - 正式任务主键（调用方生成）
/// * `task` - 集成错误事实（提供错误分类、业务身份与主题版本）
/// * `owner_user_id` - 调用方已解析的当前个人责任人
/// * `now` - 调用方当前时间，用于任务创建时间
///
/// # 返回
/// 返回新建的开放正式责任任务。
///
/// # 错误
/// 责任字段或关联字段不满足任务实体不变式时返回错误。
///
/// # 约束
/// 纯领域构造，不访问数据库、不生成 ID、不读取全局时钟。
pub(super) fn new_error_work_item(
    id: WorkItemId,
    task: &IntegrationErrorTask,
    now: Instant,
) -> Result<WorkItem> {
    new_work_item(id, error_responsibility(task), now)
}
/// 为对账差异构造指定到人的正式任务。
///
/// # 参数
/// * `id` - 正式任务主键（调用方生成）
/// * `difference` - 对账差异事实（提供差异分类与业务身份）
/// * `owner_user_id` - 调用方已解析的当前个人责任人
/// * `now` - 调用方当前时间，用于任务创建时间
///
/// # 返回
/// 返回新建的开放正式责任任务。
///
/// # 错误
/// 差异类型未注册责任规则，或责任字段不满足任务实体不变式时返回错误。
///
/// # 约束
/// 纯领域构造，不访问数据库、不生成 ID、不读取全局时钟；初始主题版本为 0。
pub(super) fn new_difference_work_item(
    id: WorkItemId,
    difference: &ReconciliationDifference,
    now: Instant,
) -> Result<WorkItem> {
    new_work_item(id, difference_responsibility(difference)?, now)
}
fn new_work_item(id: WorkItemId, spec: IntegrationResponsibilitySpec, now: Instant) -> Result<WorkItem> {
    WorkItem::new_at(
        id,
        WorkItemData {
            work_item_type: match spec.task_kind {
                IntegrationResponsibilityKind::BusinessException => WorkItemType::BusinessException,
                IntegrationResponsibilityKind::IntegrationResultUnknown => {
                    WorkItemType::IntegrationResultUnknown
                },
            },
            business_object_type: spec.business_object_type,
            business_object_id: spec.business_object_id,
            subject_version: spec.subject_version,
            owner_role: spec.owner_role,
            owner_organization_id: spec.owner_organization_id,
            owner_user_id: spec.owner_user_id,
            assignment_source: AssignmentSource::SystemRule,
            priority: match spec.priority {
                IntegrationResponsibilityPriority::Normal => WorkItemPriority::Normal,
                IntegrationResponsibilityPriority::High => WorkItemPriority::High,
                IntegrationResponsibilityPriority::Urgent => WorkItemPriority::Urgent,
            },
            due_at: None,
            reason_code: spec.reason_code,
            impact_summary: spec.impact_summary,
        },
        now,
    )
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{IntegrationErrorTaskId, ReconciliationDifferenceId};
    use erp_integration::entity::integration_ops::{
        DIFFERENCE_WORK_ITEM_OBJECT_TYPE, ERROR_WORK_ITEM_OBJECT_TYPE, ErrorClass, IntegrationErrorTaskData,
        ReconciliationDifferenceData,
    };

    use super::*;
    const NOW: i64 = 1_700_000_000;
    fn error_task(error_class: ErrorClass) -> IntegrationErrorTask {
        IntegrationErrorTask::new(
            IntegrationErrorTaskId::new("task-1"),
            IntegrationErrorTaskData {
                message_id: None,
                business_object_id: Some("so-1".to_string()),
                error_class,
                owner_role: None,
                owner_user_id: Some("user-1".to_string()),
                owner_org_unit_id: "org-sysadmin".to_string(),
            },
        )
        .unwrap()
    }

    fn difference(difference_type: &str) -> ReconciliationDifference {
        ReconciliationDifference::new(
            ReconciliationDifferenceId::new("diff-1"),
            ReconciliationDifferenceData {
                business_object_type: "mall_order".to_string(),
                business_object_id: "MO-1".to_string(),
                difference_type: difference_type.to_string(),
                left_fact_reference: Some("mall_order_fact://f-1".to_string()),
                right_fact_reference: None,
                owner_user_id: "user-2".to_string(),
                owner_org_unit_id: "org-finance".to_string(),
            },
        )
        .unwrap()
    }

    #[test]
    fn error_work_item_carries_full_formal_fields() {
        let item = new_error_work_item(
            WorkItemId::new("wi-1"),
            &error_task(ErrorClass::ResultUnknown),
            Instant::from_unix_secs(NOW),
        )
        .unwrap();

        assert_eq!(item.work_item_type, WorkItemType::IntegrationResultUnknown);
        assert_eq!(item.business_object_type.as_str(), ERROR_WORK_ITEM_OBJECT_TYPE);
        assert_eq!(item.business_object_id.as_str(), "task-1");
        assert_eq!(item.subject_version.as_str(), "1");
        assert_eq!(item.owner_role.as_str(), "role-sysadmin");
        assert_eq!(item.owner_organization_id.as_str(), "org-sysadmin");
        assert_eq!(item.owner_user_id.as_deref(), Some("user-1"));
        assert_eq!(item.assignment_source, AssignmentSource::SystemRule);
        assert_eq!(item.priority, WorkItemPriority::Urgent);
        assert_eq!(item.reason_code.as_deref(), Some("result_unknown"));
        assert!(item.impact_summary.as_deref().is_some_and(|summary| summary.contains("结果未知")));
    }

    #[test]
    fn difference_work_item_carries_full_formal_fields() {
        let item = new_difference_work_item(
            WorkItemId::new("wi-2"),
            &difference("amount_mismatch"),
            Instant::from_unix_secs(NOW),
        )
        .unwrap();

        assert_eq!(item.work_item_type, WorkItemType::BusinessException);
        assert_eq!(item.business_object_type.as_str(), DIFFERENCE_WORK_ITEM_OBJECT_TYPE);
        assert_eq!(item.business_object_id.as_str(), "diff-1");
        assert_eq!(item.subject_version.as_str(), "0");
        assert_eq!(item.owner_role.as_str(), "role-finance");
        assert_eq!(item.owner_user_id.as_deref(), Some("user-2"));
        assert_eq!(item.priority, WorkItemPriority::High);
        assert_eq!(item.reason_code.as_deref(), Some("amount_mismatch"));
        assert!(item.impact_summary.as_deref().is_some_and(|summary| summary.contains("amount_mismatch")));
    }

    #[test]
    fn factories_reject_unknown_difference() {
        let unknown = ReconciliationDifference::new(
            ReconciliationDifferenceId::new("diff-9"),
            ReconciliationDifferenceData {
                business_object_type: "mall_order".to_string(),
                business_object_id: "MO-9".to_string(),
                difference_type: "free_form_type".to_string(),
                left_fact_reference: Some("mall_order_fact://f-9".to_string()),
                right_fact_reference: None,
                owner_user_id: "user-9".to_string(),
                owner_org_unit_id: "org-ops".to_string(),
            },
        )
        .unwrap();
        assert!(
            new_difference_work_item(WorkItemId::new("wi-4"), &unknown, Instant::from_unix_secs(NOW),)
                .is_err()
        );
    }
}
