//! 供应连接命令、外部引用解析、审计与后台任务事务。
use erp_identity::SharedRbacService;
use erp_read_models::supplier_center::supplier_api::SupplierApiReadService;
use erp_supply::ports::supplier_reference_registry::SupplierReferenceRegistry;
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;
use std::sync::Arc;
mod command;
mod context;
mod creation;
mod jobs;
mod receipt;
mod reference;
/// 供应连接跨域命令入口，默认引用注册表保持失败关闭。
pub struct SupplierApiGovernanceProcess {
    db: Database,
    reference_registry: Arc<dyn SupplierReferenceRegistry>,
    rbac: Option<SharedRbacService>,
}
impl SupplierApiGovernanceProcess {
    /// 复用应用数据库，未注入的授权和引用元数据保持失败关闭。
    pub fn new(db: Database) -> Self {
        Self {
            db,
            reference_registry: Arc::new(crate::adapters::supplier_api::UnavailableSupplierReferenceRegistry),
            rbac: None,
        }
    }
    /// 注入应用已有的权威 RBAC。
    pub fn with_rbac(mut self, rbac: SharedRbacService) -> Self {
        self.rbac = Some(rbac);
        self
    }
    /// 复用组合根的引用注册表；不创建新的外部连接器。
    pub fn with_reference_registry(mut self, registry: Arc<dyn SupplierReferenceRegistry>) -> Self {
        self.reference_registry = registry;
        self
    }
    fn domain(&self) -> SupplierApiService {
        SupplierApiService::new(self.db.clone())
    }
    fn reads(&self) -> SupplierApiReadService {
        let service = SupplierApiReadService::new(self.db.clone())
            .with_reference_registry(Arc::clone(&self.reference_registry));
        match &self.rbac {
            Some(rbac) => service.with_rbac(rbac.clone()),
            None => service,
        }
    }
}
