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
use erp_supply::ports::supplier_reference_registry::SupplierReferenceTarget;

mod callback;
mod catalog;
mod catalog_details;
mod catalog_scan;
mod categories;
mod delivery;
mod order;
mod parsing;
mod proof;
mod runtime;
mod signing;
mod transform;
mod transport;
pub use transform::DangaoshushuOffer;
mod wire;
pub use runtime::{DangaoshushuRuntime, SupplierReferenceTickets};
mod read;
pub mod reception;
pub use read::{DangaoshushuReadKind, DangaoshushuReadQuery};
use transport::Transport;

/// 只转换供应商协议；业务写入由持久化各步骤的 Process 承担。
pub struct DangaoshushuConnector {
    connection_id: SupplierApiConnectionId,
    settings: DangaoshushuConfig,
    binding: String,
    transport: Arc<dyn Transport>,
}

impl DangaoshushuConnector {
    fn bind(
        settings: DangaoshushuConfig,
        target: &SupplierReferenceTarget,
        transport: Arc<dyn Transport>,
    ) -> ConnectorResult<Self> {
        Ok(Self {
            connection_id: target.connection_id.clone(),
            binding: proof::binding_hash(&settings, target)?,
            settings,
            transport,
        })
    }

    #[cfg(test)]
    fn with_transport(settings: DangaoshushuConfig, transport: Arc<dyn Transport>) -> Self {
        Self::bind(settings, &test_support::target(), transport).unwrap()
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

#[cfg(test)]
mod tests {
    use erp_supply::entity::supplier_api::SupplierApiCapabilityCode;
    use erp_supply::ports::connector::SupplierConnector;

    use super::test_support;

    /// 治理能力声明必须与适配器实际实现的窄 trait 保持一致。
    #[test]
    fn declared_capabilities_match_implemented_connector_traits() {
        let connector = test_support::connector(vec![]);
        let port: &dyn SupplierConnector = &connector;
        let declared = port.capabilities();

        for (implemented, code, label) in [
            (port.offers().is_some(), SupplierApiCapabilityCode::Product, "商品"),
            (port.offers().is_some(), SupplierApiCapabilityCode::Price, "价格"),
            (port.availability().is_some(), SupplierApiCapabilityCode::Stock, "库存"),
            (port.service_areas().is_some(), SupplierApiCapabilityCode::ServiceAreas, "可售地区"),
            (port.delivery().is_some(), SupplierApiCapabilityCode::Delivery, "配送"),
            (
                port.payment_confirmation().is_some(),
                SupplierApiCapabilityCode::PaymentConfirmation,
                "履约通知",
            ),
            (port.orders().is_some(), SupplierApiCapabilityCode::Order, "下单"),
            (port.orders().is_some(), SupplierApiCapabilityCode::Query, "查询"),
            (port.callbacks().is_some(), SupplierApiCapabilityCode::Callback, "回调"),
        ] {
            assert_eq!(implemented, declared.contains(&code), "{label}能力声明与适配器实现不一致");
        }
        assert_eq!(declared.iter().collect::<std::collections::HashSet<_>>().len(), declared.len());
        assert!(!declared.contains(&SupplierApiCapabilityCode::Cancel));
        assert!(!declared.contains(&SupplierApiCapabilityCode::Refund));
        assert!(!declared.contains(&SupplierApiCapabilityCode::Settlement));
        assert!(port.cancellations().is_none());
        assert!(port.refunds().is_none());
        assert!(port.settlements().is_none());
    }
}
