//! 结算服务和流程的组合根装配。
use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_supply::SettlementAccess;
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply_settlement::supply_settlement::SupplierSettlementProcess;
use erp_workflow::service::work_item::WorkItemService;
use mongodb::Database;

use super::identity::shared_rbac_service;
use super::workflow::{WorkflowAuth, work_item_service};

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
pub fn settlement_access(db: Database, rbac: SharedRbacService) -> SettlementAccess {
    SettlementAccess::new(db, rbac)
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
    SupplierSettlementService::new(db.clone()).with_data_scope(SettlementAccess::new(db, rbac))
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
        SettlementAccess::new(db.clone(), rbac.clone()),
        rbac.clone(),
        work_item_service(db.clone(), rbac.clone()),
        Arc::new(fresh_work_items(db.clone())),
    )
}

fn fresh_work_items(db: Database) -> impl Fn() -> WorkItemService<WorkflowAuth> + Send + Sync {
    move || work_item_service(db.clone(), shared_rbac_service(db.clone()))
}
