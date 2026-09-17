//! 已接线 DataScope v2 消费者登记；不得用初始化清单代替准入。

use crate::access_control::{ScopeBinding, ScopeDimension};
use crate::error::{Error, Result};

/// 已由真实消费者接入公共解析的资源动作及必需维度。
///
/// 工作流按审批动作、任务管理及结算复核分别准入。采购变更单及退货沿 `purchase_order`
/// 资源动作接入，不单独登记平行资源。
/// 初始化清单见 `predefined_data_scopes`，不得当作本表替代。
const WIRED_CONSUMERS: &[(&str, &[&str], &[ScopeDimension])] = &[
    (
        "approval_instance",
        &["read", "decide", "resume", "cancel", "cancel_blocked", "upgrade_binding"],
        &[ScopeDimension::InternalOrg, ScopeDimension::Warehouse, ScopeDimension::SettlementParty],
    ),
    ("stock_adjustment", &["list", "detail", "create", "update", "submit"], &[ScopeDimension::Warehouse]),
    ("stock_balance", &["list", "detail"], &[ScopeDimension::Warehouse]),
    ("stock_movement", &["list"], &[ScopeDimension::Warehouse]),
    ("stock_reservation", &["list"], &[ScopeDimension::Warehouse]),
    ("customer_refund", &["submit"], &[ScopeDimension::SettlementParty]),
    ("supplier_refund", &["submit"], &[ScopeDimension::SettlementParty]),
    (
        "supplier_settlement_statement",
        &["list", "detail", "create", "update", "submit", "confirm"],
        &[ScopeDimension::InternalOrg],
    ),
    ("work_item", &["manage"], &[ScopeDimension::InternalOrg]),
    ("org_unit", &["list", "manage"], &[ScopeDimension::InternalOrg]),
    ("customer", &["list", "detail", "create", "update", "delete"], &[ScopeDimension::InternalOrg]),
    ("contract", &["list", "detail", "create", "update"], &[ScopeDimension::InternalOrg]),
    (
        "sales_order",
        &["list", "detail", "create", "update", "delete", "submit", "cancel_approval"],
        &[ScopeDimension::InternalOrg],
    ),
    (
        "purchase_order",
        &["list", "detail", "create", "update", "delete", "submit", "cancel_approval"],
        &[ScopeDimension::InternalOrg],
    ),
    ("cost_entry", &["list", "detail"], &[ScopeDimension::InternalOrg]),
    ("cost_allocation", &["list"], &[ScopeDimension::InternalOrg]),
    (
        "sales_selection_booklet",
        &[
            "list",
            "get",
            "create",
            "maintain",
            "prepare",
            "publish",
            "copy_link",
            "rotate_link",
            "close",
            "revoke",
            "void",
        ],
        &[ScopeDimension::InternalOrg],
    ),
    ("sales_selection_proposal", &["list", "get"], &[ScopeDimension::InternalOrg]),
    ("receivable_account", &["list", "detail"], &[ScopeDimension::InternalOrg]),
    ("customer_receipt", &["list", "detail"], &[ScopeDimension::InternalOrg]),
    ("invoice", &["list", "detail"], &[ScopeDimension::InternalOrg]),
    ("sales_invoice_request", &["list", "detail"], &[ScopeDimension::InternalOrg]),
    ("payable_account", &["list", "detail"], &[ScopeDimension::InternalOrg]),
    ("supplier_payment", &["list", "detail"], &[ScopeDimension::InternalOrg]),
    ("purchase_invoice_allocation", &["list"], &[ScopeDimension::InternalOrg]),
    ("integration_error_task", &["list", "detail", "create"], &[ScopeDimension::InternalOrg]),
    ("reconciliation_difference", &["list", "detail", "create", "decide"], &[ScopeDimension::InternalOrg]),
    ("supplier", &["list", "detail", "create", "update", "delete"], &[ScopeDimension::InternalOrg]),
    ("product", &["list", "detail", "create", "update"], &[ScopeDimension::InternalOrg]),
    ("supplier_offering", &["list", "create", "update"], &[ScopeDimension::InternalOrg]),
];

