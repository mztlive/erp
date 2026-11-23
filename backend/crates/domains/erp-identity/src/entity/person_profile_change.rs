//! 人员资料的原子变更：限定单人，按差异更新组织关系。
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{Error, Permission, Result, RoleId, RoleIdSet};

/// 同角色维护组织关系与人员账号资料，沿组织管理范围检查。
pub(crate) const ORGANIZATION_PERSON_PROFILE_CHANGE: Permission =
    Permission::new("org_unit", "manage_person_profile");

/// 对单个账号的姓名、完整角色集和主属组织按差异执行原子修改。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PersonProfileChange {
    /// 要修改人员资料的账号 ID。
    pub user_id: String,
    /// 编辑时读到的原姓名，服务在修改姓名前检查是否仍一致。
    pub expected_name: String,
    /// 拟保存的新姓名，`None` 表示保持原姓名。
    pub name: Option<String>,
    /// 编辑时的完整角色集，替换角色时必填，用于识别并发变更。
    pub expected_role_ids: Option<RoleIdSet>,
    /// 要替换成的完整非空角色集，`None` 表示保留现有角色。
    pub role_ids: Option<RoleIdSet>,
    /// 要调入的主属组织 ID，`None` 表示保持原组织关系。
    pub org_unit_id: Option<String>,
}

impl PersonProfileChange {
    /// 替换角色时要求新集合非空，并将期望集合与当前授权快照 `current` 按集合比较，忽略顺序。
    /// 缺少期望集合或集合已变化时拒绝；未提供新角色集时直接通过。
    pub fn validate_roles(&self, current: &[RoleId]) -> Result<()> {
        let Some(roles) = &self.role_ids else {
            return Ok(());
        };
        roles.ensure_non_empty()?;
        let expected = self
            .expected_role_ids
            .as_ref()
            .ok_or_else(|| Error::ValidationError("修改角色必须提供原角色集合".into()))?;
        if expected.iter().collect::<BTreeSet<_>>() != current.iter().collect::<BTreeSet<_>>() {
            return Err(Error::ConflictError("人员角色已变化，请刷新后重新编辑".into()));
        }
        Ok(())
    }

    /// 校验单人资料请求：人员不能为空且至少有一项改动。
    pub(super) fn validate(&self) -> Result<()> {
        if self.user_id.trim().is_empty() {
            return Err(Error::ValidationError("人员不能为空".into()));
        }
        if self.org_unit_id.is_none() && self.name.is_none() && self.role_ids.is_none() {
            return Err(Error::ValidationError("没有需要保存的修改".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change() -> PersonProfileChange {
        PersonProfileChange {
            user_id: "one".into(),
            expected_name: "旧姓名".into(),
            name: None,
            expected_role_ids: None,
            role_ids: None,
            org_unit_id: None,
        }
    }

    #[test]
    fn role_change_checks_original_set_and_accepts_role_only_change() {
        let mut value = change();
        value.role_ids = Some(RoleIdSet::parse(["new"]).unwrap());
        let current = vec![RoleId::parse("old").unwrap()];
        assert!(value.validate_roles(&current).is_err());
        value.expected_role_ids = Some(RoleIdSet::parse(["other"]).unwrap());
        assert!(matches!(value.validate_roles(&current), Err(Error::ConflictError(_))));
        value.expected_role_ids = Some(RoleIdSet::parse(&current).unwrap());
        assert!(value.validate_roles(&current).is_ok());
        assert!(value.validate().is_ok());
        value.expected_role_ids = Some(RoleIdSet::parse(["second", "old"]).unwrap());
        assert!(
            value.validate_roles(&[RoleId::parse("old").unwrap(), RoleId::parse("second").unwrap()]).is_ok()
        );
        value.expected_role_ids = Some(RoleIdSet::parse(&current).unwrap());
        value.role_ids = Some(RoleIdSet::default());
        assert!(value.validate_roles(&current).is_err());
    }

    #[test]
    fn transfer_and_name_validation() {
        let mut value = change();
        assert!(value.validate().is_err());
        value.org_unit_id = Some("sales".into());
        assert!(value.validate().is_ok());
        value.user_id = " ".into();
        assert!(value.validate().is_err());
    }
}
