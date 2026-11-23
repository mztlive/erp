//! 岗位模板身份与入口最低权限；映射不授予任何业务权限。
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::rbac::Permission;
use super::role::Role;

/// 岗位模板的稳定身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RoleWorkbench {
    #[serde(rename = "WB-01")]
    Sales,
    #[serde(rename = "WB-02")]
    Fulfillment,
    #[serde(rename = "WB-03")]
    Procurement,
    #[serde(rename = "WB-04")]
    Cashier,
    #[serde(rename = "WB-05")]
    Management,
}
impl RoleWorkbench {
    /// 按内建角色稳定 ID 返回推荐工作台，其他角色为空；不使用展示名称推断。
    /// 仅在显式生成新角色时持久化，不覆盖现有配置。
    pub fn recommended(role_id: &str) -> BTreeSet<Self> {
        match role_id {
            "role-sales" => BTreeSet::from([Self::Sales]),
            "role-warehouse" => BTreeSet::from([Self::Fulfillment]),
            "role-procurement" => BTreeSet::from([Self::Procurement]),
            "role-cashier" => BTreeSet::from([Self::Cashier]),
            "role-management" => BTreeSet::from([Self::Management]),
            _ => BTreeSet::new(),
        }
    }

    /// 检查账号有效权限是否覆盖该工作台的全部核心入口；不授予数据范围或任务执行资格。
    pub fn accessible(self, permissions: &[Permission]) -> bool {
        match self {
            Self::Sales => ["sales_order:list", "sales_inquiry:list", "work_item:list"]
                .into_iter()
                .all(|code| covers_code(permissions, code)),
            Self::Fulfillment => {
                covers_code(permissions, "work_item:list")
                    && [
                        "purchase_receipt:list",
                        "delivery:list",
                        "electronic_delivery:list",
                        "service_fulfillment:list",
                    ]
                    .into_iter()
                    .any(|code| covers_code(permissions, code))
            },
            Self::Procurement => ["purchase_order:list", "procurement_inquiry:list", "work_item:list"]
                .into_iter()
                .all(|code| covers_code(permissions, code)),
            Self::Cashier => ["customer_receipt:list", "supplier_payment:list", "work_item:list"]
                .into_iter()
                .all(|code| covers_code(permissions, code)),
            Self::Management => ["cost_entry:list", "sales_order:list", "work_item:list"]
                .into_iter()
                .all(|code| covers_code(permissions, code)),
        }
    }
    /// 汇总启用绑定角色显式配置且满足核心入口权限的工作台，返回去重并稳定排序的集合。
    pub fn available(roles: &[Role], permissions: &[Permission]) -> BTreeSet<Self> {
        roles
            .iter()
            .filter(|role| !role.disabled)
            .flat_map(|role| role.workbenches.iter().copied())
            .filter(|template| template.accessible(permissions))
            .collect()
    }
}

fn covers_code(permissions: &[Permission], code: &str) -> bool {
    Permission::parse(code).is_ok_and(|required| permissions.iter().any(|grant| grant.covers(&required)))
}
#[cfg(test)]
mod tests {
    use super::super::role::RoleData;
    use super::*;
    use crate::RoleId;
    // 验证没有有效角色映射或缺少核心权限时都不展示工作台。
    #[test]
    fn mapping_and_permissions_are_both_required() {
        let all = vec![Permission::parse("*:*").unwrap()];
        let mut role = Role::new(RoleId::parse("sales").unwrap(), RoleData::new("销售")).unwrap();
        assert!(RoleWorkbench::available(&[role.clone()], &all).is_empty());

        role.workbenches.insert(RoleWorkbench::Sales);
        assert_eq!(RoleWorkbench::available(&[role.clone()], &all), BTreeSet::from([RoleWorkbench::Sales]));
        assert!(RoleWorkbench::available(&[role.clone()], &[]).is_empty());

        role.disabled = true;
        assert!(RoleWorkbench::available(&[role], &all).is_empty());
    }
    // 验证旧角色不自动绑定工作台，显式配置后仍校验核心权限，并支持去重和清空映射。
    #[test]
    fn legacy_roles_and_explicit_updates_preserve_permission_boundary() {
        let role = Role::new(RoleId::parse("role-sales").unwrap(), RoleData::new("销售")).unwrap();
        let mut json = serde_json::to_value(role).unwrap();
        json.as_object_mut().unwrap().remove("workbenches");
        let mut legacy: Role = serde_json::from_value(json).unwrap();
        assert!(legacy.workbenches.is_empty());

        legacy
            .update(super::super::role::RoleUpdate {
                workbenches: Some(RoleWorkbench::recommended("role-sales")),
                ..Default::default()
            })
            .unwrap();
        let partial = [Permission::parse("sales_order:list").unwrap()];
        assert!(RoleWorkbench::available(&[legacy.clone()], &partial).is_empty());
        let all = [Permission::parse("*:*").unwrap()];
        assert_eq!(RoleWorkbench::available(&[legacy.clone(), legacy.clone()], &all).len(), 1);

        legacy
            .update(super::super::role::RoleUpdate {
                workbenches: Some(BTreeSet::new()),
                ..Default::default()
            })
            .unwrap();
        assert!(RoleWorkbench::available(&[legacy], &all).is_empty());

        assert!(RoleWorkbench::recommended("role-root").is_empty());
        assert!(RoleWorkbench::recommended("role-sysadmin").is_empty());
    }
    // 验证反序列化拒绝未登记的工作台编码，相同编码只保留一项。
    #[test]
    fn unknown_templates_are_rejected_and_duplicates_collapsed() {
        assert!(serde_json::from_str::<RoleWorkbench>("\"WB-99\"").is_err());
        let values: BTreeSet<RoleWorkbench> = serde_json::from_str("[\"WB-01\",\"WB-01\"]").unwrap();
        assert_eq!(values.len(), 1);
    }

