//! 电子交付 crypto port 与草稿编排（FUL-E04）。
//!
//! 指纹密钥不出本文件；领域工厂只收强类型指纹结果。

use entities::common::time::Instant;
use entities::fulfillment::{
    ElectronicDelivery, ElectronicDeliveryDraft, ElectronicDeliveryDraftData, ElectronicRecipientFingerprint,
};
use entities::ids::ElectronicDeliveryId;
use id_generator::next_id;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::CreateElectronicDeliveryRequest;

/// 在 Service/crypto port 计算交付对象快照指纹。
///
/// # 参数
/// * `plain` - 交付对象原文（不透明快照的上游明文）
/// * `key` - 查询指纹密钥字节
///
/// # 返回
/// 返回强类型指纹（算法 `HMAC-SHA256-v1`，密钥版本由调用方注入的 key 决定）。
///
/// # 错误
/// 指纹格式非法时返回错误。
///
/// # 约束
/// 密钥与明文不得进入实体、错误、日志或持久化。
pub fn electronic_recipient_fingerprint(plain: &str, key: &[u8]) -> Result<ElectronicRecipientFingerprint> {
    ElectronicRecipientFingerprint::from_precomputed(ElectronicDelivery::recipient_snapshot_fingerprint(
        plain, key,
    ))
    .map_err(Error::Logic)
}

/// 由创建请求构造电子交付草稿（来源默认 `Erp` 由领域工厂负责）。
///
/// # 参数
/// * `req` - 已通过校验的创建请求
/// * `actor` - 已通过鉴权的审计操作人
/// * `fingerprint_key` - 查询指纹密钥
///
/// # 返回
/// 返回新建草稿实体。
///
/// # 错误
/// 指纹计算或实体规范化失败时返回错误。
pub(super) fn electronic_delivery_draft_from_request(
    req: CreateElectronicDeliveryRequest,
    actor: &AuditActor,
    fingerprint_key: &[u8],
) -> Result<ElectronicDelivery> {
    let occurred_at = Instant::from_unix_secs(req.occurred_at);
    let recorded_at = Instant::now();
    ElectronicDeliveryDraft::build(
        ElectronicDeliveryId::new(next_id()),
        ElectronicDeliveryDraftData {
            fulfillment_no: req.fulfillment_no,
            sales_order_line_id: req.sales_order_line_id,
            purchase_order_id: req.purchase_order_id,
            purchase_line_sales_allocation_id: req.purchase_line_sales_allocation_id,
            recipient_snapshot: req.recipient_snapshot.clone(),
            recipient_snapshot_fingerprint: electronic_recipient_fingerprint(
                &req.recipient_snapshot,
                fingerprint_key,
            )?,
            quantity: req.quantity,
            result: req.result,
            evidence_attachment_id: req.evidence_attachment_id,
            fact_no: next_id(),
            occurred_at,
            recorded_at,
            recorded_by: actor.id().to_string(),
        },
    )
    .map_err(Error::Logic)
}

#[cfg(test)]
mod tests {
    use super::electronic_recipient_fingerprint;

    /// 指纹 golden：固定算法（HMAC-SHA256）与密钥版本，密钥轮换则指纹变化。
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
