//! 内建超级管理员的组织配置权与业务数据范围分离。

use super::{ResolvedScope, ScopeClause};
use crate::entity::role::{ROOT_ROLE_ID, Role};

/// 为已通过当前账号、同角色完整动作权限证明的角色集合解析系统治理范围。
///
/// # 参数
/// * `qualified_roles` - 调用方在当前事务中验证的角色，不得传入未经权限证明的角色。
/// * `resource` - 已登记的目标资源。
/// * `action` - 已登记的目标动作。
/// # 返回
/// 仅有效内建 root 的组织读取和管理返回公司范围；业务资源继续读取人员配置。
/// # 错误
/// 无；不符合治理条件时返回 None，由调用方继续解析人员范围。
pub(crate) fn root_organization_scope(
    qualified_roles: &[Role],
    resource: &str,
    action: &str,
) -> Option<ResolvedScope> {
    let root = qualified_roles
        .iter()
        .any(|role| role.base.id == ROOT_ROLE_ID && role.system && !role.disabled && !role.base.is_deleted());
    (root && resource == "org_unit" && matches!(action, "list" | "manage")).then(|| ResolvedScope {
        role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
        user_limit: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::role::RoleData;

    /// 系统配置入口不要求已存在人员范围，也不向业务资源提供兜底。
    #[test]
    fn root_governs_organization_without_granting_business_data() {
        let root = Role::new(ROOT_ROLE_ID.into(), RoleData::new("超级管理员").with_system(true)).unwrap();
        for action in ["list", "manage"] {
            let scope = root_organization_scope(std::slice::from_ref(&root), "org_unit", action).unwrap();
            assert!(scope.role_clauses[0].company);
            assert!(scope.user_limit.is_none());
        }
        for resource in ["sales_order", "purchase_order", "work_item", "approval_instance", "warehouse"] {
            assert!(root_organization_scope(std::slice::from_ref(&root), resource, "list").is_none());
        }
        assert!(root_organization_scope(&[root], "org_unit", "delete").is_none());
    }

    /// 同名角色、普通角色、失效角色及未通过动作资格的空集合均不能取得治理范围。
    #[test]
    fn root_identity_and_enabled_system_role_are_required() {
        assert!(root_organization_scope(&[], "org_unit", "manage").is_none());
        let ordinary =
            Role::new("role-custom".into(), RoleData::new("超级管理员").with_system(true)).unwrap();
        assert!(root_organization_scope(&[ordinary], "org_unit", "manage").is_none());
        let mut root = Role::new(ROOT_ROLE_ID.into(), RoleData::new("超级管理员")).unwrap();
        assert!(root_organization_scope(std::slice::from_ref(&root), "org_unit", "manage").is_none());
        root.system = true;
        root.disabled = true;
        assert!(root_organization_scope(std::slice::from_ref(&root), "org_unit", "manage").is_none());
        root.disabled = false;
        root.base.deleted_at = 1;
        assert!(root_organization_scope(&[root], "org_unit", "manage").is_none());
    }
}
