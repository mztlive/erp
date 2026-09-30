//! 旧 S2 角色范围合同的测试样本；本模块仅在 cfg(test) 编译。
//! 不用于账号或种子初始化，ManagedOrgs 仅保留旧规则样本，运行时必须重设人员范围。

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
    ("supplier_settlement_statement", &["list", "detail", "create", "update", "submit", "confirm"]),
    ("work_item", &["manage"]),
    ("org_unit", &["list", "manage"]),
    ("cost_entry", &["list", "detail"]),
    ("cost_allocation", &["list"]),
    ("receivable_account", &["list", "detail"]),
    ("customer_receipt", &["list", "detail"]),
    ("invoice", &["list", "detail"]),
    ("sales_invoice_request", &["list", "detail"]),
    ("payable_account", &["list", "detail"]),
    ("supplier_payment", &["list", "detail"]),
    ("purchase_invoice_allocation", &["list"]),
    ("customer", &["list", "detail", "create", "update", "delete"]),
    ("contract", &["list", "detail", "create", "update"]),
    ("sales_order", &["list", "detail", "create", "update", "delete", "submit", "cancel_approval"]),
    ("purchase_order", &["list", "detail", "create", "update", "delete", "submit", "cancel_approval"]),
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
    ),
    ("sales_selection_proposal", &["list", "get"]),
    ("integration_error_task", &["list", "detail", "create"]),
    ("reconciliation_difference", &["list", "detail", "create", "decide"]),
    ("supplier", &["list", "detail", "create", "update", "delete"]),
    ("product", &["list", "detail", "create", "update"]),
    ("supplier_offering", &["list", "create", "update"]),
    (
        "supplier_fulfillment_order",
        &["list", "detail", "investigate", "complete", "submit", "cancel", "refund", "reject", "handover"],
    ),
    ("person_query_qualification", &["manage"]),
    ("settlement_party", &["list"]),
    ("warehouse", &["list"]),
    ("business_person", &["list"]),
    ("sales_person", &["list"]),
    ("procurement_person", &["list"]),
];

/// 明确岗位、资源与动作的默认规则，RBAC 仍独立决定实际动作权限。
fn definitions(role: &str, resource: &str, actions: &[&str]) -> Vec<DataScopeData> {
    if matches!(resource, "settlement_party" | "warehouse") {
        let eligible = matches!(role, "role-root" | "role-finance" | "role-management")
            || (resource == "settlement_party"
                && matches!(role, "role-sales" | "role-sales-leader" | "role-procurement"))
            || (resource == "warehouse"
                && matches!(
                    role,
                    "role-warehouse"
                        | "role-procurement"
                        | "role-sales"
                        | "role-sales-leader"
                        | "role-operations"
                ));
        return if eligible {
            vec![definition(role, resource, actions, DataScopeType::Company)]
        } else {
            Vec::new()
        };
    }
    if resource == "person_query_qualification" {
        return if matches!(role, "role-root" | "role-sysadmin") {
            vec![definition(role, resource, actions, DataScopeType::Company)]
        } else {
            Vec::new()
        };
    }
    if matches!(resource, "sales_person" | "procurement_person" | "business_person") {
        return person_directory_definitions(role, resource, actions);
    }
    if !definition_applies(role, resource) {
        return Vec::new();
    }
    if matches!(resource, "integration_error_task" | "reconciliation_difference") {
        return integration_definitions(role, resource, actions);
    }
    let mut scope_types = vec![DataScopeType::Company];
    let mut granted_actions = actions.to_vec();
    apply_role_action_narrowing(role, resource, &mut scope_types, &mut granted_actions);
    if scope_types.is_empty() {
        return Vec::new();
    }
    scope_types
        .into_iter()
        .map(|scope_type| definition(role, resource, &granted_actions, scope_type))
        .collect()
}

