//! 供给服务的组合根装配。
use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_supplier_catalog::supply_governance::SupplierOfferingProcess;
use erp_supply::{OfferingAccess, SupplierOfferingService};
use erp_workflow::service::work_item::WorkItemService;
use mongodb::Database;

use super::identity::shared_rbac_service;
use super::workflow::{WorkflowAuth, work_item_service};

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
/// 授权由本域直接调用 identity，组合根仅注入身份设施。
pub fn offering_access(db: Database, rbac: SharedRbacService) -> OfferingAccess {
    OfferingAccess::new(db, rbac)
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
    SupplierOfferingService::new(db.clone()).with_data_scope(OfferingAccess::new(db, rbac))
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
        .with_data_scope(OfferingAccess::new(db, rbac))
}

fn fresh_work_items(db: Database) -> impl Fn() -> WorkItemService<WorkflowAuth> + Send + Sync {
    move || work_item_service(db.clone(), shared_rbac_service(db.clone()))
}
