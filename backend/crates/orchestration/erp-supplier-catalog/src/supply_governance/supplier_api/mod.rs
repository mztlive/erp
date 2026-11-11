//! 供应连接命令、外部引用解析、审计与后台任务事务。
use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;
use erp_supply::ports::supplier_reference_registry::SupplierReferenceRegistry;
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;

use crate::wiring::CatalogServices;
mod activation;
mod catalog_identity_review;
mod catalog_recovery;
mod command;
mod context;
mod creation;
mod deployment;
mod jobs;
mod receipt;
mod reference;
/// 供应连接跨域命令入口，默认引用注册表保持失败关闭。
pub struct SupplierApiGovernanceProcess {
    db: Database,
    services: Arc<CatalogServices>,
    reference_registry: Arc<dyn SupplierReferenceRegistry>,
    rbac: SharedRbacService,
    gateway: Option<Arc<dyn SupplierApiGateway>>,
}
impl SupplierApiGovernanceProcess {
    /// 绑定应用数据库和必需授权源，未注入的引用元数据保持失败关闭。
    ///
    /// # 参数
    /// * `db` - 应用数据库。
    /// * `rbac` - 应用共享的权威授权源。
    /// * `services` - 已装配的商品和供给对象授权。
    ///
    /// # 返回
    /// 引用注册表为不可用实现的流程。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService, services: Arc<CatalogServices>) -> Self {
        Self {
            db,
            services,
            reference_registry: Arc::new(
                erp_supply::ports::supplier_reference_registry::UnavailableSupplierReferenceRegistry,
            ),
            rbac,
            gateway: None,
        }
    }
    /// 复用组合根的引用注册表；不创建新的外部连接器。消耗 `self`。
    ///
    /// # 参数
    /// * `registry` - 组合根已有的供应商引用注册表。
    ///
    /// # 返回
    /// 换上该注册表后的流程。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_reference_registry(mut self, registry: Arc<dyn SupplierReferenceRegistry>) -> Self {
        self.reference_registry = registry;
        self
    }
    /// 注入只读连接检查网关，开启自动下单时在事务外调用。
    /// # 参数
    /// `gateway` 为已注册的供应商技术网关。
    /// # 返回
    /// 绑定网关后的流程。
    /// # 错误
    /// 不返回错误。
    pub fn with_gateway(mut self, gateway: Arc<dyn SupplierApiGateway>) -> Self {
        self.gateway = Some(gateway);
        self
    }

    fn domain(&self) -> SupplierApiService {
        SupplierApiService::new(self.db.clone())
    }
}