/// 判断岗位与资源的默认规则是否适用（纯角色/资源包含判定）。
///
/// # 参数
/// * `role` - 岗位角色 ID
/// * `resource` - 资源代码
///
/// # 返回
/// 适用时返回 `true`；不适用返回 `false`（调用方返回空集）。
fn definition_applies(role: &str, resource: &str) -> bool {
    if resource == "work_item" && !matches!(role, "role-root" | "role-sysadmin" | "role-management") {
        return false;
    }
    if resource == "org_unit" && !matches!(role, "role-root" | "role-sysadmin") {
        return false;
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
        return false;
    }
    if resource == "supplier_settlement_statement" && !matches!(role, "role-root" | "role-finance") {
        return false;
    }
    if resource == "approval_instance" && !matches!(role, "role-root" | "role-finance" | "role-management") {
        return false;
    }
    true
}

/// 按岗位收敛默认范围类型与可授予动作（纯角色/资源包含判定）。
///
/// # 参数
/// * `role` - 岗位角色 ID
/// * `resource` - 资源代码
/// * `scope_types` - 待收敛的范围类型（默认公司级）
/// * `granted_actions` - 待收敛的可授予动作
fn apply_role_action_narrowing(
    role: &str,
    resource: &str,
    scope_types: &mut Vec<DataScopeType>,
    granted_actions: &mut Vec<&str>,
) {
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
            *scope_types = vec![DataScopeType::Team];
            granted_actions.retain(|action| matches!(*action, "list" | "detail"));
        },
        "role-sales"
            if !matches!(
                resource,
                "purchase_order" | "payable_account" | "supplier_payment" | "purchase_invoice_allocation"
            ) =>
        {
            *scope_types = vec![DataScopeType::SelfOwned]
        },
        "role-procurement" if matches!(resource, "supplier" | "product") => {
            *scope_types = vec![DataScopeType::SelfOwned]
        },
        "role-procurement" if resource == "supplier_fulfillment_order" => {
            *scope_types = vec![DataScopeType::SelfOwned]
        },
        "role-procurement" if resource == "purchase_order" => *scope_types = vec![DataScopeType::SelfOwned],
        "role-procurement"
            if matches!(resource, "payable_account" | "supplier_payment" | "purchase_invoice_allocation") =>
        {
            *scope_types = vec![DataScopeType::SelfOwned]
        },
        _ => {
            scope_types.clear();
        },
    }
}

/// 集成异常与差异的岗位默认范围；处理人按本人，管理者按公司。
fn integration_definitions(role: &str, resource: &str, actions: &[&str]) -> Vec<DataScopeData> {
    let (scope_types, granted) = match role {
        "role-root" | "role-sysadmin" => (vec![DataScopeType::Company], actions.to_vec()),
        "role-management" => (
            vec![DataScopeType::Company],
            actions.iter().copied().filter(|a| matches!(*a, "list" | "detail")).collect(),
        ),
        "role-finance" | "role-procurement" => (vec![DataScopeType::SelfOwned], actions.to_vec()),
        _ => return Vec::new(),
    };
    scope_types
        .into_iter()
        .map(|scope_type| definition(role, resource, granted.as_slice(), scope_type))
        .collect()
}

/// 人员目录的默认范围。不复制合同或采购单的既有范围。
///
/// 销售本人、采购本人、销售领导按管理组织、财务和管理层按公司。
/// 超级管理员沿用公司级。没有列出的岗位保持关闭。
fn person_directory_definitions(role: &str, resource: &str, actions: &[&str]) -> Vec<DataScopeData> {
    let scope_type = match (role, resource) {
        ("role-sales" | "role-procurement" | "role-operations" | "role-warehouse", "business_person") => {
            DataScopeType::SelfOwned
        },
        ("role-sales-leader", "business_person") => DataScopeType::Team,
        ("role-finance" | "role-management", "business_person") => DataScopeType::Company,
        ("role-root", "business_person" | "sales_person" | "procurement_person") => DataScopeType::Company,
        ("role-sales", "sales_person") => DataScopeType::SelfOwned,
        ("role-procurement", "procurement_person") => DataScopeType::SelfOwned,
        ("role-sales-leader", "sales_person" | "procurement_person") => DataScopeType::Team,
        ("role-finance" | "role-management", "sales_person" | "procurement_person") => DataScopeType::Company,
        _ => return Vec::new(),
    };
    let granted = actions.iter().copied().filter(|action| *action == "list").collect::<Vec<_>>();
    if granted.is_empty() {
        return Vec::new();
    }
    vec![definition(role, resource, &granted, scope_type)]
}

