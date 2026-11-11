//! 集成统一授权接线；业务事实与 Mapping 由集成领域提供。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::{CompiledAccess, SharedRbacService};
use erp_integration::service::authorization::IntegrationAuthorization;
use erp_integration::{IntegrationDataScopePort, IntegrationOpsService};
use mongodb::Database;
use persistence_core::Executor;

use super::identity_error::map_identity_error;
use super::scope_support;
map_identity_error!(erp_integration);

/// 组合根持有的一次登记的集成授权设施。
#[derive(Clone)]
pub struct MongoIntegrationDataScope {
    db: Database,
    authorization: IntegrationAuthorization,
}
impl MongoIntegrationDataScope {
    /// 绑定共享身份设施及业务映射。
    /// # 参数
    /// db、rbac 为组合根共享资源。
    /// # 返回
    /// 固定登记的授权适配器。
    /// # 错误
    /// 不执行 I/O。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { authorization: IntegrationAuthorization::new(db.clone(), rbac), db }
    }
    /// 创建共享的集成授权端口。
    /// # 参数
    /// db、rbac 为组合根共享资源。
    /// # 返回
    /// 固定登记的共享端口。
    /// # 错误
    /// 不执行 I/O。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn IntegrationDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}
#[async_trait]
impl IntegrationDataScopePort for MongoIntegrationDataScope {
    async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_integration::Result<CompiledAccess> {
        self.authorization.query(actor, resource, action, executor).await.map_err(map_identity_error)
    }
    async fn validate(
        &self,
        access: &CompiledAccess,
        executor: &mut dyn Executor,
    ) -> erp_integration::Result<()> {
        self.authorization.validate(access, executor).await.map_err(map_identity_error)
    }
    async fn require(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> erp_integration::Result<()> {
        self.authorization.require(actor, resource, action, id, executor).await.map_err(map_identity_error)
    }
    async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> erp_integration::Result<BTreeSet<String>> {
        scope_support::expand_org_units(
            &self.db,
            org_unit_ids,
            include_descendants,
            executor,
            Into::into,
            map_identity_error,
        )
        .await
    }

    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_integration::Result<Vec<String>> {
        scope_support::org_member_ids(&self.db, org_unit_ids, at, executor, Into::into).await
    }

    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_integration::Result<Option<String>> {
        scope_support::own_org(&self.db, user_id, at, executor, Into::into, map_identity_error).await
    }
}
/// 创建已接入统一授权的集成服务。
/// # 参数
/// db、rbac 为组合根共享资源。
/// # 返回
/// 可执行统一授权的集成业务入口。
/// # 错误
/// 不执行 I/O。
pub fn scoped_integration_ops_service(db: Database, rbac: SharedRbacService) -> IntegrationOpsService {
    IntegrationOpsService::with_scope(db.clone(), MongoIntegrationDataScope::shared(db, rbac))
}
