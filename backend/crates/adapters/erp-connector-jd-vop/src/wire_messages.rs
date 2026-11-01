//! 京东5871消息行及官方消息类型guide解析；原始行字节独立保留为本地加密证据。

use std::collections::{BTreeMap, BTreeSet};

use erp_supply::ports::connector::callback::{ChangeTopic, RefreshTarget};
use erp_supply::ports::connector::common::{ConnectorResult, SupplierSku};
use erp_supply::ports::connector::messages::{MessageInterpretation, MessageRefreshHint, PulledMessage};
use erp_supply::ports::connector::order::OrderKey;
use serde::Deserialize;
use serde_json::value::RawValue;

use super::mapping;
use super::wire::{WireId, WireText};

#[derive(Deserialize)]
struct MessageRecord {
    id: WireId,
    #[serde(rename = "type")]
    kind: Option<Box<RawValue>>,
    yn: Option<Box<RawValue>>,
    content: Option<Box<RawValue>>,
}

pub(super) fn pulled(raw: &RawValue, verified_types: &BTreeSet<u32>) -> ConnectorResult<PulledMessage> {
    let record: MessageRecord =
        serde_json::from_str(raw.get()).map_err(|_| mapping("京东消息缺少有效来源消息号"))?;
    let message_type =
        record.kind.as_ref().and_then(|value| scalar(value).ok()).unwrap_or_else(|| "UNKNOWN".into());
    let interpretation = match interpret(&record, &message_type, verified_types) {
        Ok(hints) => MessageInterpretation::Refresh(hints),
        Err(code) => MessageInterpretation::Quarantine { reason_code: code.into() },
    };
    Ok(PulledMessage {
        source_id: record.id.0,
        message_type,
        body: raw.get().as_bytes().to_vec(),
        interpretation,
    })
}

fn interpret(
    record: &MessageRecord,
    kind: &str,
    verified_types: &BTreeSet<u32>,
) -> Result<Vec<MessageRefreshHint>, &'static str> {
    let kind = kind.parse::<u32>().map_err(|_| "JD_VOP_MESSAGE_TYPE_INVALID")?;
    if !verified_types.contains(&kind) {
        return Err("JD_VOP_MESSAGE_TYPE_NOT_AUTHORIZED");
    }
    if record.yn.as_ref().and_then(|value| scalar(value).ok()).as_deref() != Some("1") {
        return Err("JD_VOP_MESSAGE_VALIDITY_UNKNOWN");
    }
    let content = record.content.as_ref().ok_or("JD_VOP_MESSAGE_CONTENT_MISSING")?;
    if kind == 50 {
        return address_hints(content);
    }
    let payload = payload(content, &record.id.0, kind)?;
    match kind {
        1 => order_hints(&payload, "pOrder"),
        2 => sku_hint(&payload, "skuId", ChangeTopic::Price),
        4 | 6 | 126 => sku_hint(&payload, "skuId", ChangeTopic::Availability),
        // 取消、支付失败和等待收货仅提供订单线索，不包含资金交易号或到账证明。
        5 | 10 | 12 | 14 | 15 | 18 | 31 => order_hints(&payload, "orderId"),
        16 => sku_hint(&payload, "skuId", ChangeTopic::Product),
        100 => sku_hint(&payload, "sku_id", ChangeTopic::Price),
        132 => sku_hint(&payload, "skuId", ChangeTopic::Region),
        48 => Err("JD_VOP_POOL_BASELINE_REQUIRED"),
        _ => Err("JD_VOP_MESSAGE_TYPE_UNSUPPORTED"),
    }
}

fn scalar(value: &RawValue) -> Result<String, &'static str> {
    serde_json::from_str::<WireText>(value.get())
        .map(|value| value.0)
        .map_err(|_| "JD_VOP_MESSAGE_SCALAR_INVALID")
}

fn object(raw: &RawValue) -> Result<BTreeMap<String, Box<RawValue>>, &'static str> {
    if raw.get().starts_with('"') {
        let decoded: String =
            serde_json::from_str(raw.get()).map_err(|_| "JD_VOP_MESSAGE_CONTENT_INVALID")?;
        serde_json::from_str(&decoded).map_err(|_| "JD_VOP_MESSAGE_CONTENT_INVALID")
    } else {
        serde_json::from_str(raw.get()).map_err(|_| "JD_VOP_MESSAGE_CONTENT_INVALID")
    }
}

