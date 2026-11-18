//! 授权管理的固定公司边界；调用方不组合资格或选择组织范围。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{access_control, admin, organization};
use persistence_core::Executor;

use super::AuthorizationService;
use super::resolve::Resolution;
use crate::entity::authorization::AccessContext;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Permission, QualifiedPermissions, Result};

/// 返回授权管理入口固定使用的组织权限。
pub(super) fn source(permission: &Permission) -> Option<Permission> {
    if [access_control::PERSON_SCOPE_LIST, access_control::POLICY_BUNDLE_EXPORT].contains(permission) {
        Some(organization::LIST)
    } else if [
        access_control::PERSON_SCOPE_SAVE,
        access_control::POLICY_BUNDLE_PREVIEW,
        access_control::POLICY_BUNDLE_APPLY,
    ]
    .contains(permission)
    {
        Some(organization::CHANGE)
    } else {
        None
    }
}

/// 公司组织边界和配置资格必须由同一启用角色提供。
pub(super) fn requirements(permission: &Permission) -> Option<Vec<Permission>> {
    if let Some(required) = super::policy_bundles::requirements(permission, None) {
        return Some(required);
    }
    source(permission)
        .map(|source| vec![admin::ACCOUNT_LIST_ADMINS, admin::ROLE_LIST_ROLES, permission.clone(), source])
}

impl AuthorizationService {
    /// 在统一入口内部读取并检查管理操作的固定公司范围。
    pub(super) async fn require_operation_scope(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        qualified: &QualifiedPermissions,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let Some(source) = source(permission) else { return Ok(()) };
        let required = requirements(permission).expect("固定管理入口具有完整资格定义");
        let context = AccessContext {
            actor: actor.clone(),
            permission: permission.clone(),
            resource: permission.resource().into(),
            model: None,
            as_of: Instant::now(),
        };
        self.company_operation_scope(&context, &source, &required, qualified, executor).await?;
        Ok(())
    }

    /// 固定管理入口共用同角色公司范围判断，并保留原策略和组织事实供同域配置用例使用。
    pub(super) async fn company_operation_scope(
        &self,
        context: &AccessContext,
        source: &Permission,
        required: &[Permission],
        qualified: &QualifiedPermissions,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let access = self.facilities.scope(context, source, required, qualified, executor).await?;
        if !access.scope.role_clauses.iter().any(|clause| clause.company)
            || access.scope.user_limit.as_ref().is_some_and(|clause| !clause.company)
        {
            return Err(Error::Forbidden("授权管理需要同角色完整资格和公司组织范围".into()));
        }

        let mut resolution = Resolution::default();
        resolution.include(&access);
        if resolution.valid_until.is_some_and(|until| until <= Instant::now()) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：管理范围已到期，请重新读取".into()));
        }
        Ok(access)
    }
}
