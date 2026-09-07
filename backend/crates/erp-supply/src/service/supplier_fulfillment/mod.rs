//! 供应商履约本域读取、状态、受控证据与调用方执行器内写入。
pub mod cancel;
pub mod complete;
pub mod investigate;
pub mod mapping;
pub mod place;
pub mod query;
pub mod receipt;
pub mod refund_result;
pub mod reject;
use mongodb::Database;
/// W26 正式待办绑定的供应商履约订单业务对象类型。
pub const W26_BUSINESS_OBJECT_TYPE: &str = "SUPPLIER_FULFILLMENT_ORDER";
/// 本域服务；不持外部网关、工作项或审计能力。
pub struct SupplierFulfillmentService {
    db: Database,
}
impl SupplierFulfillmentService {
    /// 绑定本域数据访问；无外部调用或授权。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[cfg(test)]
mod boundary_tests;
