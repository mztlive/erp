//! 组织配置的域内范围能力；复用统一资格与原事务边界。
use std::ops::Deref;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{Permission, organization};
use persistence_core::{ExecutionBoundary, Executor};

use super::AuthorizationService;
use super::resolve::scope_deadline;
use crate::entity::authorization::AccessContext;
use crate::entity::person_profile_change::ORGANIZATION_PERSON_PROFILE_CHANGE;
use crate::repository::authorization::AccessValidity;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{Error, Result};

/// 域内范围凭据，绑定原快照读取阶段和组织期限。
pub(crate) struct SnapshotScope {
    // 同次授权解析出的范围、组织事实和策略版本。
    access: AuthorizedDataScope,
    // 签发时的快照事务及读取阶段。
    pub(super) boundary: ExecutionBoundary,
    // 成员关系变化导致范围失效的最早时点。
    pub(super) valid_until: Option<Instant>,
}

impl Deref for SnapshotScope {
    type Target = AuthorizedDataScope;

    // 借出已解析的范围和组织事实，不重新取证或刷新原凭据。
    fn deref(&self) -> &Self::Target {
        &self.access
    }
}

impl SnapshotScope {
    /// 绑定同次范围及其最早失效时点，签发入口负责在交付前核对原快照。
    pub(super) fn new(access: AuthorizedDataScope, boundary: ExecutionBoundary) -> Self {
        Self { valid_until: scope_deadline(&access), access, boundary }
    }

    /// 组织预览、交付或首次写入前检查原快照阶段与范围期限。
    pub(crate) fn ensure_current(&self, executor: &mut dyn Executor) -> Result<()> {
        self.check(executor, "组织范围不能跨事务或读取阶段使用", "DATA_SCOPE_CHANGED：组织范围已到期")
    }

    pub(super) fn check(&self, executor: &mut dyn Executor, wrong_stage: &str, expired: &str) -> Result<()> {
        AccessValidity::check_stage(&self.boundary, self.valid_until, executor, wrong_stage, expired)
    }
}

impl AuthorizationService {
    /// 为固定组织配置入口在原快照取得同次完整资格、人员范围和组织事实，不开放政策选择。入口不支持、资格不足、非快照或取证失败时拒绝。
    pub(crate) async fn organization_scope(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<SnapshotScope> {
        if ![organization::LIST, organization::CHANGE, ORGANIZATION_PERSON_PROFILE_CHANGE]
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

        let scope = SnapshotScope::new(access, boundary);
        scope.ensure_current(executor)?;
        Ok(scope)
    }
}
