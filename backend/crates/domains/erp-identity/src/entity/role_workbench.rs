//! 岗位模板身份与入口最低权限；映射不授予任何业务权限。
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::rbac::Permission;
use super::role::Role;

/// 首批岗位模板的稳定身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RoleWorkbench {
    #[serde(rename = "WB-01")]
    Sales,
    #[serde(rename = "WB-03")]
    Procurement,
    #[serde(rename = "WB-04")]
    Cashier,
}
impl RoleWorkbench {
    /// 为显式生成的内建角色提供推荐映射。
    /// # 参数
    /// `role_id` 为内建角色稳定编号，禁止按展示名称推断。
    /// # 返回
    /// 首批岗位的推荐集合；其他角色为空。
    /// # 错误
    /// 无；调用方仅在新建角色时持久化，不覆盖现有配置。
    pub fn recommended(role_id: &str) -> BTreeSet<Self> {
        match role_id {
            "role-sales" => BTreeSet::from([Self::Sales]),
            "role-procurement" => BTreeSet::from([Self::Procurement]),
            "role-cashier" => BTreeSet::from([Self::Cashier]),
            _ => BTreeSet::new(),
        }
    }

    /// 核验岗位核心模块的入口条件。
    /// # 参数
    /// `permissions` 为当前账号的有效权限。
    /// # 返回
    /// 全部核心入口权限满足时返回 true。
    /// # 错误
    /// 无；不授予业务范围或任务执行资格。
    pub fn accessible(self, permissions: &[Permission]) -> bool {
        let required: &[&str] = match self {
            Self::Sales => &["sales_order:list", "sales_inquiry:list", "work_item:list"],
            Self::Procurement => &["purchase_order:list", "procurement_inquiry:list", "work_item:list"],
            Self::Cashier => &["customer_receipt:list", "supplier_payment:list", "work_item:list"],
        };
        required.iter().all(|code| {
            Permission::parse(code)
                .is_ok_and(|required| permissions.iter().any(|grant| grant.covers(&required)))
        })
    }
    /// 从启用角色的显式映射解析可用岗位。
    /// # 参数
    /// `roles` 为账号实际绑定的角色；`permissions` 为有效权限。
    /// # 返回
    /// 去重、稳定排序且满足核心权限的模板。
    /// # 错误
    /// 无；没有显式映射时返回空集合。
    pub fn available(roles: &[Role], permissions: &[Permission]) -> BTreeSet<Self> {
        roles
            .iter()
            .filter(|role| !role.disabled)
            .flat_map(|role| role.workbenches.iter().copied())
            .filter(|template| template.accessible(permissions))
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::super::role::RoleData;
    use super::*;
    #[test]
    fn mapping_and_permissions_are_both_required() {
        let all = vec![Permission::parse("*:*").unwrap()];
        let mut role = Role::new("sales".into(), RoleData::new("销售")).unwrap();
        assert!(RoleWorkbench::available(&[role.clone()], &all).is_empty());
        role.workbenches.insert(RoleWorkbench::Sales);
        assert_eq!(RoleWorkbench::available(&[role.clone()], &all), BTreeSet::from([RoleWorkbench::Sales]));
        assert!(RoleWorkbench::available(&[role.clone()], &[]).is_empty());
        role.disabled = true;
        assert!(RoleWorkbench::available(&[role], &all).is_empty());
    }
    #[test]
    fn legacy_roles_and_explicit_updates_preserve_permission_boundary() {
        let role = Role::new("role-sales".into(), RoleData::new("销售")).unwrap();
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
    #[test]
    fn unknown_templates_are_rejected_and_duplicates_collapsed() {
        assert!(serde_json::from_str::<RoleWorkbench>("\"WB-99\"").is_err());
        let values: BTreeSet<RoleWorkbench> = serde_json::from_str("[\"WB-01\",\"WB-01\"]").unwrap();
        assert_eq!(values.len(), 1);
    }
}
