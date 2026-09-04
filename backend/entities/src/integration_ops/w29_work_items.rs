//! W29 独立人工任务的固定责任注册表与构造器（INT-E19）。
//!
//! 错误分类与差异类型的责任角色、任务类型、优先级固定映射属于领域政策，
//! 由本模块独占；任务主键、当前责任人与时间由调用方注入，责任解析与持久化
//! 仍归服务；通用 BPM 不得拥有 ERP 责任路由。

use crate::common::time::Instant;
use crate::errors::Result;
use crate::ids::WorkItemId;
use crate::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};

use super::{ErrorClass, IntegrationErrorTask, ReconciliationDifference};

/// W29 任务正式责任的归属组织。
pub const W29_OWNER_ORGANIZATION: &str = "company";
/// 运营责任角色。
pub const W29_OPERATIONS_ROLE: &str = "role-operations";
/// 采购责任角色。
pub const W29_PROCUREMENT_ROLE: &str = "role-procurement";
/// 财务责任角色。
pub const W29_FINANCE_ROLE: &str = "role-finance";
/// 系统管理责任角色。
pub const W29_SYSADMIN_ROLE: &str = "role-sysadmin";
/// 错误任务的业务对象类型。
pub const ERROR_WORK_ITEM_OBJECT_TYPE: &str = "integration_error_task";
/// 对账差异的业务对象类型。
pub const DIFFERENCE_WORK_ITEM_OBJECT_TYPE: &str = "reconciliation_difference";
/// 对账差异任务的初始业务主题版本（尚无追加式决定时为 0）。
pub const DIFFERENCE_INITIAL_SUBJECT_VERSION: &str = "0";

/// 返回错误分类对应的固定责任角色。
///
/// # 参数
/// * `error_class` - 集成错误分类
///
/// # 返回
/// 返回固定的责任角色代码，全矩阵覆盖，无失败分支。
pub fn error_owner_role(error_class: ErrorClass) -> &'static str {
    match error_class {
        ErrorClass::CapabilityGap | ErrorClass::MappingError => W29_OPERATIONS_ROLE,
        ErrorClass::BusinessRejected => W29_PROCUREMENT_ROLE,
        ErrorClass::TransientFailure
        | ErrorClass::ResultUnknown
        | ErrorClass::AuthSignature
        | ErrorClass::RateLimited
        | ErrorClass::OutOfOrder => W29_SYSADMIN_ROLE,
    }
}

/// 返回错误分类对应的固定任务类型。
///
/// # 参数
/// * `error_class` - 集成错误分类
///
/// # 返回
/// 结果未知使用结果未知任务类型，其余使用业务异常任务类型。
pub fn error_work_item_type(error_class: ErrorClass) -> WorkItemType {
    if error_class == ErrorClass::ResultUnknown {
        WorkItemType::IntegrationResultUnknown
    } else {
        WorkItemType::BusinessException
    }
}

/// 返回错误分类对应的固定优先级。
///
/// # 参数
/// * `error_class` - 集成错误分类
///
/// # 返回
/// 返回固定的任务优先级，全矩阵覆盖，无失败分支。
pub fn error_priority(error_class: ErrorClass) -> WorkItemPriority {
    match error_class {
        ErrorClass::AuthSignature | ErrorClass::ResultUnknown => WorkItemPriority::Urgent,
        ErrorClass::RateLimited | ErrorClass::OutOfOrder | ErrorClass::TransientFailure => {
            WorkItemPriority::High
        }
        ErrorClass::CapabilityGap | ErrorClass::MappingError | ErrorClass::BusinessRejected => {
            WorkItemPriority::Normal
        }
    }
}

