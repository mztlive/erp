//! W29 独立人工任务的固定责任注册表与事实（INT-E19）。
//!
//! 错误分类与差异类型的责任角色、任务类型、优先级固定映射属于领域政策，
//! 由本模块独占；任务主键、当前责任人与时间由调用方注入，责任解析与持久化
//! 仍归服务；通用 BPM 不得拥有 ERP 责任路由。

use erp_core::Result;

use super::{ErrorClass, IntegrationErrorTask, ReconciliationDifference};

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
pub fn error_work_item_type(error_class: ErrorClass) -> IntegrationResponsibilityKind {
    if error_class == ErrorClass::ResultUnknown {
        IntegrationResponsibilityKind::IntegrationResultUnknown
    } else {
        IntegrationResponsibilityKind::BusinessException
    }
}

/// 返回错误分类对应的固定优先级。
///
/// # 参数
/// * `error_class` - 集成错误分类
///
/// # 返回
/// 返回固定的任务优先级，全矩阵覆盖，无失败分支。
pub fn error_priority(error_class: ErrorClass) -> IntegrationResponsibilityPriority {
    match error_class {
        ErrorClass::AuthSignature | ErrorClass::ResultUnknown => IntegrationResponsibilityPriority::Urgent,
        ErrorClass::RateLimited | ErrorClass::OutOfOrder | ErrorClass::TransientFailure => {
            IntegrationResponsibilityPriority::High
        },
        ErrorClass::CapabilityGap | ErrorClass::MappingError | ErrorClass::BusinessRejected => {
            IntegrationResponsibilityPriority::Normal
        },
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
        },
        "result_unknown" | "integration_result_unknown" => Ok(W29_SYSADMIN_ROLE),
        _ => Err(erp_core::Error::from("差异类型未注册固定责任规则，禁止创建任务")),
    }
}

/// 正式任务装配所需的固定集成责任类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrationResponsibilityKind {
    /// 普通业务异常。
    BusinessException,
    /// 原集成结果未知。
    IntegrationResultUnknown,
}

/// 正式任务装配所需的固定集成优先级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrationResponsibilityPriority {
    /// 普通。
    Normal,
    /// 高。
    High,
    /// 紧急。
    Urgent,
}

/// W29 固定责任事实；正式任务的 ID、取时与字段不变式由组合层原构造器处理。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationResponsibilitySpec {
    /// 固定任务种类。
    pub task_kind: IntegrationResponsibilityKind,
    /// 固定优先级。
    pub priority: IntegrationResponsibilityPriority,
    /// 当前责任任务指向的业务对象类型。
    pub business_object_type: String,
    /// 当前责任任务指向的业务对象 ID。
    pub business_object_id: String,
    /// 当前业务主题版本。
    pub subject_version: String,
    /// 固定责任角色。
    pub owner_role: String,
    /// 固定责任组织。
    pub owner_organization_id: String,
    /// 调用方已解析的原始个人责任人，不在本事实层规范化或验证。
    pub owner_user_id: String,
    /// 原错误分类或差异分类。
    pub reason_code: Option<String>,
    /// 原责任影响摘要。
    pub impact_summary: Option<String>,
}

/// 从错误事实提取固定责任，不生成 ID、不取时、不提前验证任务字段。
///
/// 处理人及其内部组织必须已写入错误任务；本层不再写入公司占位。
pub fn error_responsibility(task: &IntegrationErrorTask) -> IntegrationResponsibilitySpec {
    IntegrationResponsibilitySpec {
        task_kind: error_work_item_type(task.error_class),
        business_object_type: ERROR_WORK_ITEM_OBJECT_TYPE.to_string(),
        business_object_id: task.base.id.clone(),
        subject_version: task.base.version.to_string(),
        owner_role: error_owner_role(task.error_class).to_string(),
        owner_organization_id: task.owner_org_unit_id.clone(),
        owner_user_id: task.owner_user_id.clone().unwrap_or_default(),
        priority: error_priority(task.error_class),
        reason_code: Some(task.error_class.as_str().to_string()),
        impact_summary: Some(format!("集成异常待处理：{}", task.error_class.label())),
    }
}

