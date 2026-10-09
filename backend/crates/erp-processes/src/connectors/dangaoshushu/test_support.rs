use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use config::DangaoshushuConfig;
use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
use erp_supply::entity::supplier_api::ConnectionEnvironment;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::supplier_reference_registry::SupplierReferenceTarget;
use serde_json::{Value, from_value, json};

use super::DangaoshushuConnector;
use super::transport::{Request, Transport};

pub(super) struct FakeTransport {
    pub responses: Mutex<Vec<ConnectorResult<Value>>>,
    pub requests: Mutex<Vec<Request>>,
}

#[async_trait]
impl Transport for FakeTransport {
    async fn send(&self, request: Request) -> ConnectorResult<Value> {
        self.requests.lock().unwrap().push(request);
        self.responses.lock().unwrap().remove(0)
    }
}

pub(super) fn settings() -> DangaoshushuConfig {
    from_value(json!({"enabled":true,"channel_no":"test-channel","private_key":"test-key","user_id":"user-1","spec_units":{"spec-1":"个"},"city_regions":{"2":"110100"},"clearing_price_is_tax_inclusive_cny":true})).unwrap()
}
pub(super) fn connector(responses: Vec<Value>) -> DangaoshushuConnector {
    DangaoshushuConnector::with_transport(
        settings(),
        Arc::new(FakeTransport {
            responses: Mutex::new(responses.into_iter().map(Ok).collect()),
            requests: Mutex::new(vec![]),
        }),
    )
}

pub(super) fn recording_connector(
    responses: Vec<ConnectorResult<Value>>,
) -> (DangaoshushuConnector, Arc<FakeTransport>) {
    let transport =
        Arc::new(FakeTransport { responses: Mutex::new(responses), requests: Mutex::new(vec![]) });
    (DangaoshushuConnector::with_transport(settings(), transport.clone()), transport)
}

pub(super) fn target() -> SupplierReferenceTarget {
    SupplierReferenceTarget {
        connection_id: SupplierApiConnectionId::new("connection-1"),
        supplier_id: SupplierAccountId::new("supplier-1"),
        environment: ConnectionEnvironment::Testing,
    }
}
