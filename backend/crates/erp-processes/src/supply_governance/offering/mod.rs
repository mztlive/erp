//! 供给创建/修订/可供更新及供应停止任务的真实跨域根。
use std::sync::Arc;

use erp_supply::ports::offering_qualification::QualificationPort;
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
}
impl SupplierOfferingProcess {
    /// 使用生产catalog/supplier资格适配器创建流程。
    pub fn new(db: Database) -> Self {
        Self { qualification: Arc::new(MongoOfferingQualification::new(db.clone())), db }
    }
    /// 注入供给消费方资格Port，保留调用时点和错误类别。
    pub fn with_qualification(mut self, qualification: Arc<dyn QualificationPort<Error = Error>>) -> Self {
        self.qualification = qualification;
        self
    }
    fn domain(&self) -> SupplierOfferingService {
        SupplierOfferingService::new(self.db.clone())
    }
}
