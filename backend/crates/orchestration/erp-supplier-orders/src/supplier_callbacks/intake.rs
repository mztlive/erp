use erp_core::ids::SupplierApiConnectionId;
use erp_integration::entity::supplier_callback::{SupplierCallbackReceipt, SupplierRefreshHint};
use erp_party::SensitiveDataCodec;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::callback::{
    CallbackIntegrity, CallbackReply, CallbackRequest, Callbacks, ChangeNotice, ChangeTopic, RefreshTarget,
    VerifiedCallback,
};
use erp_supply::ports::connector::order::OrderKey;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{Error, Result};

pub(super) fn prepare(
    callbacks: &dyn Callbacks,
    connection: &SupplierApiConnectionId,
    scope: &str,
    codec: &SensitiveDataCodec,
    request: &CallbackRequest<'_>,
) -> Result<(SupplierCallbackReceipt, CallbackReply)> {
    let verified = callbacks.verify(request).map_err(|failure| {
        if failure.class == SupplierFailureClass::AuthSignature {
            Error::Unauthenticated(failure.summary)
        } else {
            Error::ValidationError(failure.summary)
        }
    })?;
    validate(&verified)?;
    let hints = verified.notices.iter().map(hint).collect::<Result<Vec<_>>>()?;
    let route = if hints.iter().all(|hint| matches!(hint.topic.as_str(), "order" | "refund")) {
        "order"
    } else {
        "catalog"
    };
    let event = verified.notices[0].source_event_id.clone();
    let fingerprint =
        digest(codec, &[request.method.as_bytes(), request.path_and_query.as_bytes(), request.body]);
    let id = match &event {
        Some(event) => {
            format!("spc-event-{}", digest(codec, &[connection.as_ref().as_bytes(), event.as_bytes()]))
        },
        None => callbacks.reception_id(request).unwrap_or_else(|| {
            format!(
                "spc-{}",
                digest(codec, &[connection.as_ref().as_bytes(), scope.as_bytes(), fingerprint.as_bytes()])
            )
        }),
    };
    let plaintext =
        std::str::from_utf8(request.body).map_err(|_| Error::ValidationError("推送正文不是 UTF-8".into()))?;
    let mut receipt = SupplierCallbackReceipt::received(
        id,
        connection.clone(),
        request.received_at,
        route.into(),
        codec.encrypt(plaintext)?,
        hints,
    )?;
    receipt.source_event_id = event;
    receipt.integrity = match verified.integrity {
        CallbackIntegrity::EnvelopeOnly => "envelope_only",
        CallbackIntegrity::BodyAuthenticated => "body_authenticated",
    }
    .into();
    receipt.content_fingerprint = Some(fingerprint);
    if route == "catalog" {
        receipt.ensure_catalog()?;
    }
    Ok((receipt, verified.durable_ack))
}

fn validate(verified: &VerifiedCallback) -> Result<()> {
    let first =
        verified.notices.first().ok_or_else(|| Error::ValidationError("供应商回调缺少刷新意图".into()))?;
    if verified.notices.len() > 1000
        || !(200..300).contains(&verified.durable_ack.status)
        || verified.durable_ack.content_type.is_empty()
        || !verified.durable_ack.content_type.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
        || first.source_event_id.as_ref().is_some_and(|id| id.trim().is_empty() || id.len() > 256)
        || verified
            .notices
            .iter()
            .any(|notice| notice.source_event_id != first.source_event_id || notice.observation.is_some())
    {
        return Err(Error::ValidationError(
            "回调身份、应答或消费模式无效；当前接收器只支持可补查通知".into(),
        ));
    }
    Ok(())
}

pub(super) fn verify_replay(
    codec: &SensitiveDataCodec,
    existing: &SupplierCallbackReceipt,
    incoming: &SupplierCallbackReceipt,
) -> Result<()> {
    let same = existing.connection_id == incoming.connection_id
        && existing.source_event_id == incoming.source_event_id
        && match &existing.content_fingerprint {
            Some(fingerprint) => incoming.content_fingerprint.as_ref() == Some(fingerprint),
            None => codec.decrypt(&existing.body_encrypted)? == codec.decrypt(&incoming.body_encrypted)?,
        };
    if !same {
        return Err(Error::ConflictError("同一回调身份的连接或报文不一致，原接收证据保持不变".into()));
    }
    Ok(())
}

fn digest(codec: &SensitiveDataCodec, parts: &[&[u8]]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(codec.fingerprint_key()).expect("HMAC接受任意长度密钥");
    mac.update(b"erp-supplier-callback-v1\0");
    for part in parts {
        mac.update(&u64::try_from(part.len()).expect("内存长度可表示为u64").to_be_bytes());
        mac.update(part);
    }
    hex::encode(mac.finalize().into_bytes())
}

fn hint(notice: &ChangeNotice) -> Result<SupplierRefreshHint> {
    let topic = match notice.topic {
        ChangeTopic::Product => "product",
        ChangeTopic::Availability => "availability",
        ChangeTopic::Price => "price",
        ChangeTopic::Region => "region",
        ChangeTopic::Order => "order",
        ChangeTopic::Refund => "refund",
        ChangeTopic::Address => return Err(Error::ValidationError("回调暂不支持地址字典消费".into())),
    };
    let (target_kind, object_id, product_id) = match &notice.target {
        RefreshTarget::Brand(id) => ("brand", id.clone(), None),
        RefreshTarget::Product(id) => ("product", id.clone(), None),
        RefreshTarget::Sku(sku) => ("sku", sku.spec_id.clone(), sku.product_id.clone()),
        RefreshTarget::Order(OrderKey::Merchant(id)) => ("merchant_order", id.clone(), None),
        RefreshTarget::Order(OrderKey::External(id)) => ("external_order", id.clone(), None),
        _ => return Err(Error::ValidationError("回调目标尚无可恢复消费者".into())),
    };
    if matches!(topic, "order" | "refund") != matches!(target_kind, "merchant_order" | "external_order") {
        return Err(Error::ValidationError("回调主题与目标不一致".into()));
    }
    Ok(SupplierRefreshHint {
        topic: topic.into(),
        target_kind: target_kind.into(),
        object_id,
        product_id,
        funds_transaction_id: None,
    })
}
