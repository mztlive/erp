//! 应用组合层离线合同：真实京东协议经消息编排持久化后才能确认原消息。

use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use config::JdVopConfig;
use erp_connector_jd_vop::JdVopRuntime;
use erp_connector_jd_vop::transport::{
    JdVopReadRequest, JdVopReadTransport, JdVopWriteRequest, JdVopWriteTransport,
};
use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
use erp_integration::entity::supplier_message::{SupplierMessageState, SupplierPulledMessage};
use erp_party::SensitiveDataCodec;
use erp_supplier_orders::supplier_messages::{
    SupplierMessageProcess, SupplierMessageRefreshApplier, SupplierMessageStore,
};
use erp_supplier_orders::{Error, Result};
use erp_supply::connector_registry::SupplierConnectorRegistry;
use erp_supply::entity::supplier_api::{
    SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierConnectorType,
};
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::supplier_reference_registry::{SupplierReferenceRegistry, SupplierReferenceTarget};
use serde_json::json;

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Received,
    Saved(SupplierMessageState),
    Refreshed,
    Acknowledged,
}

struct Transport {
    kind: u32,
    reads: Mutex<Vec<JdVopReadRequest>>,
    writes: Mutex<Vec<Vec<String>>>,
    confirmed: bool,
    events: Arc<Mutex<Vec<Event>>>,
}

#[async_trait]
impl JdVopReadTransport for Transport {
    async fn read(&self, request: &JdVopReadRequest) -> ConnectorResult<Vec<u8>> {
        self.reads.lock().unwrap().push(request.clone());
        Ok(serde_json::to_vec(&json!({
            "jingdong_vop_message_queryTransByVopNormal_responce": {
                "openRpcResult": {
                    "success": true,
                    "resultCode": "success",
                    "result": [{
                        "id": 9_007_199_254_740_993_u64,
                        "type": self.kind,
                        "yn": 1,
                        "content": "{\"orderId\":\"123\"}"
                    }]
                }
            }
        }))
        .unwrap())
    }
}

#[async_trait]
impl JdVopWriteTransport for Transport {
    async fn write(&self, request: &JdVopWriteRequest) -> ConnectorResult<Vec<u8>> {
        let JdVopWriteRequest::DeleteMessages { ids, source_type } = request else {
            panic!("消息通道不得发送订单写入");
        };
        assert_eq!(*source_type, 3);
        let mut events = self.events.lock().unwrap();
        assert_eq!(events.last(), Some(&Event::Saved(SupplierMessageState::AckSending)));
        events.push(Event::Acknowledged);
        self.writes.lock().unwrap().push(ids.clone());
        Ok(serde_json::to_vec(&json!({
            "jingdong_vop_message_deleteClientMsgByIdList_responce": {
                "vopOrderRpcResult": {
                    "success": true,
                    "resultCode": "0000",
                    "result": self.confirmed
                }
            }
        }))
        .unwrap())
    }
}

struct Store {
    connection: SupplierApiConnectionId,
    record: Mutex<Option<SupplierPulledMessage>>,
    events: Arc<Mutex<Vec<Event>>>,
}

#[async_trait]
impl SupplierMessageStore for Store {
    fn binding(&self) -> (&str, &SupplierApiConnectionId) {
        ("verified-account", &self.connection)
    }

    async fn receive(&self, incoming: SupplierPulledMessage) -> Result<SupplierPulledMessage> {
        let mut record = self.record.lock().unwrap();
        if let Some(previous) = record.as_mut() {
            previous.observe_replay(&incoming)?;
            return Ok(previous.clone());
        }
        self.events.lock().unwrap().push(Event::Received);
        *record = Some(incoming.clone());
        Ok(incoming)
    }

    async fn pending(&self) -> Result<Vec<SupplierPulledMessage>> {
        Ok(self
            .record
            .lock()
            .unwrap()
            .iter()
            .filter(|message| {
                matches!(
                    message.state,
                    SupplierMessageState::Received
                        | SupplierMessageState::Applying
                        | SupplierMessageState::AckPending
                        | SupplierMessageState::AckSending
                )
            })
            .cloned()
            .collect())
    }

    async fn save(&self, message: &mut SupplierPulledMessage) -> Result<()> {
        message.base.version += 1;
        *self.record.lock().unwrap() = Some(message.clone());
        self.events.lock().unwrap().push(Event::Saved(message.state));
        Ok(())
    }

    async fn check_lease(&self) -> Result<()> {
        Ok(())
    }
}

struct OriginalOrderApplication {
    fail: Mutex<bool>,
    events: Arc<Mutex<Vec<Event>>>,
}

#[async_trait]
impl SupplierMessageRefreshApplier for OriginalOrderApplication {
    async fn refresh(&self, message: &SupplierPulledMessage) -> Result<()> {
        let mut events = self.events.lock().unwrap();
        assert_eq!(events.last(), Some(&Event::Saved(SupplierMessageState::Applying)));
        assert_eq!(message.source_message_id, "9007199254740993");
        assert_eq!(message.refresh_hints.len(), 1);
        let hint = &message.refresh_hints[0];
        assert_eq!(hint.topic, "order");
        assert_eq!(hint.target_kind, "external_order");
        assert_eq!(hint.object_id, "123");
        assert!(hint.funds_transaction_id.is_none());
        events.push(Event::Refreshed);
        if *self.fail.lock().unwrap() {
            return Err(Error::BusinessLogicError("原订单当前事实尚未核实".into()));
        }
        Ok(())
    }
}

