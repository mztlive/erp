//! 供应商可选退款与结算能力的公共处理器；外部 I/O 与领域事务分离。
mod refund;
mod refund_dispatch;
mod refund_result;
mod refund_store;
mod refund_validation;
mod settlement;
pub(crate) mod settlement_source;

use std::sync::Arc;

use erp_supply::ports::connector::SupplierConnector;
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::connectors::registry::SupplierConnectorRegistry;
use crate::{Error, Result};

#[derive(Clone)]
pub struct SupplierAfterSalesProcess {
    db: Database,
    registry: Arc<SupplierConnectorRegistry>,
    cursor: Arc<tokio::sync::Mutex<String>>,
}
impl SupplierAfterSalesProcess {
    /// 装配统一售后处理器，不在构造时执行 I/O。
    /// # 参数
    /// 数据库与应用共享协议注册表。
    /// # 返回
    /// 可共享的处理器。
    /// # 错误
    /// 无。
    pub fn new(db: Database, registry: Arc<SupplierConnectorRegistry>) -> Self {
        Self { db, registry, cursor: Arc::new(tokio::sync::Mutex::new(String::new())) }
    }

    async fn connector(&self, connection: &str, supplier: &str) -> Result<Arc<dyn SupplierConnector>> {
        let bound =
            SupplierApiService::new(self.db.clone()).load_connection(connection, &mut NoTransaction).await?;
        if bound.supplier_id.as_ref() != supplier {
            return Err(Error::Forbidden("售后连接不属于原供应商".into()));
        }
        self.registry.after_sales_connector(&bound).map_err(|e| Error::BusinessLogicError(e.summary))
    }
}
