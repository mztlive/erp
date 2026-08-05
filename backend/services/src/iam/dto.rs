use entities::{Permission, Role};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 创建角色请求。
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateRoleParams {
    #[validate(length(min = 2, max = 32, message = "角色名称长度必须在2-32个字符之间"))]
    pub name: String,
    pub permissions: Vec<Permission>,
}

/// 更新角色请求。
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateRoleParams {
    #[validate(length(min = 2, max = 32, message = "角色名称长度必须在2-32个字符之间"))]
    pub name: Option<String>,
    pub permissions: Option<Vec<Permission>>,
}

/// 角色响应项。
#[derive(Debug, Serialize)]
pub struct RoleItem {
    pub id: String,
    pub name: String,
    pub permissions: Vec<Permission>,
    pub created_at: u64,
}

impl RoleItem {
    /// 从角色实体与直接权限策略构建响应项。
    ///
    /// # 返回值
    /// 返回不暴露内部持久化字段的角色响应项。
    pub(crate) fn from_role(role: Role, permissions: Vec<Permission>) -> Self {
        Self {
            id: role.base.id,
            name: role.name,
            permissions,
            created_at: role.base.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use entities::Permission;
    use validator::Validate;

    use super::{RoleItem, UpdateRoleParams};

    #[test]
    fn empty_update_is_valid_noop_payload() {
        let payload = UpdateRoleParams {
            name: None,
            permissions: None,
        };

        assert!(payload.validate().is_ok());
    }

    #[test]
    fn update_rejects_too_short_role_name() {
        let payload = UpdateRoleParams {
            name: Some("a".to_string()),
            permissions: None,
        };

        assert!(payload.validate().is_err());
    }

    #[test]
    fn role_item_keeps_existing_json_contract() {
        let item = RoleItem {
            id: "role-a".to_string(),
            name: "管理员".to_string(),
            permissions: vec![Permission::parse("admin:list").unwrap()],
            created_at: 42,
        };

        assert_eq!(
            serde_json::to_value(item).unwrap(),
            serde_json::json!({
                "id": "role-a",
                "name": "管理员",
                "permissions": ["admin:list"],
                "created_at": 42,
            })
        );
    }
}