/// 构造旧清单的测试输入；ManagedOrgs 不得重新用于当前人员授权。
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
                "customer_refund" | "supplier_refund" | "settlement_party" => ScopeDimension::SettlementParty,
                "warehouse" => ScopeDimension::Warehouse,
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
        assert_eq!(sales.len(), 1);
        assert_eq!(sales[0].scope_type, DataScopeType::SelfOwned);
        let manager = definitions("role-sales-leader", "sales_order", &["list", "update"]);
        assert_eq!(manager[0].binding.target_mode, Some(ScopeTargetMode::ManagedOrgs));
        assert!(!manager[0].binding.actions.contains(&"update".into()));
    }

    #[test]
    fn procurement_products_grant_only_owned_objects_for_each_catalog_action() {
        let (_, actions) = RESOURCE_ACTIONS.iter().find(|(resource, _)| *resource == "product").unwrap();
        let rules = definitions("role-procurement", "product", actions);
        assert_eq!(rules.len(), 1);
        let rule = &rules[0];
        assert_eq!(rule.scope_type, DataScopeType::SelfOwned);
        assert_eq!(rule.binding.resource, "product");
        assert_eq!(rule.binding.actions, ["list", "detail", "create", "update"]);
        assert_eq!(rule.binding.target_dimension, ScopeDimension::InternalOrg);
        assert!(rule.scope_targets.is_empty());
        assert!(definitions("role-procurement", "contract", &["list"]).is_empty());
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

    #[test]
    fn funds_defaults_follow_sales_or_procurement_responsibility() {
        for resource in ["receivable_account", "customer_receipt", "invoice", "sales_invoice_request"] {
            let sales = definitions("role-sales", resource, &["list", "detail"]);
            assert_eq!(sales.len(), 1, "{resource} 销售默认本人负责");
            assert_eq!(sales[0].scope_type, DataScopeType::SelfOwned);
            assert!(definitions("role-procurement", resource, &["list", "detail"]).is_empty());
        }
        for resource in ["payable_account", "supplier_payment", "purchase_invoice_allocation"] {
            let procurement = definitions("role-procurement", resource, &["list", "detail"]);
            assert_eq!(procurement.len(), 1, "{resource} 采购默认本人");
            assert_eq!(procurement[0].scope_type, DataScopeType::SelfOwned);
            assert!(definitions("role-sales", resource, &["list", "detail"]).is_empty());
        }
        let finance = definitions("role-finance", "supplier_payment", &["list", "detail"]);
        assert!(finance.iter().all(|rule| rule.scope_type == DataScopeType::Company));
    }

    #[test]
    fn person_directories_do_not_copy_business_list_scopes() {
        let sales = definitions("role-sales", "sales_person", &["list"]);
        assert_eq!(sales.len(), 1);
        assert_eq!(sales[0].scope_type, DataScopeType::SelfOwned);
        assert!(definitions("role-sales", "procurement_person", &["list"]).is_empty());
        assert!(definitions("role-procurement", "sales_person", &["list"]).is_empty());
        let procurement = definitions("role-procurement", "procurement_person", &["list"]);
        assert_eq!(procurement[0].scope_type, DataScopeType::SelfOwned);
        let leader = definitions("role-sales-leader", "sales_person", &["list"]);
        assert_eq!(leader[0].scope_type, DataScopeType::Team);
        assert_eq!(leader[0].binding.target_mode, Some(ScopeTargetMode::ManagedOrgs));
        assert!(
            definitions("role-finance", "sales_person", &["list"])
                .iter()
                .all(|rule| rule.scope_type == DataScopeType::Company)
        );
        assert!(definitions("role-operations", "sales_person", &["list"]).is_empty());
        assert!(definitions("role-warehouse", "procurement_person", &["list"]).is_empty());
    }
}
