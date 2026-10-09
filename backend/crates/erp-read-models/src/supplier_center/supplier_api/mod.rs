//! 供应连接详情、动作权限和后台任务只读投影。
use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_supply::ports::supplier_reference_registry::SupplierReferenceRegistry;
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;
mod context;
pub mod dto;
mod jobs;
mod query;
mod references;
/// 供应连接跨域只读入口。
pub struct SupplierApiReadService {
    db: Database,
    rbac: Option<SharedRbacService>,
    reference_registry: Option<Arc<dyn SupplierReferenceRegistry>>,
}
impl SupplierApiReadService {
    /// 复用应用数据库，未注入的授权和引用元数据保持失败关闭。
    ///
    /// # 参数
    /// * `db` - 应用数据库。
    ///
    /// # 返回
    /// 返回未注入 RBAC 和引用注册表的读取服务。构造不执行查询。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db, rbac: None, reference_registry: None }
    }
    /// 注入应用已有的权威 RBAC。
    ///
    /// # 参数
    /// * `rbac` - 应用已有的 RBAC 服务。
    ///
    /// # 返回
    /// 消耗当前服务并返回已写入 `rbac` 的同一服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_rbac(mut self, rbac: SharedRbacService) -> Self {
        self.rbac = Some(rbac);
        self
    }
    /// 复用组合根的引用注册表；不创建新的外部连接器。
    ///
    /// # 参数
    /// * `registry` - 组合根的引用注册表。
    ///
    /// # 返回
    /// 消耗当前服务并返回已写入注册表的同一服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_reference_registry(mut self, registry: Arc<dyn SupplierReferenceRegistry>) -> Self {
        self.reference_registry = Some(registry);
        self
    }
    fn domain(&self) -> SupplierApiService {
        SupplierApiService::new(self.db.clone())
    }
}
