//! 身份域设施边界；一次解析共享资格，业务事实仍通过独立 Facts 读取。
use std::collections::BTreeSet;

use application_core::AuditActor;
use async_trait::async_trait;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::authorization::{AccessContext, ResourceTargets};
use crate::entity::resource_access::ResourceKind;
use crate::repository::resource_grants;
use crate::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use crate::{Permission, QualifiedPermissions, Result, SharedRbacService};

#[async_trait]
pub(super) trait Facilities: Send + Sync {
    async fn permissions(
        &self,
        actor: &AuditActor,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>>;

    async fn scope(
        &self,
        context: &AccessContext,
        source: &Permission,
        required: &[Permission],
        qualified: &QualifiedPermissions,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope>;

    async fn grant_revision(
        &self,
        actor: &AuditActor,
        resource: ResourceKind,
        executor: &mut dyn Executor,
    ) -> Result<u64>;

    async fn grants(
        &self,
        context: &AccessContext,
        resource: ResourceKind,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>>;
}

pub(super) struct IdentityFacilities {
    pub db: Database,
    pub rbac: SharedRbacService,
}

#[async_trait]
impl Facilities for IdentityFacilities {
    async fn permissions(
        &self,
        actor: &AuditActor,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        self.rbac.qualified_permissions(actor, permissions, executor).await
    }

    async fn scope(
        &self,
        context: &AccessContext,
        source: &Permission,
        required: &[Permission],
        qualified: &QualifiedPermissions,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        DataScopeService::new(self.db.clone(), self.rbac.clone())
            .qualified(context, source, required, qualified, executor)
            .await
    }

    async fn grant_revision(
        &self,
        actor: &AuditActor,
        resource: ResourceKind,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        resource_grants::revision(&self.db, actor.id(), resource, executor).await
    }

    async fn grants(
        &self,
        context: &AccessContext,
        resource: ResourceKind,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        resource_grants::target_ids(&self.db, context.actor.id(), resource, ids, executor).await
    }
}
