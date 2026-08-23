//! 预定义角色的默认数据范围种子。
//!
//! 指定到人的人工任务仍要求责任人具备可证明的数据范围。第一期单公司部署下，
//! 尚未配置任何范围的预定义岗位补齐公司级范围；管理员已配置、收窄或软删除的
//! 范围不会被覆盖或重建。

use super::predefined_roles::PREDEFINED_ROLES;
use super::SharedRbacService;
use crate::errors::Result;

/// 为全部业务预定义角色补齐缺失的公司级数据范围。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
///
/// # 返回值
/// 全部角色检查或写入完成后返回 `Ok(())`。
///
/// # 错误
/// 角色查询或数据范围写入失败时返回错误。
///
/// # 业务约束
/// 只追加空范围岗位的公司级范围，不删除、不扩大管理员已配置的组织/团队范围。
pub async fn ensure_predefined_role_data_scopes(rbac: &SharedRbacService) -> Result<()> {
    for role in PREDEFINED_ROLES {
        if rbac.seed_role_company_data_scope_if_absent(role.id).await? {
            tracing::info!(
                role_id = role.id,
                role_name = role.name,
                "predefined role company data scope seeded"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::PREDEFINED_ROLES;
    use entities::access_control::{
        DataScope, DataScopeData, DataScopeId, DataScopeSubjectType, DataScopeType,
    };

    #[test]
    fn predefined_roles_accept_company_data_scope() {
        for role in PREDEFINED_ROLES {
            let scope = DataScope::new(
                DataScopeId::new(format!("data-scope-{}-company", role.id)),
                DataScopeData {
                    subject_type: DataScopeSubjectType::Role,
                    subject_id: role.id.to_string(),
                    scope_type: DataScopeType::Company,
                    scope_targets: Vec::new(),
                },
            )
            .unwrap_or_else(|error| panic!("{} 公司级范围不合法: {error}", role.id));
            assert_eq!(scope.subject_id, role.id);
            assert_eq!(scope.scope_type, DataScopeType::Company);
            assert!(scope.scope_targets.is_empty());
        }
    }

    #[test]
    fn work_item_roles_include_procurement() {
        assert!(PREDEFINED_ROLES
            .iter()
            .any(|role| role.id == "role-procurement" && role.permissions.contains(&"work_item:list")));
    }
}
