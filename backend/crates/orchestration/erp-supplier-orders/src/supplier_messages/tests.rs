use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use erp_supply::ports::connector::address::{
    AddressCandidateQuery, ConfirmedSupplierAddress, SupplierAddressCandidate, SupplierAddressDepth,
    SupplierAddressSource,
};
use erp_supply::ports::connector::callback::{ChangeTopic, RefreshTarget};
use erp_supply::ports::connector::common::{ConnectorResult, Snapshot, SourceRevision, SourceStamp};
use erp_supply::ports::connector::messages::{MessageInterpretation, MessageRefreshHint, PulledMessage};
use erp_supply::ports::connector::order::OrderKey;

use super::*;

type Trace = Arc<Mutex<Vec<String>>>;

struct Store {
    connection: SupplierApiConnectionId,
    records: Mutex<BTreeMap<String, SupplierPulledMessage>>,
    trace: Trace,
    fail_save: Mutex<Option<SupplierMessageState>>,
    fail_receive: Mutex<bool>,
}

impl Store {
    fn new(trace: Trace) -> Self {
        Self {
            connection: SupplierApiConnectionId::new("connection"),
            records: Mutex::new(BTreeMap::new()),
            trace,
            fail_save: Mutex::new(None),
            fail_receive: Mutex::new(false),
        }
    }

    fn record(&self) -> SupplierPulledMessage {
        self.records.lock().unwrap().values().next().unwrap().clone()
    }
}

#[async_trait]
impl SupplierMessageStore for Store {
    fn binding(&self) -> (&str, &SupplierApiConnectionId) {
        ("account", &self.connection)
    }

    async fn receive(&self, incoming: SupplierPulledMessage) -> Result<SupplierPulledMessage> {
        if *self.fail_receive.lock().unwrap() {
            return Err(Error::Internal("save failed".into()));
        }
        self.trace.lock().unwrap().push("receive".into());
        let mut records = self.records.lock().unwrap();
        if let Some(existing) = records.get_mut(&incoming.base.id) {
            existing.observe_replay(&incoming)?;
            return Ok(existing.clone());
        }
        records.insert(incoming.base.id.clone(), incoming.clone());
        Ok(incoming)
    }

    async fn pending(&self) -> Result<Vec<SupplierPulledMessage>> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .values()
            .filter(|message| {
                matches!(
                    message.state,
                    SupplierMessageState::Received
                        | SupplierMessageState::Applying
                        | SupplierMessageState::AckPending
                        | SupplierMessageState::AckSending,
                )
            })
            .cloned()
            .collect())
    }

    async fn save(&self, message: &mut SupplierPulledMessage) -> Result<()> {
        let mut failure = self.fail_save.lock().unwrap();
        if *failure == Some(message.state) {
            *failure = None;
            return Err(Error::Internal("save failed".into()));
        }
        let mut records = self.records.lock().unwrap();
        let previous = records.get(&message.base.id).unwrap();
        if previous.base.version != message.base.version {
            return Err(Error::ConflictError("version changed".into()));
        }
        message.base.version += 1;
        self.trace.lock().unwrap().push(format!("save:{:?}", message.state));
        records.insert(message.base.id.clone(), message.clone());
        Ok(())
    }

    async fn check_lease(&self) -> Result<()> {
        Ok(())
    }
}

struct Source {
    messages: Mutex<Vec<PulledMessage>>,
    trace: Trace,
    ack: MessageAcknowledgement,
    order_only: bool,
}

#[async_trait]
impl SupplierMessageSource for Source {
    fn account_scope(&self) -> &str {
        "account"
    }

    fn order_only_notice(&self, _: &str) -> bool {
        self.order_only
    }

    async fn pull(&self, _: NonZeroU32) -> ConnectorResult<Vec<PulledMessage>> {
        self.trace.lock().unwrap().push("pull".into());
        Ok(std::mem::take(&mut *self.messages.lock().unwrap()))
    }

    async fn acknowledge(&self, _: &str) -> ConnectorResult<MessageAcknowledgement> {
        self.trace.lock().unwrap().push("remote_ack".into());
        Ok(self.ack)
    }
}

struct Applier {
    trace: Trace,
    applied: Mutex<BTreeSet<String>>,
    fail: Mutex<bool>,
}

