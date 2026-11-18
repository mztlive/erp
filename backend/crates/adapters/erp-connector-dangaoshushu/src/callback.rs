use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::callback::{
    CallbackIntegrity, CallbackReply, CallbackRequest, Callbacks, ChangeNotice, ChangeTopic, RefreshTarget,
    VerifiedCallback,
};
use erp_supply::ports::connector::common::{ConnectorResult, SourceRevision, SupplierSku};
use erp_supply::ports::connector::order::OrderKey;
use serde_json::{Value, from_slice};

use super::parsing::{field, id, mapping};
use super::{DangaoshushuConnector, error, proof, signing};

impl Callbacks for DangaoshushuConnector {
    fn reception_id(&self, request: &CallbackRequest<'_>) -> Option<String> {
        let body =
            [self.binding.as_bytes(), b"\0", request.path_and_query.as_bytes(), b"\0", request.body].concat();
        Some(format!("dgss-{}", proof::hash(&self.settings.private_key, &body)))
    }

    fn verify(&self, request: &CallbackRequest<'_>) -> ConnectorResult<VerifiedCallback> {
        if request.method != "POST" || request.body.len() > 256 * 1024 {
            return Err(mapping());
        }
        let value: Value = from_slice(request.body).map_err(|_| mapping())?;
        let timestamp = field(&value, "timestamp")?;
        let seconds = timestamp_seconds(&timestamp)?;
        let expected = signing::sign(&self.settings.channel_no, &timestamp, &self.settings.private_key);
        if value["channel_no"].as_str() != Some(&self.settings.channel_no)
            || !value["sign"]
                .as_str()
                .is_some_and(|signature| signing::signature_matches(&expected, signature))
            || request.received_at.unix_secs().abs_diff(seconds) > self.settings.callback_max_skew_seconds
        {
            return Err(error(
                SupplierFailureClass::AuthSignature,
                "DGSS_CALLBACK_AUTH",
                "供应商推送渠道、签名或时间窗口无效",
            ));
        }
        let path = request.path_and_query.split_once('?').map_or(request.path_and_query, |(path, _)| path);
        let kind = path.rsplit_once('/').map_or(path, |(_, kind)| kind);
        let notices = notices(kind, &value)?;
        Ok(VerifiedCallback {
            integrity: CallbackIntegrity::EnvelopeOnly,
            notices,
            durable_ack: CallbackReply {
                status: 200,
                content_type: "application/json".into(),
                body: br#"{"code":200,"message":"success"}"#.to_vec(),
            },
        })
    }
}

fn timestamp_seconds(timestamp: &str) -> ConnectorResult<i64> {
    if !timestamp.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(mapping());
    }
    let value: i64 = timestamp.parse().map_err(|_| mapping())?;
    match timestamp.len() {
        10 => Ok(value),
        13 => Ok(value / 1000),
        _ => Err(mapping()),
    }
}

fn notice(target: RefreshTarget, topic: ChangeTopic) -> ChangeNotice {
    // 签名未覆盖业务正文，也没有可靠来源事件号/业务变更时间；不制造完整观察事实。
    ChangeNotice {
        source_event_id: None,
        revision: SourceRevision::Unversioned,
        occurred_at: None,
        target,
        topic,
        observation: None,
    }
}

