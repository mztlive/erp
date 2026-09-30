//! 人员资料的原子变更：限定单人，按差异更新组织关系。
use std::collections::BTreeSet;

use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::organization_change::{OrganizationOperation, OrganizationState};
use crate::{Error, Result, RoleIdSet};

/// 单人的新增部门管理关系。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PersonManagementChange {
    pub role_id: String,
    pub org_unit_id: String,
    pub include_descendants: bool,
    pub valid_to: Option<Instant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PersonProfileChange {
    pub user_id: String,
    pub expected_name: String,
    pub name: Option<String>,
    pub expected_role_ids: Option<Vec<String>>,
    pub role_ids: Option<Vec<String>>,
    pub org_unit_id: Option<String>,
    pub remove_management_ids: Vec<String>,
    pub add_management: Vec<PersonManagementChange>,
}

impl PersonProfileChange {
    /// 校验完整角色集的期望值以及保留、新增管理关系的角色依赖。
    ///
    /// # 参数
    /// * `current` - 当前授权快照的人员角色。
    /// * `state` - 同一快照的组织关系。
    /// * `at` - 校验时点。
    /// # 错误
    /// 空角色集、角色版本冲突、未移除的管理关系依赖被撤销角色时拒绝。
    pub fn validate_roles(&self, current: &[String], state: &OrganizationState, at: Instant) -> Result<()> {
        let Some(roles) = &self.role_ids else {
            return Ok(());
        };
        let roles = RoleIdSet::parse_non_empty(roles.clone())?.to_strings();
        let expected = self
            .expected_role_ids
            .as_ref()
            .ok_or_else(|| Error::ValidationError("修改角色必须提供原角色集合".into()))?;
        if RoleIdSet::parse(expected.clone())?.to_strings().into_iter().collect::<BTreeSet<_>>()
            != RoleIdSet::parse(current.to_vec())?.to_strings().into_iter().collect::<BTreeSet<_>>()
        {
            return Err(Error::ConflictError("人员角色已变化，请刷新后重新编辑".into()));
        }
        let retained_dependency = state.management.iter().any(|grant| {
            grant.user_id == self.user_id
                && grant.validity.contains(at)
                && !self.remove_management_ids.contains(&grant.base.id)
                && !roles.contains(&grant.role_id)
        });
        if retained_dependency || self.add_management.iter().any(|grant| !roles.contains(&grant.role_id)) {
            return Err(Error::ValidationError(
                "部门管理关系使用了未选择的角色，请同时移除该关系或保留角色".into(),
            ));
        }
        Ok(())
    }

