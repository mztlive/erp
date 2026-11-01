use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use crate::entity::rbac::{PermissionSet, RoleId};

const NAME_MAX_LEN: usize = 32;
const DESCRIPTION_MAX_LEN: usize = 256;

/// 不可通过普通角色管理接口分配的内建超级管理员角色。
pub const ROOT_ROLE_ID: &str = "role-root";

/// 角色创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleData {
    pub name: String,
    pub description: Option<String>,
    pub system: bool,
}

impl RoleData {
    /// 由必填名称构造角色创建数据。
    ///
    /// # 参数
    /// * `name` - 角色展示名称
    ///
    /// # 返回
    /// 返回描述为空、非系统角色的创建数据。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), description: None, system: false }
    }

    /// 设置角色描述。
    ///
    /// # 参数
    /// * `description` - 角色描述
    ///
    /// # 返回
    /// 返回更新后的创建数据。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 设置是否为系统角色。
    ///
    /// # 参数
    /// * `system` - 系统角色标记
    ///
    /// # 返回
    /// 返回更新后的创建数据。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_system(mut self, system: bool) -> Self {
        self.system = system;
        self
    }
}

/// 角色更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RoleUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub disabled: Option<bool>,
}

/// RBAC 角色实体。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct Role {
    #[serde(flatten)]
    pub base: BaseModel,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub system: bool,
    #[serde(default)]
    pub disabled: bool,
}

impl Role {
    /// 创建角色并校验角色 ID 与展示字段。
    ///
    /// # 参数
    /// * `id` - 角色 ID，须通过 [`RoleId::parse`]。
    /// * `data` - 名称、描述和是否系统角色。
    ///
    /// # 返回
    /// 返回未停用的新角色。
    ///
    /// # 错误
    /// 当角色 ID、名称或描述非法时返回错误。
    pub fn new(id: String, data: RoleData) -> Result<Self> {
        RoleId::parse(&id)?;
        Ok(Self {
            base: BaseModel::new(id),
            name: normalize_required_text(data.name, "角色名称不能为空", NAME_MAX_LEN, "角色名称过长")?,
            description: normalize_optional_text(data.description, "角色描述", DESCRIPTION_MAX_LEN)?,
            system: data.system,
            disabled: false,
        })
    }

    /// 更新角色展示信息与启用状态。
    ///
    /// # 参数
    /// * `update` - 可选的名称、描述和停用标记；缺省字段保持不变。
    ///
    /// # 返回
    /// 校验通过后就地更新。
    ///
    /// # 错误
    /// 当名称或描述非法时返回错误。停用标记本身不产生错误。
    pub fn update(&mut self, update: RoleUpdate) -> Result<()> {
        if let Some(name) = update.name {
            self.name = normalize_required_text(name, "角色名称不能为空", NAME_MAX_LEN, "角色名称过长")?;
        }
        if let Some(description) = update.description {
            self.description = normalize_optional_text(Some(description), "角色描述", DESCRIPTION_MAX_LEN)?;
        }
        if let Some(disabled) = update.disabled {
            self.disabled = disabled;
        }
        Ok(())
    }

    /// 校验当前角色是否允许删除。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 非系统角色时无返回值。
    ///
    /// # 错误
    /// 系统角色属于内建安全边界，禁止删除时返回业务错误。
    pub fn ensure_deletable(&self) -> Result<()> {
        if self.system {
            return Err(Error::from("系统角色不能删除"));
        }
        Ok(())
    }

    /// 校验当前角色是否允许通过普通管理接口修改。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 非系统角色时无返回值。
    ///
    /// # 错误
    /// 系统角色属于内建安全边界，禁止修改时返回业务错误。
    pub fn ensure_mutable(&self) -> Result<()> {
        if self.system {
            return Err(Error::from("系统角色不能修改"));
        }
        Ok(())
    }

    /// 校验当前角色是否允许通过普通管理接口分配。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 非系统且未停用时无返回值。
    ///
    /// # 错误
    /// 系统角色或已停用角色不能分配时返回业务错误。
    pub fn ensure_assignable(&self) -> Result<()> {
        if self.system {
            return Err(Error::from("系统角色不能通过普通接口分配"));
        }
        if self.disabled {
            return Err(Error::from("已停用角色不能分配"));
        }
        Ok(())
    }

    /// 为显式种子命令合并缺失权限，拒绝认领失效、系统或同 ID 异名角色。
    /// # 参数
    /// `name` 为种子身份名称；`current` 为当前直接权限；`desired` 为声明权限。
    /// # 返回
    /// 需要补齐时返回保留现有权限的并集，否则返回 None。
    /// # 错误
    /// 身份不符、角色被删除或不能分配时拒绝。
    pub fn seeded_permissions(
        &self,
        name: &str,
        current: &PermissionSet,
        desired: &PermissionSet,
    ) -> Result<Option<PermissionSet>> {
        if self.base.id == ROOT_ROLE_ID || self.base.is_deleted() || self.name != name {
            return Err(Error::from("种子角色身份不符或已删除，不能自动修改"));
        }
        self.ensure_assignable()?;
        Ok(current.with_missing(desired))
    }
}

#[cfg(test)]
mod tests {
    use super::{Role, RoleData, RoleUpdate};

    #[test]
    fn constructor_builds_non_system_data_by_default() {
        let data = RoleData::new("测试角色");
        assert_eq!(data.name, "测试角色");
        assert!(data.description.is_none());
        assert!(!data.system);
        let system = RoleData::new("系统角色").with_system(true).with_description("内建");
        assert!(system.system);
        assert_eq!(system.description.as_deref(), Some("内建"));
    }

    #[test]
    fn role_should_normalize_name() {
        let role = Role::new("role-a".to_string(), RoleData::new(" 运营管理员 ")).unwrap();
        assert_eq!(role.name, "运营管理员");
    }

    #[test]
    fn role_should_reject_empty_name() {
        let result = Role::new("role-a".to_string(), RoleData::new(" "));
        assert!(result.is_err());
    }

    #[test]
    fn role_update_should_change_disabled_state() {
        let mut role = Role::new("role-a".to_string(), RoleData::new("角色")).unwrap();
        role.update(RoleUpdate { disabled: Some(true), ..Default::default() }).unwrap();
        assert!(role.disabled);
    }

    #[test]
    fn system_role_should_not_be_deletable() {
        let role = Role::new("role-system".to_string(), RoleData::new("系统角色").with_system(true)).unwrap();

        assert!(role.ensure_deletable().is_err());
        assert!(role.ensure_mutable().is_err());
    }

    #[test]
    fn custom_role_should_be_deletable() {
        let role = Role::new("role-custom".to_string(), RoleData::new("自定义角色")).unwrap();

        assert!(role.ensure_deletable().is_ok());
    }

    #[test]
    fn only_enabled_custom_role_should_be_assignable() {
        let custom = Role::new("role-custom".to_string(), RoleData::new("自定义角色")).unwrap();
        let system =
            Role::new("role-system".to_string(), RoleData::new("系统角色").with_system(true)).unwrap();
        let mut disabled = custom.clone();
        disabled.update(RoleUpdate { disabled: Some(true), ..Default::default() }).unwrap();

        assert!(custom.ensure_assignable().is_ok());
        assert!(system.ensure_assignable().is_err());
        assert!(disabled.ensure_assignable().is_err());
    }
}
