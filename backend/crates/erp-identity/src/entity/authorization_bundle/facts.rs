//! 文件模型只接受可以完整表达的直接角色授权事实。
use super::plan::{PolicyRoleState, PolicyUserState};
use crate::{Error, Permission, Result, Role, RoleIdSet};

impl PolicyRoleState {
    /// 校验已读取的角色事实，禁止新建分支接回遗留继承权限。
    /// # 参数
    /// role 为可缺失实体，permissions、parents、subjects 为同一事务读取的 Casbin 事实。
    /// # 返回
    /// 完整普通角色事实；完全不存在的身份返回空。
    /// # 错误
    /// 继承、孤立授权、非后台主体或影响规模超限时拒绝。
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
    /// 只接受实际角色键，不能静默丢弃文件无法表达的主体继承。
    /// # 参数
    /// version 为账号版本，keys 为持久化直接分组键。
    /// # 返回
    /// 稳定排序的人员角色事实。
    /// # 错误
    /// 存在非角色分组或非法角色标识时拒绝。
    pub(crate) fn from_role_keys(version: u64, keys: Vec<String>) -> Result<Self> {
        if keys.iter().any(|key| !key.starts_with("role:")) {
            return Err(Error::ValidationError("人员绑定包含非角色分组，不能用1.0文件管理".into()));
        }
        Ok(Self { version, role_ids: RoleIdSet::from_casbin_role_keys(keys)?.to_strings() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RoleData;

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

    #[test]
    fn existing_role_preserves_complete_impact_and_rejects_unsupported_links() {
        let role = Role::new("reader".into(), RoleData::new("销售只读")).unwrap();
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

    #[test]
    fn person_group_inheritance_is_not_silently_removed_from_the_preview() {
        assert!(PolicyUserState::from_role_keys(1, vec!["user:admin:another".into()]).is_err());
        let user =
            PolicyUserState::from_role_keys(2, vec!["role:reader".into(), "role:reader".into()]).unwrap();
        assert_eq!(user.role_ids, ["reader"]);
        assert_eq!(user.version, 2);
    }
}
