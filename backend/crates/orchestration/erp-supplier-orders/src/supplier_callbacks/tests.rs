use erp_core::common::time::Instant;
use erp_core::ids::SupplierApiConnectionId;
use erp_party::SensitiveDataCodec;
use erp_supply::ports::connector::callback::{
    CallbackIntegrity, CallbackReply, CallbackRequest, Callbacks, ChangeNotice, ChangeTopic, RefreshTarget,
    VerifiedCallback,
};
use erp_supply::ports::connector::common::{ConnectorResult, SourceRevision};
use erp_supply::ports::connector::order::OrderKey;

use super::{Error, acknowledge_after_persistence, intake};

struct Protocol {
    event: Option<String>,
}
impl Callbacks for Protocol {
    fn verify(&self, request: &CallbackRequest<'_>) -> ConnectorResult<VerifiedCallback> {
        assert_eq!(request.headers[0].0, "x-signature");
        Ok(VerifiedCallback {
            integrity: CallbackIntegrity::BodyAuthenticated,
            notices: vec![ChangeNotice {
                source_event_id: self.event.clone(),
                revision: SourceRevision::Unversioned,
                occurred_at: None,
                target: RefreshTarget::Product("product".into()),
                topic: ChangeTopic::Product,
                observation: None,
            }],
            durable_ack: reply(),
        })
    }
}
fn reply() -> CallbackReply {
    CallbackReply { status: 200, content_type: "text/plain".into(), body: b"ok".to_vec() }
}

#[tokio::test]
async fn successful_ack_requires_durable_persistence() {
    assert!(
        acknowledge_after_persistence(reply(), async { Err(Error::ConflictError("write failed".into())) })
            .await
            .is_err()
    );
    let mut persisted = false;
    let result = acknowledge_after_persistence(reply(), async {
        persisted = true;
        Ok(())
    })
    .await
    .unwrap();
    assert!(persisted);
    assert_eq!(result.body, b"ok");
}

#[test]
fn generic_protocol_receives_catalog_events_and_rejects_same_event_changed_body() {
    let codec = SensitiveDataCodec::from_secret(b"offline-fixture");
    let headers = vec![("x-signature".into(), b"signed".to_vec())];
    let mut request = CallbackRequest {
        method: "POST",
        path_and_query: "/callbacks/custom/c/events?x=1",
        headers: &headers,
        body: b"first",
        received_at: Instant::from_unix_secs(1),
    };
    let protocol = Protocol { event: Some("event-1".into()) };
    let connection = SupplierApiConnectionId::new("c");
    let (original, ack) = intake::prepare(&protocol, &connection, "binding", &codec, &request).unwrap();
    assert_eq!(ack.body, b"ok");
    assert_eq!(original.route_kind, "catalog");
    assert_eq!(original.integrity, "body_authenticated");
    assert_eq!(original.source_event_id.as_deref(), Some("event-1"));
    assert_eq!(codec.decrypt(&original.body_encrypted).unwrap(), "first");
    let (repeat, _) = intake::prepare(&protocol, &connection, "binding", &codec, &request).unwrap();
    intake::verify_replay(&codec, &original, &repeat).unwrap();
    request.body = b"changed";
    let (changed, _) = intake::prepare(&protocol, &connection, "binding", &codec, &request).unwrap();
    assert_eq!(original.base.id, changed.base.id);
    assert!(matches!(intake::verify_replay(&codec, &original, &changed), Err(Error::ConflictError(_))));
    let (other, _) =
        intake::prepare(&protocol, &SupplierApiConnectionId::new("other"), "binding", &codec, &request)
            .unwrap();
    assert_ne!(original.base.id, other.base.id);
}