fn payload(
    raw: &RawValue,
    message_id: &str,
    kind: u32,
) -> Result<BTreeMap<String, Box<RawValue>>, &'static str> {
    let mut fields = object(raw)?;
    if fields.contains_key("content") {
        if fields.get("id").is_some_and(|value| id(value).as_deref() != Ok(message_id))
            || fields.get("type").is_some_and(|value| {
                scalar(value).ok().and_then(|value| value.parse::<u32>().ok()) != Some(kind)
            })
        {
            return Err("JD_VOP_MESSAGE_ENVELOPE_CONFLICT");
        }
        return object(&fields.remove("content").ok_or("JD_VOP_MESSAGE_CONTENT_MISSING")?);
    }
    Ok(fields)
}

fn id(raw: &RawValue) -> Result<String, &'static str> {
    let value = serde_json::from_str::<WireId>(raw.get()).map_err(|_| "JD_VOP_MESSAGE_TARGET_INVALID")?.0;
    if value.bytes().all(|byte| byte == b'0') {
        return Err("JD_VOP_MESSAGE_TARGET_INVALID");
    }
    Ok(value)
}

fn target(payload: &BTreeMap<String, Box<RawValue>>, field: &str) -> Result<String, &'static str> {
    let raw = payload.get(field).ok_or("JD_VOP_MESSAGE_TARGET_MISSING")?;
    id(raw)
}

fn sku_hint(
    payload: &BTreeMap<String, Box<RawValue>>,
    field: &str,
    topic: ChangeTopic,
) -> Result<Vec<MessageRefreshHint>, &'static str> {
    Ok(vec![MessageRefreshHint {
        topic,
        target: RefreshTarget::Sku(SupplierSku { product_id: None, spec_id: target(payload, field)? }),
        funds_transaction_id: None,
    }])
}

fn order_hints(
    payload: &BTreeMap<String, Box<RawValue>>,
    field: &str,
) -> Result<Vec<MessageRefreshHint>, &'static str> {
    let order = OrderKey::External(target(payload, field)?);
    Ok(vec![MessageRefreshHint {
        topic: ChangeTopic::Order,
        target: RefreshTarget::Order(order),
        funds_transaction_id: None,
    }])
}

fn address_hints(raw: &RawValue) -> Result<Vec<MessageRefreshHint>, &'static str> {
    let body = if raw.get().starts_with('"') {
        serde_json::from_str::<String>(raw.get()).map_err(|_| "JD_VOP_ADDRESS_NOTICE_INVALID")?
    } else {
        raw.get().to_owned()
    };
    let records: Vec<Box<RawValue>> = if body.trim_start().starts_with('[') {
        serde_json::from_str(&body).map_err(|_| "JD_VOP_ADDRESS_NOTICE_INVALID")?
    } else {
        vec![serde_json::from_str(&body).map_err(|_| "JD_VOP_ADDRESS_NOTICE_INVALID")?]
    };
    if records.is_empty() || records.len() > 100 {
        return Err("JD_VOP_ADDRESS_NOTICE_INVALID");
    }
    let mut ids = BTreeSet::new();
    for record in records {
        let mut fields = object(&record)?;
        if fields.get("type").is_some_and(|value| scalar(value).as_deref() != Ok("50")) {
            return Err("JD_VOP_MESSAGE_ENVELOPE_CONFLICT");
        }
        if let Some(content) = fields.remove("content") {
            fields = object(&content)?;
        }
        validate_address_fields(&fields)?;
        ids.insert(target(&fields, "areaId")?);
    }
    Ok(ids
        .into_iter()
        .map(|id| MessageRefreshHint {
            topic: ChangeTopic::Address,
            target: RefreshTarget::Address(id),
            funds_transaction_id: None,
        })
        .collect())
}

