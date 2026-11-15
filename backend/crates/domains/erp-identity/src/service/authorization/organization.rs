//! 组织配置的域内范围能力；复用统一资格与原事务边界。
use std::ops::Deref;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{Permission, internal, organization};
use persistence_core::{ExecutionBoundary, Executor};

use super::AuthorizationService;
use super::resolve::Resolution;
use crate::entity::authorization::AccessContext;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Result};

/// 只在身份域组织配置用例中保留本次范围、期限及阶段。
pub(crate) struct OrganizationScope {
    access: AuthorizedDataScope,
    boundary: ExecutionBoundary,
    valid_until: Option<Instant>,
}

impl Deref for OrganizationScope {
    type Target = AuthorizedDataScope;

    fn deref(&self) -> &Self::Target {
        &self.access
    }
}

impl OrganizationScope {
    /// 组织预览、交付及首次写入前核验原快照和全部范围期限。
    /// # 参数
    /// executor 为取得组织范围的原事务执行器。
    /// # 返回
    /// 原快照阶段与组织范围有效期均满足时成功。
    /// # 错误
    /// 跨事务、跨阶段或关系到期时拒绝。
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
    /// 为组织配置的固定域内入口取得单次范围。
    /// # 参数
    /// actor 为当前身份，permission 为固定组织入口，executor 为原快照事务。
    /// # 返回
    /// 同一次完整资格、组织范围及边界；不向业务域开放政策选择。
    /// # 错误
    /// 入口不支持、资格不完整、非快照或组织设施失败时拒绝。
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
        let mut resolution = Resolution::default();
        resolution.include(&access);
        let scope = OrganizationScope { access, boundary, valid_until: resolution.valid_until };
        scope.ensure_current(executor)?;
        Ok(scope)
    }
}
