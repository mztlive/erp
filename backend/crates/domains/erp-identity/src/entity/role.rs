use std::collections::BTreeSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::role_workbench::RoleWorkbench;
use crate::entity::rbac::{PermissionSet, RoleId};

const NAME_MAX_LEN: usize = 32;
const DESCRIPTION_MAX_LEN: usize = 256;

/// 不可通过普通角色管理接口分配的内建超级管理员角色。
pub const ROOT_ROLE_ID: &str = "role-root";

/// 新建角色时提交的展示信息、系统标记和工作台映射。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoleData {
    /// 角色展示名称，创建时去除首尾空白并校验长度。
    pub name: String,
    /// 用于说明角色职责的可选描述。
    pub description: Option<String>,
    /// 是否受系统角色保护，不能通过普通接口修改、删除或分配。
    pub system: bool,
    /// 显式关联的岗位工作台；映射本身不授予权限。
    #[serde(default)]
    pub workbenches: BTreeSet<RoleWorkbench>,
}

impl RoleData {
    /// 用展示名称准备普通角色的创建数据，描述和工作台映射默认留空。
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), description: None, system: false, workbenches: BTreeSet::new() }
    }

    /// 在创建角色前补充职责描述，文本校验留给实体构造。
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 在构造创建数据时显式指定系统角色标记。
    pub fn with_system(mut self, system: bool) -> Self {
        self.system = system;
        self
    }
}

/// 角色局部更新；未提供的字段保留现值。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RoleUpdate {
    /// 替换展示名称；提供的名称不得为空。
    pub name: Option<String>,
    /// 替换职责描述；提供空白文本可清除描述。
    pub description: Option<String>,
    /// 设置停用状态；未提供时不变更状态。
    pub disabled: Option<bool>,
    /// 整体替换工作台映射；空集合清除现有映射。
    pub workbenches: Option<BTreeSet<RoleWorkbench>>,
}

/// 持久化的 RBAC 角色，保存角色身份、展示信息及普通管理限制，不保存直接权限。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct Role {
    /// 角色 ID、时间戳及软删除等持久化元数据。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 用于管理界面展示的规范化名称。
    pub name: String,
    /// 规范化后的可选职责说明。
    #[serde(default)]
    pub description: Option<String>,
    /// 是否按内建角色限制普通管理操作。
    #[serde(default)]
    pub system: bool,
    /// 配置的岗位工作台；使用时仍须检查有效权限。
    #[serde(default)]
    pub workbenches: BTreeSet<RoleWorkbench>,
    /// 是否停止参与角色分配和有效权限计算。
    #[serde(default)]
    pub disabled: bool,
}