    // 验证仓储履约（WB-02）与经营管理（WB-05）工作台的编码顺序、内建推荐与入口权限边界。
    #[test]
    fn fulfillment_and_management_workbenches_enforce_recommendations_and_entry_permissions() {
        let parsed: BTreeSet<RoleWorkbench> =
            serde_json::from_str("[\"WB-05\",\"WB-02\",\"WB-01\",\"WB-04\",\"WB-03\"]").unwrap();
        let ordered: Vec<_> = parsed.into_iter().collect();
        assert_eq!(
            ordered,
            vec![
                RoleWorkbench::Sales,
                RoleWorkbench::Fulfillment,
                RoleWorkbench::Procurement,
                RoleWorkbench::Cashier,
                RoleWorkbench::Management,
            ]
        );
        assert_eq!(serde_json::to_string(&RoleWorkbench::Fulfillment).unwrap(), "\"WB-02\"");
        assert_eq!(serde_json::to_string(&RoleWorkbench::Management).unwrap(), "\"WB-05\"");

        assert_eq!(
            RoleWorkbench::recommended("role-warehouse"),
            BTreeSet::from([RoleWorkbench::Fulfillment])
        );
        assert_eq!(
            RoleWorkbench::recommended("role-management"),
            BTreeSet::from([RoleWorkbench::Management])
        );

        let perms = |codes: &[&str]| codes.iter().map(|c| Permission::parse(c).unwrap()).collect::<Vec<_>>();
        assert!(!RoleWorkbench::Fulfillment.accessible(&perms(&["work_item:list"])));
        assert!(!RoleWorkbench::Fulfillment.accessible(&perms(&["delivery:list"])));
        for fulfillment_perm in
            ["purchase_receipt:list", "delivery:list", "electronic_delivery:list", "service_fulfillment:list"]
        {
            assert!(RoleWorkbench::Fulfillment.accessible(&perms(&["work_item:list", fulfillment_perm])));
        }
        assert!(RoleWorkbench::Fulfillment.accessible(&perms(&["work_item:list", "delivery:*"])));

        assert!(!RoleWorkbench::Management.accessible(&perms(&["cost_entry:list", "sales_order:list"])));
        assert!(!RoleWorkbench::Management.accessible(&perms(&["cost_entry:list", "work_item:list"])));
        assert!(!RoleWorkbench::Management.accessible(&perms(&["sales_order:list", "work_item:list"])));
        assert!(RoleWorkbench::Management.accessible(&perms(&[
            "cost_entry:list",
            "sales_order:list",
            "work_item:list",
        ])));

        let templates = crate::service::iam::builtin_role_templates().unwrap();
        for (role_id, expected) in [
            ("role-sales", RoleWorkbench::Sales),
            ("role-warehouse", RoleWorkbench::Fulfillment),
            ("role-procurement", RoleWorkbench::Procurement),
            ("role-cashier", RoleWorkbench::Cashier),
            ("role-management", RoleWorkbench::Management),
        ] {
            let template = templates.iter().find(|t| t.id == role_id).unwrap();
            assert_eq!(RoleWorkbench::recommended(role_id), BTreeSet::from([expected]));
            assert!(expected.accessible(&template.permissions), "{role_id} must satisfy {expected:?}");
        }
    }
}
