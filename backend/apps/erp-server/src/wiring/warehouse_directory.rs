//! 仓库目录复用进程统一授权服务。
use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_permissions::warehouse;
use erp_warehouse::authorization::WarehouseAuthorization;
use erp_warehouse::service::directory::WarehouseDirectoryService;
use mongodb::Database;

/// 装配仓库目录的完整授权读取入口。
/// # 参数
/// db 为数据库，authorization 为进程共享统一授权服务。
/// # 返回
/// 仓库领域目录服务。
/// # 错误
/// 构造不执行 I/O；授权错误由读取入口返回。
pub fn warehouse_directory(
    db: Database,
    authorization: Arc<AuthorizationService>,
) -> WarehouseDirectoryService {
    WarehouseDirectoryService::new(
        db.clone(),
        WarehouseAuthorization::new(db, authorization, warehouse::LIST),
    )
}
