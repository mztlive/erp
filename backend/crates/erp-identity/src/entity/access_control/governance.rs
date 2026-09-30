//! 有效内建超级管理员始终具有全公司数据范围。

use super::{ResolvedScope, ScopeClause};
use crate::entity::role::{ROOT_ROLE_ID, Role};

/// 为已通过当前账号、同角色完整动作权限证明的角色集合解析超管范围。
///
/// # 参数
/// * `qualified_roles` - 调用方在当前事务中验证的角色，不得传入未经权限证明的角色。
/// # 返回
/// 有效内建 root 返回不附加人员上限的公司范围，适用于全部已登记范围消费者。
/// # 错误
/// 无；不符合超管条件时返回 None，由调用方继续解析人员范围。
pub(crate) fn root_scope(qualified_roles: &[Role]) -> Option<ResolvedScope> {
    let root = qualified_roles
        .iter()
        .any(|role| role.base.id == ROOT_ROLE_ID && role.system && !role.disabled && !role.base.is_deleted());
    root.then(|| ResolvedScope {
        role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
        user_limit: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access_control::ScopedObject;
    use crate::entity::role::RoleData;

    /// 全公司范围覆盖非本人、其他部门、仓库和结算主体，不依赖参与关系。
    #[test]
    fn root_covers_all_business_dimensions_without_person_limit() {
        let root = Role::new(ROOT_ROLE_ID.into(), RoleData::new("超级管理员").with_system(true)).unwrap();
        let scope = root_scope(&[root]).unwrap();
        assert!(scope.user_limit.is_none());
        assert!(scope.allows(
            &ScopedObject {
                owned: false,
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: Some("other-department"),
                settlement_party_id: Some("other-party"),
                warehouse_id: Some("other-warehouse"),
            },
            false
        ));
    }

    /// 同名角色、普通角色、失效角色及未通过动作资格的空集合均不能取得治理范围。
    #[test]
    fn root_identity_and_enabled_system_role_are_required() {
        assert!(root_scope(&[]).is_none());
        let ordinary =
            Role::new("role-custom".into(), RoleData::new("超级管理员").with_system(true)).unwrap();
        assert!(root_scope(&[ordinary]).is_none());
        let mut root = Role::new(ROOT_ROLE_ID.into(), RoleData::new("超级管理员")).unwrap();
        assert!(root_scope(std::slice::from_ref(&root)).is_none());
        root.system = true;
        root.disabled = true;
        assert!(root_scope(std::slice::from_ref(&root)).is_none());
        root.disabled = false;
        root.base.deleted_at = 1;
        assert!(root_scope(&[root]).is_none());
    }
}
