//! 供给创建/修订/可供更新及供应停止任务的真实跨域根。
use std::sync::Arc;

use erp_supply::ports::offering_qualification::QualificationPort;
use erp_supply::ports::{FailClosedOfferingDataScopePort, OfferingDataScopePort};
use erp_supply::service::supplier_offering::SupplierOfferingService;
use mongodb::Database;

use crate::Error;
pub mod batch;
mod command;
mod commit;
mod exception;
mod qualification;
mod quote_target;
pub use qualification::MongoOfferingQualification;
/// 供给治理流程。资格适配器构造不读取事实。
pub struct SupplierOfferingProcess {
    db: Database,
    qualification: Arc<dyn QualificationPort<Error = Error>>,
    data_scope: Arc<dyn OfferingDataScopePort>,
}
impl SupplierOfferingProcess {
    /// 使用生产 catalog 与 supplier 资格适配器创建流程，数据范围默认失败关闭。
    ///
    /// # 参数
    /// * `db` - 数据库。
    ///
    /// # 返回
    /// 尚未替换数据范围端口的供给流程。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self {
            qualification: Arc::new(MongoOfferingQualification::new(db.clone())),
            data_scope: FailClosedOfferingDataScopePort::shared(),
            db,
        }
    }
    /// 注入供给范围 Port。
    ///
    /// # 参数
    /// * `data_scope` - 组合层装配的公共解析 adapter
    ///
    /// # 返回
    /// 返回绑定范围 Port 的流程。消耗 `self`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_data_scope(mut self, data_scope: Arc<dyn OfferingDataScopePort>) -> Self {
        self.data_scope = data_scope;
        self
    }
    fn domain(&self) -> SupplierOfferingService {
        SupplierOfferingService::new(self.db.clone()).with_data_scope(self.data_scope.clone())
    }
}
