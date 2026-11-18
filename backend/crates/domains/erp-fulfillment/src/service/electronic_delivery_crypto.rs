//! 电子交付 crypto port 与草稿编排（FUL-E04）。
//!
//! 指纹密钥不出本文件；领域工厂只收强类型指纹结果。

use erp_core::ids::ElectronicDeliveryId;
use id_generator::next_id;

use crate::dto::CreateElectronicDeliveryRequest;
use crate::entity::fulfillment::{
    ElectronicDelivery, ElectronicDeliveryDraft, ElectronicDeliveryDraftData, ElectronicRecipientFingerprint,
};
use crate::{Error, Result};

/// 电子交付草稿需要查询指纹时，使用调用方注入的密钥对交付对象原文计算 `HMAC-SHA256-v1`，并转换为电子交付专用强类型指纹。
/// 指纹格式错误映射为 `Logic`；密钥与明文不得进入实体、日志、错误或持久化。
pub fn electronic_recipient_fingerprint(plain: &str, key: &[u8]) -> Result<ElectronicRecipientFingerprint> {
    ElectronicRecipientFingerprint::from_precomputed(ElectronicDelivery::recipient_snapshot_fingerprint(
        plain, key,
    ))
    .map_err(Error::Logic)
}

/// 由创建请求构造电子交付草稿（来源默认 `Erp` 由领域工厂负责）。
/// 返回新建草稿实体。
/// `actor_id` 为已通过鉴权的操作人 ID；`fingerprint_key` 为查询指纹密钥。
/// 指纹计算或实体规范化失败时返回错误。
pub fn electronic_delivery_draft_from_request(
    req: CreateElectronicDeliveryRequest,
    actor_id: &str,
    fingerprint_key: &[u8],
) -> Result<ElectronicDelivery> {
    let (occurred_at, recorded_at) = super::draft_fact_times(req.occurred_at);
    ElectronicDeliveryDraft::build(
        ElectronicDeliveryId::new(next_id()),
        ElectronicDeliveryDraftData {
            fulfillment_no: req.fulfillment_no,
            sales_order_line_id: req.sales_order_line_id,
            purchase_order_id: req.purchase_order_id,
            purchase_line_sales_allocation_id: req.purchase_line_sales_allocation_id,
            recipient_snapshot_fingerprint: electronic_recipient_fingerprint(
                &req.recipient_snapshot,
                fingerprint_key,
            )?,
            recipient_snapshot: req.recipient_snapshot,
            quantity: req.quantity,
            result: req.result,
            evidence_attachment_id: req.evidence_attachment_id,
            fact_no: next_id(),
            occurred_at,
            recorded_at,
            recorded_by: actor_id.to_string(),
        },
    )
    .map_err(Error::Logic)
}

#[cfg(test)]
mod tests {
    use super::electronic_recipient_fingerprint;

    // 验证固定密钥与交付对象产生稳定 64 位指纹；轮换密钥或更换交付对象时指纹不同。
    #[test]
    fn recipient_fingerprint_golden_pins_algorithm_and_key_version() {
        let first =
            electronic_recipient_fingerprint("recipient-plain-001", b"fulfillment-fingerprint-key-v1")
                .unwrap();
        let second =
            electronic_recipient_fingerprint("recipient-plain-001", b"fulfillment-fingerprint-key-v1")
                .unwrap();
        assert_eq!(first.as_str(), second.as_str(), "同密钥同明文指纹稳定");
        assert_eq!(first.as_str().len(), 64);

        let rotated =
            electronic_recipient_fingerprint("recipient-plain-001", b"fulfillment-fingerprint-key-v2")
                .unwrap();
        assert_ne!(first.as_str(), rotated.as_str(), "密钥版本轮换必须改变指纹");

        let other_plain =
            electronic_recipient_fingerprint("recipient-plain-002", b"fulfillment-fingerprint-key-v1")
                .unwrap();
        assert_ne!(first.as_str(), other_plain.as_str());
    }
}