/// 已接线消费者的资源动作登记。
pub struct ConsumerRegistration {
    /// 本资源动作接受的身份维度；未列出的维度必须拒绝。
    pub supported_dimensions: &'static [ScopeDimension],
    /// 该资源动作解析所需维度。
    pub required_dimensions: &'static [ScopeDimension],
    /// 合法历史参与是否可补充该动作的正向读取范围。
    pub allows_history: bool,
}

/// 按真实消费者登记查找资源动作。
///
/// # 参数
/// * `resource` - 业务资源
/// * `action` - 已注册动作
///
/// # 返回
/// 返回该资源动作的必需维度。
///
/// # 错误
/// 未接线资源或动作返回校验错误。
///
/// # 关键业务约束
/// 不得把初始化清单当作消费者已接入的证据。
pub fn registration(resource: &str, action: &str) -> Result<ConsumerRegistration> {
    let entry = WIRED_CONSUMERS
        .iter()
        .find(|(name, _, _)| *name == resource)
        .ok_or_else(|| Error::ValidationError(format!("{resource}:{action} 尚未接入 DataScope v2")))?;
    if !entry.1.contains(&action) {
        return Err(Error::ValidationError(format!("{resource}:{action} 尚未接入 DataScope v2")));
    }
    Ok(ConsumerRegistration {
        supported_dimensions: entry.2,
        required_dimensions: if resource == "approval_instance" { &[] } else { entry.2 },
        allows_history: matches!(resource, "customer" | "contract" | "sales_order" | "purchase_order")
            && matches!(action, "list" | "detail"),
    })
}

