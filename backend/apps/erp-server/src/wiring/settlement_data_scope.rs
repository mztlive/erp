//! 组合根只装配业务事实；权限检查及查询编译统一交给身份域。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply::{SettlementDataScopePort, SettlementResolvedScope};
use erp_supply_settlement::SupplierSettlementProcess;
use erp_workflow::service::work_item::WorkItemService;
use mongodb::Database;
use persistence_core::Executor;

use super::identity::shared_rbac_service;
use super::identity_error::map_identity_error;
use super::scope_support;
use super::workflow::{WorkflowAuth, work_item_service};
map_identity_error!(erp_supply);

#[derive(Clone)]
pub struct MongoSettlementDataScope {
    db: Database,
    rbac: SharedRbacService,
}
impl MongoSettlementDataScope {
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
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn SettlementDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
}
#[async_trait]
impl SettlementDataScopePort for MongoSettlementDataScope {
    async fn resolve(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_supply::Result<SettlementResolvedScope> {
        erp_supply::authorization::resolve(
            &self.db,
            self.rbac.clone(),
            actor,
            "supplier_settlement_statement",
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
            "supplier_settlement_statement",
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
    ) -> erp_supply::Result<BTreeSet<String>> {
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
/// 构造绑定身份数据库的结算访问器。
///
/// # 参数
/// * `db` - 结算与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
///
/// # 返回
/// 返回未缓存授权的结算访问器。
///
/// # 错误
/// 不返回错误。
pub fn settlement_access(db: Database, rbac: SharedRbacService) -> erp_supply::SettlementAccess {
    erp_supply::SettlementAccess::new(db.clone(), MongoSettlementDataScope::shared(db, rbac))
}

/// 构造已接入公共解析器的结算服务。
///
/// # 参数
/// * `db` - 结算与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
///
/// # 返回
/// 返回可解析结算范围的服务。
///
/// # 错误
/// 不返回错误。
pub fn scoped_settlement_service(db: Database, rbac: SharedRbacService) -> SupplierSettlementService {
    SupplierSettlementService::new(db.clone()).with_data_scope(MongoSettlementDataScope::shared(db, rbac))
}

/// 构造已接入公共解析器的结算流程。
///
/// # 参数
/// * `db` - 结算与身份集合所在数据库
/// * `rbac` - 现有 RBAC 快照服务
///
/// # 返回
/// 返回可解析结算范围的跨域流程。
///
/// # 错误
/// 不返回错误。
pub fn scoped_settlement_process(
    db: Database,
    rbac: SharedRbacService,
) -> SupplierSettlementProcess<WorkflowAuth> {
    SupplierSettlementProcess::new(
        db.clone(),
        work_item_service(db.clone(), rbac.clone()),
        Arc::new(fresh_work_items(db.clone())),
    )
    .with_scope(MongoSettlementDataScope::shared(db.clone(), rbac.clone()), rbac)
}

fn fresh_work_items(db: Database) -> impl Fn() -> WorkItemService<WorkflowAuth> + Send + Sync {
    move || work_item_service(db.clone(), shared_rbac_service(db.clone()))
}
