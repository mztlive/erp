use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::{
    errors::{Error, Result},
    validation::{normalize_optional_text, normalize_required_text},
    RoleId,
};

const NAME_MAX_LEN: usize = 32;
const DESCRIPTION_MAX_LEN: usize = 256;

/// 角色创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleData {
    pub name: String,
    pub description: Option<String>,
    pub system: bool,
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
    /// # 错误
    /// 当名称或描述非法时返回错误。
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
}

#[cfg(test)]
mod tests {
    use super::{Role, RoleData, RoleUpdate};

    #[test]
    fn role_should_normalize_name() {
        let role = Role::new(
            "role-a".to_string(),
            RoleData {
                name: " 运营管理员 ".to_string(),
                description: None,
                system: false,
            },
        )
        .unwrap();
        assert_eq!(role.name, "运营管理员");
    }

    #[test]
    fn role_should_reject_empty_name() {
        let result = Role::new(
            "role-a".to_string(),
            RoleData {
                name: " ".to_string(),
                description: None,
                system: false,
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn role_update_should_change_disabled_state() {
        let mut role = Role::new(
            "role-a".to_string(),
            RoleData {
                name: "角色".to_string(),
                description: None,
                system: false,
            },
        )
        .unwrap();
        role.update(RoleUpdate {
            disabled: Some(true),
            ..Default::default()
        })
        .unwrap();
        assert!(role.disabled);
    }

    #[test]
    fn system_role_should_not_be_deletable() {
        let role = Role::new(
            "role-system".to_string(),
            RoleData {
                name: "系统角色".to_string(),
                description: None,
                system: true,
            },
        )
        .unwrap();

        assert!(role.ensure_deletable().is_err());
        assert!(role.ensure_mutable().is_err());
    }

    #[test]
    fn custom_role_should_be_deletable() {
        let role = Role::new(
            "role-custom".to_string(),
            RoleData {
                name: "自定义角色".to_string(),
                description: None,
                system: false,
            },
        )
        .unwrap();

        assert!(role.ensure_deletable().is_ok());
    }

    #[test]
    fn only_enabled_custom_role_should_be_assignable() {
        let custom = Role::new(
            "role-custom".to_string(),
            RoleData {
                name: "自定义角色".to_string(),
                description: None,
                system: false,
            },
        )
        .unwrap();
        let system = Role::new(
            "role-system".to_string(),
            RoleData {
                name: "系统角色".to_string(),
                description: None,
                system: true,
            },
        )
        .unwrap();
        let mut disabled = custom.clone();
        disabled
            .update(RoleUpdate {
                disabled: Some(true),
                ..Default::default()
            })
            .unwrap();

        assert!(custom.ensure_assignable().is_ok());
        assert!(system.ensure_assignable().is_err());
        assert!(disabled.ensure_assignable().is_err());
    }
}