    /// 展开固定人员的关系差异；不允许删除其他人员关系。
    ///
    /// # 参数
    /// * `state` - 同一快照的完整组织事实。
    /// # 返回
    /// 先撤销、再调岗、再新增的组织命令。
    /// # 错误
    /// 空修改、重复删除、超量操作及越人删除均拒绝。
    pub fn operations(&self, state: &OrganizationState) -> Result<Vec<OrganizationOperation>> {
        if self.user_id.trim().is_empty()
            || self.remove_management_ids.len() + self.add_management.len() > 100
        {
            return Err(Error::ValidationError("人员不能为空且单次最多修改100条管理关系".into()));
        }
        let mut operations = Vec::new();
        let mut seen = BTreeSet::new();
        for id in &self.remove_management_ids {
            if !seen.insert(id)
                || !state.management.iter().any(|g| g.base.id == *id && g.user_id == self.user_id)
            {
                return Err(Error::ValidationError("撤销关系必须属于当前人员且不能重复".into()));
            }
            operations.push(OrganizationOperation::RevokeManagement { assignment_id: id.clone() });
        }
        if let Some(org_unit_id) = &self.org_unit_id {
            operations.push(OrganizationOperation::TransferMember {
                user_id: self.user_id.clone(),
                org_unit_id: org_unit_id.clone(),
            });
        }
        operations.extend(self.add_management.iter().map(|g| OrganizationOperation::GrantManagement {
            user_id: self.user_id.clone(),
            role_id: g.role_id.clone(),
            org_unit_id: g.org_unit_id.clone(),
            include_descendants: g.include_descendants,
            valid_to: g.valid_to,
        }));
        if operations.is_empty() && self.name.is_none() && self.role_ids.is_none() {
            return Err(Error::ValidationError("没有需要保存的修改".into()));
        }
        Ok(operations)
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
            remove_management_ids: vec![],
            add_management: vec![],
        }
    }
    /// 角色保存不得覆盖过期快照，也不能生成空角色集。
    #[test]
    fn role_change_checks_original_set_and_accepts_role_only_change() {
        let mut value = change();
        value.role_ids = Some(vec!["new".into()]);
        let current = vec!["old".into()];
        let state = OrganizationState::default();
        let at = Instant::from_unix_secs(10);
        assert!(value.validate_roles(&current, &state, at).is_err());
        value.expected_role_ids = Some(vec!["other".into()]);
        assert!(matches!(value.validate_roles(&current, &state, at), Err(Error::ConflictError(_))));
        value.expected_role_ids = Some(current.clone());
        assert!(value.validate_roles(&current, &state, at).is_ok());
        assert!(value.operations(&state).unwrap().is_empty());
        value.expected_role_ids = Some(vec!["second".into(), "old".into()]);
        assert!(value.validate_roles(&["old".into(), "second".into()], &state, at).is_ok());
        value.expected_role_ids = Some(current.clone());
        value.role_ids = Some(vec![]);
        assert!(value.validate_roles(&current, &state, at).is_err());
    }

    /// 新角色可供同次新增关系使用；移除旧角色必须同步移除其现行管理关系。
    #[test]
    fn management_uses_resulting_roles_and_requires_explicit_removal() {
        use entity_core::BaseModel;

        use crate::entity::organization::{OrgManagementAssignment, OrgValidity};
        let mut value = change();
        let current = vec!["old".into()];
        value.expected_role_ids = Some(current.clone());
        value.role_ids = Some(vec!["new".into()]);
        value.add_management.push(PersonManagementChange {
            role_id: "new".into(),
            org_unit_id: "dept".into(),
            include_descendants: false,
            valid_to: None,
        });
        let at = Instant::from_unix_secs(10);
        let mut state = OrganizationState::default();
        assert!(value.validate_roles(&current, &state, at).is_ok());
        state.management.push(OrgManagementAssignment {
            base: BaseModel::new("grant".into()),
            user_id: "one".into(),
            role_id: "old".into(),
            org_unit_id: "dept".into(),
            include_descendants: false,
            validity: OrgValidity { valid_from: Instant::from_unix_secs(1), valid_to: None },
            granted_by: "admin".into(),
            reason: "初始化".into(),
        });
        assert!(value.validate_roles(&current, &state, at).is_err());
        value.remove_management_ids.push("grant".into());
        assert!(value.validate_roles(&current, &state, at).is_ok());
        value.add_management[0].role_id = "not-selected".into();
        assert!(value.validate_roles(&current, &state, at).is_err());
    }

    #[test]
    fn profile_expands_only_target_person() {
        let mut value = change();
        value.org_unit_id = Some("sales".into());
        value.add_management.push(PersonManagementChange {
            role_id: "role".into(),
            org_unit_id: "sales".into(),
            include_descendants: true,
            valid_to: None,
        });
        let ops = value.operations(&OrganizationState::default()).unwrap();
        assert!(matches!(&ops[0], OrganizationOperation::TransferMember { user_id, .. } if user_id == "one"));
        assert!(
            matches!(&ops[1], OrganizationOperation::GrantManagement { user_id, include_descendants: true, .. } if user_id == "one")
        );
    }
    #[test]
    fn empty_and_foreign_removal_fail() {
        let mut value = change();
        assert!(value.operations(&OrganizationState::default()).is_err());
        value.name = Some("新姓名".into());
        assert!(value.operations(&OrganizationState::default()).unwrap().is_empty());
        value.remove_management_ids.push("foreign".into());
        assert!(value.operations(&OrganizationState::default()).is_err());
    }
}