impl Role {
    /// 用已校验的 `RoleId` 创建启用角色，规范化展示名称和描述；非法文本返回错误。
    pub fn new(id: RoleId, data: RoleData) -> Result<Self> {
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            name: normalize_required_text(data.name, "角色名称不能为空", NAME_MAX_LEN, "角色名称过长")?,
            description: normalize_optional_text(data.description, "角色描述", DESCRIPTION_MAX_LEN)?,
            system: data.system,
            workbenches: data.workbenches,
            disabled: false,
        })
    }

    /// 按已提供字段更新展示信息、工作台及停用状态；文本不合规时返回错误。
    /// 描述校验失败时名称可能已在内存中更新，调用方不得保存此次结果。
    pub fn update(&mut self, update: RoleUpdate) -> Result<()> {
        if let Some(name) = update.name {
            self.name = normalize_required_text(name, "角色名称不能为空", NAME_MAX_LEN, "角色名称过长")?;
        }
        if let Some(description) = update.description {
            self.description = normalize_optional_text(Some(description), "角色描述", DESCRIPTION_MAX_LEN)?;
        }
        if let Some(workbenches) = update.workbenches {
            self.workbenches = workbenches;
        }
        if let Some(disabled) = update.disabled {
            self.disabled = disabled;
        }
        Ok(())
    }

    /// 普通删除前拒绝系统角色和保留的超级管理员角色 ID。
    pub fn ensure_deletable(&self) -> Result<()> {
        if self.system || self.base.id == ROOT_ROLE_ID {
            return Err(Error::from("系统角色不能删除"));
        }
        Ok(())
    }

    /// 普通修改前拒绝系统角色和保留的超级管理员角色 ID。
    pub fn ensure_mutable(&self) -> Result<()> {
        if self.system || self.base.id == ROOT_ROLE_ID {
            return Err(Error::from("系统角色不能修改"));
        }
        Ok(())
    }

    /// 普通分配前拒绝系统角色、保留的超级管理员角色 ID 和已停用角色。
    pub fn ensure_assignable(&self) -> Result<()> {
        if self.system || self.base.id == ROOT_ROLE_ID {
            return Err(Error::from("系统角色不能通过普通接口分配"));
        }
        if self.disabled {
            return Err(Error::from("已停用角色不能分配"));
        }
        Ok(())
    }

    /// 显式种子命令只补齐缺失的直接权限，已有权限保留；无需补齐时返回 `None`。
    /// 拒绝同 ID 异名、已删除或不能普通分配的角色。
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
    use super::{ROOT_ROLE_ID, Role, RoleData, RoleUpdate};
    use crate::RoleId;

    // 验证角色持久化规范化后的 ID，非法 ID 无法用于创建。
    #[test]
    fn role_identity_uses_the_normalized_value() {
        let role = Role::new(RoleId::parse(" role-a ").unwrap(), RoleData::new("角色")).unwrap();
        assert_eq!(role.base.id, "role-a");
        assert!(RoleId::parse("bad/id").is_err());
    }

    // 验证超级管理员角色即使未标记为系统角色也不可普通修改、分配或删除。
    #[test]
    fn reserved_root_identity_remains_protected_without_system_flag() {
        let role = Role::new(RoleId::parse(ROOT_ROLE_ID).unwrap(), RoleData::new("root")).unwrap();
        assert!(!role.system);
        assert!(role.ensure_assignable().is_err());
        assert!(role.ensure_mutable().is_err());
        assert!(role.ensure_deletable().is_err());
    }

    // 验证创建数据默认是无描述的普通角色，构造辅助方法可显式补充设置。
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

    // 验证创建角色时去除展示名称首尾空白。
    #[test]
    fn role_should_normalize_name() {
        let role = Role::new(RoleId::parse(" role-a ").unwrap(), RoleData::new(" 运营管理员 ")).unwrap();
        assert_eq!(role.name, "运营管理员");
    }

    // 验证仅含空白的名称不能创建角色。
    #[test]
    fn role_should_reject_empty_name() {
        let result = Role::new(RoleId::parse("role-a").unwrap(), RoleData::new(" "));
        assert!(result.is_err());
    }

    // 验证只提交停用标记即可停用角色。
    #[test]
    fn role_update_should_change_disabled_state() {
        let mut role = Role::new(RoleId::parse("role-a").unwrap(), RoleData::new("角色")).unwrap();
        role.update(RoleUpdate { disabled: Some(true), ..Default::default() }).unwrap();
        assert!(role.disabled);
    }

    // 验证非超级管理员的系统角色同样不得普通删除或修改。
    #[test]
    fn system_role_should_not_be_deletable() {
        let role =
            Role::new(RoleId::parse("role-system").unwrap(), RoleData::new("系统角色").with_system(true))
                .unwrap();

        assert!(role.ensure_deletable().is_err());
        assert!(role.ensure_mutable().is_err());
    }

    // 验证普通自定义角色可通过删除资格检查。
    #[test]
    fn custom_role_should_be_deletable() {
        let role = Role::new(RoleId::parse("role-custom").unwrap(), RoleData::new("自定义角色")).unwrap();
        assert!(role.ensure_deletable().is_ok());
    }

    // 对照启用、停用和系统角色，验证普通分配只接受启用的自定义角色。
    #[test]
    fn only_enabled_custom_role_should_be_assignable() {
        let custom = Role::new(RoleId::parse("role-custom").unwrap(), RoleData::new("自定义角色")).unwrap();
        let system =
            Role::new(RoleId::parse("role-system").unwrap(), RoleData::new("系统角色").with_system(true))
                .unwrap();
        let mut disabled = custom.clone();
        disabled.update(RoleUpdate { disabled: Some(true), ..Default::default() }).unwrap();

        assert!(custom.ensure_assignable().is_ok());
        assert!(system.ensure_assignable().is_err());
        assert!(disabled.ensure_assignable().is_err());
    }
}