/// 从差异事实提取固定责任；仅保留原固定责任注册校验，初始主题版本为 0。
///
/// # 错误
/// 差异类型未注册固定责任规则时返回原错误。
pub fn difference_responsibility(
    difference: &ReconciliationDifference,
) -> Result<IntegrationResponsibilitySpec> {
    let owner_role = difference_owner_role(&difference.difference_type)?;
    Ok(IntegrationResponsibilitySpec {
        task_kind: IntegrationResponsibilityKind::BusinessException,
        business_object_type: DIFFERENCE_WORK_ITEM_OBJECT_TYPE.to_string(),
        business_object_id: difference.base.id.clone(),
        subject_version: DIFFERENCE_INITIAL_SUBJECT_VERSION.to_string(),
        owner_role: owner_role.to_string(),
        owner_organization_id: difference.owner_org_unit_id.clone(),
        owner_user_id: difference.owner_user_id.clone(),
        priority: IntegrationResponsibilityPriority::High,
        reason_code: Some(difference.difference_type.clone()),
        impact_summary: Some(format!("对账差异待核验：{}", difference.difference_type)),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        IntegrationResponsibilityKind, IntegrationResponsibilityPriority, difference_owner_role,
        error_owner_role, error_priority, error_work_item_type,
    };
    use crate::entity::integration_ops::ErrorClass;

    #[test]
    fn error_registry_covers_every_class() {
        let cases = [
            (
                ErrorClass::CapabilityGap,
                "role-operations",
                IntegrationResponsibilityKind::BusinessException,
                IntegrationResponsibilityPriority::Normal,
            ),
            (
                ErrorClass::MappingError,
                "role-operations",
                IntegrationResponsibilityKind::BusinessException,
                IntegrationResponsibilityPriority::Normal,
            ),
            (
                ErrorClass::BusinessRejected,
                "role-procurement",
                IntegrationResponsibilityKind::BusinessException,
                IntegrationResponsibilityPriority::Normal,
            ),
            (
                ErrorClass::TransientFailure,
                "role-sysadmin",
                IntegrationResponsibilityKind::BusinessException,
                IntegrationResponsibilityPriority::High,
            ),
            (
                ErrorClass::ResultUnknown,
                "role-sysadmin",
                IntegrationResponsibilityKind::IntegrationResultUnknown,
                IntegrationResponsibilityPriority::Urgent,
            ),
            (
                ErrorClass::AuthSignature,
                "role-sysadmin",
                IntegrationResponsibilityKind::BusinessException,
                IntegrationResponsibilityPriority::Urgent,
            ),
            (
                ErrorClass::RateLimited,
                "role-sysadmin",
                IntegrationResponsibilityKind::BusinessException,
                IntegrationResponsibilityPriority::High,
            ),
            (
                ErrorClass::OutOfOrder,
                "role-sysadmin",
                IntegrationResponsibilityKind::BusinessException,
                IntegrationResponsibilityPriority::High,
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
        for code in
            ["amount_mismatch", "refund_mismatch", "balance_mismatch", "settlement_mismatch", "cost_mismatch"]
        {
            assert_eq!(difference_owner_role(code).unwrap(), "role-finance");
        }
        for code in ["supplier_supply_mismatch", "supplier_order_mismatch", "supply_mismatch"] {
            assert_eq!(difference_owner_role(code).unwrap(), "role-procurement");
        }
        for code in ["result_unknown", "integration_result_unknown"] {
            assert_eq!(difference_owner_role(code).unwrap(), "role-sysadmin");
        }
        assert_eq!(difference_owner_role(" Amount_Mismatch ").unwrap(), "role-finance");
        assert!(difference_owner_role("free_form_type").is_err());
        assert!(difference_owner_role("  ").is_err());
    }

    #[test]
    fn error_spec_uses_task_handler_and_internal_org() {
        let mut task = crate::entity::integration_ops::integration_error_task::tests::task();
        task.base.version = 7;
        task.error_class = ErrorClass::ResultUnknown;
        task.owner_role = Some("old-role".to_string());
        task.owner_user_id = Some("old-owner".to_string());
        task.owner_org_unit_id = "org-sysadmin".to_string();
        let spec = super::error_responsibility(&task);
        assert_eq!(
            spec,
            super::IntegrationResponsibilitySpec {
                task_kind: IntegrationResponsibilityKind::IntegrationResultUnknown,
                priority: IntegrationResponsibilityPriority::Urgent,
                business_object_type: "integration_error_task".to_string(),
                business_object_id: task.base.id.clone(),
                subject_version: "7".to_string(),
                owner_role: "role-sysadmin".to_string(),
                owner_organization_id: "org-sysadmin".to_string(),
                owner_user_id: "old-owner".to_string(),
                reason_code: Some("result_unknown".to_string()),
                impact_summary: Some("集成异常待处理：结果未知".to_string()),
            }
        );
    }

    #[test]
    fn difference_spec_preserves_raw_reason_and_initial_subject() {
        use crate::entity::integration_ops::{ReconciliationDifference, ReconciliationDifferenceId};
        let mut difference = ReconciliationDifference::new(
            ReconciliationDifferenceId::new("diff-spec"),
            crate::entity::integration_ops::reconciliation_difference::tests::difference_data(),
        )
        .unwrap();
        difference.base.version = 9;
        // 既有事实允许带历史大小写和空白；注册查询归一化，冻结原因仍用原值。
        difference.difference_type = " Amount_Mismatch ".to_string();
        let spec = super::difference_responsibility(&difference).unwrap();
        assert_eq!(
            spec,
            super::IntegrationResponsibilitySpec {
                task_kind: IntegrationResponsibilityKind::BusinessException,
                priority: IntegrationResponsibilityPriority::High,
                business_object_type: "reconciliation_difference".to_string(),
                business_object_id: "diff-spec".to_string(),
                subject_version: "0".to_string(),
                owner_role: "role-finance".to_string(),
                owner_organization_id: "org-finance".to_string(),
                owner_user_id: "user-1".to_string(),
                reason_code: Some(" Amount_Mismatch ".to_string()),
                impact_summary: Some("对账差异待核验： Amount_Mismatch ".to_string()),
            }
        );
    }

    #[test]
    fn difference_spec_only_validates_registered_responsibility() {
        use crate::entity::integration_ops::{ReconciliationDifference, ReconciliationDifferenceId};
        let mut difference = ReconciliationDifference::new(
            ReconciliationDifferenceId::new("diff-first-error"),
            crate::entity::integration_ops::reconciliation_difference::tests::difference_data(),
        )
        .unwrap();
        assert_eq!(super::difference_responsibility(&difference).unwrap().owner_user_id, "user-1");
        difference.difference_type = "free_form_type".to_string();
        assert_eq!(
            super::difference_responsibility(&difference).unwrap_err().to_string(),
            erp_core::Error::from("差异类型未注册固定责任规则，禁止创建任务").to_string(),
        );
    }
}
