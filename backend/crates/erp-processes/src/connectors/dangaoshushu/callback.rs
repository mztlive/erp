use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::callback::{
    CallbackIntegrity, CallbackReply, CallbackRequest, Callbacks, ChangeNotice, ChangeTopic, RefreshTarget,
    VerifiedCallback,
};
use erp_supply::ports::connector::common::{ConnectorResult, SourceRevision, SupplierSku};
use erp_supply::ports::connector::order::OrderKey;
use serde_json::{Value, from_slice};

use super::parsing::{field, id, mapping};
use super::{DangaoshushuConnector, error, signing};

impl Callbacks for DangaoshushuConnector {
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
        let kind = request
            .path_and_query
            .split('?')
            .next()
            .and_then(|path| path.rsplit('/').next())
            .ok_or_else(mapping)?;
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
}
