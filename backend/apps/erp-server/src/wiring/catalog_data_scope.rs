//! 商品授权与业务能力装配。
use std::sync::Arc;

use erp_catalog::{CatalogAccess, CatalogService};
use erp_identity::AuthorizationService;
use mongodb::Database;

use super::catalog::{MongoCatalogAudit, MongoCatalogFileAssets};

/// 构造绑定身份数据库的商品访问器。
/// 返回已注入本 adapter 的商品访问器。
///
/// HTTP 与业务流程必须复用进程共享统一授权服务。
pub fn catalog_access(db: Database, authorization: Arc<AuthorizationService>) -> CatalogAccess {
    CatalogAccess::new(db, authorization)
}

/// 装配已接线授权能力 的商品服务。
/// 返回注入生产范围 adapter 的商品服务。
pub fn scoped_catalog_service(db: Database, authorization: Arc<AuthorizationService>) -> CatalogService {
    CatalogService::new(
        db.clone(),
        MongoCatalogAudit::shared(db.clone()),
        MongoCatalogFileAssets::shared(db.clone()),
        CatalogAccess::new(db, authorization),
    )
}
