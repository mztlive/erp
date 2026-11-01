//! 原订单消息应用顺序测试：协议只提供已经解释的定向补查线索。
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierApiConnectionId;
use erp_integration::entity::supplier_message::{SupplierMessageState, SupplierPulledMessage};
use erp_party::SensitiveDataCodec;
use erp_supply::ports::connector::callback::{ChangeTopic, RefreshTarget};
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::connector::messages::{
    MessageAcknowledgement, MessageInterpretation, MessageRefreshHint, PulledMessage, SupplierMessageSource,
};
use erp_supply::ports::connector::order::OrderKey;

use super::{SupplierMessageProcess, SupplierMessageRefreshApplier, SupplierMessageStore};
use crate::{Error, Result};

struct Source {
    kind: u32,
    writes: Mutex<Vec<String>>,
    reads: Mutex<usize>,
    acknowledgement: MessageAcknowledgement,
}
#[async_trait]
impl SupplierMessageSource for Source {
    fn account_scope(&self) -> &str {
        "verified-account"
    }
    fn order_only_notice(&self, message_type: &str) -> bool {
        matches!(message_type, "10" | "14" | "15" | "18")
    }
    async fn pull(&self, _: NonZeroU32) -> ConnectorResult<Vec<PulledMessage>> {
        *self.reads.lock().unwrap() += 1;
        Ok(vec![PulledMessage {
            source_id: "9007199254740993".into(),
            message_type: self.kind.to_string(),
            body: b"original-source-evidence".to_vec(),
            interpretation: MessageInterpretation::Refresh(vec![MessageRefreshHint {
                topic: ChangeTopic::Order,
                target: RefreshTarget::Order(OrderKey::External("123".into())),
                funds_transaction_id: None,
            }]),
        }])
    }
    async fn acknowledge(&self, id: &str) -> ConnectorResult<MessageAcknowledgement> {
        self.writes.lock().unwrap().push(id.into());
        Ok(self.acknowledgement)
    }
}
fn source(kind: u32, acknowledgement: MessageAcknowledgement) -> Source {
    Source { kind, writes: Mutex::new(vec![]), reads: Mutex::new(0), acknowledgement }
}
struct OrderMessageStore {
    connection: SupplierApiConnectionId,
    record: Mutex<Option<SupplierPulledMessage>>,
}

#[async_trait]
impl SupplierMessageStore for OrderMessageStore {
    fn binding(&self) -> (&str, &SupplierApiConnectionId) {
        ("verified-account", &self.connection)
    }

    async fn receive(&self, incoming: SupplierPulledMessage) -> Result<SupplierPulledMessage> {
        let mut record = self.record.lock().unwrap();
        if let Some(previous) = record.as_mut() {
            previous.observe_replay(&incoming)?;
            return Ok(previous.clone());
        }
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
        Ok(())
    }

    async fn check_lease(&self) -> Result<()> {
        Ok(())
    }
}

struct OriginalOrderApplication {
    fail: Mutex<bool>,
    transport: Arc<Source>,
}

#[async_trait]
impl SupplierMessageRefreshApplier for OriginalOrderApplication {
    async fn refresh(&self, message: &SupplierPulledMessage) -> Result<()> {
        assert!(self.transport.writes.lock().unwrap().is_empty());
        assert_eq!(message.refresh_hints.len(), 1);
        let hint = &message.refresh_hints[0];
        assert_eq!(hint.topic, "order");
        assert_eq!(hint.target_kind, "external_order");
        assert_eq!(hint.object_id, "123");
        assert!(hint.funds_transaction_id.is_none());
        if *self.fail.lock().unwrap() {
            return Err(Error::BusinessLogicError("原订单当前事实尚未核实".into()));
        }
        Ok(())
    }
}

#[tokio::test]
async fn order_failure_and_receipt_notices_wait_for_original_order_then_ack_without_trade_number() {
    for kind in [10, 14, 15, 18] {
        let transport = Arc::new(source(kind, MessageAcknowledgement::Confirmed));
        let store = OrderMessageStore {
            connection: SupplierApiConnectionId::new("connection"),
            record: Mutex::new(None),
        };
        let application = OriginalOrderApplication { fail: Mutex::new(true), transport: transport.clone() };
        let codec = SensitiveDataCodec::from_secret(b"jd-order-message-application");
        let process = SupplierMessageProcess {
            store: &store,
            source: transport.as_ref(),
            applier: &application,
            codec: &codec,
        };
        assert_eq!(process.run(NonZeroU32::new(1).unwrap()).await.unwrap().deferred, 1);
        assert!(transport.writes.lock().unwrap().is_empty());
        *application.fail.lock().unwrap() = false;
        assert_eq!(process.run(NonZeroU32::new(1).unwrap()).await.unwrap().acknowledged, 1);
        assert_eq!(transport.writes.lock().unwrap().len(), 1);
        assert_eq!(store.record.lock().unwrap().as_ref().unwrap().state, SupplierMessageState::Acknowledged);
        assert!(*transport.reads.lock().unwrap() >= 1);
    }
}

#[tokio::test]
async fn false_delete_result_keeps_unknown_state_and_cannot_send_a_second_delete() {
    let transport = source(1, MessageAcknowledgement::Unknown);
    let mut message = SupplierPulledMessage {
        base: BaseModel::fake(),
        account_scope: "verified-account".into(),
        connection_id: SupplierApiConnectionId::new("connection"),
        source_message_id: "123".into(),
        message_type: "1".into(),
        content_fingerprint: "a".repeat(64),
        body_encrypted: "original-evidence".into(),
        refresh_hints: vec![],
        received_at: Instant::from_unix_secs(1),
        state: SupplierMessageState::AckPending,
        processing_error: None,
        next_attempt_at: None,
        applied_at: Some(Instant::from_unix_secs(2)),
        acknowledged_at: None,
        conflict: None,
    };
    message.begin_acknowledge().unwrap();
    let acknowledgement = transport.acknowledge(&message.source_message_id).await.unwrap();
    assert_eq!(acknowledgement, MessageAcknowledgement::Unknown);
    message
        .acknowledged(acknowledgement == MessageAcknowledgement::Confirmed, Instant::from_unix_secs(3))
        .unwrap();
    assert_eq!(message.state, SupplierMessageState::AckUnknown);
    assert!(message.begin_acknowledge().is_err());
    assert!(message.acknowledged_at.is_none());
    assert_eq!(message.body_encrypted, "original-evidence");
    assert_eq!(transport.writes.lock().unwrap().len(), 1);
}
