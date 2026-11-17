//! 供应目录消费方的领域服务接线；最终由应用持有。

use std::sync::Arc;

use erp_identity::{AuthorizationService, SharedRbacService};
use erp_supplier_catalog::wiring::CatalogServices;
use mongodb::Database;

use super::workflow::work_item_service;
use super::{
    catalog_access, offering_access, scoped_catalog_service, scoped_offering_service, supplier_access,
};

/// 为供应目录装配同一数据库和身份政策下的消费合同。
/// # 参数
/// db、rbac 为数据库与身份设施，authorization 为进程共享统一授权服务。
/// # 返回
/// 已注入范围、审计、文件和正式任务端口的目录依赖。
/// # 错误
/// 不返回错误；构造不读取或写入事实。
pub fn catalog_services(
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
) -> Arc<CatalogServices> {
    Arc::new(CatalogServices {
        catalog: scoped_catalog_service(db.clone(), authorization.clone()),
        catalog_access: catalog_access(db.clone(), authorization.clone()),
        offerings: scoped_offering_service(db.clone(), authorization.clone()),
        offering_access: offering_access(db.clone(), authorization.clone()),
        suppliers: supplier_access(db.clone(), authorization.clone()),
        work_items: Arc::new(work_item_service(db, rbac, authorization)),
    })
}