#[async_trait]
impl SupplierMessageRefreshApplier for Applier {
    async fn refresh(&self, message: &SupplierPulledMessage) -> Result<()> {
        self.trace.lock().unwrap().push("refresh_query".into());
        if *self.fail.lock().unwrap() {
            return Err(Error::BusinessLogicError("query failed".into()));
        }
        if self.applied.lock().unwrap().insert(message.base.id.clone()) {
            self.trace.lock().unwrap().push("domain_apply".into());
        }
        Ok(())
    }
}

fn message() -> PulledMessage {
    PulledMessage {
        source_id: "remote-message".into(),
        message_type: "1".into(),
        body: b"private raw message".to_vec(),
        interpretation: MessageInterpretation::Refresh(vec![MessageRefreshHint {
            topic: ChangeTopic::Order,
            target: RefreshTarget::Order(OrderKey::External("root".into())),
            funds_transaction_id: None,
        }]),
    }
}

fn source(trace: &Trace, ack: MessageAcknowledgement) -> Source {
    Source { messages: Mutex::new(vec![message()]), trace: trace.clone(), ack, order_only: false }
}

fn applier(trace: &Trace) -> Applier {
    Applier { trace: trace.clone(), applied: Mutex::new(BTreeSet::new()), fail: Mutex::new(false) }
}

#[tokio::test]
async fn durable_intent_precedes_query_and_durable_ack_intent_precedes_remote_delete() {
    let trace = Trace::default();
    let store = Store::new(trace.clone());
    let source = source(&trace, MessageAcknowledgement::Confirmed);
    let applier = applier(&trace);
    let codec = SensitiveDataCodec::from_secret(b"message-test-secret");
    let process = SupplierMessageProcess { store: &store, source: &source, applier: &applier, codec: &codec };

    let result = process.run(NonZeroU32::new(10).unwrap()).await.unwrap();
    assert_eq!(result.applied, 1);
    assert_eq!(result.acknowledged, 1);
    assert_eq!(store.record().state, SupplierMessageState::Acknowledged);
    assert_eq!(
        *trace.lock().unwrap(),
        [
            "pull",
            "receive",
            "save:Applying",
            "refresh_query",
            "domain_apply",
            "save:AckPending",
            "save:AckSending",
            "remote_ack",
            "save:Acknowledged"
        ]
    );

    *source.messages.lock().unwrap() = vec![message()];
    process.run(NonZeroU32::new(10).unwrap()).await.unwrap();
    assert_eq!(trace.lock().unwrap().iter().filter(|item| *item == "remote_ack").count(), 1);
    assert_eq!(applier.applied.lock().unwrap().len(), 1);
    assert!(!store.record().body_encrypted.contains("private raw message"));
}

#[tokio::test]
async fn persistence_or_refresh_failure_never_deletes_remote_message() {
    let trace = Trace::default();
    let store = Store::new(trace.clone());
    let source = source(&trace, MessageAcknowledgement::Confirmed);
    let applier = applier(&trace);
    let codec = SensitiveDataCodec::from_secret(b"message-test-secret");
    let process = SupplierMessageProcess { store: &store, source: &source, applier: &applier, codec: &codec };

    *store.fail_receive.lock().unwrap() = true;
    assert!(process.run(NonZeroU32::new(10).unwrap()).await.is_err());
    assert!(store.records.lock().unwrap().is_empty());
    assert_eq!(*trace.lock().unwrap(), ["pull"]);

    *store.fail_receive.lock().unwrap() = false;
    *source.messages.lock().unwrap() = vec![message()];
    *applier.fail.lock().unwrap() = true;
    let result = process.run(NonZeroU32::new(10).unwrap()).await.unwrap();
    assert_eq!(result.deferred, 1);
    assert_eq!(store.record().state, SupplierMessageState::Applying);
    assert!(store.record().next_attempt_at.is_some());
    assert!(!trace.lock().unwrap().iter().any(|item| item == "remote_ack"));
}

#[tokio::test]
async fn crash_after_remote_ack_recovers_unknown_without_reapplying_or_deleting_twice() {
    let trace = Trace::default();
    let store = Store::new(trace.clone());
    let source = source(&trace, MessageAcknowledgement::Confirmed);
    let applier = applier(&trace);
    let codec = SensitiveDataCodec::from_secret(b"message-test-secret");
    let process = SupplierMessageProcess { store: &store, source: &source, applier: &applier, codec: &codec };

    *store.fail_save.lock().unwrap() = Some(SupplierMessageState::Acknowledged);
    assert!(process.run(NonZeroU32::new(10).unwrap()).await.is_err());
    assert_eq!(store.record().state, SupplierMessageState::AckSending);

    let result = process.run(NonZeroU32::new(10).unwrap()).await.unwrap();
    assert_eq!(result.acknowledgement_unknown, 1);
    assert_eq!(store.record().state, SupplierMessageState::AckUnknown);
    assert_eq!(trace.lock().unwrap().iter().filter(|item| *item == "remote_ack").count(), 1);
    assert_eq!(trace.lock().unwrap().iter().filter(|item| *item == "domain_apply").count(), 1);
}

