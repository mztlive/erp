//! 供给创建/修订/可供更新及供应停止任务的真实跨域根。
use std::sync::Arc;

use erp_supply::ports::offering_qualification::QualificationPort;
use erp_supply::ports::{FailClosedOfferingDataScopePort, OfferingDataScopePort};
use erp_supply::service::supplier_offering::SupplierOfferingService;
use mongodb::Database;

use crate::Error;
mod command;
mod commit;
mod exception;
mod qualification;
pub use qualification::MongoOfferingQualification;
/// 供给治理流程。资格适配器构造不读取事实。
pub struct SupplierOfferingProcess {
    db: Database,
    qualification: Arc<dyn QualificationPort<Error = Error>>,
    data_scope: Arc<dyn OfferingDataScopePort>,
}
impl SupplierOfferingProcess {
    /// 使用生产catalog/supplier资格适配器创建流程。
    pub fn new(db: Database) -> Self {
        Self {
            qualification: Arc::new(MongoOfferingQualification::new(db.clone())),
            data_scope: FailClosedOfferingDataScopePort::shared(),
            db,
        }
    }
    /// 注入供给消费方资格Port，保留调用时点和错误类别。
    pub fn with_qualification(mut self, qualification: Arc<dyn QualificationPort<Error = Error>>) -> Self {
        self.qualification = qualification;
        self
    }
    /// 注入供给范围 Port。
    ///
    /// # 参数
    /// * `data_scope` - 组合层装配的公共解析 adapter
    ///
    /// # 返回
    /// 返回绑定范围 Port 的流程。
    pub fn with_data_scope(mut self, data_scope: Arc<dyn OfferingDataScopePort>) -> Self {
        self.data_scope = data_scope;
        self
    }
    fn domain(&self) -> SupplierOfferingService {
        SupplierOfferingService::new(self.db.clone()).with_data_scope(self.data_scope.clone())
    }
}
