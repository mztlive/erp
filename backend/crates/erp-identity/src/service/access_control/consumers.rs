//! 已接线 DataScope v2 消费者登记；不得用初始化清单代替准入。

use crate::access_control::{ScopeBinding, ScopeDimension};
use crate::error::{Error, Result};

/// 已由真实消费者接入公共解析的资源动作及必需维度。
///
/// 工作流与工作台尚未接线，不得列入本表。采购变更单及退货沿 `purchase_order`
/// 资源动作接入，不单独登记平行资源。
/// 初始化清单见 `predefined_data_scopes`，不得当作本表替代。
const WIRED_CONSUMERS: &[(&str, &[&str], &[ScopeDimension])] = &[
    ("org_unit", &["list", "manage"], &[ScopeDimension::InternalOrg]),
    (
        "customer",
        &["list", "detail", "create", "update", "delete"],
        &[ScopeDimension::InternalOrg],
    ),
    (
        "contract",
        &["list", "detail", "create", "update"],
        &[ScopeDimension::InternalOrg],
    ),
    (
        "sales_order",
        &[
            "list",
            "detail",
            "create",
            "update",
            "delete",
            "submit",
            "cancel_approval",
        ],
        &[ScopeDimension::InternalOrg],
    ),
    (
        "purchase_order",
        &[
            "list",
            "detail",
            "create",
            "update",
            "delete",
            "submit",
            "cancel_approval",
        ],
        &[ScopeDimension::InternalOrg],
    ),
    ("cost_entry", &["list", "detail"], &[ScopeDimension::InternalOrg]),
    ("cost_allocation", &["list"], &[ScopeDimension::InternalOrg]),
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
        return Err(Error::ValidationError(format!(
            "{resource}:{action} 尚未接入 DataScope v2"
        )));
    }
    Ok(ConsumerRegistration {
        supported_dimensions: entry.2,
        required_dimensions: entry.2,
        allows_history: matches!(
            resource,
            "customer" | "contract" | "sales_order" | "purchase_order"
        ) && matches!(action, "list" | "detail"),
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
        assert!(registration("purchase_change_order", "list").is_err());
        assert!(registration("purchase_return_order", "list").is_err());
        assert!(registration("contract", "delete").is_err());
        assert!(registration("purchase_order", "transfer").is_err());
        assert!(registration("work_item", "list").is_err());
        assert!(registration("approval_instance", "decide").is_err());
    }
}
