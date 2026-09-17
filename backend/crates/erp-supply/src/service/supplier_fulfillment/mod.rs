//! 供应商履约本域读取、状态、受控证据与调用方执行器内写入。
pub mod access;
pub mod cancel;
pub mod complete;
pub mod handover;
pub mod investigate;
pub mod mapping;
pub mod place;
pub mod query;
pub mod receipt;
pub mod refund_result;
pub mod reject;
use std::sync::Arc;

use mongodb::Database;

use crate::ports::{
    FailClosedFulfillmentExceptionHandlerPort, FailClosedFulfillmentOrderDataScopePort,
    FulfillmentExceptionHandlerPort, FulfillmentOrderDataScopePort,
};
/// W26 正式待办绑定的供应商履约订单业务对象类型。
pub const W26_BUSINESS_OBJECT_TYPE: &str = "SUPPLIER_FULFILLMENT_ORDER";
/// 本域服务；不持外部网关、工作项或审计能力。
pub struct SupplierFulfillmentService {
    pub(crate) db: Database,
    data_scope: Arc<dyn FulfillmentOrderDataScopePort>,
    handlers: Arc<dyn FulfillmentExceptionHandlerPort>,
}
impl SupplierFulfillmentService {
    /// 绑定本域数据访问；范围 Port 缺省失败关闭。
    pub fn new(db: Database) -> Self {
        Self {
            db,
            data_scope: FailClosedFulfillmentOrderDataScopePort::shared(),
            handlers: FailClosedFulfillmentExceptionHandlerPort::shared(),
        }
    }

    /// 注入履约订单范围与 W26 处理人 Port。
    ///
    /// # 参数
    /// * `data_scope` - 组合层装配的公共解析 adapter
    /// * `handlers` - 当前开放 W26 处理人事实
    ///
    /// # 返回
    /// 返回绑定范围 Port 的服务。
    ///
    /// # 错误
    /// 无。
    pub fn with_scope(
        mut self,
        data_scope: Arc<dyn FulfillmentOrderDataScopePort>,
        handlers: Arc<dyn FulfillmentExceptionHandlerPort>,
    ) -> Self {
        self.data_scope = data_scope;
        self.handlers = handlers;
        self
    }

    /// 构造本域范围访问器。
    pub fn access(&self) -> FulfillmentOrderAccess {
        FulfillmentOrderAccess::new(self.db.clone(), self.data_scope.clone())
    }

    /// 当前开放 W26 处理人端口。
    pub fn handlers(&self) -> Arc<dyn FulfillmentExceptionHandlerPort> {
        self.handlers.clone()
    }
}

pub use access::{FulfillmentOrderAccess, fulfillment_order_scope};

#[cfg(test)]
mod boundary_tests;
