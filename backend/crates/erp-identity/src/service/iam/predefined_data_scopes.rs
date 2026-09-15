//! S2 首次授权清单；范围身份固定，重跑不恢复已撤销记录。

use super::SharedRbacService;
use crate::Result;
use crate::access_control::{
    DataScopeData, DataScopeSubjectType, DataScopeType, ScopeBinding, ScopeDimension, ScopeTargetMode,
};
use crate::service::access_control::consumers::validate_binding;

/// 已接入 S2 的资源与完整动作目录；不得用通配符初始化范围。
pub(crate) const RESOURCE_ACTIONS: &[(&str, &[&str])] = &[
    ("approval_instance", &["read", "decide", "resume", "cancel", "cancel_blocked", "upgrade_binding"]),
    ("stock_adjustment", &["list", "detail", "create", "update", "submit"]),
    ("stock_balance", &["list", "detail"]),
    ("stock_movement", &["list"]),
    ("stock_reservation", &["list"]),
    ("customer_refund", &["submit"]),
    ("supplier_refund", &["submit"]),
    ("supplier_settlement_statement", &["confirm"]),
    ("work_item", &["manage"]),
    ("org_unit", &["list", "manage"]),
    ("cost_entry", &["list", "detail"]),
    ("cost_allocation", &["list"]),
    ("customer", &["list", "detail", "create", "update", "delete"]),
    ("contract", &["list", "detail", "create", "update"]),
    ("sales_order", &["list", "detail", "create", "update", "delete", "submit", "cancel_approval"]),
    ("purchase_order", &["list", "detail", "create", "update", "delete", "submit", "cancel_approval"]),
];

/// 首次初始化显式岗位清单；没有条目的岗位不获得兜底范围。
///
/// # 错误
/// 任一资源初始化失败时返回错误，日志携带角色及资源定位信息。
pub async fn ensure_predefined_role_data_scopes(rbac: &SharedRbacService) -> Result<()> {
    for role in [
        "role-sales",
        "role-sales-leader",
        "role-procurement",
        "role-finance",
        "role-management",
        "role-sysadmin",
    ] {
        seed_role(rbac, role).await?;
    }
    Ok(())
}

/// 按清单初始化指定岗位，已有配置或撤销记录均保留。
///
/// # 错误
/// 模型校验或事务写入失败时返回错误。
pub(crate) async fn seed_role(rbac: &SharedRbacService, role: &str) -> Result<()> {
    let manifest = RESOURCE_ACTIONS
        .iter()
        .map(|(resource, actions)| (*resource, definitions(role, resource, actions)))
        .collect::<Vec<_>>();
    for (_, definitions) in &manifest {
        for data in definitions {
            validate_binding(&data.binding)?;
        }
    }
    for (resource, definitions) in manifest {
        if !definitions.is_empty() {
            rbac.seed_data_scope_manifest(role, resource, definitions).await?;
        }
    }
    Ok(())
}

/// 明确岗位、资源与动作的默认规则，RBAC 仍独立决定实际动作权限。
fn definitions(role: &str, resource: &str, actions: &[&str]) -> Vec<DataScopeData> {
    if resource == "work_item" && !matches!(role, "role-root" | "role-sysadmin" | "role-management") {
        return Vec::new();
    }
    if resource == "org_unit" && !matches!(role, "role-root" | "role-sysadmin") {
        return Vec::new();
    }
    if matches!(
        resource,
        "stock_adjustment"
            | "stock_balance"
            | "stock_movement"
            | "stock_reservation"
            | "customer_refund"
            | "supplier_refund"
    ) && !matches!(role, "role-root" | "role-finance")
    {
        return Vec::new();
    }
    if resource == "supplier_settlement_statement" && !matches!(role, "role-root" | "role-finance") {
        return Vec::new();
    }
    if resource == "approval_instance" && !matches!(role, "role-root" | "role-finance" | "role-management") {
        return Vec::new();
    }
    let mut scope_types = vec![DataScopeType::Company];
    let mut granted_actions = actions.to_vec();
    match role {
        "role-root" => {},
        "role-finance"
            if matches!(
                resource,
                "supplier_settlement_statement"
                    | "stock_adjustment"
                    | "stock_balance"
                    | "stock_movement"
                    | "stock_reservation"
                    | "customer_refund"
                    | "supplier_refund"
            ) => {},
        "role-finance" | "role-management" if resource == "approval_instance" => {},
        "role-sysadmin" if matches!(resource, "org_unit" | "work_item") => {},
        "role-management" if resource == "work_item" => {},
        "role-management" | "role-finance" => {
            granted_actions.retain(|action| matches!(*action, "list" | "detail"))
        },
        "role-sales-leader" => {
            scope_types = vec![DataScopeType::Team];
            granted_actions.retain(|action| matches!(*action, "list" | "detail"));
        },
        "role-sales" if resource != "purchase_order" => {
            scope_types = vec![DataScopeType::SelfOwned, DataScopeType::Collaborative]
        },
        "role-procurement" if resource == "purchase_order" => scope_types = vec![DataScopeType::SelfOwned],
        _ => return Vec::new(),
    }
    scope_types
        .into_iter()
        .map(|scope_type| definition(role, resource, &granted_actions, scope_type))
        .collect()
}

/// 构造清单中的单条规范化输入；动态管理范围必须显式授予管理关系才产生目标。
fn definition(role: &str, resource: &str, actions: &[&str], scope_type: DataScopeType) -> DataScopeData {
    DataScopeData {
        subject_type: DataScopeSubjectType::Role,
        subject_id: role.into(),
        scope_type,
        scope_targets: vec![],
        binding: ScopeBinding {
            schema_version: 2,
            resource: resource.into(),
            actions: actions.iter().map(|value| (*value).into()).collect(),
            target_dimension: match resource {
                "stock_adjustment" | "stock_balance" | "stock_movement" | "stock_reservation" => {
                    ScopeDimension::Warehouse
                },
                "customer_refund" | "supplier_refund" => ScopeDimension::SettlementParty,
                _ => ScopeDimension::InternalOrg,
            },
            target_mode: scope_type.requires_targets().then_some(ScopeTargetMode::ManagedOrgs),
            include_descendants: None,
            enabled: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access_control::{DataScope, DataScopeId};

    #[test]
    fn explicit_manifest_has_no_company_fallback_for_operational_roles() {
        assert!(definitions("role-warehouse", "sales_order", &["list"]).is_empty());
        assert!(definitions("role-sales", "purchase_order", &["list"]).is_empty());
        let sales = definitions("role-sales", "sales_order", &["list", "update"]);
        assert!(sales.iter().all(|rule| rule.scope_type != DataScopeType::Company));
        assert_eq!(sales.len(), 2);
        let manager = definitions("role-sales-leader", "sales_order", &["list", "update"]);
        assert_eq!(manager[0].binding.target_mode, Some(ScopeTargetMode::ManagedOrgs));
        assert!(!manager[0].binding.actions.contains(&"update".into()));
    }

    #[test]
    fn every_manifest_entry_uses_version_two_and_explicit_resource_actions() {
        for role in [
            "role-root",
            "role-sales",
            "role-sales-leader",
            "role-procurement",
            "role-finance",
            "role-management",
        ] {
            for (resource, actions) in RESOURCE_ACTIONS {
                for data in definitions(role, resource, actions) {
                    let scope = DataScope::new(DataScopeId::new("test"), data).unwrap();
                    validate_binding(&scope.binding).unwrap();
                    assert_eq!(scope.binding.schema_version, 2);
                    assert_eq!(scope.binding.resource, *resource);
                }
            }
        }
    }
}
