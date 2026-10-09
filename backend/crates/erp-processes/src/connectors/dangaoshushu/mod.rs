//! 蛋糕叔叔新版协议：绑定连接、一次发送、独立创建与支付、推送只作补查线索。
use std::sync::Arc;

use config::DangaoshushuConfig;
use erp_core::ids::SupplierApiConnectionId;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::callback::Callbacks;
use erp_supply::ports::connector::common::{ConnectorError, ConnectorResult};
use erp_supply::ports::connector::offer::{AvailabilitySource, DeliverySource, OfferSource, ServiceAreas};
use erp_supply::ports::connector::order::{Orders, PaymentConfirmation};

mod callback;
mod catalog;
mod delivery;
mod order;
mod parsing;
mod proof;
mod runtime;
mod signing;
mod transport;
mod wire;
pub use runtime::{DangaoshushuRuntime, SupplierReferenceTickets};
mod read;
pub mod reception;
pub use read::{DangaoshushuReadKind, DangaoshushuReadQuery};
use transport::{HttpTransport, Transport};

/// 只转换供应商协议；业务写入由持久化各步骤的 Process 承担。
pub struct DangaoshushuConnector {
    connection_id: SupplierApiConnectionId,
    settings: DangaoshushuConfig,
    binding: String,
    transport: Arc<dyn Transport>,
}

impl DangaoshushuConnector {
    /// 从 SafeConfig 提供的配置构建连接，不发起供应商请求。
    ///
    /// # 参数
    /// `settings` 是已绑定 ERP 连接与供应商的服务端配置。
    /// # 返回
    /// 一个可共享的具体 Connector。
    /// # 错误
    /// 未启用、配置无效或 HTTP 客户端构建失败时返回脱敏分类错误。
    pub fn new(settings: DangaoshushuConfig) -> ConnectorResult<Self> {
        settings.validate().map_err(|_| {
            error(SupplierFailureClass::MappingError, "DGSS_CONFIG_INVALID", "蛋糕叔叔配置无效")
        })?;
        if !settings.enabled {
            return Err(error(SupplierFailureClass::CapabilityGap, "DGSS_DISABLED", "蛋糕叔叔连接未启用"));
        }
        let transport = Arc::new(HttpTransport::new(&settings)?);
        Ok(Self {
            connection_id: SupplierApiConnectionId::new(settings.connection_id.clone()),
            binding: proof::configuration_hash(&settings)?,
            settings,
            transport,
        })
    }

    #[cfg(test)]
    fn with_transport(settings: DangaoshushuConfig, transport: Arc<dyn Transport>) -> Self {
        Self {
            connection_id: SupplierApiConnectionId::new(settings.connection_id.clone()),
            binding: proof::configuration_hash(&settings).unwrap(),
            settings,
            transport,
        }
    }
}

impl SupplierConnector for DangaoshushuConnector {
    fn connection_id(&self) -> &SupplierApiConnectionId {
        &self.connection_id
    }
    fn offers(&self) -> Option<&dyn OfferSource> {
        Some(self)
    }
    fn availability(&self) -> Option<&dyn AvailabilitySource> {
        Some(self)
    }
    fn service_areas(&self) -> Option<&dyn ServiceAreas> {
        Some(self)
    }
    fn delivery(&self) -> Option<&dyn DeliverySource> {
        Some(self)
    }
    fn orders(&self) -> Option<&dyn Orders> {
        Some(self)
    }
    fn payment_confirmation(&self) -> Option<&dyn PaymentConfirmation> {
        Some(self)
    }
    fn callbacks(&self) -> Option<&dyn Callbacks> {
        Some(self)
    }
}

fn error(class: SupplierFailureClass, code: &str, summary: &str) -> ConnectorError {
    ConnectorError { class, code: code.into(), summary: summary.into(), retry_after: None }
}

#[cfg(test)]
mod test_support;