#[test]
fn no_event_identity_is_byte_scoped_and_historical_receipts_require_matching_evidence() {
    let codec = SensitiveDataCodec::from_secret(b"offline-fixture");
    let headers = vec![("x-signature".into(), b"signed".to_vec())];
    let mut request = CallbackRequest {
        method: "POST",
        path_and_query: "/callbacks/custom/c/events",
        headers: &headers,
        body: b"first",
        received_at: Instant::from_unix_secs(1),
    };
    let protocol = Protocol { event: None };
    let connection = SupplierApiConnectionId::new("c");
    let (mut original, _) = intake::prepare(&protocol, &connection, "binding", &codec, &request).unwrap();
    assert!(original.source_event_id.is_none());
    original.content_fingerprint = None;
    let (repeat, _) = intake::prepare(&protocol, &connection, "binding", &codec, &request).unwrap();
    intake::verify_replay(&codec, &original, &repeat).unwrap();
    request.body = b"changed";
    let (changed, _) = intake::prepare(&protocol, &connection, "binding", &codec, &request).unwrap();
    assert_ne!(original.base.id, changed.base.id);
    assert!(intake::verify_replay(&codec, &original, &changed).is_err());
}

struct LegacyProtocol;
impl Callbacks for LegacyProtocol {
    fn reception_id(&self, _: &CallbackRequest<'_>) -> Option<String> {
        Some("dgss-original-reception".into())
    }
    fn verify(&self, _: &CallbackRequest<'_>) -> ConnectorResult<VerifiedCallback> {
        Ok(VerifiedCallback {
            integrity: CallbackIntegrity::EnvelopeOnly,
            notices: vec![ChangeNotice {
                source_event_id: None,
                revision: SourceRevision::Unversioned,
                occurred_at: None,
                target: RefreshTarget::Order(OrderKey::Merchant("order-1".into())),
                topic: ChangeTopic::Order,
                observation: None,
            }],
            durable_ack: CallbackReply {
                status: 200,
                content_type: "application/json".into(),
                body: br#"{"code":200,"message":"success"}"#.to_vec(),
            },
        })
    }
}
#[test]
fn protocol_reception_identity_and_original_url_are_preserved_by_generic_intake() {
    let connector = LegacyProtocol;
    let codec = SensitiveDataCodec::from_secret(b"offline-fixture");
    let request = CallbackRequest {
        method: "POST",
        path_and_query: "/callbacks/dangaoshushu/connection-1/order",
        headers: &[],
        body: b"original",
        received_at: Instant::from_unix_secs(1700000000),
    };
    let (receipt, reply) =
        intake::prepare(&connector, &SupplierApiConnectionId::new("connection-1"), "scope", &codec, &request)
            .unwrap();
    assert_eq!(Some(receipt.base.id.clone()), connector.reception_id(&request));
    assert!(receipt.base.id.starts_with("dgss-"));
    assert_eq!(receipt.integrity, "envelope_only");
    assert!(receipt.source_event_id.is_none());
    assert_eq!(receipt.route_kind, "order");
    assert_eq!(reply.body, br#"{"code":200,"message":"success"}"#);
}

struct Parsed(VerifiedCallback);
impl Callbacks for Parsed {
    fn verify(&self, _: &CallbackRequest<'_>) -> ConnectorResult<VerifiedCallback> {
        Ok(self.0.clone())
    }
}

#[test]
fn invalid_ack_and_mixed_consumers_never_produce_a_receivable_record() {
    let codec = SensitiveDataCodec::from_secret(b"offline-fixture");
    let headers = vec![("x-signature".into(), b"signed".to_vec())];
    let request = CallbackRequest {
        method: "POST",
        path_and_query: "/callback",
        headers: &headers,
        body: b"{}",
        received_at: Instant::from_unix_secs(1),
    };
    let valid = Protocol { event: None }.verify(&request).unwrap();
    for mode in 0..4 {
        let mut invalid = valid.clone();
        match mode {
            0 => invalid.notices.clear(),
            1 => invalid.durable_ack.content_type = "text/plain\r\nx-inject: bad".into(),
            2 => {
                let mut notice = invalid.notices[0].clone();
                notice.topic = ChangeTopic::Order;
                notice.target = RefreshTarget::Order(OrderKey::Merchant("order".into()));
                invalid.notices.push(notice);
            },
            _ => invalid.notices[0].source_event_id = Some(String::new()),
        }
        assert!(
            intake::prepare(&Parsed(invalid), &SupplierApiConnectionId::new("c"), "scope", &codec, &request)
                .is_err()
        );
    }
}