/// 从固定注册表解析对账差异责任角色。
///
/// # 参数
/// * `difference_type` - 对账差异分类（允许首尾空白与大小写差异）
///
/// # 返回
/// 返回固定的责任角色代码。
///
/// # 错误
/// 差异类型未注册固定责任规则时返回错误，调用方不得创建无责任任务。
///
/// # 约束
/// 未注册分类失败关闭；通用 BPM 不得复用本注册表。
pub fn difference_owner_role(difference_type: &str) -> Result<&'static str> {
    match difference_type.trim().to_ascii_lowercase().as_str() {
        "mall_missing"
        | "erp_missing"
        | "status_difference"
        | "content_fingerprint_difference"
        | "duplicate_identity" => Ok(W29_OPERATIONS_ROLE),
        "amount_mismatch"
        | "refund_mismatch"
        | "balance_mismatch"
        | "settlement_mismatch"
        | "cost_mismatch" => Ok(W29_FINANCE_ROLE),
        "supplier_supply_mismatch" | "supplier_order_mismatch" | "supply_mismatch" => {
            Ok(W29_PROCUREMENT_ROLE)
        }
        "result_unknown" | "integration_result_unknown" => Ok(W29_SYSADMIN_ROLE),
        _ => Err(crate::errors::Error::from(
            "差异类型未注册固定责任规则，禁止创建任务",
        )),
    }
}

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
pub fn new_error_work_item(
    id: WorkItemId,
    task: &IntegrationErrorTask,
    owner_user_id: &str,
    now: Instant,
) -> Result<WorkItem> {
    WorkItem::new_at(
        id,
        WorkItemData {
            work_item_type: error_work_item_type(task.error_class),
            business_object_type: ERROR_WORK_ITEM_OBJECT_TYPE.to_string(),
            business_object_id: task.base.id.clone(),
            subject_version: task.base.version.to_string(),
            owner_role: error_owner_role(task.error_class).to_string(),
            owner_organization_id: W29_OWNER_ORGANIZATION.to_string(),
            owner_user_id: owner_user_id.to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: error_priority(task.error_class),
            due_at: None,
            reason_code: Some(task.error_class.as_str().to_string()),
            impact_summary: Some(format!("集成异常待处理：{}", task.error_class.label())),
        },
        now,
    )
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
pub fn new_difference_work_item(
    id: WorkItemId,
    difference: &ReconciliationDifference,
    owner_user_id: &str,
    now: Instant,
) -> Result<WorkItem> {
    let owner_role = difference_owner_role(&difference.difference_type)?;
    WorkItem::new_at(
        id,
        WorkItemData {
            work_item_type: WorkItemType::BusinessException,
            business_object_type: DIFFERENCE_WORK_ITEM_OBJECT_TYPE.to_string(),
            business_object_id: difference.base.id.clone(),
            subject_version: DIFFERENCE_INITIAL_SUBJECT_VERSION.to_string(),
            owner_role: owner_role.to_string(),
            owner_organization_id: W29_OWNER_ORGANIZATION.to_string(),
            owner_user_id: owner_user_id.to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::High,
            due_at: None,
            reason_code: Some(difference.difference_type.clone()),
            impact_summary: Some(format!("对账差异待核验：{}", difference.difference_type)),
        },
        now,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        difference_owner_role, error_owner_role, error_priority, error_work_item_type,
        new_difference_work_item, new_error_work_item, DIFFERENCE_WORK_ITEM_OBJECT_TYPE,
        ERROR_WORK_ITEM_OBJECT_TYPE, W29_OWNER_ORGANIZATION,
    };
    use crate::common::time::Instant;
    use crate::ids::{IntegrationErrorTaskId, ReconciliationDifferenceId, WorkItemId};
    use crate::integration_ops::ErrorClass;
    use crate::integration_ops::{
        IntegrationErrorTask, IntegrationErrorTaskData, ReconciliationDifference,
        ReconciliationDifferenceData,
    };
    use crate::work_item::{AssignmentSource, WorkItemPriority, WorkItemType};

    const NOW: i64 = 1_700_000_000;

    fn error_task(error_class: ErrorClass) -> IntegrationErrorTask {
        IntegrationErrorTask::new(
            IntegrationErrorTaskId::new("task-1"),
            IntegrationErrorTaskData {
                message_id: None,
                business_object_id: Some("so-1".to_string()),
                error_class,
                owner_role: None,
                owner_user_id: None,
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
            },
        )
        .unwrap()
    }

    #[test]
    fn error_registry_covers_every_class() {
        let cases = [
            (
                ErrorClass::CapabilityGap,
                "role-operations",
                WorkItemType::BusinessException,
                WorkItemPriority::Normal,
            ),
            (
                ErrorClass::MappingError,
                "role-operations",
                WorkItemType::BusinessException,
                WorkItemPriority::Normal,
            ),
            (
                ErrorClass::BusinessRejected,
                "role-procurement",
                WorkItemType::BusinessException,
                WorkItemPriority::Normal,
            ),
            (
                ErrorClass::TransientFailure,
                "role-sysadmin",
                WorkItemType::BusinessException,
                WorkItemPriority::High,
            ),
            (
                ErrorClass::ResultUnknown,
                "role-sysadmin",
                WorkItemType::IntegrationResultUnknown,
                WorkItemPriority::Urgent,
            ),
            (
                ErrorClass::AuthSignature,
                "role-sysadmin",
                WorkItemType::BusinessException,
                WorkItemPriority::Urgent,
            ),
            (
                ErrorClass::RateLimited,
                "role-sysadmin",
                WorkItemType::BusinessException,
                WorkItemPriority::High,
            ),
            (
                ErrorClass::OutOfOrder,
                "role-sysadmin",
                WorkItemType::BusinessException,
                WorkItemPriority::High,
            ),
        ];
        for (class, role, work_item_type, priority) in cases {
            assert_eq!(error_owner_role(class), role);
            assert_eq!(error_work_item_type(class), work_item_type);
            assert_eq!(error_priority(class), priority);
        }
    }

    #[test]
    fn difference_registry_covers_every_code_and_rejects_unknown() {
        for code in [
            "mall_missing",
            "erp_missing",
            "status_difference",
            "content_fingerprint_difference",
            "duplicate_identity",
        ] {
            assert_eq!(difference_owner_role(code).unwrap(), "role-operations");
        }
        for code in [
            "amount_mismatch",
            "refund_mismatch",
            "balance_mismatch",
            "settlement_mismatch",
            "cost_mismatch",
        ] {
            assert_eq!(difference_owner_role(code).unwrap(), "role-finance");
        }
        for code in [
            "supplier_supply_mismatch",
            "supplier_order_mismatch",
            "supply_mismatch",
        ] {
            assert_eq!(difference_owner_role(code).unwrap(), "role-procurement");
        }
        for code in ["result_unknown", "integration_result_unknown"] {
            assert_eq!(difference_owner_role(code).unwrap(), "role-sysadmin");
        }
        assert_eq!(
            difference_owner_role(" Amount_Mismatch ").unwrap(),
            "role-finance"
        );
        assert!(difference_owner_role("free_form_type").is_err());
        assert!(difference_owner_role("  ").is_err());
    }

    #[test]
    fn error_work_item_carries_full_formal_fields() {
        let item = new_error_work_item(
            WorkItemId::new("wi-1"),
            &error_task(ErrorClass::ResultUnknown),
            "user-1",
            Instant::from_unix_secs(NOW),
        )
        .unwrap();

        assert_eq!(item.work_item_type, WorkItemType::IntegrationResultUnknown);
        assert_eq!(item.business_object_type.as_str(), ERROR_WORK_ITEM_OBJECT_TYPE);
        assert_eq!(item.business_object_id.as_str(), "task-1");
        assert_eq!(item.subject_version.as_str(), "1");
        assert_eq!(item.owner_role.as_str(), "role-sysadmin");
        assert_eq!(item.owner_organization_id.as_str(), W29_OWNER_ORGANIZATION);
        assert_eq!(item.owner_user_id.as_deref(), Some("user-1"));
        assert_eq!(item.assignment_source, AssignmentSource::SystemRule);
        assert_eq!(item.priority, WorkItemPriority::Urgent);
        assert_eq!(item.reason_code.as_deref(), Some("result_unknown"));
        assert!(item
            .impact_summary
            .as_deref()
            .is_some_and(|summary| summary.contains("结果未知")));
    }

    #[test]
    fn difference_work_item_carries_full_formal_fields() {
        let item = new_difference_work_item(
            WorkItemId::new("wi-2"),
            &difference("amount_mismatch"),
            "user-2",
            Instant::from_unix_secs(NOW),
        )
        .unwrap();

        assert_eq!(item.work_item_type, WorkItemType::BusinessException);
        assert_eq!(
            item.business_object_type.as_str(),
            DIFFERENCE_WORK_ITEM_OBJECT_TYPE
        );
        assert_eq!(item.business_object_id.as_str(), "diff-1");
        assert_eq!(item.subject_version.as_str(), "0");
        assert_eq!(item.owner_role.as_str(), "role-finance");
        assert_eq!(item.owner_user_id.as_deref(), Some("user-2"));
        assert_eq!(item.priority, WorkItemPriority::High);
        assert_eq!(item.reason_code.as_deref(), Some("amount_mismatch"));
        assert!(item
            .impact_summary
            .as_deref()
            .is_some_and(|summary| summary.contains("amount_mismatch")));
    }

    #[test]
    fn factories_reject_unknown_difference_and_blank_owner() {
        assert!(new_difference_work_item(
            WorkItemId::new("wi-3"),
            &difference("status_difference"),
            "   ",
            Instant::from_unix_secs(NOW),
        )
        .is_err());

        let unknown = ReconciliationDifference::new(
            ReconciliationDifferenceId::new("diff-9"),
            ReconciliationDifferenceData {
                business_object_type: "mall_order".to_string(),
                business_object_id: "MO-9".to_string(),
                difference_type: "free_form_type".to_string(),
                left_fact_reference: Some("mall_order_fact://f-9".to_string()),
                right_fact_reference: None,
            },
        )
        .unwrap();
        assert!(new_difference_work_item(
            WorkItemId::new("wi-4"),
            &unknown,
            "user-9",
            Instant::from_unix_secs(NOW),
        )
        .is_err());
    }
}
