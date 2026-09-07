use super::{
    sales_order_create_audit_id, sales_order_create_fingerprint, sales_submission_audit_id,
    sales_submission_fingerprint,
};
use serde_json::json;

fn submission_request(version: u64) -> super::SubmitSalesOrderRequest {
    serde_json::from_value(json!({
        "version": version,
        "idempotency_key": "secret-request",
        "contract_id": "contract-1",
        "draft": {
            "editor_user_id": "actor-1",
            "requested_contract_revision_id": "contract-revision-1",
            "project_name": null,
            "business_remark": null,
            "voucher_category_sku_id": null,
            "voucher_expiry_at": null,
            "receivable_due_date": null,
            "lines": [{
                "line_no": 1,
                "line_type": "GOODS_SERVICE",
                "sales_tax_rate": "0.13",
                "item_name_snapshot": "测试商品",
                "spec_snapshot": null,
                "unit_snapshot": "件",
                "goods": {
                    "sku_id": "sku-1",
                    "sku_revision_id": "sku-revision-1",
                    "welfare_scenario": null,
                    "service_region": null,
                    "fulfillment_due_at": 1800000000,
                    "quantity": "1",
                    "base_unit_code": "件",
                    "unit_price_gross": "100"
                },
                "voucher": null
            }]
        }
    }))
    .unwrap()
}

#[test]
fn submission_idempotency_identity_is_stable_and_payload_bound() {
    let receipt = sales_submission_audit_id("actor-1", "order-1", "secret-request");
    assert_eq!(
        receipt,
        sales_submission_audit_id("actor-1", "order-1", "secret-request")
    );
    assert!(!receipt.contains("secret-request"));
    assert_ne!(
        sales_submission_fingerprint("actor-1", "order-1", &submission_request(1)).unwrap(),
        sales_submission_fingerprint("actor-1", "order-1", &submission_request(2)).unwrap()
    );
}

#[test]
fn creation_idempotency_identity_is_stable_and_full_payload_bound() {
    let receipt = sales_order_create_audit_id("actor-1", " secret-request ");
    assert_eq!(receipt, sales_order_create_audit_id("actor-1", "secret-request"));
    assert!(!receipt.contains("secret-request"));

    let first =
        sales_order_create_fingerprint("actor-1", &json!({"order_no": "SO-1", "intent": "SAVE_DRAFT"}))
            .unwrap();
    assert_eq!(
        first,
        sales_order_create_fingerprint("actor-1", &json!({"order_no": "SO-1", "intent": "SAVE_DRAFT"}))
            .unwrap()
    );
    assert_ne!(
        first,
        sales_order_create_fingerprint("actor-1", &json!({"order_no": "SO-2", "intent": "SAVE_DRAFT"}))
            .unwrap()
    );
}
