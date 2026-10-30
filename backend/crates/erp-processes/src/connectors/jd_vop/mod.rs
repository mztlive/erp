//! 京东 VOP 供应商适配；未核实认证协议时不声明远端能力。
use std::sync::Arc;

use erp_core::ids::SupplierApiConnectionId;
use erp_party::SensitiveDataCodec;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::address::{AddressRequirement, SupplierAddressSource};
use erp_supply::ports::connector::common::ConnectorError;
use erp_supply::ports::connector::funds::OrderFundingRequirement;
use erp_supply::ports::connector::offer::OfferSource;
use erp_supply::ports::supplier_api_gateway::ClassifiedError;

mod address;
mod catalog;
pub mod credential;
mod delivery;
mod funds;
pub mod http_transport;
mod logistics;
mod messages;
pub mod oauth;
mod oauth_http;
mod order;
mod order_dispatch;
mod order_mapping;
mod order_payload;
mod preflight;
mod receipt;
pub mod request_encoding;
mod runtime;
pub mod signing;
pub mod token;
mod transform;
pub mod transport;
pub mod wire;
mod wire_funds;
mod wire_logistics;
mod wire_messages;
pub use funds::{JdVopFundsProfile, JdVopFundsSign, JdVopFundsTimeFormat, JdVopFundsTradeRule};
mod wire_order;
use erp_supply::ports::connector::funds::SupplierFundsSource;
use erp_supply::ports::connector::messages::SupplierMessageSource;
use erp_supply::ports::connector::offer::DeliverySource;
use erp_supply::ports::connector::order::{Cancellations, OrderConfirmation, Orders};
use erp_supply::ports::connector::preflight::OrderPreflight;
use erp_supply::ports::connector::receipt::OrderReceiptConfirmation;
pub use order_payload::{
    JdVopConfirmationFundsPolicy, JdVopExecutionProfile, JdVopInvoiceProfile, JdVopInvoiceRecipient,
};
pub use runtime::JdVopRuntime;
pub use transform::{JdVopOffer, JdVopOfferMapper};
use transport::{ReadClient, WriteClient};

/// 绑定业务连接及账号配置的协议客户端，公共流程负责所有持久化写入。
pub struct JdVopConnector {
    connection_id: SupplierApiConnectionId,
    account_scope: String,
    binding: String,
    cursor_secret: String,
    reader: Option<Arc<ReadClient>>,
    mapper: JdVopOfferMapper,
    writer: Option<Arc<WriteClient>>,
    profile: Option<Arc<JdVopExecutionProfile>>,
    evidence_codec: Option<Arc<SensitiveDataCodec>>,
}

impl SupplierConnector for JdVopConnector {
    fn connection_id(&self) -> &SupplierApiConnectionId {
        &self.connection_id
    }
    fn offers(&self) -> Option<&dyn OfferSource> {
        self.reader.as_ref().map(|_| self as &dyn OfferSource)
    }
    fn address_requirement(&self) -> AddressRequirement {
        AddressRequirement::FourLevel
    }
    fn address_source(&self) -> Option<&dyn SupplierAddressSource> {
        self.reader.as_ref().map(|_| self as &dyn SupplierAddressSource)
    }
    fn funding_requirement(&self) -> OrderFundingRequirement {
        OrderFundingRequirement::VerifiedSupplierDeposit
    }
    fn funds_account_reference(&self) -> Option<&str> {
        self.profile.as_ref().map(|profile| profile.verified_external_funds_account_ref.as_str())
    }
    fn orders(&self) -> Option<&dyn Orders> {
        self.writer.as_ref().map(|_| self as &dyn Orders)
    }
    fn order_confirmation(&self) -> Option<&dyn OrderConfirmation> {
        self.supports_creation().then_some(self as &dyn OrderConfirmation)
    }
    fn receipt_confirmation(&self) -> Option<&dyn OrderReceiptConfirmation> {
        self.writer.as_ref().map(|_| self as &dyn OrderReceiptConfirmation)
    }
    fn cancellations(&self) -> Option<&dyn Cancellations> {
        self.writer.as_ref().map(|_| self as &dyn Cancellations)
    }
    fn delivery(&self) -> Option<&dyn DeliverySource> {
        self.profile.as_ref().map(|_| self as &dyn DeliverySource)
    }
    fn preflight(&self) -> Option<&dyn OrderPreflight> {
        self.profile.as_ref().map(|_| self as &dyn OrderPreflight)
    }
    fn messages(&self) -> Option<&dyn SupplierMessageSource> {
        (self.writer.is_some()
            && self.profile.as_ref().is_some_and(|profile| profile.verified_message_types.is_some()))
        .then_some(self as &dyn SupplierMessageSource)
    }
    fn funds_source(&self) -> Option<&dyn SupplierFundsSource> {
        self.funds_profile().is_ok().then_some(self as &dyn SupplierFundsSource)
    }
}

fn error(class: SupplierFailureClass, code: &str, summary: &str) -> ConnectorError {
    ConnectorError { class, code: code.into(), summary: summary.into(), retry_after: None }
}

fn mapping(summary: &str) -> ConnectorError {
    error(SupplierFailureClass::MappingError, "JD_VOP_MAPPING_INVALID", summary)
}

fn protocol_required() -> ConnectorError {
    error(
        SupplierFailureClass::CapabilityGap,
        "JD_VOP_PROTOCOL_REQUIRED",
        "京东认证协议及账号授权尚未核验，请联系管理员补齐接入资料",
    )
}

fn classified(error: ConnectorError) -> ClassifiedError {
    ClassifiedError { class: error.class, code: error.code, summary: error.summary }
}
