use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::rbac::Permission;
use crate::entity::role::Role;
use crate::entity::role_workbench::RoleWorkbench;

/// 创建后台角色时提交名称、直接权限和工作台配置。
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateRoleParams {
    /// 角色显示名称，长度为 2 至 32 个字符。
    #[validate(length(min = 2, max = 32, message = "角色名称长度必须在2-32个字符之间"))]
    pub name: String,
    /// 创建时拟授予该角色的直接权限，仍须通过操作者授予上限校验。
    pub permissions: Vec<Permission>,
    /// 显式配置的工作台集合，未提供时为空；不授予业务权限。
    #[serde(default)]
    pub workbenches: BTreeSet<RoleWorkbench>,
}

/// 角色编辑时提交可选字段；空补丁不改配置，但仍可记录操作审计。
#[derive(Debug, Default, Serialize, Deserialize, Validate)]
pub struct UpdateRoleParams {
    /// 新显示名称，长度为 2 至 32 个字符；未提供则保留原值。
    #[validate(length(min = 2, max = 32, message = "角色名称长度必须在2-32个字符之间"))]
    pub name: Option<String>,
    /// 提供时替换完整直接权限集合，空列表清空，未提供则不改写。
    pub permissions: Option<Vec<Permission>>,
    /// 提供时替换工作台配置，空集合清空，未提供则保留原配置。
    pub workbenches: Option<BTreeSet<RoleWorkbench>>,
}

/// 供后台角色管理页面展示基础资料、直接权限与工作台配置。
#[derive(Debug, Serialize)]
pub struct RoleItem {
    /// 系统保护标记；普通管理入口不可修改或删除此角色。
    pub system: bool,
    /// 角色选择及管理请求使用的稳定角色 ID。
    pub id: String,
    /// 角色当前显示名称。
    pub name: String,
    /// 该角色的直接权限，不是某个账号的最终授权结果。
    pub permissions: Vec<Permission>,
    /// 角色显式配置的工作台，实际可用入口另行校验权限。
    #[serde(default)]
    pub workbenches: BTreeSet<RoleWorkbench>,
    /// 角色创建时间（Unix 秒）。
    pub created_at: u64,
}

impl RoleItem {
    /// 查询角色时合并实体与直接权限；内建系统标记或 `ROOT_ROLE_ID` 均显示为受保护角色。
    pub fn from_role(role: Role, permissions: Vec<Permission>) -> Self {
        Self {
            system: role.system || role.base.id == crate::ROOT_ROLE_ID,
            id: role.base.id,
            name: role.name,
            permissions,
            created_at: role.base.created_at,
            workbenches: role.workbenches,
        }
    }
}

#[cfg(test)]
mod tests {
    use validator::Validate;

    use super::{RoleItem, UpdateRoleParams};
    use crate::RoleId;
    use crate::entity::rbac::Permission;

    // 验证空角色补丁通过校验，不要求客户端提交变更字段。
    #[test]
    fn empty_update_is_valid_noop_payload() {
        let payload = UpdateRoleParams::default();
        assert!(payload.validate().is_ok());
    }

    // 验证角色补丁不能绕过名称长度下限。
    #[test]
    fn update_rejects_too_short_role_name() {
        let payload = UpdateRoleParams { name: Some("a".to_string()), ..Default::default() };
        assert!(payload.validate().is_err());
    }

    // 验证默认补丁不提供名称或权限，避免无意改写角色配置。
    #[test]
    fn update_role_params_default_is_empty() {
        let payload = UpdateRoleParams::default();
        assert!(payload.name.is_none());
        assert!(payload.permissions.is_none());
    }

    // 验证系统角色及保留的根角色均在响应中标记为受保护，普通角色不标记。
    #[test]
    fn role_item_exposes_builtin_protection() {
        use crate::entity::role::{Role, RoleData};
        for (id, system, protected) in
            [("role-custom", false, false), ("role-built-in", true, true), (crate::ROOT_ROLE_ID, false, true)]
        {
            let role =
                Role::new(RoleId::parse(id).unwrap(), RoleData::new("测试角色").with_system(system)).unwrap();
            assert_eq!(RoleItem::from_role(role, vec![]).system, protected);
        }
    }

    // 验证角色响应序列化保留字段名、权限码格式及空工作台集合。
    #[test]
    fn role_item_keeps_existing_json_contract() {
        let item = RoleItem {
            system: false,
            id: "role-a".to_string(),
            name: "管理员".to_string(),
            permissions: vec![Permission::parse("admin:list").unwrap()],
            created_at: 42,
            workbenches: Default::default(),
        };

        assert_eq!(
            serde_json::to_value(item).unwrap(),
            serde_json::json!({
                "system": false,
                "id": "role-a",
                "name": "管理员",
                "permissions": ["admin:list"],
                "created_at": 42,
                "workbenches": [],
            })
        );
    }
}
