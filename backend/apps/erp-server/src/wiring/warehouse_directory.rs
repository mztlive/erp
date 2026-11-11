//! Warehouse 目录的身份授权装配。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::{Permission, SharedRbacService};
use erp_warehouse::Result;
use erp_warehouse::authorization::{WarehouseAuthorization, WarehouseQuery};
use erp_warehouse::ports::WarehouseDirectoryAccess;
use erp_warehouse::service::directory::WarehouseDirectoryService;
use mongodb::Database;
use persistence_core::Executor;

use super::identity_error::map_identity_error;
map_identity_error!(erp_warehouse);

struct Access(WarehouseAuthorization);
#[async_trait]
impl WarehouseDirectoryAccess for Access {
    async fn resolve(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<WarehouseQuery> {
        self.0.query(actor, executor).await.map_err(map_identity_error)
    }
}

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
        Arc::new(Access(
            WarehouseAuthorization::new(
                db,
                rbac,
                Permission::parse("warehouse:list").expect("固定权限"),
                Vec::new(),
            )
            .expect("仓库目录登记完整"),
        )),
    )
}
