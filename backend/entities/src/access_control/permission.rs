//! `permission`：配置化权限定义目录（数据模型 §5.1 / §4.6）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::Result;
use crate::ids::PermissionId;
use crate::rbac;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 权限名称最大长度。
const NAME_MAX_LEN: usize = 64;
/// 权限描述最大长度。
const DESCRIPTION_MAX_LEN: usize = 256;

/// 权限定义创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionData {
    /// 权限资源（如 `sales_order`）。
    pub resource: String,
    /// 权限动作（如 `approve`）。
    pub action: String,
    /// 展示名称。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 是否为系统内建权限（禁止删除/修改）。
    pub system: bool,
}

/// 权限定义更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PermissionUpdate {
    /// 展示名称；`None` 表示不修改。
    pub name: Option<String>,
    /// 描述；`None` 表示不修改。
    pub description: Option<String>,
    /// 停用标记；`None` 表示不修改。
    pub disabled: Option<bool>,
}

/// 配置化权限定义实体（数据模型 §5.1）。
///
/// 角色、用户、团队、权限和数据范围配置化，不把角色枚举硬编码到业务逻辑
/// （erp-phase-1 §11.1）。`resource:action` 组合复用既有
/// `entities::rbac::Permission` 的解析与规范化（小写、白名单字符）；
/// 角色 ↔ 权限的授权绑定仍由既有 Casbin 规则承载，本实体只维护定义目录，
/// 不重建绑定能力。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct Permission {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 权限资源。
    pub resource: String,
    /// 权限动作。
    pub action: String,
    /// 展示名称。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 系统内建权限标记。
    pub system: bool,
    /// 停用标记。
    pub disabled: bool,
}

impl Permission {
    /// 创建权限定义。
    ///
    /// 通过 `entities::rbac::Permission::parse` 校验并规范化 `resource:action`
    /// （trim、小写、白名单字符、长度上限），完成 name/description 的规范化。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PermissionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的权限定义（未停用）。
    ///
    /// # 错误
    /// 当 `resource:action` 非法或 name/description 为空/超长时返回错误。
    pub fn new(id: PermissionId, data: PermissionData) -> Result<Self> {
        let parsed = rbac::Permission::parse(format!("{}:{}", data.resource, data.action))?;
        let name = normalize_required_text(data.name, "权限名称不能为空", NAME_MAX_LEN, "权限名称过长")?;
        let description = normalize_optional_text(data.description, "权限描述", DESCRIPTION_MAX_LEN)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            resource: parsed.resource().to_string(),
            action: parsed.action().to_string(),
            name,
            description,
            system: data.system,
            disabled: false,
        })
    }

    /// 更新权限定义。
    ///
    /// 复用 `new` 的校验规则；`resource:action` 与 `system` 是权限身份与
    /// 安全边界，不允许在通用更新中修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当名称或描述校验失败时返回错误。
    pub fn update(&mut self, update: PermissionUpdate) -> Result<()> {
        if let Some(name) = update.name {
            self.name = normalize_required_text(name, "权限名称不能为空", NAME_MAX_LEN, "权限名称过长")?;
        }
        if let Some(description) = update.description {
            self.description = normalize_optional_text(Some(description), "权限描述", DESCRIPTION_MAX_LEN)?;
        }
        if let Some(disabled) = update.disabled {
            self.disabled = disabled;
        }
        Ok(())
    }

    /// 校验权限定义是否允许删除。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 系统内建权限禁止删除时返回业务错误。
    pub fn ensure_deletable(&self) -> Result<()> {
        if self.system {
            return Err(crate::errors::Error::from("系统权限不能删除"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Permission, PermissionData, PermissionUpdate};
    use crate::ids::PermissionId;

    fn data() -> PermissionData {
        PermissionData {
            resource: " SalesOrder".to_string(),
            action: "APPROVE".to_string(),
            name: " 销售单审批 ".to_string(),
            description: Some(" 审批销售单 ".to_string()),
            system: false,
        }
    }

    /// happy path：resource:action 小写规范化，name/description trim。
    #[test]
    fn new_normalizes_resource_action_and_text() {
        let permission = Permission::new(PermissionId::new("perm-1"), data()).unwrap();
        assert_eq!(permission.resource, "salesorder");
        assert_eq!(permission.action, "approve");
        assert_eq!(permission.name, "销售单审批");
        assert_eq!(permission.description.as_deref(), Some("审批销售单"));
        assert!(!permission.disabled);
    }

    /// 失败路径：resource:action 非法（缺冒号）被拒。
    #[test]
    fn new_rejects_malformed_resource_action() {
        let payload = PermissionData {
            action: "approve:extra".to_string(),
            ..data()
        };
        assert!(Permission::new(PermissionId::new("perm-1"), payload).is_err());
    }

    /// 失败路径：名称为空与超长被拒。
    #[test]
    fn new_rejects_blank_and_overlong_name() {
        let blank = PermissionData {
            name: "  ".to_string(),
            ..data()
        };
        assert!(Permission::new(PermissionId::new("perm-1"), blank).is_err());

        let overlong = PermissionData {
            name: "名".repeat(65),
            ..data()
        };
        assert!(Permission::new(PermissionId::new("perm-2"), overlong).is_err());
    }

    /// 更新：复用校验且不改关键字段。
    #[test]
    fn update_applies_name_and_disabled_only() {
        let mut permission = Permission::new(PermissionId::new("perm-1"), data()).unwrap();
        permission
            .update(PermissionUpdate {
                name: Some(" 新名称 ".to_string()),
                description: None,
                disabled: Some(true),
            })
            .unwrap();
        assert_eq!(permission.name, "新名称");
        assert!(permission.disabled);
        assert_eq!(permission.resource, "salesorder");
        assert_eq!(permission.action, "approve");

        assert!(permission
            .update(PermissionUpdate {
                name: Some("  ".to_string()),
                ..Default::default()
            })
            .is_err());
    }

    /// 系统权限删除保护。
    #[test]
    fn system_permission_is_not_deletable() {
        let system = Permission::new(
            PermissionId::new("perm-2"),
            PermissionData {
                system: true,
                ..data()
            },
        )
        .unwrap();
        assert!(system.ensure_deletable().is_err());
        let custom = Permission::new(PermissionId::new("perm-3"), data()).unwrap();
        assert!(custom.ensure_deletable().is_ok());
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let permission = Permission::new(PermissionId::new("perm-1"), data()).unwrap();
        let roundtrip: Permission = bson::from_document(bson::to_document(&permission).unwrap()).unwrap();
        assert_eq!(roundtrip, permission);
    }
}
