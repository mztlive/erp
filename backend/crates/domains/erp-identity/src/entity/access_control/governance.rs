//! 有效内建超级管理员始终具有全公司数据范围。

use super::{ResolvedScope, ScopeClause};
use crate::entity::role::{ROOT_ROLE_ID, Role};

/// 从已验证的动作资格中解析超管范围。调用方须在当前事务中确认账号有效，且
/// `qualified_roles` 中每个角色均具备完整动作权限。
/// 启用且未删除的内建 `root` 为已登记的范围消费者提供无个人上限的全公司范围；
/// 未命中时返回 `None`，由调用方继续解析人员范围。
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
    use crate::RoleId;
    use crate::access_control::ScopedObject;
    use crate::entity::role::RoleData;

    // 确认超管范围没有个人上限，即使无归属或参与关系也可跨部门、仓库和结算主体。
    #[test]
    fn root_covers_all_business_dimensions_without_person_limit() {
        let root =
            Role::new(RoleId::parse(ROOT_ROLE_ID).unwrap(), RoleData::new("超级管理员").with_system(true))
                .unwrap();
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

    // 确认空资格集合、同名自定义角色、非内建角色以及停用或已删除的 root 均无法取得超管范围。
    #[test]
    fn root_identity_and_enabled_system_role_are_required() {
        assert!(root_scope(&[]).is_none());
        let ordinary =
            Role::new(RoleId::parse("role-custom").unwrap(), RoleData::new("超级管理员").with_system(true))
                .unwrap();
        assert!(root_scope(&[ordinary]).is_none());
        let mut root = Role::new(RoleId::parse(ROOT_ROLE_ID).unwrap(), RoleData::new("超级管理员")).unwrap();
        assert!(root_scope(std::slice::from_ref(&root)).is_none());
        root.system = true;
        root.disabled = true;
        assert!(root_scope(std::slice::from_ref(&root)).is_none());
        root.disabled = false;
        root.base.deleted_at = 1;
        assert!(root_scope(&[root]).is_none());
    }
}
