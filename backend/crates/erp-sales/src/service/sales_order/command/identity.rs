//! Stable sales command receipt identities and payload fingerprints.
use sha2::{Digest, Sha256};

use crate::dto::sales_order::{HandoverSalesOrderRequest, SubmitSalesOrderRequest};
use crate::{Error, Result};

/// 为销售提交幂等命令生成不泄露原始幂等键的稳定收据 ID。
pub fn sales_submission_audit_id(actor_id: &str, sales_order_id: &str, idempotency_key: &str) -> String {
    format!(
        "sales-order-submit-{}",
        hex::encode(Sha256::digest(format!("{actor_id}|{sales_order_id}|{idempotency_key}").as_bytes()))
    )
}

/// 锁定同一幂等键可重放的完整请求身份。
///
/// # 错误
/// 请求序列化失败时返回内部错误。
pub fn sales_submission_fingerprint(
    actor_id: &str,
    sales_order_id: &str,
    request: &SubmitSalesOrderRequest,
) -> Result<String> {
    let payload = serde_json::to_vec(&(actor_id, sales_order_id, request))
        .map_err(|error| Error::Internal(format!("销售提交命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(payload)))
}

/// Compute the unchanged sales command identity from its complete original input.
///
/// Errors preserve serialization failure; no persistence or approval binding occurs here.
pub fn sales_order_create_audit_id(actor_id: &str, idempotency_key: &str) -> String {
    let mut digest = Sha256::new();
    for part in [actor_id, idempotency_key.trim()] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("sales-order-create-{}", hex::encode(digest.finalize()))
}

/// Compute the unchanged sales command identity from its complete original input.
///
/// Errors preserve serialization failure; no persistence or approval binding occurs here.
pub fn sales_order_create_fingerprint<T: serde::Serialize>(actor_id: &str, request: &T) -> Result<String> {
    let payload = serde_json::to_vec(&(actor_id, request))
        .map_err(|error| Error::Internal(format!("销售建单命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(payload)))
}

/// 为销售责任交接幂等命令生成不泄露原始幂等键的稳定收据 ID。
pub fn sales_handover_audit_id(actor_id: &str, sales_order_id: &str, idempotency_key: &str) -> String {
    format!(
        "sales-order-handover-{}",
        hex::encode(Sha256::digest(format!("{actor_id}|{sales_order_id}|{idempotency_key}").as_bytes()))
    )
}

/// 锁定同一幂等键可重放的完整交接请求身份。
///
/// # 错误
/// 请求序列化失败时返回内部错误。
pub fn sales_handover_fingerprint(
    actor_id: &str,
    sales_order_id: &str,
    request: &HandoverSalesOrderRequest,
) -> Result<String> {
    let payload = serde_json::to_vec(&(actor_id, sales_order_id, request))
        .map_err(|error| Error::Internal(format!("销售交接命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(payload)))
}

/// 构造销售责任交接审计收据的消息体。
///
/// 收据绑定完整交接指纹、目标负责人、交接原因与本次转交的验收任务清单；
/// 回放时凭指纹前缀判定同一幂等键是否重放同一载荷。
///
/// # 参数
/// * `fingerprint` - 本次交接请求的完整载荷指纹
/// * `target` - 目标负责销售
/// * `reason` - 交接原因
/// * `transferred` - 本次原子转交的开放验收任务 ID 清单
///
/// # 返回
/// 返回写入审计收据 `message` 字段的稳定消息体。
///
/// # 错误
/// 无。
pub fn sales_handover_audit_message(
    fingerprint: &str,
    target: &str,
    reason: &str,
    transferred: &[String],
) -> String {
    format!(
        "command_sha256={fingerprint};target={target};reason={reason};acceptance={}",
        transferred.join(",")
    )
}

/// 校验已存交接审计收据是否绑定同一交接指纹。
///
/// 接受携带扩展段的新格式（指纹前缀加 `;` 后续段）与仅指纹的旧格式；
/// 指纹不同、缺失或前后空白不一致时拒绝，为异载荷同幂等键冲突。
///
/// # 参数
/// * `message` - 已存审计收据的 `message` 字段
/// * `expected_fingerprint` - 本次回放期望的完整载荷指纹
///
/// # 返回
/// 同一载荷重放时返回 `true`，否则返回 `false`。
///
/// # 错误
/// 无。
pub fn sales_handover_fingerprint_matches(message: Option<&str>, expected_fingerprint: &str) -> bool {
    let Some(message) = message else {
        return false;
    };
    let exact = format!("command_sha256={expected_fingerprint}");
    message == exact || message.starts_with(&format!("{exact};"))
}

#[cfg(test)]
mod compile_probe_equivalence_tests {
    use sha2::{Digest, Sha256};

    use super::{
        HandoverSalesOrderRequest, SubmitSalesOrderRequest, sales_handover_audit_id,
        sales_handover_audit_message, sales_handover_fingerprint, sales_handover_fingerprint_matches,
        sales_submission_fingerprint,
    };

    #[test]
    fn complete_submission_payload_and_hash_match_frozen_goods_and_voucher_bytes() {
        // 固定外部黄金字节与摘要；反序列化和重序列化均使用真实请求及嵌套领域类型。
        let cases = [
            (
                "goods",
                r#"["操作者-🧾","销售单-一",{"version":7,"idempotency_key":"请求－键","contract_id":"合同-1","draft":{"editor_user_id":"编辑人-1","requested_contract_revision_id":"合同版本-1","project_name":"","business_remark":"备注𠮷","voucher_category_sku_id":null,"voucher_expiry_at":null,"receivable_due_date":null,"lines":[{"line_no":1,"line_type":"GOODS_SERVICE","sales_tax_rate":"0.130000","item_name_snapshot":"商品－测试","spec_snapshot":"规格 A","unit_snapshot":"件","goods":{"sku_id":"sku-1","sku_revision_id":"sku-r1","welfare_scenario":"ANNUAL_GIFT_BAG","service_region":"上海","fulfillment_due_at":1800000000,"quantity":"2.000000","base_unit_code":"件","unit_price_gross":"100.0000"},"voucher":null}]}}]"#,
                "5947b0f9feb013e11f451e34a3297602566a3edc3417eb27ba49f6ea6399b296",
            ),
            (
                "voucher",
                r#"["操作者-🧾","销售单-一",{"version":7,"idempotency_key":"请求－键","contract_id":"合同-1","draft":{"editor_user_id":"编辑人-1","requested_contract_revision_id":"合同版本-1","project_name":"卡券项目","business_remark":"备注𠮷","voucher_category_sku_id":"券类目-1","voucher_expiry_at":1800000001,"receivable_due_date":"2026-12-31","lines":[{"line_no":2,"line_type":"VOUCHER","sales_tax_rate":"0.060000","item_name_snapshot":"电子券－测试","spec_snapshot":null,"unit_snapshot":"张","goods":null,"voucher":{"face_value":"100.00","card_count":2,"unit_price_gross":"90.0000","face_value_total":"200.00","transaction_amount":"180.00","gift_amount":"20.00","gift_rate":"0.111111","card_form":"ELECTRONIC"}}]}}]"#,
                "a34501e69937f7a41c7a14e46ae7da8e713b3b94ca3f2f16615826065f754b42",
            ),
        ];
        for (label, golden_payload, golden_hash) in cases {
            let (actor, order, request): (String, String, SubmitSalesOrderRequest) =
                serde_json::from_str(golden_payload).unwrap();
            let serialized = serde_json::to_vec(&(actor.as_str(), order.as_str(), &request)).unwrap();
            assert_eq!(serialized, golden_payload.as_bytes(), "{label}: complete JSON");
            assert_eq!(
                hex::encode(Sha256::digest(&serialized)),
                golden_hash,
                "{label}: borrowed payload digest"
            );
            assert_eq!(
                sales_submission_fingerprint(&actor, &order, &request).unwrap(),
                golden_hash,
                "{label}: actual submission fingerprint"
            );
        }
    }

    fn handover_request(
        target: &str,
        org: Option<&str>,
        reason: &str,
        key: &str,
    ) -> HandoverSalesOrderRequest {
        HandoverSalesOrderRequest {
            expected_version: 3,
            target_owner_user_id: target.to_string(),
            target_business_org_unit_id: org.map(str::to_string),
            reason: reason.to_string(),
            idempotency_key: key.to_string(),
        }
    }

    /// 同一交接载荷可幂等重放：指纹与收据稳定。
    #[test]
    fn same_handover_payload_replays_to_same_receipt_identity() {
        let request = handover_request("sales-b", None, "轮换", "key-1");
        assert_eq!(
            sales_handover_fingerprint("actor-1", "so-1", &request).unwrap(),
            sales_handover_fingerprint("actor-1", "so-1", &request.clone()).unwrap()
        );
        assert_eq!(
            sales_handover_audit_id("actor-1", "so-1", "key-1"),
            sales_handover_audit_id("actor-1", "so-1", "key-1")
        );
    }

    /// 交接审计收据往返：构造的消息体必须被同一指纹回放接受，
    /// 异指纹、空收据必须拒绝；写入格式与回放判定由同一对函数承载。
    #[test]
    fn handover_audit_message_round_trips_through_replay_match() {
        let transferred = vec!["accept-1".to_string()];
        let stored = sales_handover_audit_message("fp-1", "sales-b", "轮换", &transferred);
        assert!(sales_handover_fingerprint_matches(Some(&stored), "fp-1"));
        assert!(!sales_handover_fingerprint_matches(Some(&stored), "fp-2"));
        assert!(!sales_handover_fingerprint_matches(None, "fp-1"));
    }

    /// 异载荷同幂等键必须拒绝：目标、组织、原因任一变化指纹即不同；
    /// 收据按操作人、单据与幂等键隔离。
    #[test]
    fn different_handover_payload_rejects_replay_under_same_key() {
        let base = handover_request("sales-b", None, "轮换", "key-1");
        let base_fingerprint = sales_handover_fingerprint("actor-1", "so-1", &base).unwrap();
        for altered in [
            handover_request("sales-c", None, "轮换", "key-1"),
            handover_request("sales-b", Some("org-b"), "轮换", "key-1"),
            handover_request("sales-b", None, "其他原因", "key-1"),
            handover_request("sales-b", None, "轮换", "key-2"),
        ] {
            assert_ne!(sales_handover_fingerprint("actor-1", "so-1", &altered).unwrap(), base_fingerprint);
        }
        assert_ne!(
            sales_handover_audit_id("actor-1", "so-1", "key-1"),
            sales_handover_audit_id("actor-1", "so-1", "key-2")
        );
        assert_ne!(
            sales_handover_audit_id("actor-1", "so-1", "key-1"),
            sales_handover_audit_id("actor-2", "so-1", "key-1")
        );
        assert_ne!(
            sales_handover_audit_id("actor-1", "so-1", "key-1"),
            sales_handover_audit_id("actor-1", "so-2", "key-1")
        );
    }
}