#[tokio::test]
async fn source_ack_unknown_and_unknown_message_type_are_not_silently_retried() {
    let trace = Trace::default();
    let store = Store::new(trace.clone());
    let source = source(&trace, MessageAcknowledgement::Unknown);
    let applier = applier(&trace);
    let codec = SensitiveDataCodec::from_secret(b"message-test-secret");
    let process = SupplierMessageProcess { store: &store, source: &source, applier: &applier, codec: &codec };

    assert_eq!(process.run(NonZeroU32::new(10).unwrap()).await.unwrap().acknowledgement_unknown, 1);
    *source.messages.lock().unwrap() = vec![message()];
    process.run(NonZeroU32::new(10).unwrap()).await.unwrap();
    assert_eq!(trace.lock().unwrap().iter().filter(|item| *item == "remote_ack").count(), 1);

    let mut unknown = message();
    unknown.source_id = "unknown-type".into();
    unknown.interpretation = MessageInterpretation::Quarantine { reason_code: "UNKNOWN_MESSAGE_TYPE".into() };
    *source.messages.lock().unwrap() = vec![unknown];
    assert_eq!(process.run(NonZeroU32::new(10).unwrap()).await.unwrap().quarantined, 1);
    assert_eq!(trace.lock().unwrap().iter().filter(|item| *item == "remote_ack").count(), 1);
}

struct LiveAddressApplication {
    trace: Trace,
}

#[async_trait]
impl SupplierAddressSource for LiveAddressApplication {
    async fn candidates(
        &self,
        query: &AddressCandidateQuery,
    ) -> ConnectorResult<Snapshot<Vec<SupplierAddressCandidate>>> {
        self.trace.lock().unwrap().push("live_address_query".into());
        assert_eq!(query.depth, SupplierAddressDepth::Province);
        assert!(query.parent_code.is_none());
        Ok(Snapshot {
            stamp: SourceStamp {
                revision: SourceRevision::Unversioned,
                changed_at: None,
                observed_at: Instant::now(),
            },
            value: vec![SupplierAddressCandidate {
                depth: SupplierAddressDepth::Province,
                code: "1".into(),
                name: "实时省级名称".into(),
                parent_code: None,
            }],
        })
    }

    async fn validate(&self, _: &ConfirmedSupplierAddress) -> ConnectorResult<Snapshot<bool>> {
        panic!("更新提示不应确认或修改任何原订单的收货地址")
    }
}