/// 对配置、初始化及已存储规则执行相同的消费者准入。
///
/// # 参数
/// * `binding` - 已通过模型形态校验的资源动作绑定。
/// # 返回
/// 全部动作及目标维度已接线时成功。
/// # 错误
/// 任一动作未接线或维度不支持时拒绝，停用规则也不能绕过准入。
pub fn validate_binding(binding: &ScopeBinding) -> Result<()> {
    for action in &binding.actions {
        let consumer = registration(&binding.resource, action)?;
        if !consumer.supported_dimensions.contains(&binding.target_dimension) {
            return Err(Error::ValidationError(format!(
                "{}:{} 不支持 {:?} 范围维度",
                binding.resource, action, binding.target_dimension
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_checks_every_action_and_dimension_even_when_disabled() {
        let mut binding = ScopeBinding {
            schema_version: 2,
            resource: "customer".into(),
            actions: vec!["list".into()],
            target_dimension: ScopeDimension::InternalOrg,
            target_mode: None,
            include_descendants: None,
            enabled: false,
        };
        assert!(validate_binding(&binding).is_ok());
        binding.actions.push("resume".into());
        assert!(validate_binding(&binding).is_err());
        binding.actions.pop();
        binding.target_dimension = ScopeDimension::Warehouse;
        assert!(validate_binding(&binding).is_err());
        assert!(registration("customer", "detail").unwrap().allows_history);
        assert!(!registration("customer", "update").unwrap().allows_history);
        assert!(!registration("org_unit", "list").unwrap().allows_history);
    }

    /// 合同与采购主单已接线可解析；未接线动作与工作流资源必须拒绝。
    #[test]
    fn wired_contract_and_purchase_are_admitted_unwired_actions_are_not() {
        assert!(registration("contract", "list").is_ok());
        assert!(registration("contract", "detail").is_ok());
        assert!(registration("purchase_order", "list").is_ok());
        assert!(registration("purchase_order", "detail").is_ok());
        assert!(registration("purchase_order", "create").is_ok());
        assert!(registration("purchase_order", "update").is_ok());
        assert!(registration("purchase_order", "delete").is_ok());
        assert!(registration("purchase_order", "submit").is_ok());
        assert!(registration("purchase_order", "cancel_approval").is_ok());
        assert!(registration("sales_selection_booklet", "list").is_ok());
        assert!(registration("sales_selection_booklet", "create").is_ok());
        assert!(registration("sales_selection_proposal", "list").is_ok());
        assert!(registration("sales_selection_proposal", "get").is_ok());
        assert!(registration("sales_selection_proposal", "detail").is_err());
        assert!(registration("purchase_change_order", "list").is_err());
        assert!(registration("purchase_return_order", "list").is_err());
        assert!(registration("contract", "delete").is_err());
        assert!(registration("purchase_order", "transfer").is_err());
        assert!(registration("work_item", "list").is_err());
        assert!(registration("approval_instance", "decide").is_ok());
        assert!(registration("work_item", "manage").is_ok());
    }

    /// S3-03 资金资源已接线可解析；写动作与历史参与均不在本次接线内。
    #[test]
    fn wired_funds_resources_admit_reads_without_history_or_commands() {
        for resource in [
            "receivable_account",
            "customer_receipt",
            "invoice",
            "sales_invoice_request",
            "payable_account",
            "supplier_payment",
        ] {
            let list = registration(resource, "list").unwrap();
            assert!(!list.allows_history);
            let detail = registration(resource, "detail").unwrap();
            assert!(!detail.allows_history);
            assert!(registration(resource, "submit").is_err());
            assert!(registration(resource, "commit").is_err());
        }
        let allocation = registration("purchase_invoice_allocation", "list").unwrap();
        assert!(!allocation.allows_history);
        assert!(registration("purchase_invoice_allocation", "detail").is_err());
    }

    /// S3-08 新增消费动作已接线可解析；未接线动作与资源必须失败关闭。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无；接线与关闭均符合登记时通过。
    ///
    /// # 关键业务约束
    /// 不得以初始化清单代替本登记；历史参与仅四类读取动作。
    #[test]
    fn wired_s3_actions_admit_reads_unwired_actions_fail_closed() {
        for action in ["list", "get", "create", "maintain", "prepare", "publish", "copy_link"] {
            assert!(registration("sales_selection_booklet", action).is_ok());
        }
        for action in ["list", "get"] {
            assert!(registration("sales_selection_proposal", action).is_ok());
            assert!(!registration("sales_selection_proposal", action).unwrap().allows_history);
        }
        assert!(registration("sales_selection_proposal", "create").is_err());
        assert!(registration("sales_selection_booklet", "approve").is_err());
        for action in ["list", "detail"] {
            assert!(registration("cost_entry", action).is_ok());
            assert!(!registration("cost_entry", action).unwrap().allows_history);
        }
        assert!(registration("cost_allocation", "list").is_ok());
        assert!(registration("cost_allocation", "detail").is_err());
        assert!(registration("fulfillment_queue", "list").is_err());
        assert!(registration("customer_quality", "list").is_err());
        assert!(registration("handover", "update").is_err());
        for action in ["list", "detail", "create", "update", "delete"] {
            let consumer = registration("supplier", action).unwrap();
            assert!(!consumer.allows_history);
        }
        assert!(registration("supplier", "submit").is_err());
    }

    #[test]
    fn wired_integration_handlers_reject_history_writes() {
        for action in ["list", "detail", "create"] {
            let consumer = registration("integration_error_task", action).unwrap();
            assert!(!consumer.allows_history);
            assert_eq!(consumer.required_dimensions, &[ScopeDimension::InternalOrg]);
        }
        for action in ["list", "detail", "create", "decide"] {
            let consumer = registration("reconciliation_difference", action).unwrap();
            assert!(!consumer.allows_history);
        }
        assert!(registration("integration_error_task", "decide").is_err());
        assert!(registration("reconciliation_difference", "update").is_err());
    }

    #[test]
    fn wired_supplier_offering_admits_list_and_writes_without_history() {
        for action in ["list", "create", "update"] {
            let consumer = registration("supplier_offering", action).unwrap();
            assert!(!consumer.allows_history);
            assert_eq!(consumer.required_dimensions, &[ScopeDimension::InternalOrg]);
        }
        assert!(registration("supplier_offering", "detail").is_err());
        assert!(registration("supplier_offering", "delete").is_err());
    }
}