fn validate_address_fields(fields: &BTreeMap<String, Box<RawValue>>) -> Result<(), &'static str> {
    for (field, maximum) in [("areaLevel", 5), ("operateType", 3)] {
        if let Some(value) = fields.get(field) {
            let value = scalar(value)?.parse::<u8>().map_err(|_| "JD_VOP_ADDRESS_NOTICE_INVALID")?;
            if value == 0 || value > maximum {
                return Err("JD_VOP_ADDRESS_NOTICE_INVALID");
            }
        }
    }
    // 缺少层级时仅作为连接级更新线索；不把areaId当作省、市等任一层级编码。
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(kind: u32, content: &str) -> PulledMessage {
        let raw = format!(
            "{{\"id\":9007199254740993,\"type\":{kind},\"yn\":1,\"clientId\":\"account\",\"content\":{content}}}"
        );
        let raw: Box<RawValue> = serde_json::from_str(&raw).unwrap();
        pulled(
            raw.as_ref(),
            &BTreeSet::from([1, 2, 4, 5, 6, 10, 12, 14, 15, 16, 18, 31, 48, 50, 100, 126, 132]),
        )
        .unwrap()
    }

    #[test]
    fn split_message_uses_original_parent_and_preserves_long_source_and_target_ids() {
        let message = message(1, r#""{\"pOrder\":9007199254740995}""#);
        assert_eq!(message.source_id, "9007199254740993");
        let MessageInterpretation::Refresh(hints) = message.interpretation else {
            panic!("refresh expected");
        };
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].target, RefreshTarget::Order(OrderKey::External("9007199254740995".into())));
        assert!(std::str::from_utf8(&message.body).unwrap().contains("9007199254740993"));
    }

    #[test]
    fn official_type_mapping_never_applies_delivery_cancellation_or_money_directly() {
        for kind in [5, 10, 12, 14, 15, 18, 31] {
            let MessageInterpretation::Refresh(hints) = message(kind, r#"{"orderId":"123"}"#).interpretation
            else {
                panic!("refresh expected");
            };
            assert_eq!(hints.len(), 1);
            assert_eq!(hints[0].topic, ChangeTopic::Order);
            assert!(hints.iter().all(|hint| hint.funds_transaction_id.is_none()));
            assert_eq!(hints[0].target, RefreshTarget::Order(OrderKey::External("123".into())));
        }
    }

    #[test]
    fn unknown_type_invalid_body_and_pool_change_are_durable_quarantine() {
        for (kind, content) in [
            (999, r#"{"skuId":"123"}"#),
            (2, r#""not json""#),
            (48, r#"{"page_num":"12"}"#),
            (2, r#"{"skuId":1.5}"#),
        ] {
            let result = message(kind, content);
            assert!(matches!(result.interpretation, MessageInterpretation::Quarantine { .. }));
            assert!(!result.body.is_empty());
        }
        let raw =
            r#"{"id":"1","type":2,"yn":1,"content":{"id":"another","type":2,"content":{"skuId":"123"}}}"#;
        let raw: Box<RawValue> = serde_json::from_str(raw).unwrap();
        let parsed = pulled(raw.as_ref(), &BTreeSet::from([2])).unwrap();
        assert!(matches!(parsed.interpretation, MessageInterpretation::Quarantine { .. }));
    }

    #[test]
    fn pool_sku_removal_and_sale_messages_only_schedule_current_sku_checks() {
        for kind in [4, 6, 126] {
            let MessageInterpretation::Refresh(hints) =
                message(kind, r#"{"skuId":"123","state":2,"page_num":"88"}"#).interpretation
            else {
                panic!("refresh expected");
            };
            assert_eq!(hints[0].topic, ChangeTopic::Availability);
            assert_eq!(
                hints[0].target,
                RefreshTarget::Sku(SupplierSku { product_id: None, spec_id: "123".into() })
            );
        }
    }

    #[test]
    fn address_changes_keep_source_ids_without_guessing_hierarchy_or_mutating_order_addresses() {
        for content in [
            r#"{"areaId":"123","areaName":"街道新名称","parentId":"12","areaLevel":5,"operateType":2}"#,
            r#""{\"id\":\"source-push\",\"type\":50,\"content\":{\"areaId\":\"123\",\"areaLevel\":5,\"operateType\":3}}""#,
            r#"[{"id":"first","type":50,"content":{"areaId":"123","areaLevel":5,"operateType":1}}]"#,
            r#"{"areaId":"123"}"#,
        ] {
            let incoming = message(50, content);
            let MessageInterpretation::Refresh(hints) = incoming.interpretation else {
                panic!("address refresh expected");
            };
            assert_eq!(hints.len(), 1);
            assert_eq!(hints[0].topic, ChangeTopic::Address);
            assert_eq!(hints[0].target, RefreshTarget::Address("123".into()));
            assert!(hints[0].funds_transaction_id.is_none());
        }
        for content in [
            r#"{"areaName":"未知编码"}"#,
            r#"{"areaId":"123","areaLevel":6}"#,
            r#"{"areaId":"123","operateType":0}"#,
            r#"[{"type":18,"content":{"areaId":"123"}}]"#,
        ] {
            assert!(matches!(message(50, content).interpretation, MessageInterpretation::Quarantine { .. }));
        }
    }
}
