//! 供给服务的组合根装配。
use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_supplier_catalog::supply_governance::SupplierOfferingProcess;
use erp_supply::{OfferingAccess, SupplierOfferingService};
use erp_workflow::service::work_item::WorkItemService;
use mongodb::Database;

use super::identity::shared_rbac_service;
use super::workflow::{WorkflowAuth, work_item_service};

/// 构造绑定身份数据库的供给访问器。
/// 返回已注入本 adapter 的供给访问器。
///
/// 授权由本域直接调用 identity，组合根仅注入身份设施。
pub fn offering_access(db: Database, authorization: Arc<AuthorizationService>) -> OfferingAccess {
    OfferingAccess::new(db, authorization)
}

/// 装配已接线范围 Port 的供给服务。
/// 返回注入生产范围 adapter 的供给服务。
pub fn scoped_offering_service(
    db: Database,
    authorization: Arc<AuthorizationService>,
) -> SupplierOfferingService {
    SupplierOfferingService::new(db.clone()).with_data_scope(OfferingAccess::new(db, authorization))
}

/// 装配已接线范围 Port 的供给治理流程。
/// 返回注入生产范围 adapter 的供给流程。
pub fn scoped_offering_process(
    db: Database,
    authorization: Arc<AuthorizationService>,
) -> SupplierOfferingProcess {
    let work_items = fresh_work_items(db.clone(), authorization.clone());
    SupplierOfferingProcess::new(db.clone(), Arc::new(move || Arc::new(work_items())))
        .with_data_scope(OfferingAccess::new(db, authorization))
}

fn fresh_work_items(
    db: Database,
    authorization: Arc<AuthorizationService>,
) -> impl Fn() -> WorkItemService<WorkflowAuth> + Send + Sync {
    move || work_item_service(db.clone(), shared_rbac_service(db.clone()), authorization.clone())
}
