//! 供应连接详情、动作权限和后台任务只读投影。
use erp_identity::SharedRbacService;
use erp_supply::{
    ports::supplier_reference_registry::SupplierReferenceRegistry, service::supplier_api::SupplierApiService,
};
use mongodb::Database;
use std::sync::Arc;
mod context;
pub mod dto;
mod jobs;
mod query;
/// 供应连接跨域只读入口。
pub struct SupplierApiReadService {
    db: Database,
    rbac: Option<SharedRbacService>,
    reference_registry: Option<Arc<dyn SupplierReferenceRegistry>>,
}
impl SupplierApiReadService {
    /// 复用应用数据库，未注入的授权和引用元数据保持失败关闭。
    pub fn new(db: Database) -> Self {
        Self {
            db,
            rbac: None,
            reference_registry: None,
        }
    }
    /// 注入应用已有的权威 RBAC。
    pub fn with_rbac(mut self, rbac: SharedRbacService) -> Self {
        self.rbac = Some(rbac);
        self
    }
    /// 复用组合根的引用注册表；不创建新的外部连接器。
    pub fn with_reference_registry(mut self, registry: Arc<dyn SupplierReferenceRegistry>) -> Self {
        self.reference_registry = Some(registry);
        self
    }
    fn domain(&self) -> SupplierApiService {
        SupplierApiService::new(self.db.clone())
    }
}
