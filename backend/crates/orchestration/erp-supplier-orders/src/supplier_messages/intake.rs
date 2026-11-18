use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierApiConnectionId;
use erp_integration::entity::supplier_callback::SupplierRefreshHint;
use erp_integration::entity::supplier_message::{SupplierMessageData, SupplierPulledMessage};
use erp_party::SensitiveDataCodec;
use erp_supply::ports::connector::callback::{ChangeTopic, RefreshTarget};
use erp_supply::ports::connector::messages::{MessageInterpretation, MessageRefreshHint, PulledMessage};
use erp_supply::ports::connector::order::OrderKey;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

pub(super) fn receive(
    account: &str,
    connection: &SupplierApiConnectionId,
    incoming: PulledMessage,
    codec: &SensitiveDataCodec,
    at: Instant,
) -> Result<SupplierPulledMessage> {
    if incoming.body.len() > 1024 * 1024 {
        return Err(Error::ValidationError("供应商消息正文超过接收上限".into()));
    }

    let content_fingerprint = hash(&[incoming.message_type.as_bytes(), &incoming.body]);
    let id = format!("spm-{}", hash(&[account.as_bytes(), incoming.source_id.as_bytes()]));
    let (refresh_hints, quarantine_reason) = match incoming.interpretation {
        MessageInterpretation::Refresh(hints) if !hints.is_empty() => {
            (hints.iter().map(hint).collect(), None)
        },
        MessageInterpretation::Refresh(_) => (vec![], Some("EMPTY_MESSAGE_REFRESH_HINTS".into())),
        MessageInterpretation::Quarantine { reason_code } => (vec![], Some(reason_code)),
    };

    Ok(SupplierPulledMessage::received(SupplierMessageData {
        id,
        account_scope: account.into(),
        connection_id: connection.clone(),
        source_message_id: incoming.source_id,
        message_type: incoming.message_type,
        content_fingerprint,
        body_encrypted: codec.encrypt(&STANDARD.encode(incoming.body))?,
        refresh_hints,
        quarantine_reason,
        received_at: at,
    })?)
}

pub(super) fn hash(parts: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(u64::try_from(part.len()).expect("内存长度可用u64表达").to_be_bytes());
        digest.update(part);
    }
    hex::encode(digest.finalize())
}

fn hint(hint: &MessageRefreshHint) -> SupplierRefreshHint {
    let topic = match hint.topic {
        ChangeTopic::Product => "product",
        ChangeTopic::Availability => "availability",
        ChangeTopic::Price => "price",
        ChangeTopic::Region => "region",
        ChangeTopic::Address => "address",
        ChangeTopic::Order => "order",
        ChangeTopic::Refund => "refund",
    };
    let (target_kind, object_id, product_id) = match &hint.target {
        RefreshTarget::Brand(id) => ("brand", id.clone(), None),
        RefreshTarget::Product(id) => ("product", id.clone(), None),
        RefreshTarget::Sku(sku) => ("sku", sku.spec_id.clone(), sku.product_id.clone()),
        RefreshTarget::Address(id) => ("supplier_address", id.clone(), None),
        RefreshTarget::Order(order) | RefreshTarget::Refund { order, .. } => match order {
            OrderKey::Merchant(id) => ("merchant_order", id.clone(), None),
            OrderKey::External(id) => ("external_order", id.clone(), None),
        },
    };
    SupplierRefreshHint {
        topic: topic.into(),
        target_kind: target_kind.into(),
        object_id,
        product_id,
        funds_transaction_id: hint.funds_transaction_id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn funds_identity_is_copied_from_verified_protocol_instead_of_using_message_or_order_number() {
        let source = MessageRefreshHint {
            topic: ChangeTopic::Refund,
            target: RefreshTarget::Refund { order: OrderKey::External("order-id".into()), action_id: None },
            funds_transaction_id: Some("transaction-id".into()),
        };
        let stored = hint(&source);
        assert_eq!(stored.object_id, "order-id");
        assert_eq!(stored.funds_transaction_id.as_deref(), Some("transaction-id"));
        let mut unverified = source;
        unverified.funds_transaction_id = None;
        assert!(hint(&unverified).funds_transaction_id.is_none());
    }
}
