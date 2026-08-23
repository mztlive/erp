//! W29 独立人工任务的固定责任注册表与构造器。

use entities::ids::WorkItemId;
use entities::integration_ops::{ErrorClass, IntegrationErrorTask, ReconciliationDifference};
use entities::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};
use id_generator::next_id;

use crate::errors::{Error, Result};

const OWNER_ORGANIZATION: &str = "company";
const OPERATIONS_ROLE: &str = "role-operations";
const PROCUREMENT_ROLE: &str = "role-procurement";
const FINANCE_ROLE: &str = "role-finance";
const SYSADMIN_ROLE: &str = "role-sysadmin";

/// 返回错误分类对应的固定责任角色。
pub(crate) fn error_owner_role(error_class: ErrorClass) -> &'static str {
    match error_class {
        ErrorClass::CapabilityGap | ErrorClass::MappingError => OPERATIONS_ROLE,
        ErrorClass::BusinessRejected => PROCUREMENT_ROLE,
        ErrorClass::TransientFailure
        | ErrorClass::ResultUnknown
        | ErrorClass::AuthSignature
        | ErrorClass::RateLimited
        | ErrorClass::OutOfOrder => SYSADMIN_ROLE,
    }
}

/// 返回错误分类对应的固定任务类型。
pub(super) fn error_work_item_type(error_class: ErrorClass) -> WorkItemType {
    if error_class == ErrorClass::ResultUnknown {
        WorkItemType::IntegrationResultUnknown
    } else {
        WorkItemType::BusinessException
    }
}

/// 从固定注册表解析对账差异责任角色。
///
/// # 错误
/// 未注册的差异类型返回业务错误，调用方不得创建无责任任务。
pub(super) fn difference_owner_role(difference_type: &str) -> Result<&'static str> {
    match difference_type.trim().to_ascii_lowercase().as_str() {
        "mall_missing"
        | "erp_missing"
        | "status_difference"
        | "content_fingerprint_difference"
        | "duplicate_identity" => Ok(OPERATIONS_ROLE),
        "amount_mismatch"
        | "refund_mismatch"
        | "balance_mismatch"
        | "settlement_mismatch"
        | "cost_mismatch" => Ok(FINANCE_ROLE),
        "supplier_supply_mismatch" | "supplier_order_mismatch" | "supply_mismatch" => Ok(PROCUREMENT_ROLE),
        "result_unknown" | "integration_result_unknown" => Ok(SYSADMIN_ROLE),
        _ => Err(Error::BusinessLogicError(
            "差异类型未注册固定责任规则，禁止创建任务".to_string(),
        )),
    }
}

/// 构造指定到人的错误处理任务。
///
/// # 参数
/// * `task` - 集成错误事实
/// * `owner_user_id` - 创建时明确解析的当前责任人
///
/// # 错误
/// 责任字段或关联字段不满足任务实体不变式时返回错误。
pub(crate) fn error_work_item(task: &IntegrationErrorTask, owner_user_id: &str) -> Result<WorkItem> {
    WorkItem::new(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: error_work_item_type(task.error_class),
            business_object_type: "integration_error_task".to_string(),
            business_object_id: task.base.id.clone(),
            subject_version: task.base.version.to_string(),
            owner_role: error_owner_role(task.error_class).to_string(),
            owner_organization_id: OWNER_ORGANIZATION.to_string(),
            owner_user_id: owner_user_id.to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: error_priority(task.error_class),
            due_at: None,
            reason_code: Some(task.error_class.as_str().to_string()),
            impact_summary: Some(format!("集成异常待处理：{}", task.error_class.label())),
        },
    )
    .map_err(Into::into)
}

/// 构造指定到人的对账差异任务。
///
/// # 参数
/// * `difference` - 对账差异事实
/// * `owner_user_id` - 创建时明确解析的当前责任人
///
/// # 错误
/// 差异类型未注册责任规则，或任务实体不变式不成立时返回错误。
pub(super) fn difference_work_item(
    difference: &ReconciliationDifference,
    owner_user_id: &str,
) -> Result<WorkItem> {
    let owner_role = difference_owner_role(&difference.difference_type)?;
    WorkItem::new(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: WorkItemType::BusinessException,
            business_object_type: "reconciliation_difference".to_string(),
            business_object_id: difference.base.id.clone(),
            subject_version: "0".to_string(),
            owner_role: owner_role.to_string(),
            owner_organization_id: OWNER_ORGANIZATION.to_string(),
            owner_user_id: owner_user_id.to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::High,
            due_at: None,
            reason_code: Some(difference.difference_type.clone()),
            impact_summary: Some(format!("对账差异待核验：{}", difference.difference_type)),
        },
    )
    .map_err(Into::into)
}

fn error_priority(error_class: ErrorClass) -> WorkItemPriority {
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

#[cfg(test)]
mod tests {
    use super::{difference_owner_role, error_owner_role, error_work_item_type};
    use entities::integration_ops::ErrorClass;
    use entities::work_item::WorkItemType;

    #[test]
    fn error_registry_is_fixed_by_error_class() {
        assert_eq!(error_owner_role(ErrorClass::ResultUnknown), "role-sysadmin");
        assert_eq!(
            error_work_item_type(ErrorClass::ResultUnknown),
            WorkItemType::IntegrationResultUnknown
        );
        assert_eq!(error_owner_role(ErrorClass::MappingError), "role-operations");
        assert_eq!(
            error_work_item_type(ErrorClass::MappingError),
            WorkItemType::BusinessException
        );
    }

    #[test]
    fn difference_registry_rejects_unknown_type() {
        assert_eq!(difference_owner_role("amount_mismatch").unwrap(), "role-finance");
        assert!(difference_owner_role("free_form_type").is_err());
    }
}
