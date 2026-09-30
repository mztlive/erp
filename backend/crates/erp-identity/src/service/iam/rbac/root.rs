use persistence_core::NoTransaction;

use super::policy::{permissions_for_role, root_role_is_current};
use super::{ROOT_ROLE_ID, ROOT_ROLE_INIT_ATTEMPTS, ROOT_ROLE_NAME, SharedRbacService};
use crate::AccessControlExt;
use crate::entity::rbac::Permission;
use crate::entity::role::{Role, RoleData};
use crate::error::{Error, Result};

/// 确保 root 角色存在且拥有全量操作权限。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
///
/// # 返回值
/// 返回当前生效的超级管理员角色。
///
/// # 错误
/// 角色或 Casbin policy 写入失败时返回错误。
///
/// # 业务约束
/// 角色元数据与 `*:*` policy 按固定 root ID 修复；不写入或恢复业务数据范围。
/// 内建超级管理员的组织配置权由治理规则提供，不依赖业务人员范围初始化。
pub async fn ensure_root_role(rbac: &SharedRbacService) -> Result<Role> {
    let root_permission = Permission::parse("*:*")?;
    let mut attempts = 0;
    let role = loop {
        attempts += 1;
        match ensure_root_role_once(rbac, &root_permission).await {
            Err(Error::ConflictError(_)) if attempts < ROOT_ROLE_INIT_ATTEMPTS => continue,
            result => break result?,
        }
    };
    Ok(role)
}

/// 执行一次可重入的 root 角色校验或修复。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
/// * `root_permission` - 固定的 `*:*` 权限
///
/// # 返回值
/// 返回已存在且元数据/policy 正确，或本次创建/修复后的角色。
///
/// # 错误
/// 角色读取、Enforcer 刷新、policy 解析或创建/修复写入失败时返回错误。
///
/// # 业务约束
/// 本方法不处理 DataScope，不得通过角色修复补齐业务范围。
async fn ensure_root_role_once(rbac: &SharedRbacService, root_permission: &Permission) -> Result<Role> {
    if let Some(role) = rbac.db.roles().find_by_id_including_deleted(ROOT_ROLE_ID, &mut NoTransaction).await?
    {
        let enforcer = rbac.fresh_enforcer().await?.read().await;
        let permissions = permissions_for_role(&enforcer, ROOT_ROLE_ID)?;
        drop(enforcer);
        if root_role_is_current(&role, &permissions, root_permission) {
            return Ok(role);
        }
        return rbac.repair_root_role(role, root_permission.clone()).await;
    }

    let data = RoleData::new(ROOT_ROLE_NAME.to_string()).with_system(true);
    rbac.create_role_with_id(ROOT_ROLE_ID.to_string(), data, vec![root_permission.clone()], None, None).await
}

#[cfg(test)]
mod tests {
    use super::ROOT_ROLE_ID;
    use crate::entity::access_control::{
        DataScope, DataScopeData, DataScopeId, DataScopeSubjectType, DataScopeType, OrganizationCoverage,
    };

    fn root_company_scope() -> DataScope {
        DataScope::new(
            DataScopeId::new("data-scope-role-root-company"),
            DataScopeData {
                binding: crate::access_control::ScopeBinding {
                    schema_version: 2,
                    resource: "sales_order".into(),
                    actions: vec!["list".into()],
                    target_dimension: crate::access_control::ScopeDimension::InternalOrg,
                    target_mode: None,
                    include_descendants: None,
                    enabled: true,
                },
                subject_type: DataScopeSubjectType::Role,
                subject_id: ROOT_ROLE_ID.to_string(),
                scope_type: DataScopeType::Company,
                scope_targets: Vec::new(),
            },
        )
        .expect("role-root 公司级范围必须合法")
    }

    #[test]
    fn root_role_accepts_company_data_scope() {
        let scope = root_company_scope();
        assert_eq!(scope.subject_type, DataScopeSubjectType::Role);
        assert_eq!(scope.subject_id, ROOT_ROLE_ID);
        assert_eq!(scope.scope_type, DataScopeType::Company);
        assert!(scope.scope_targets.is_empty());
    }

    #[test]
    fn root_company_scope_covers_all_organizations() {
        let coverage = OrganizationCoverage::from_scopes(std::slice::from_ref(&root_company_scope()))
            .expect("公司级范围必须形成 All 覆盖");
        assert_eq!(coverage, OrganizationCoverage::All);
        assert!(coverage.covers("any-org"));
    }

    #[test]
    fn root_without_data_scope_has_no_organization_coverage() {
        assert_eq!(OrganizationCoverage::from_scopes(&[]), None);
    }
}
