//! 组合根只装配业务事实；权限检查及查询编译统一交给身份域。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_supplier_catalog::supply_governance::SupplierOfferingProcess;
use erp_supply::{OfferingAccess, OfferingDataScopePort, OfferingResolvedScope, SupplierOfferingService};
use erp_workflow::service::work_item::WorkItemService;
use mongodb::Database;
use persistence_core::Executor;

use super::identity::shared_rbac_service;
use super::identity_error::map_identity_error;
use super::scope_support;
use super::workflow::{WorkflowAuth, work_item_service};
map_identity_error!(erp_supply);

#[derive(Clone)]
pub struct MongoOfferingDataScope {
    db: Database,
    rbac: SharedRbacService,
}
impl MongoOfferingDataScope {
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
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn OfferingDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}
#[async_trait]
impl OfferingDataScopePort for MongoOfferingDataScope {
    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<OfferingResolvedScope> {
        erp_supply::authorization::resolve(
            &self.db,
            self.rbac.clone(),
            actor,
            "supplier_offering",
            action,
            executor,
        )
        .await
        .map_err(map_identity_error)
    }
    async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<()> {
        erp_supply::authorization::require(
            &self.db,
            self.rbac.clone(),
            actor,
            "supplier_offering",
            action,
            id,
            executor,
        )
        .await
        .map_err(map_identity_error)
    }
    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<Vec<String>> {
        scope_support::org_member_ids(&self.db, org_unit_ids, at, executor, Into::into).await
    }

    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<Option<String>> {
        scope_support::own_org(&self.db, user_id, at, executor, Into::into, map_identity_error).await
    }
}
/// 构造绑定身份数据库的供给访问器。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回已注入本 adapter 的供给访问器。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// HTTP 与命名 Process 必须经此入口，不得把 RBAC 直接交给供给域。
pub fn offering_access(db: Database, rbac: SharedRbacService) -> OfferingAccess {
    OfferingAccess::new(db.clone(), MongoOfferingDataScope::shared(db, rbac))
}

/// 装配已接线范围 Port 的供给服务。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回注入生产范围 adapter 的供给服务。
///
/// # 错误
/// 无。
pub fn scoped_offering_service(db: Database, rbac: SharedRbacService) -> SupplierOfferingService {
    SupplierOfferingService::new(db.clone()).with_data_scope(MongoOfferingDataScope::shared(db, rbac))
}

/// 装配已接线范围 Port 的供给治理流程。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回注入生产范围 adapter 的供给流程。
///
/// # 错误
/// 无。
pub fn scoped_offering_process(db: Database, rbac: SharedRbacService) -> SupplierOfferingProcess {
    let work_items = fresh_work_items(db.clone());
    SupplierOfferingProcess::new(db.clone(), Arc::new(move || Arc::new(work_items())))
        .with_data_scope(MongoOfferingDataScope::shared(db, rbac))
}

fn fresh_work_items(db: Database) -> impl Fn() -> WorkItemService<WorkflowAuth> + Send + Sync {
    move || work_item_service(db.clone(), shared_rbac_service(db.clone()))
}
