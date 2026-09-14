//! 已接线 DataScope v2 消费者登记；不得用初始化清单代替准入。

use crate::access_control::ScopeDimension;
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
    /// 该资源动作解析所需维度。
    pub required_dimensions: &'static [ScopeDimension],
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
        .ok_or_else(|| Error::ValidationError("资源或动作尚未接入 DataScope v2".into()))?;
    if !entry.1.contains(&action) {
        return Err(Error::ValidationError("资源或动作尚未接入 DataScope v2".into()));
    }
    Ok(ConsumerRegistration {
        required_dimensions: entry.2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
