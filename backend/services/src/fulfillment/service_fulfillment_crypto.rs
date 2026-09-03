//! 服务履约 crypto port 与草稿编排（FUL-E05）。
//!
//! 双指纹分别计算、类型不可混用；领域工厂只收强类型结果。

use entities::common::time::Instant;
use entities::fulfillment::{
    ServiceFulfillment, ServiceFulfillmentDraft, ServiceFulfillmentDraftData, ServiceLocationFingerprint,
    ServiceRecipientFingerprint,
};
use entities::ids::ServiceFulfillmentId;
use id_generator::next_id;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::CreateServiceFulfillmentRequest;

/// 在 Service/crypto port 计算交付对象快照指纹。
///
/// # 参数
/// * `plain` - 交付对象原文
/// * `key` - 查询指纹密钥字节
///
/// # 返回
/// 返回交付对象强类型指纹（算法 `HMAC-SHA256-v1`）。
///
/// # 错误
/// 指纹格式非法时返回错误。
///
/// # 约束
/// 密钥与明文不得进入实体、错误、日志或持久化。
pub fn service_recipient_fingerprint(plain: &str, key: &[u8]) -> Result<ServiceRecipientFingerprint> {
    ServiceRecipientFingerprint::from_precomputed(ServiceFulfillment::recipient_snapshot_fingerprint(
        plain, key,
    ))
    .map_err(Error::Logic)
}

/// 在 Service/crypto port 计算服务地点指纹。
///
/// # 参数
/// * `plain` - 服务地点原文
/// * `key` - 查询指纹密钥字节
///
/// # 返回
/// 返回服务地点强类型指纹（算法 `HMAC-SHA256-v1`，与交付对象指纹不可混用）。
///
/// # 错误
/// 指纹格式非法时返回错误。
///
/// # 约束
/// 密钥与明文不得进入实体、错误、日志或持久化。
pub fn service_location_fingerprint(plain: &str, key: &[u8]) -> Result<ServiceLocationFingerprint> {
    ServiceLocationFingerprint::from_precomputed(ServiceFulfillment::service_location_fingerprint(plain, key))
        .map_err(Error::Logic)
}

/// 由创建请求构造线下服务履约草稿（来源默认 `Erp` 由领域工厂负责）。
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
/// 双指纹计算或实体规范化失败时返回错误。
pub(super) fn service_fulfillment_draft_from_request(
    req: CreateServiceFulfillmentRequest,
    actor: &AuditActor,
    fingerprint_key: &[u8],
) -> Result<ServiceFulfillment> {
    let occurred_at = Instant::from_unix_secs(req.occurred_at);
    let recorded_at = Instant::now();
    ServiceFulfillmentDraft::build(
        ServiceFulfillmentId::new(next_id()),
        ServiceFulfillmentDraftData {
            fulfillment_no: req.fulfillment_no,
            sales_order_line_id: req.sales_order_line_id,
            purchase_order_id: req.purchase_order_id,
            purchase_line_sales_allocation_id: req.purchase_line_sales_allocation_id,
            recipient_snapshot: req.recipient_snapshot.clone(),
            recipient_snapshot_fingerprint: service_recipient_fingerprint(
                &req.recipient_snapshot,
                fingerprint_key,
            )?,
            quantity: req.quantity,
            result: req.result,
            evidence_attachment_id: req.evidence_attachment_id,
            service_location_encrypted: req.service_location.clone(),
            service_location_fingerprint: service_location_fingerprint(
                &req.service_location,
                fingerprint_key,
            )?,
            service_started_at: req.service_started_at.map(Instant::from_unix_secs),
            service_ended_at: req.service_ended_at.map(Instant::from_unix_secs),
            completion_note: req.completion_note,
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
    use super::{service_location_fingerprint, service_recipient_fingerprint};

    /// 双指纹 golden：分别固定算法与密钥版本，且类型不可混用。
    #[test]
    fn dual_fingerprints_golden_pin_algorithm_and_key_version() {
        let recipient =
            service_recipient_fingerprint("recipient-plain-001", b"fulfillment-fingerprint-key-v1").unwrap();
        let location =
            service_location_fingerprint("location-plain-001", b"fulfillment-fingerprint-key-v1").unwrap();
        assert_eq!(recipient.as_str().len(), 64);
        assert_eq!(location.as_str().len(), 64);
        assert_eq!(
            service_recipient_fingerprint("recipient-plain-001", b"fulfillment-fingerprint-key-v1")
                .unwrap()
                .as_str(),
            recipient.as_str(),
            "同密钥同明文指纹稳定"
        );
        let rotated =
            service_recipient_fingerprint("recipient-plain-001", b"fulfillment-fingerprint-key-v2").unwrap();
        assert_ne!(recipient.as_str(), rotated.as_str(), "密钥版本轮换必须改变指纹");
        let rotated_location =
            service_location_fingerprint("location-plain-001", b"fulfillment-fingerprint-key-v2").unwrap();
        assert_ne!(
            location.as_str(),
            rotated_location.as_str(),
            "地点指纹同样绑定密钥版本"
        );
        fn accepts_recipient(_: &super::ServiceRecipientFingerprint) {}
        fn accepts_location(_: &super::ServiceLocationFingerprint) {}
        accepts_recipient(&recipient);
        accepts_location(&location);
    }
}
