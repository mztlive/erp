//! 已接线 DataScope v2 消费者登记；不得用初始化清单代替准入。

use crate::access_control::ScopeDimension;
use crate::error::{Error, Result};

/// 已由真实消费者接入公共解析的资源动作及必需维度。
///
/// 采购与工作流尚未接线，不得列入本表。初始化清单见 `predefined_data_scopes`。
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

    /// 合同已接线可解析；采购未接线必须拒绝。
    #[test]
    fn wired_contract_is_admitted_and_unwired_purchase_is_not() {
        assert!(registration("contract", "list").is_ok());
        assert!(registration("contract", "detail").is_ok());
        assert!(registration("purchase_order", "list").is_err());
        assert!(registration("contract", "delete").is_err());
    }
}