fn connector(kind: u32, transport: Arc<Transport>) -> Arc<dyn SupplierConnector> {
    let settings: JdVopConfig = serde_json::from_value(json!({
        "enabled": true,
        "gateway_url": "https://gateway.example/router",
        "app_key": "app-1",
        "app_secret": "offline-secret",
        "credential_id": "jd-account-credential",
        "credential_encryption_key": "offline-independent-key-at-least-32-bytes",
        "account_scope": "verified-account",
        "source_type": 3,
        "requests_per_second": 100,
        "messages": {
            "evidence_reference": "offline-verified-account",
            "account_scope": "verified-account",
            "app_key": "app-1",
            "types": [kind]
        }
    }))
    .unwrap();
    let runtime = Arc::new(
        JdVopRuntime::from_configured_transport(
            settings,
            transport.clone(),
            transport,
            Arc::new(SensitiveDataCodec::from_secret(b"offline-jd-runtime-evidence")),
        )
        .unwrap(),
    );
    let mut connection = SupplierApiConnection::new(
        SupplierApiConnectionId::new("connection"),
        SupplierApiConnectionData {
            supplier_id: SupplierAccountId::new("supplier"),
            connection_code: "jd".into(),
            connector_type: SupplierConnectorType::JdVop,
            endpoint_reference: String::new(),
            credential_reference: None,
            rate_limit_policy: None,
            status: SupplierApiConnectionStatus::Disabled,
        },
        "actor",
    )
    .unwrap();
    let references = runtime.deployment_config(&SupplierReferenceTarget::from(&connection)).unwrap();
    connection.endpoint_reference = references.endpoint;
    connection.credential_reference = Some(references.credential);
    connection.endpoint_reference_bound = true;
    connection.credential_reference_bound = true;
    let mut registry = SupplierConnectorRegistry::default();
    registry.register(SupplierConnectorType::JdVop, runtime).unwrap();
    registry.notification_connector(&connection).unwrap()
}

fn fixture(kind: u32, confirmed: bool) -> (Arc<dyn SupplierConnector>, Arc<Transport>, Store) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = Arc::new(Transport {
        kind,
        reads: Mutex::new(vec![]),
        writes: Mutex::new(vec![]),
        confirmed,
        events: events.clone(),
    });
    let connector = connector(kind, transport.clone());
    let store =
        Store { connection: SupplierApiConnectionId::new("connection"), record: Mutex::new(None), events };
    (connector, transport, store)
}

#[tokio::test]
async fn jd_order_notices_preserve_protocol_identity_and_wait_for_application_before_acknowledging() {
    for kind in [10, 14, 15, 18] {
        let (connector, transport, store) = fixture(kind, true);
        assert!(connector.orders().is_none());
        let application = OriginalOrderApplication { fail: Mutex::new(true), events: store.events.clone() };
        let codec = SensitiveDataCodec::from_secret(b"jd-order-message-composition");
        let process = SupplierMessageProcess {
            store: &store,
            source: connector.messages().unwrap(),
            applier: &application,
            codec: &codec,
        };
        let first = process.run(NonZeroU32::new(1).unwrap()).await.unwrap();
        assert_eq!((first.received, first.deferred, first.acknowledged), (1, 1, 0));
        assert!(transport.writes.lock().unwrap().is_empty());
        let received = store.record.lock().unwrap().as_ref().unwrap().clone();
        *application.fail.lock().unwrap() = false;
        let retried = process.run(NonZeroU32::new(1).unwrap()).await.unwrap();
        assert_eq!((retried.applied, retried.acknowledged), (1, 1));
        let saved = store.record.lock().unwrap().as_ref().unwrap().clone();
        assert_eq!(saved.state, SupplierMessageState::Acknowledged);
        assert_eq!(saved.base.id, received.base.id);
        assert_eq!(saved.body_encrypted, received.body_encrypted);
        assert_eq!(saved.content_fingerprint, received.content_fingerprint);
        assert_eq!(*transport.writes.lock().unwrap(), [vec!["9007199254740993".to_string()]]);
        assert!(transport.reads.lock().unwrap().iter().all(|request| {
            *request == JdVopReadRequest::Messages { types: vec![kind], read_type: 2, source_type: 3 }
        }));
        assert_eq!(process.resume_pending().await.unwrap().acknowledged, 0);
        assert_eq!(transport.writes.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn jd_false_delete_keeps_unknown_state_without_reapplying_or_sending_again() {
    let (connector, transport, store) = fixture(14, false);
    let application = OriginalOrderApplication { fail: Mutex::new(false), events: store.events.clone() };
    let codec = SensitiveDataCodec::from_secret(b"jd-order-message-composition");
    let process = SupplierMessageProcess {
        store: &store,
        source: connector.messages().unwrap(),
        applier: &application,
        codec: &codec,
    };
    let first = process.run(NonZeroU32::new(1).unwrap()).await.unwrap();
    assert_eq!((first.applied, first.acknowledgement_unknown, first.acknowledged), (1, 1, 0));
    let unknown = store.record.lock().unwrap().as_ref().unwrap().clone();
    assert_eq!(unknown.state, SupplierMessageState::AckUnknown);
    assert!(unknown.acknowledged_at.is_none());
    let replay = process.run(NonZeroU32::new(1).unwrap()).await.unwrap();
    assert_eq!((replay.applied, replay.acknowledged, replay.acknowledgement_unknown), (0, 0, 0));
    let retained = store.record.lock().unwrap().as_ref().unwrap().clone();
    assert_eq!(retained.state, SupplierMessageState::AckUnknown);
    assert_eq!(retained.base.id, unknown.base.id);
    assert_eq!(retained.body_encrypted, unknown.body_encrypted);
    assert_eq!(*transport.writes.lock().unwrap(), [vec!["9007199254740993".to_string()]]);
    assert_eq!(store.events.lock().unwrap().iter().filter(|event| **event == Event::Refreshed).count(), 1);
}
