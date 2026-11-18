//! 供给创建/修订/可供更新及供应停止任务的真实跨域根。
use std::sync::Arc;

use erp_supply::OfferingAccess;
use erp_supply::ports::offering_qualification::QualificationPort;
use erp_supply::service::supplier_offering::SupplierOfferingService;
use mongodb::Database;

use crate::Error;
use crate::wiring::CatalogWorkItems;
pub mod batch;
mod command;
mod commit;
mod exception;
mod qualification;
mod quote_target;
mod recovery;
pub use qualification::MongoOfferingQualification;
/// 供给治理流程。资格适配器构造不读取事实。
pub struct SupplierOfferingProcess {
    fresh_work_items: Arc<dyn Fn() -> Arc<dyn CatalogWorkItems> + Send + Sync>,
    db: Database,
    qualification: Arc<dyn QualificationPort<Error = Error>>,
    data_scope: OfferingAccess,
}
impl SupplierOfferingProcess {
    /// 使用生产 catalog 与 supplier 资格适配器创建流程，数据范围默认失败关闭。
    ///
    /// `fresh_work_items`：每个命令创建独立身份的正式任务操作工厂。
    ///
    /// 尚未替换数据授权访问器的供给流程。
    pub fn new(
        db: Database,
        fresh_work_items: Arc<dyn Fn() -> Arc<dyn CatalogWorkItems> + Send + Sync>,
    ) -> Self {
        Self {
            fresh_work_items,
            qualification: Arc::new(MongoOfferingQualification::new(db.clone())),
            data_scope: OfferingAccess::unwired(db.clone()),
            db,
        }
    }
    /// 注入供给授权访问器。
    ///
    /// `data_scope`：组合层装配的本域授权访问器。
    ///
    /// 返回绑定授权访问器 的流程。消耗 `self`。
    pub fn with_data_scope(mut self, data_scope: OfferingAccess) -> Self {
        self.data_scope = data_scope;
        self
    }
    fn domain(&self) -> SupplierOfferingService {
        SupplierOfferingService::new(self.db.clone()).with_data_scope(self.data_scope.clone())
    }
}
