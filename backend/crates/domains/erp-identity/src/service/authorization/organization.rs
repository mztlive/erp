//! 组织配置的域内范围能力；复用统一资格与原事务边界。
use std::ops::Deref;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{Permission, internal, organization};
use persistence_core::{ExecutionBoundary, Executor};

use super::AuthorizationService;
use super::resolve::scope_deadline;
use crate::entity::authorization::AccessContext;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Result};

/// 组织配置取得的域内范围凭据，绑定原快照读取阶段和组织期限。
pub(crate) struct OrganizationScope {
    // 同次授权解析出的范围、组织事实和策略版本。
    access: AuthorizedDataScope,
    // 签发时的快照事务及读取阶段。
    boundary: ExecutionBoundary,
    // 成员关系变化导致范围失效的最早时点。
    valid_until: Option<Instant>,
}

impl Deref for OrganizationScope {
    type Target = AuthorizedDataScope;

    // 借出已解析的范围和组织事实，不重新取证或刷新原凭据。
    fn deref(&self) -> &Self::Target {
        &self.access
    }
}

impl OrganizationScope {
    /// 组织预览、交付或首次写入前，沿原执行器检查快照阶段和范围期限；跨事务或阶段、关系到期时拒绝。
    pub(crate) fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        if !self.boundary.is_current(executor) {
            return Err(Error::ValidationError("组织范围不能跨事务或读取阶段使用".into()));
        }
        if self.valid_until.is_some_and(|until| Instant::now() >= until) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：组织范围已到期".into()));
        }
        Ok(())
    }
}

impl AuthorizationService {
    /// 为固定组织配置入口在原快照取得同次完整资格、人员范围和组织事实，不开放政策选择。入口不支持、资格不足、非快照或取证失败时拒绝。
    pub(crate) async fn organization_scope(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<OrganizationScope> {
        if ![organization::LIST, organization::CHANGE, internal::ORGANIZATION_PERSON_PROFILE_CHANGE]
            .contains(permission)
        {
            return Err(Error::ValidationError("此入口不提供组织配置范围".into()));
        }

        executor.next_read();
        let boundary = executor
            .boundary()
            .filter(ExecutionBoundary::is_snapshot)
            .ok_or_else(|| Error::ValidationError("组织配置范围必须使用快照事务".into()))?;

        let qualified = self.qualify(actor, permission, executor).await?;
        let context = AccessContext {
            actor: actor.clone(),
            permission: permission.clone(),
            resource: permission.resource().into(),
            model: None,
            as_of: Instant::now(),
        };
        let access = self.resolve_scope(&context, permission, &qualified, executor).await?;

        let scope = OrganizationScope { valid_until: scope_deadline(&access), access, boundary };
        scope.ensure_current(executor)?;
        Ok(scope)
    }
}