fn notices(kind: &str, value: &Value) -> ConnectorResult<Vec<ChangeNotice>> {
    let topic = match kind {
        "order" => ChangeTopic::Order,
        "status" => ChangeTopic::Availability,
        "product" => ChangeTopic::Product,
        "cities" => ChangeTopic::Region,
        "price" => ChangeTopic::Price,
        _ => return Err(mapping()),
    };
    if kind == "order" {
        return Ok(vec![notice(
            RefreshTarget::Order(OrderKey::Merchant(id(&value["out_order_no"])?)),
            topic,
        )]);
    }
    if kind == "price" {
        let notices = value["data"]
            .as_array()
            .ok_or_else(mapping)?
            .iter()
            .map(|row| {
                Ok(notice(
                    RefreshTarget::Sku(SupplierSku {
                        product_id: Some(id(&row["product_id"])?),
                        spec_id: id(&row["spec_id"])?,
                    }),
                    topic,
                ))
            })
            .collect::<ConnectorResult<Vec<_>>>()?;
        if notices.is_empty() || notices.len() > 1000 {
            return Err(mapping());
        }
        return Ok(notices);
    }
    let identifier = id(&value["id"])?;
    let target = if kind == "product" {
        RefreshTarget::Product(identifier)
    } else {
        match value["type"].as_str() {
            Some("brand") => RefreshTarget::Brand(identifier),
            Some("product") => RefreshTarget::Product(identifier),
            _ => return Err(mapping()),
        }
    };
    Ok(vec![notice(target, topic)])
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;

    use super::super::test_support::connector;
    use super::*;

    #[test]
    fn authenticated_envelope_never_authenticates_business_fields() {
        let connector = connector(vec![]);
        let mut value = serde_json::json!({"channel_no":"test-channel","timestamp":1700000000,"sign":signing::sign("test-channel","1700000000","test-key"),"out_order_no":"order-1","status":"4"});
        for order in ["order-1", "order-2"] {
            value["out_order_no"] = serde_json::json!(order);
            let bytes = serde_json::to_vec(&value).unwrap();
            let request = CallbackRequest {
                method: "POST",
                path_and_query: "/callbacks/dangaoshushu/connection/order",
                headers: &[],
                body: &bytes,
                received_at: Instant::from_unix_secs(1700000000),
            };
            let verified = connector.verify(&request).unwrap();
            assert_eq!(verified.integrity, CallbackIntegrity::EnvelopeOnly);
            assert!(verified.notices.iter().all(|notice| notice.observation.is_none()
                && notice.occurred_at.is_none()
                && notice.source_event_id.is_none()));
        }
    }

    #[test]
    fn callback_topic_uses_only_final_path_segment_before_first_query() {
        let connector = connector(vec![]);
        let bytes = serde_json::to_vec(&serde_json::json!({
            "channel_no":"test-channel", "timestamp":1700000000,
            "sign":"b918d6992b799b477691f8beebc45c8f", "out_order_no":"order-1"
        }))
        .unwrap();
        let mut request = CallbackRequest {
            method: "POST",
            path_and_query: "/callbacks/connection-1/order",
            headers: &[],
            body: &bytes,
            received_at: Instant::from_unix_secs(1700000000),
        };
        let expected = connector.verify(&request).unwrap();
        for path in [
            "order",
            "/order",
            "/callbacks//connection-1/order",
            "/callbacks/connection-1/order?original=/price?extra=/unknown",
            "order?",
        ] {
            request.path_and_query = path;
            let actual = connector.verify(&request).unwrap();
            assert!(actual == expected, "{path}");
        }
        for path in [
            "",
            "/",
            "?order",
            "/callbacks/connection-1/order/",
            "/callbacks/connection-1/unknown?route=order",
            "/callbacks/connection-1/order？ignored",
        ] {
            request.path_and_query = path;
            assert_eq!(connector.verify(&request).err().unwrap(), mapping(), "{path}");
        }
    }

    #[test]
    fn rejects_wrong_channel_expired_signature_and_unknown_topic() {
        let connector = connector(vec![]);
        let bytes = serde_json::to_vec(&serde_json::json!({"channel_no":"other","timestamp":"1700000000000","sign":signing::sign("test-channel","1700000000000","test-key"),"id":"1","type":"brand"})).unwrap();
        let request = CallbackRequest {
            method: "POST",
            path_and_query: "/status",
            headers: &[],
            body: &bytes,
            received_at: Instant::from_unix_secs(1700000000),
        };
        assert_eq!(connector.verify(&request).err().unwrap().class, SupplierFailureClass::AuthSignature);
        assert!(notices("unknown", &Value::Null).is_err());
        assert_eq!(timestamp_seconds("1700000000123").unwrap(), 1700000000);
        assert!(timestamp_seconds("-1700000000").is_err());
    }

    #[test]
    fn rejects_expired_envelope_and_accepts_only_bounded_skew() {
        let connector = connector(vec![]);
        let bytes = serde_json::to_vec(&serde_json::json!({"channel_no":"test-channel","timestamp":"1700000000000","sign":signing::sign("test-channel","1700000000000","test-key"),"id":"1","type":"brand"})).unwrap();
        let mut request = CallbackRequest {
            method: "POST",
            path_and_query: "/status",
            headers: &[],
            body: &bytes,
            received_at: Instant::from_unix_secs(1700000300),
        };
        assert!(connector.verify(&request).is_ok());
        request.received_at = Instant::from_unix_secs(1700000301);
        assert_eq!(connector.verify(&request).err().unwrap().class, SupplierFailureClass::AuthSignature);
    }
    #[test]
    fn byte_identical_replay_uses_local_identity_without_collapsing_other_events() {
        let connector = connector(vec![]);
        let mut request = CallbackRequest {
            method: "POST",
            path_and_query: "/callbacks/dangaoshushu/connection-1/order",
            headers: &[],
            body: b"original",
            received_at: Instant::now(),
        };
        let original = connector.reception_id(&request);
        assert_eq!(connector.reception_id(&request), original);
        request.body = b"changed";
        assert_ne!(connector.reception_id(&request), original);
        request.body = b"original";
        request.path_and_query = "/callbacks/dangaoshushu/connection-1/price";
        assert_ne!(connector.reception_id(&request), original);
    }
    #[test]
    fn original_order_callback_preserves_fixed_signature_identity_and_acknowledgement() {
        use erp_supply::ports::connector::SupplierConnector;

        use super::super::test_support::recording_connector;

        let (connector, transport) = recording_connector(vec![]);
        let bytes = serde_json::to_vec(&serde_json::json!({
            "channel_no": "test-channel", "timestamp": 1700000000,
            "sign": "b918d6992b799b477691f8beebc45c8f", "out_order_no": "order-1"
        }))
        .unwrap();
        let request = CallbackRequest {
            method: "POST",
            path_and_query: "/callbacks/dangaoshushu/connection-1/order",
            headers: &[],
            body: &bytes,
            received_at: Instant::from_unix_secs(1700000000),
        };
        let callbacks: &dyn Callbacks = &connector;
        let verified = callbacks.verify(&request).unwrap();
        assert_eq!(connector.connection_id().as_ref(), "connection-1");
        let identity = callbacks.reception_id(&request).unwrap();
        assert!(identity.starts_with("dgss-"));
        assert_eq!(callbacks.reception_id(&request).as_deref(), Some(identity.as_str()));
        assert_eq!(verified.integrity, CallbackIntegrity::EnvelopeOnly);
        assert_eq!(verified.notices.len(), 1);
        let notice = &verified.notices[0];
        assert_eq!(notice.topic, ChangeTopic::Order);
        assert_eq!(notice.target, RefreshTarget::Order(OrderKey::Merchant("order-1".into())));
        assert!(notice.source_event_id.is_none());
        assert!(notice.observation.is_none());
        assert_eq!(verified.durable_ack.status, 200);
        assert_eq!(verified.durable_ack.content_type, "application/json");
        assert_eq!(verified.durable_ack.body, br#"{"code":200,"message":"success"}"#);
        let invalid = serde_json::to_vec(&serde_json::json!({
            "channel_no": "test-channel", "timestamp": 1700000000,
            "sign": "00000000000000000000000000000000", "out_order_no": "order-1"
        }))
        .unwrap();
        let invalid_request = CallbackRequest { body: &invalid, ..request };
        assert_eq!(
            callbacks.verify(&invalid_request).err().unwrap().class,
            SupplierFailureClass::AuthSignature
        );
        assert!(transport.requests.lock().unwrap().is_empty());
    }
}
