//! Warehouse 目录的身份授权装配。
use erp_identity::{Permission, SharedRbacService};
use erp_warehouse::authorization::WarehouseAuthorization;
use erp_warehouse::service::directory::WarehouseDirectoryService;
use mongodb::Database;

/// 装配独立目录，不附加任何业务结果条件。
/// # 参数
/// `db` 为数据库，`rbac` 为公共权限服务。
/// # 返回
/// 领域目录服务。
/// # 错误
/// 无；授权失败由读取入口返回。
pub fn warehouse_directory(db: Database, rbac: SharedRbacService) -> WarehouseDirectoryService {
    WarehouseDirectoryService::new(
        db.clone(),
        WarehouseAuthorization::new(
            db,
            rbac,
            Permission::parse("warehouse:list").expect("固定权限"),
            Vec::new(),
        )
        .expect("仓库目录登记完整"),
    )
}