#[async_trait]
impl SupplierMessageRefreshApplier for LiveAddressApplication {
    async fn refresh(&self, message: &SupplierPulledMessage) -> Result<()> {
        for hint in &message.refresh_hints {
            super::address::refresh_address_notice(self, hint, message.received_at).await?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn address_change_ack_follows_live_refresh_and_durable_processing_receipt() {
    let trace = Trace::default();
    let store = Store::new(trace.clone());
    let mut incoming = message();
    incoming.message_type = "50".into();
    incoming.interpretation = MessageInterpretation::Refresh(vec![MessageRefreshHint {
        topic: ChangeTopic::Address,
        target: RefreshTarget::Address("99999".into()),
        funds_transaction_id: None,
    }]);
    let source = Source {
        messages: Mutex::new(vec![incoming]),
        trace: trace.clone(),
        ack: MessageAcknowledgement::Confirmed,
        order_only: false,
    };
    let application = LiveAddressApplication { trace: trace.clone() };
    let codec = SensitiveDataCodec::from_secret(b"address-notice-test-secret");
    let process =
        SupplierMessageProcess { store: &store, source: &source, applier: &application, codec: &codec };

    assert_eq!(process.run(NonZeroU32::new(10).unwrap()).await.unwrap().acknowledged, 1);
    let trace = trace.lock().unwrap();
    assert_eq!(
        *trace,
        [
            "pull",
            "receive",
            "save:Applying",
            "live_address_query",
            "save:AckPending",
            "save:AckSending",
            "remote_ack",
            "save:Acknowledged"
        ]
    );

    let saved = store.record();
    assert_eq!(saved.refresh_hints[0].target_kind, "supplier_address");
    assert_eq!(saved.refresh_hints[0].object_id, "99999");
    assert!(saved.applied_at.is_some());
}

struct LegacyOrderApplication<'a> {
    source: &'a Source,
    trace: Trace,
    original_ready: Mutex<bool>,
}

#[async_trait]
impl SupplierMessageRefreshApplier for LegacyOrderApplication<'_> {
    async fn refresh(&self, message: &SupplierPulledMessage) -> Result<()> {
        for hint in &message.refresh_hints {
            super::refresh::apply_order_refresh(
                self.source,
                message,
                hint,
                async {
                    self.trace.lock().unwrap().push("original_order_query".into());
                    Ok(*self.original_ready.lock().unwrap())
                },
                async {
                    self.trace.lock().unwrap().push("funds_conclusion".into());
                    Err(Error::BusinessLogicError("缺少真实资金交易身份".into()))
                },
            )
            .await?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn legacy_applying_order_notices_keep_original_evidence_and_ack_only_after_order_application() {
    for kind in ["10", "14", "15", "18"] {
        let trace = Trace::default();
        let store = Store::new(trace.clone());
        let source = Source {
            messages: Mutex::new(vec![]),
            trace: trace.clone(),
            ack: MessageAcknowledgement::Confirmed,
            order_only: true,
        };
        let codec = SensitiveDataCodec::from_secret(b"legacy-order-message-secret");
        let mut incoming = message();
        incoming.message_type = kind.into();
        incoming.interpretation = MessageInterpretation::Refresh(vec![
            MessageRefreshHint {
                topic: ChangeTopic::Order,
                target: RefreshTarget::Order(OrderKey::External("original-order".into())),
                funds_transaction_id: None,
            },
            MessageRefreshHint {
                topic: ChangeTopic::Refund,
                target: RefreshTarget::Refund {
                    order: OrderKey::External("original-order".into()),
                    action_id: None,
                },
                funds_transaction_id: None,
            },
        ]);

        let mut original =
            intake::receive("account", &store.connection, incoming, &codec, Instant::now()).unwrap();
        original.begin_apply().unwrap();
        store.records.lock().unwrap().insert(original.base.id.clone(), original.clone());

        let application = LegacyOrderApplication {
            source: &source,
            trace: trace.clone(),
            original_ready: Mutex::new(false),
        };
        let process =
            SupplierMessageProcess { store: &store, source: &source, applier: &application, codec: &codec };

        assert_eq!(process.resume_pending().await.unwrap().deferred, 1);
        assert!(!trace.lock().unwrap().iter().any(|step| step == "remote_ack"));

        *application.original_ready.lock().unwrap() = true;
        assert_eq!(process.resume_pending().await.unwrap().acknowledged, 1);
        process.resume_pending().await.unwrap();

        let saved = store.record();
        assert_eq!(saved.state, SupplierMessageState::Acknowledged);
        assert_eq!(saved.body_encrypted, original.body_encrypted);
        assert_eq!(saved.refresh_hints, original.refresh_hints);
        assert!(saved.applied_at.is_some());
        let trace = trace.lock().unwrap();
        assert!(!trace.iter().any(|step| step == "funds_conclusion"));
        assert_eq!(trace.iter().filter(|step| *step == "remote_ack").count(), 1);
    }
}

#[tokio::test]
async fn explicit_funds_identity_and_other_supplier_policies_keep_precise_funds_requirements() {
    let trace = Trace::default();
    let codec = SensitiveDataCodec::from_secret(b"message-funds-policy-secret");
    let connection = SupplierApiConnectionId::new("connection");
    let original = intake::receive("account", &connection, message(), &codec, Instant::now()).unwrap();
    for (order_only, transaction) in [(true, Some("stable-transaction")), (false, None)] {
        let source = Source {
            messages: Mutex::new(vec![]),
            trace: trace.clone(),
            ack: MessageAcknowledgement::Confirmed,
            order_only,
        };
        let hint = SupplierRefreshHint {
            topic: "refund".into(),
            target_kind: "external_order".into(),
            object_id: "original-order".into(),
            product_id: None,
            funds_transaction_id: transaction.map(str::to_owned),
        };

        let result =
            super::refresh::apply_order_refresh(&source, &original, &hint, async { Ok(true) }, async {
                trace.lock().unwrap().push("funds_conclusion".into());
                Err(Error::BusinessLogicError("真实资金仍待核实".into()))
            })
            .await;
        assert!(result.is_err());
    }
    assert_eq!(trace.lock().unwrap().len(), 2);
}
