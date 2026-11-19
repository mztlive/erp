//! 文件模型只接受可以完整表达的直接角色授权事实。
use super::plan::{PolicyRoleState, PolicyUserState};
use crate::{Error, Permission, Result, Role, RoleIdSet};

impl PolicyRoleState {
    /// 用同一事务中的角色实体和 Casbin 直接授权、父角色、绑定主体构造角色事实。
    /// 实体和授权均不存在时返回 `None`；继承、孤立授权、非后台主体或超过 1000 个绑定主体时拒绝。
    pub(crate) fn from_parts(
        role: Option<Role>,
        permissions: Vec<Permission>,
        parents: Vec<String>,
        subjects: Vec<String>,
    ) -> Result<Option<Self>> {
        if !parents.is_empty() {
            return Err(Error::ValidationError("文件格式1.0不支持角色继承，请保留原授权入口".into()));
        }
        let Some(role) = role else {
            if !subjects.is_empty() || !permissions.is_empty() {
                return Err(Error::ValidationError(
                    "角色实体缺失但存在授权记录，请先治理后使用文件入口".into(),
                ));
            }
            return Ok(None);
        };

        if subjects.len() > 1000 {
            return Err(Error::ValidationError("角色影响超过1000个人员，请分批治理后再使用文件入口".into()));
        }
        if subjects.iter().any(|subject| subject.starts_with("role:")) {
            return Err(Error::ValidationError("角色被其他角色继承，不能用1.0文件管理".into()));
        }
        let affected_user_ids = subjects
            .into_iter()
            .map(|subject| {
                subject
                    .strip_prefix("user:admin:")
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| Error::ValidationError("角色绑定包含非后台主体，不能用1.0文件管理".into()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(Self { role, permissions, affected_user_ids }))
    }
}

impl PolicyUserState {
    /// 保留账号版本，将持久化的直接分组键转换为排序去重的角色 ID。
    /// 遇到非角色分组或非法角色标识时拒绝，避免丢失文件无法表达的主体关系。
    pub(crate) fn from_role_keys(version: u64, keys: Vec<String>) -> Result<Self> {
        if keys.iter().any(|key| !key.starts_with("role:")) {
            return Err(Error::ValidationError("人员绑定包含非角色分组，不能用1.0文件管理".into()));
        }
        Ok(Self { version, role_ids: RoleIdSet::from_casbin_role_keys(keys)?.into() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RoleData, RoleId};

    // 验证只有实体及授权事实都缺失时才视为新身份，遗留继承、绑定和权限均会阻止新建。
    #[test]
    fn missing_role_with_parent_is_not_treated_as_a_fresh_identity() {
        let inherited = PolicyRoleState::from_parts(None, vec![], vec!["role:role-root".into()], vec![]);
        assert!(matches!(inherited, Err(Error::ValidationError(_))));
        assert!(PolicyRoleState::from_parts(None, vec![], vec![], vec![]).unwrap().is_none());
        let bound = PolicyRoleState::from_parts(None, vec![], vec![], vec!["user:admin:alice".into()]);
        assert!(matches!(bound, Err(Error::ValidationError(_))));
        let policies = vec![Permission::parse("sales_order:list").unwrap()];
        assert!(PolicyRoleState::from_parts(None, policies, vec![], vec![]).is_err());
    }

    // 验证后台账号绑定完整计入影响范围，角色继承、其他主体和超限绑定均被拒绝。
    #[test]
    fn existing_role_preserves_complete_impact_and_rejects_unsupported_links() {
        let role = Role::new(RoleId::parse("reader").unwrap(), RoleData::new("销售只读")).unwrap();
        let state =
            PolicyRoleState::from_parts(Some(role.clone()), vec![], vec![], vec!["user:admin:alice".into()])
                .unwrap()
                .unwrap();
        assert_eq!(state.affected_user_ids, ["alice"]);
        for subjects in [
            vec!["role:child".into()],
            vec!["user:supplier:one".into()],
            vec!["user:admin:".into()],
            vec!["user:admin:one".into(); 1001],
        ] {
            assert!(PolicyRoleState::from_parts(Some(role.clone()), vec![], vec![], subjects).is_err());
        }
        assert!(PolicyRoleState::from_parts(Some(role), vec![], vec!["role:parent".into()], vec![]).is_err());
    }

    // 验证主体继承导致校验失败，合法直接角色去重后保留账号版本。
    #[test]
    fn person_group_inheritance_is_not_silently_removed_from_the_preview() {
        assert!(PolicyUserState::from_role_keys(1, vec!["user:admin:another".into()]).is_err());
        let user =
            PolicyUserState::from_role_keys(2, vec!["role:reader".into(), "role:reader".into()]).unwrap();
        assert_eq!(user.role_ids, [RoleId::parse("reader").unwrap()]);
        assert_eq!(user.version, 2);
    }
}
