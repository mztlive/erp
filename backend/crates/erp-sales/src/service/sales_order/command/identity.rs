//! Stable sales command receipt identities and payload fingerprints.
use crate::dto::sales_order::SubmitSalesOrderRequest;
use crate::{Error, Result};
use sha2::{Digest, Sha256};

/// 为销售提交幂等命令生成不泄露原始幂等键的稳定收据 ID。
pub fn sales_submission_audit_id(actor_id: &str, sales_order_id: &str, idempotency_key: &str) -> String {
    format!(
        "sales-order-submit-{}",
        hex::encode(Sha256::digest(
            format!("{actor_id}|{sales_order_id}|{idempotency_key}").as_bytes()
        ))
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

#[cfg(test)]
mod compile_probe_equivalence_tests {
    use super::{sales_submission_fingerprint, SubmitSalesOrderRequest};
    use sha2::{Digest, Sha256};

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
}
