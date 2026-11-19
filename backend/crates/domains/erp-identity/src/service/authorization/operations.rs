//! 授权管理的固定公司边界；调用方不组合资格或选择组织范围。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{access_control, admin, organization};
use persistence_core::Executor;

use super::AuthorizationService;
use super::resolve::scope_deadline;
use crate::entity::authorization::AccessContext;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Permission, QualifiedPermissions, Result};

// 为组织配置相关的授权管理操作选取其固定组织权限，不从调用者输入推定边界。
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

// 组织授权管理用例要求同一启用角色同时证明公司组织范围的入口权限和配置资格。
pub(super) fn requirements(permission: &Permission) -> Option<Vec<Permission>> {
    if let Some(required) = super::policy_bundles::requirements(permission, None) {
        return Some(required);
    }
    source(permission)
        .map(|source| vec![admin::ACCOUNT_LIST_ADMINS, admin::ROLE_LIST_ROLES, permission.clone(), source])
}

impl AuthorizationService {
    // 统一授权处理管理操作时，按固定入口读取并校验公司级组织范围。
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

    // 管理操作先证明同角色公司范围，再把同次策略与组织事实交给本域配置用例，避免使用另一快照。
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

        if scope_deadline(&access).is_some_and(|until| until <= Instant::now()) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：管理范围已到期，请重新读取".into()));
        }
        Ok(access)
    }
}
