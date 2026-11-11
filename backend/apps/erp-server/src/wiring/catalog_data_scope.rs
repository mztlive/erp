//! 组合根只装配业务事实；权限检查及查询编译统一交给身份域。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_catalog::{CatalogAccess, CatalogDataScopePort, CatalogResolvedScope, CatalogService};
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use mongodb::Database;
use persistence_core::Executor;

use super::catalog::{MongoCatalogAudit, MongoCatalogFileAssets};
use super::identity_error::map_identity_error;
use super::scope_support;
map_identity_error!(erp_catalog);

#[derive(Clone)]
pub struct MongoCatalogDataScope {
    db: Database,
    rbac: SharedRbacService,
}
impl MongoCatalogDataScope {
    /// 绑定身份设施。
    /// # 参数
    /// db、rbac 为应用共享设施。
    /// # 返回
    /// 不执行I/O的adapter。
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
    /// 包装为领域授权端口。
    /// # 参数
    /// db、rbac 为共享身份设施。
    /// # 返回
    /// 已装配端口。
    /// # 错误
    /// 不执行I/O。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn CatalogDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}
#[async_trait]
impl CatalogDataScopePort for MongoCatalogDataScope {
    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_catalog::Result<CatalogResolvedScope> {
        erp_catalog::authorization::resolve(&self.db, self.rbac.clone(), actor, "product", action, executor)
            .await
            .map_err(map_identity_error)
    }
    async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> erp_catalog::Result<()> {
        erp_catalog::authorization::require(
            &self.db,
            self.rbac.clone(),
            actor,
            "product",
            action,
            id,
            executor,
        )
        .await
        .map_err(map_identity_error)
    }
    async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> erp_catalog::Result<BTreeSet<String>> {
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
    ) -> erp_catalog::Result<Vec<String>> {
        scope_support::org_member_ids(&self.db, org_unit_ids, at, executor, Into::into).await
    }

    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_catalog::Result<Option<String>> {
        scope_support::own_org(&self.db, user_id, at, executor, Into::into, map_identity_error).await
    }
}
/// 构造绑定身份数据库的商品访问器。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回已注入本 adapter 的商品访问器。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// HTTP 与命名 Process 必须经此入口，不得把 RBAC 直接交给商品域。
pub fn catalog_access(db: Database, rbac: SharedRbacService) -> CatalogAccess {
    CatalogAccess::new(db.clone(), MongoCatalogDataScope::shared(db, rbac))
}

/// 装配已接线范围 Port 的商品服务。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回注入生产范围 adapter 的商品服务。
///
/// # 错误
/// 无。
pub fn scoped_catalog_service(db: Database, rbac: SharedRbacService) -> CatalogService {
    CatalogService::new(
        db.clone(),
        MongoCatalogAudit::shared(db.clone()),
        MongoCatalogFileAssets::shared(db.clone()),
    )
    .with_data_scope(MongoCatalogDataScope::shared(db, rbac))
}
