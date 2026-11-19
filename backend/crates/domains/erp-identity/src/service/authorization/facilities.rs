//! 身份域设施边界；一次解析共享资格，业务事实仍通过独立 Facts 读取。
use std::collections::BTreeSet;

use application_core::AuditActor;
use async_trait::async_trait;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::authorization::{AccessContext, ResourceTargets};
use crate::entity::resource_access::{GrantTarget, ResourceKind, SourceKey};
use crate::repository::resource_grants;
use crate::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use crate::{Permission, QualifiedPermissions, Result, SharedRbacService};

/// 授权解析依赖的身份取证接口；业务对象事实由独立端口提供。
#[async_trait]
pub(super) trait Facilities: Send + Sync {
    /// 一次读取账号状态及所需权限的启用角色快照；失效账号返回 `None`，用于各规则共用同次身份取证。
    async fn permissions(
        &self,
        actor: &AuditActor,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>>;

    /// 解析指定范围来源，只采用满足完整资格的角色，并返回范围、组织事实及版本供规则构造和复验。
    async fn scope(
        &self,
        context: &AccessContext,
        source: &Permission,
        required: &[Permission],
        qualified: &QualifiedPermissions,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope>;

    /// 读取账号在指定资源上的记录授权修订号，用于凭证版本和交付复验。
    async fn grant_revision(
        &self,
        actor: &AuditActor,
        resource: ResourceKind,
        executor: &mut dyn Executor,
    ) -> Result<u64>;

    /// 仅检查本次目标 ID 的有效记录读取授权，供允许读取补充的对象规则使用。
    async fn grants(
        &self,
        context: &AccessContext,
        resource: ResourceKind,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>>;
    /// 审批补权时核对冻结实例为真实接收人建立的精确来源授权；未接入该能力时默认不放行。
    async fn source_grant(
        &self,
        _source: &SourceKey,
        _target: &GrantTarget,
        _executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(false)
    }
}

/// 生产授权使用的身份设施，将资格、范围与记录授权读取交给本域服务和仓储。
pub(super) struct IdentityFacilities {
    /// 读取人员范围、组织和记录授权的身份库。
    pub db: Database,
    /// 提供启用角色与功能资格快照的共享 RBAC 服务。
    pub rbac: SharedRbacService,
}

#[async_trait]
impl Facilities for IdentityFacilities {
    // 沿原执行器向 RBAC 读取账号及所请求权限的启用角色资格。
    async fn permissions(
        &self,
        actor: &AuditActor,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        self.rbac.qualified_permissions(actor, permissions, executor).await
    }

    // 使用人员范围服务按同次资格解析指定来源及组织事实。
    async fn scope(
        &self,
        context: &AccessContext,
        source: &Permission,
        required: &[Permission],
        qualified: &QualifiedPermissions,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        DataScopeService::new(self.db.clone()).qualified(context, source, required, qualified, executor).await
    }

    // 从记录授权仓储读取该账号和资源的修订号。
    async fn grant_revision(
        &self,
        actor: &AuditActor,
        resource: ResourceKind,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        resource_grants::revision(&self.db, actor.id(), resource, executor).await
    }

    // 从仓储返回当前账号在目标集合中拥有有效读取授权的记录 ID。
    async fn grants(
        &self,
        context: &AccessContext,
        resource: ResourceKind,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        resource_grants::target_ids(&self.db, context.actor.id(), resource, ids, executor).await
    }
    // 按授权来源键和目标记录向仓储核对有效授权，不以同账号的其他来源替代。
    async fn source_grant(
        &self,
        source: &SourceKey,
        target: &GrantTarget,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        resource_grants::source_target(&self.db, source, target, executor).await
    }
}
