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

use crate::entity::supplier_api::{SupplierApiCapabilityCode, SupplierApiConnection};
use crate::ports::supplier_reference_registry::{SupplierReferenceRegistry, SupplierReferenceTarget};
use crate::ports::{
    FailClosedFulfillmentExceptionHandlerPort, FailClosedFulfillmentOrderDataScopePort,
    FulfillmentExceptionHandlerPort, FulfillmentOrderDataScopePort,
};
use crate::{Error, Result};
/// W26 正式待办绑定的供应商履约订单业务对象类型。
pub const W26_BUSINESS_OBJECT_TYPE: &str = "SUPPLIER_FULFILLMENT_ORDER";
/// 本域服务；不持外部网关、工作项或审计能力。
pub struct SupplierFulfillmentService {
    pub(crate) db: Database,
    registry: Option<Arc<dyn SupplierReferenceRegistry>>,
    data_scope: Arc<dyn FulfillmentOrderDataScopePort>,
    handlers: Arc<dyn FulfillmentExceptionHandlerPort>,
}
impl SupplierFulfillmentService {
    /// 绑定本域数据访问；范围 Port 缺省失败关闭。
    ///
    /// # 参数
    /// * `db` - 履约集合所在数据库。
    ///
    /// # 返回
    /// 返回范围 Port 与处理人 Port 均为失败关闭的服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self {
            db,
            registry: None,
            data_scope: FailClosedFulfillmentOrderDataScopePort::shared(),
            handlers: FailClosedFulfillmentExceptionHandlerPort::shared(),
        }
    }

    /// 注入组合根已有的适配器能力来源。
    /// # 参数
    /// `registry` 为已注册的供应商适配器。
    /// # 返回
    /// 已绑定能力来源的服务。
    /// # 错误
    /// 不返回错误。
    pub fn with_registry(mut self, registry: Arc<dyn SupplierReferenceRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// 读取当前适配器实际支持的操作。
    /// # 参数
    /// `connection` 为已读取的连接身份。
    /// # 返回
    /// 不受自动下单开关影响的能力清单。
    /// # 错误
    /// 适配器未装配或身份不符时拒绝。
    pub fn capabilities(&self, connection: &SupplierApiConnection) -> Result<Vec<SupplierApiCapabilityCode>> {
        let registry =
            self.registry.as_ref().ok_or_else(|| Error::BusinessLogicError("供应商接口未配置".into()))?;
        registry
            .capabilities(&SupplierReferenceTarget::from(connection))
            .map_err(|error| Error::BusinessLogicError(error.summary))
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
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回绑定当前数据库和范围 Port 的 `FulfillmentOrderAccess`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn access(&self) -> FulfillmentOrderAccess {
        FulfillmentOrderAccess::new(self.db.clone(), self.data_scope.clone())
    }

    /// 当前开放 W26 处理人端口。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回处理人 Port 的共享句柄。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn handlers(&self) -> Arc<dyn FulfillmentExceptionHandlerPort> {
        self.handlers.clone()
    }
}

pub use access::{FulfillmentOrderAccess, fulfillment_order_scope};

#[cfg(test)]
mod boundary_tests;
