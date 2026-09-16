use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::rbac::Permission;
use crate::entity::role::Role;

/// 创建角色请求。
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateRoleParams {
    #[validate(length(min = 2, max = 32, message = "角色名称长度必须在2-32个字符之间"))]
    pub name: String,
    pub permissions: Vec<Permission>,
}

/// 更新角色请求。
#[derive(Debug, Default, Serialize, Deserialize, Validate)]
pub struct UpdateRoleParams {
    #[validate(length(min = 2, max = 32, message = "角色名称长度必须在2-32个字符之间"))]
    pub name: Option<String>,
    pub permissions: Option<Vec<Permission>>,
}

/// 角色响应项。
#[derive(Debug, Serialize)]
pub struct RoleItem {
    /// 系统角色不可由普通管理入口修改或删除。
    pub system: bool,
    pub id: String,
    pub name: String,
    pub permissions: Vec<Permission>,
    pub created_at: u64,
}

impl RoleItem {
    /// 由必填角色 ID 与名称构造响应项。
    ///
    /// # 参数
    /// * `id` - 角色 ID
    /// * `name` - 角色名称
    ///
    /// # 返回
    /// 返回非系统、无权限、零时刻的响应项。
    ///
    /// # 错误
    /// 无。
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { system: false, id: id.into(), name: name.into(), permissions: Vec::new(), created_at: 0 }
    }

    /// 设置系统角色标记。
    ///
    /// # 参数
    /// * `system` - 系统角色标记
    ///
    /// # 返回
    /// 返回更新后的响应项。
    ///
    /// # 错误
    /// 无。
    pub fn with_system(mut self, system: bool) -> Self {
        self.system = system;
        self
    }

    /// 设置直接权限策略。
    ///
    /// # 参数
    /// * `permissions` - 直接权限策略
    ///
    /// # 返回
    /// 返回更新后的响应项。
    ///
    /// # 错误
    /// 无。
    pub fn with_permissions(mut self, permissions: Vec<Permission>) -> Self {
        self.permissions = permissions;
        self
    }

    /// 设置创建时间。
    ///
    /// # 参数
    /// * `created_at` - 创建时间（秒级时间戳）
    ///
    /// # 返回
    /// 返回更新后的响应项。
    ///
    /// # 错误
    /// 无。
    pub fn with_created_at(mut self, created_at: u64) -> Self {
        self.created_at = created_at;
        self
    }

    /// 从角色实体与直接权限策略构建响应项。
    ///
    /// # 返回值
    /// 返回不暴露内部持久化字段的角色响应项。
    pub fn from_role(role: Role, permissions: Vec<Permission>) -> Self {
        Self {
            system: role.system || role.base.id == crate::ROOT_ROLE_ID,
            id: role.base.id,
            name: role.name,
            permissions,
            created_at: role.base.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use validator::Validate;

    use super::{RoleItem, UpdateRoleParams};
    use crate::entity::rbac::Permission;

    #[test]
    fn empty_update_is_valid_noop_payload() {
        let payload = UpdateRoleParams::default();

        assert!(payload.validate().is_ok());
    }

    #[test]
    fn update_rejects_too_short_role_name() {
        let payload = UpdateRoleParams { name: Some("a".to_string()), ..Default::default() };

        assert!(payload.validate().is_err());
    }

    #[test]
    fn update_role_params_default_is_empty() {
        let payload = UpdateRoleParams::default();
        assert!(payload.name.is_none());
        assert!(payload.permissions.is_none());
    }

    #[test]
    fn role_item_constructor_sets_identity_only() {
        let item = RoleItem::new("role-a", "管理员");
        assert_eq!(item.id, "role-a");
        assert_eq!(item.name, "管理员");
        assert!(!item.system);
        assert!(item.permissions.is_empty());
        assert_eq!(item.created_at, 0);
        let system = RoleItem::new("role-a", "管理员").with_system(true).with_created_at(42);
        assert!(system.system);
        assert_eq!(system.created_at, 42);
    }

    /// 系统角色及保留根角色身份均向客户端声明删除保护。
    #[test]
    fn role_item_exposes_builtin_protection() {
        use crate::entity::role::{Role, RoleData};
        for (id, system, protected) in
            [("role-custom", false, false), ("role-built-in", true, true), (crate::ROOT_ROLE_ID, false, true)]
        {
            let role = Role::new(id.to_string(), RoleData::new("测试角色").with_system(system)).unwrap();
            assert_eq!(RoleItem::from_role(role, vec![]).system, protected);
        }
    }

    #[test]
    fn role_item_keeps_existing_json_contract() {
        let item = RoleItem {
            system: false,
            id: "role-a".to_string(),
            name: "管理员".to_string(),
            permissions: vec![Permission::parse("admin:list").unwrap()],
            created_at: 42,
        };

        assert_eq!(
            serde_json::to_value(item).unwrap(),
            serde_json::json!({
                "system": false,
                "id": "role-a",
                "name": "管理员",
                "permissions": ["admin:list"],
                "created_at": 42,
            })
        );
    }
}
