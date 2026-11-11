//! 商品授权与业务能力装配。
use erp_catalog::{CatalogAccess, CatalogService};
use erp_identity::SharedRbacService;
use mongodb::Database;

use super::catalog::{MongoCatalogAudit, MongoCatalogFileAssets};

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
/// HTTP 与命名 Process 复用本域固定授权登记。
pub fn catalog_access(db: Database, rbac: SharedRbacService) -> CatalogAccess {
    CatalogAccess::new(db, rbac)
}

/// 装配已接线授权能力 的商品服务。
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
    .with_data_scope(CatalogAccess::new(db, rbac))
}
