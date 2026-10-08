//! 销售命令收据身份与请求载荷指纹。
use sha2::{Digest, Sha256};

use crate::dto::sales_order::{HandoverSalesOrderRequest, SubmitSalesOrderRequest};
use crate::{Error, Result};

/// 为销售提交幂等命令生成不泄露原始幂等键的稳定收据 ID。
///
/// # 参数
/// * `actor_id` - 操作人。
/// * `sales_order_id` - 销售单。
/// * `idempotency_key` - 原始幂等键，不参与明文输出。
///
/// # 返回
/// 返回 `sales-order-submit-` 前缀的十六进制摘要。
///
/// # 错误
/// 不返回错误。
pub fn sales_submission_audit_id(actor_id: &str, sales_order_id: &str, idempotency_key: &str) -> String {
    format!(
        "sales-order-submit-{}",
        hex::encode(Sha256::digest(format!("{actor_id}|{sales_order_id}|{idempotency_key}").as_bytes()))
    )
}

/// 锁定同一幂等键可重放的完整请求身份。
///
/// # 参数
/// * `actor_id` - 操作人。
/// * `sales_order_id` - 销售单。
/// * `request` - 完整提交请求。
///
/// # 返回
/// 返回操作人、销售单和请求一起序列化后的十六进制摘要。
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

/// 由操作人与幂等键生成建单收据 ID；不序列化请求，也不落库或绑定审批。
///
/// 幂等键先 `trim` 再参与摘要。
///
/// # 参数
/// * `actor_id` - 操作人。
/// * `idempotency_key` - 原始幂等键。
///
/// # 返回
/// 返回 `sales-order-create-` 前缀的十六进制摘要。
///
/// # 错误
/// 不返回错误。
pub fn sales_order_create_audit_id(actor_id: &str, idempotency_key: &str) -> String {
    let mut digest = Sha256::new();
    for part in [actor_id, idempotency_key.trim()] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("sales-order-create-{}", hex::encode(digest.finalize()))
}

/// 锁定同一建单幂等键可重放的操作人与完整请求载荷；不落库，也不绑定审批。
///
/// # 参数
/// * `actor_id` - 操作人。
/// * `request` - 完整建单请求。
///
/// # 返回
/// 返回操作人与请求一起序列化后的十六进制摘要。
///
/// # 错误
/// 请求序列化失败时返回 `Internal`。
pub fn sales_order_create_fingerprint<T: serde::Serialize>(actor_id: &str, request: &T) -> Result<String> {
    let payload = serde_json::to_vec(&(actor_id, request))
        .map_err(|error| Error::Internal(format!("销售建单命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(payload)))
}

/// 为销售责任交接幂等命令生成不泄露原始幂等键的稳定收据 ID。
///
/// # 参数
/// * `actor_id` - 操作人。
/// * `sales_order_id` - 销售单。
/// * `idempotency_key` - 原始幂等键，不参与明文输出。
///
/// # 返回
/// 返回 `sales-order-handover-` 前缀的十六进制摘要。
///
/// # 错误
/// 不返回错误。
pub fn sales_handover_audit_id(actor_id: &str, sales_order_id: &str, idempotency_key: &str) -> String {
    format!(
        "sales-order-handover-{}",
        hex::encode(Sha256::digest(format!("{actor_id}|{sales_order_id}|{idempotency_key}").as_bytes()))
    )
}

/// 锁定同一幂等键可重放的完整交接请求身份。
///
/// # 参数
/// * `actor_id` - 操作人。
/// * `sales_order_id` - 销售单。
/// * `request` - 完整交接请求。
///
/// # 返回
/// 返回操作人、销售单和请求一起序列化后的十六进制摘要。
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

#[cfg(test)]
mod compile_probe_equivalence_tests {
    use sha2::{Digest, Sha256};

    use super::{
        HandoverSalesOrderRequest, SubmitSalesOrderRequest, sales_handover_audit_id,
        sales_handover_fingerprint, sales_submission_fingerprint,
    };
    use crate::entity::sales_order::SalesPricingMode;

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
            let (actor, order, mut request): (String, String, SubmitSalesOrderRequest) =
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
            if let Some(goods) = request.draft.lines[0].goods.as_mut() {
                goods.pricing_mode = SalesPricingMode::Auto;
                assert_ne!(sales_submission_fingerprint(&actor, &order, &request).unwrap(), golden_hash);
            }
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
