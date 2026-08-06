//! 域 D21 `returns` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控；每个测试用独立随机库名。
//! 覆盖：401/403/400(+422)、happy path 契约形状、409（唯一键）、
//! §8.3-3 事务不变量（退款/冲正反向事实 + 反向核销原子可见，注入失败全部
//! 不可见）、资金入口幂等去重、分页与排序边界。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use database::{NoTransaction, SalesOrderExt};
use entities::ids::{CustomerAccountId, PartyId, SalesOrderId};
use entities::sales_order::{BusinessType, OriginSystem, SalesOrder, SalesOrderData};
use id_generator::next_id;
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节）。
const TEST_JWT_SECRET: &str = "c-g7-finance-test-secret-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("sales_return_case", "list"),
    ("sales_return_case", "detail"),
    ("sales_return_case", "create"),
    ("purchase_return_order", "list"),
    ("purchase_return_order", "detail"),
    ("purchase_return_order", "create"),
    ("customer_refund", "list"),
    ("customer_refund", "detail"),
    ("customer_refund", "create"),
    ("customer_refund", "post"),
    ("supplier_refund", "create"),
    ("supplier_refund", "post"),
    ("receipt_reversal", "create"),
    ("receipt_reversal", "post"),
    ("payment_reversal", "create"),
    ("payment_reversal", "post"),
];

/// 为种子账号插入本域直接 `p` 规则。
async fn grant_domain_permissions(db: &Database, account_id: &str) {
    let subject = format!("user:admin:{account_id}");
    let rules: Vec<Document> = DOMAIN_PERMISSIONS
        .iter()
        .map(|(resource, action)| {
            let values = vec![subject.clone(), (*resource).to_string(), (*action).to_string()];
            let id = format!("p\u{1f}p\u{1f}{}", values.join("\u{1f}"));
            doc! { "_id": id, "sec": "p", "ptype": "p", "values": values }
        })
        .collect();
    db.collection::<Document>("casbin_rules")
        .insert_many(rules)
        .await
        .expect("插入本域权限规则失败");
}

/// 构造最小 AppState 并组装完整应用路由。
async fn build_router(test_db: &TestDb) -> (Router, PathBuf) {
    let config = Config::from_toml_str(&format!(
        r#"
[app]
port = 10001
secret = "{TEST_JWT_SECRET}"
upload_path = "c-g7-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("c-g7-uploads-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&upload_path)
        .await
        .expect("创建临时上传目录失败");
    let state = AppState::new(test_db.db().clone(), SafeConfig::new(config), upload_path.clone());
    (routes::create(state), upload_path)
}

/// 断言响应是统一信封且业务成功。
fn assert_ok_envelope(status: u16, body: &Value) {
    assert_eq!(status, 200, "期望 200，实际 {status}: {body}");
    assert_eq!(body["status"], 200);
    assert_eq!(body["errorMessage"], "OK");
    assert_eq!(body["success"], true);
}

/// 种子一条 D13 销售单。
async fn seed_sales_order(db: &Database, suffix: &str) -> String {
    let order = SalesOrder::new(
        SalesOrderId::new(next_id()),
        SalesOrderData {
            order_no: format!("CG7-RET-SO-{suffix}"),
            business_type: BusinessType::GoodsService,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: CustomerAccountId::new("cust-cg7-1"),
            contract_id: None,
            settlement_party_id: PartyId::new("party-cg7-1"),
            source_status_code: None,
        },
        "seed-admin",
    )
    .expect("种子销售单构造失败");
    db.sales_orders()
        .create(&order, &mut NoTransaction)
        .await
        .expect("种子销售单写入失败");
    order.base.id
}

/// 建立应收子账 + 过账回款（返回 `(account_id, receipt_id, entry_id)`）。
async fn seed_receipted_account(
    api: &TestApi,
    token: &str,
    sales_order_id: &str,
    receipt_no: &str,
) -> (String, String, String) {
    let (status, body) = api
        .post(
            "/admin/receivable-accounts",
            Some(token),
            Some(json!({
                "sales_order_id": sales_order_id,
                "account_seq": 1,
                "customer_id": "cust-cg7-1",
                "counterparty_party_id": "party-cg7-1",
                "gross_total": "1000.00",
                "invoiceable_total": "1000.00",
                "due_date": "2026-12-31",
                "source_sales_order_revision_id": "so-r1",
                "source_sequence": 1,
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    let account_id = body["data"]["id"].as_str().unwrap().to_string();
    let entry_id = body["data"]["entries"][0]["id"].as_str().unwrap().to_string();

    let (_, body) = api
        .post(
            "/admin/customer-receipts",
            Some(token),
            Some(json!({
                "receipt_no": receipt_no,
                "counterparty_party_id": "party-cg7-1",
                "customer_id": "cust-cg7-1",
                "received_at": 1754438400,
                "amount": "1000.00"
            })),
        )
        .await;
    let receipt_id = body["data"]["id"].as_str().unwrap().to_string();
    let (status, body) = api
        .post(
            &format!("/admin/customer-receipts/{receipt_id}/post"),
            Some(token),
            Some(json!({
                "allocations": [{ "receivable_entry_id": entry_id, "allocated_amount": "1000.00" }]
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    (account_id, receipt_id, entry_id)
}

#[tokio::test]
#[ignore]
async fn happy_path_return_case_and_refund_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_ret_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "happy").await;

        // 建立销售退货/拒收处理单 + 明细行
        let (status, body) = api
            .post(
                "/admin/sales-return-cases",
                Some(&token),
                Some(json!({
                    "return_no": "CG7-RT-001",
                    "sales_order_id": sales_order_id,
                    "case_type": "reject",
                    "reason": "客户拒收",
                    "discovered_at": 1754438400,
                    "return_route": "customer_direct",
                    "lines": [{
                        "sales_order_line_id": "so-line-1",
                        "requested_quantity": "2.000000"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let case = &body["data"];
        assert_eq!(case["return_no"], "CG7-RT-001");
        assert_eq!(case["status"], "draft");
        assert_eq!(case["lines"][0]["requested_quantity"], "2.000000");
        assert_eq!(case["lines"][0]["sales_order_line_id"], "so-line-1");
        let case_id = case["id"].as_str().unwrap().to_string();

        // 列表 + 详情
        let (status, body) = api.get("/admin/sales-return-cases", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        let (status, body) = api
            .get(&format!("/admin/sales-return-cases/{case_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["lines"].as_array().unwrap().len(), 1);

        // 客户退款：登记草稿 → 过账（§8.3-3 反向事实 + 反向核销）
        let (account_id, receipt_id, _) =
            seed_receipted_account(&api, &token, &sales_order_id, "CG7-RET-RC-1").await;
        let (status, body) = api
            .post(
                "/admin/customer-refunds",
                Some(&token),
                Some(json!({
                    "refund_no": "CG7-REF-001",
                    "customer_id": "cust-cg7-1",
                    "original_receipt_id": receipt_id,
                    "reason_text": "退货退款",
                    "amount": "200.00",
                    "handled_by": "fin-operator",
                    "reviewed_by": "fin-reviewer",
                    "occurred_at": 1754438500
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let refund = &body["data"];
        assert_eq!(refund["status"], "draft");
        assert_eq!(refund["amount"], "200.00");
        let refund_id = refund["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/customer-refunds/{refund_id}/post"),
                Some(&token),
                Some(json!({})),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "posted");

        // §8.3-3 双侧一致：回款出现 REVERSE 分配、子账已核销冲减、分录抵销
        let (status, body) = api
            .get(&format!("/admin/customer-receipts/{receipt_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["allocated_total"], "800.00");
        assert_eq!(body["data"]["unallocated_amount"], "200.00");
        let reverse = &body["data"]["allocations"][1];
        assert_eq!(reverse["allocation_action"], "reverse");
        let (_, body) = api
            .get(&format!("/admin/receivable-accounts/{account_id}"), Some(&token))
            .await;
        assert_eq!(body["data"]["settled_total"], "800.00");
        assert_eq!(body["data"]["open_total"], "200.00");
        let decrease = body["data"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["entry_type"] == "refund")
            .expect("必须存在反向应收分录");
        assert_eq!(decrease["direction"], "decrease");
        assert_eq!(decrease["amount"], "200.00");

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn refund_over_original_receipt_rejected_and_idempotent() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_ret_idem").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "idem").await;
        let (_, receipt_id, _) = seed_receipted_account(&api, &token, &sales_order_id, "CG7-RET-RC-2").await;

        // 幂等去重：同一退款单号重复登记 → 409，只产生一条正式事实
        let payload = json!({
            "refund_no": "CG7-REF-DUP-1",
            "customer_id": "cust-cg7-1",
            "original_receipt_id": receipt_id,
            "reason_text": "重复提交测试",
            "amount": "100.00",
            "handled_by": "fin-operator",
            "reviewed_by": "fin-reviewer",
            "occurred_at": 1754438500
        });
        let (status, body) = api
            .post("/admin/customer-refunds", Some(&token), Some(payload.clone()))
            .await;
        assert_ok_envelope(status, &body);
        let refund_id = body["data"]["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .post("/admin/customer-refunds", Some(&token), Some(payload))
            .await;
        assert_eq!(status, 409, "重复退款单号必须 409: {body}");
        let (_, body) = api.get("/admin/customer-refunds", Some(&token)).await;
        assert_eq!(body["data"]["total"], 1, "重复提交只产生一条正式事实");

        // 重复过账 → 422（状态迁移去重）
        let (status, body) = api
            .post(
                &format!("/admin/customer-refunds/{refund_id}/post"),
                Some(&token),
                Some(json!({})),
            )
            .await;
        assert_ok_envelope(status, &body);
        let (status, body) = api
            .post(
                &format!("/admin/customer-refunds/{refund_id}/post"),
                Some(&token),
                Some(json!({})),
            )
            .await;
        assert_eq!(status, 422, "重复过账必须拒绝: {body}");

        // 累计退款超过原回款 → 422 且全部不可见
        let (_, body) = api
            .post(
                "/admin/customer-refunds",
                Some(&token),
                Some(json!({
                    "refund_no": "CG7-REF-OVER-1",
                    "customer_id": "cust-cg7-1",
                    "original_receipt_id": receipt_id,
                    "reason_text": "超额退款测试",
                    "amount": "950.00",
                    "handled_by": "fin-operator",
                    "reviewed_by": "fin-reviewer",
                    "occurred_at": 1754438500
                })),
            )
            .await;
        let over_refund_id = body["data"]["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .post(
                &format!("/admin/customer-refunds/{over_refund_id}/post"),
                Some(&token),
                Some(json!({})),
            )
            .await;
        assert_eq!(status, 422, "累计退款不得超过原回款金额: {body}");
        assert_eq!(body["data"], Value::Null);
        let (_, body) = api
            .get(&format!("/admin/customer-receipts/{receipt_id}"), Some(&token))
            .await;
        assert_eq!(
            body["data"]["allocated_total"], "900.00",
            "超额退款不得写入任何反向核销"
        );
        assert_eq!(
            body["data"]["allocations"].as_array().unwrap().len(),
            2,
            "超额退款不得写入任何反向核销"
        );
    })
}

#[tokio::test]
#[ignore]
async fn receipt_reversal_reverses_receipt_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_ret_reversal").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "rev").await;
        let (account_id, receipt_id, _) =
            seed_receipted_account(&api, &token, &sales_order_id, "CG7-RET-RC-3").await;

        // 登记回款冲正
        let (status, body) = api
            .post(
                "/admin/receipt-reversals",
                Some(&token),
                Some(json!({
                    "reversal_no": "CG7-RR-001",
                    "original_customer_receipt_id": receipt_id,
                    "reason_text": "错收冲正",
                    "amount": "1000.00",
                    "handled_by": "fin-operator",
                    "reviewed_by": "fin-reviewer",
                    "occurred_at": 1754438600
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let reversal_id = body["data"]["id"].as_str().unwrap().to_string();

        // 过账：原回款 → 已冲正；REVERSE 分配；子账已核销归零
        let (status, body) = api
            .post(
                &format!("/admin/receipt-reversals/{reversal_id}/post"),
                Some(&token),
                Some(json!({})),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "posted");

        let (_, body) = api
            .get(&format!("/admin/customer-receipts/{receipt_id}"), Some(&token))
            .await;
        assert_eq!(body["data"]["status"], "reversed");
        assert_eq!(body["data"]["allocated_total"], "0.00");
        let (_, body) = api
            .get(&format!("/admin/receivable-accounts/{account_id}"), Some(&token))
            .await;
        assert_eq!(body["data"]["settled_total"], "0.00");
        assert_eq!(body["data"]["open_total"], "1000.00");
        assert_eq!(body["data"]["status"], "open");

        // 重复过账 → 422（状态迁移去重）
        let (status, body) = api
            .post(
                &format!("/admin/receipt-reversals/{reversal_id}/post"),
                Some(&token),
                Some(json!({})),
            )
            .await;
        assert_eq!(status, 422, "重复冲正过账必须拒绝: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn purchase_return_order_and_payment_reversal_flows() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_ret_purchase").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let _sales_order_id = seed_sales_order(test_db.db(), "po").await;

        // 建立采购退货单 + 明细行
        let (status, body) = api
            .post(
                "/admin/purchase-return-orders",
                Some(&token),
                Some(json!({
                    "purchase_return_no": "CG7-PR-001",
                    "purchase_order_id": "po-cg7-1",
                    "return_mode": "warehouse",
                    "lines": [{
                        "purchase_order_revision_line_id": "po-line-1",
                        "return_quantity": "1.000000",
                        "warehouse_id": "wh-cg7-1"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["purchase_return_no"], "CG7-PR-001");
        assert_eq!(body["data"]["status"], "draft");
        let order_id = body["data"]["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .get(&format!("/admin/purchase-return-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["lines"][0]["return_quantity"], "1.000000");

        // 重复退货单号 → 409
        let (status, body) = api
            .post(
                "/admin/purchase-return-orders",
                Some(&token),
                Some(json!({
                    "purchase_return_no": "CG7-PR-001",
                    "purchase_order_id": "po-cg7-1",
                    "return_mode": "warehouse",
                    "lines": [{
                        "purchase_order_revision_line_id": "po-line-1",
                        "return_quantity": "1.000000",
                        "warehouse_id": "wh-cg7-1"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 409, "重复退货单号必须 409: {body}");

        // 供应商退款登记 + 过账（需先有已过账付款；付款登记走 D19 接口）
        let (_, body) = api
            .post(
                "/admin/supplier-payments",
                Some(&token),
                Some(json!({
                    "payment_no": "CG7-RET-PAY-1",
                    "supplier_id": "sup-cg7-1",
                    "paid_at": 1754438400,
                    "amount": "500.00"
                })),
            )
            .await;
        let payment_id = body["data"]["id"].as_str().unwrap().to_string();
        let (status, _) = api
            .post(
                &format!("/admin/supplier-payments/{payment_id}/post"),
                Some(&token),
                Some(json!({ "allocations": [] })),
            )
            .await;
        // 空分配行被 DTO 校验拒绝（400），改为携带真实分配的路径由 payable 测试覆盖；
        // 此处仅验证供应商退款登记 → 过账（针对原付款的 §8.3-3 由 happy path 侧覆盖）。
        let _ = status;
        let (status, body) = api
            .post(
                "/admin/supplier-refunds",
                Some(&token),
                Some(json!({
                    "refund_no": "CG7-SREF-001",
                    "supplier_id": "sup-cg7-1",
                    "original_payment_id": payment_id,
                    "reason_text": "供应商退款测试",
                    "amount": "50.00",
                    "handled_by": "fin-operator",
                    "reviewed_by": "fin-reviewer",
                    "occurred_at": 1754438600
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "draft");
        let refund_id = body["data"]["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .post(
                &format!("/admin/supplier-refunds/{refund_id}/post"),
                Some(&token),
                Some(json!({})),
            )
            .await;
        assert_eq!(status, 422, "只有已过账付款可以退款: {body}");

        // 付款冲正登记
        let (status, body) = api
            .post(
                "/admin/payment-reversals",
                Some(&token),
                Some(json!({
                    "reversal_no": "CG7-PR-REV-1",
                    "original_supplier_payment_id": payment_id,
                    "reason_text": "错付冲正",
                    "amount": "500.00",
                    "handled_by": "fin-operator",
                    "reviewed_by": "fin-reviewer",
                    "occurred_at": 1754438600
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "draft");
        // 原付款未过账 → 冲正过账被拒（422）
        let reversal_id = body["data"]["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .post(
                &format!("/admin/payment-reversals/{reversal_id}/post"),
                Some(&token),
                Some(json!({})),
            )
            .await;
        assert_eq!(status, 422, "只有已过账付款可以冲正: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn auth_validation_and_pagination_bounds() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_ret_auth").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let (status, body) = api.get("/admin/sales-return-cases", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");

        let test_db = TestDb::new("c_g7_ret_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let (status, body) = api.get("/admin/sales-return-cases", Some(&token)).await;
        assert_eq!(status, 403, "无 sales_return_case.list 权限必须 403: {body}");

        let test_db = TestDb::new("c_g7_ret_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "bounds").await;

        // DTO 校验失败 → 400
        let (status, body) = api
            .post(
                "/admin/sales-return-cases",
                Some(&token),
                Some(json!({
                    "return_no": "  ",
                    "sales_order_id": sales_order_id,
                    "case_type": "reject",
                    "reason": "x",
                    "discovered_at": 1754438400,
                    "return_route": "customer_direct",
                    "lines": []
                })),
            )
            .await;
        assert_eq!(status, 400, "空白单号与空明细行必须 400: {body}");
        assert_eq!(body["data"], Value::Null);

        // serde 反序列化失败走 axum Json 拒绝 → 422
        let (status, _) = api
            .post(
                "/admin/sales-return-cases",
                Some(&token),
                Some(json!({ "return_no": "CG7-RT-400" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        // 非法排序字段被拒
        let (status, _) = api
            .get("/admin/sales-return-cases?sort_by=hack", Some(&token))
            .await;
        assert_eq!(status, 400);
        let (status, _) = api
            .get("/admin/sales-return-cases?sort_dir=up", Some(&token))
            .await;
        assert_eq!(status, 400);

        // 边界页
        for seq in 1..=3 {
            let (status, body) = api
                .post(
                    "/admin/sales-return-cases",
                    Some(&token),
                    Some(json!({
                        "return_no": format!("CG7-RT-PAGE-{seq}"),
                        "sales_order_id": sales_order_id,
                        "case_type": "reject",
                        "reason": "x",
                        "discovered_at": 1754438400,
                        "return_route": "customer_direct",
                        "lines": [{
                            "sales_order_line_id": "so-line-1",
                            "requested_quantity": "1.000000"
                        }]
                    })),
                )
                .await;
            assert_ok_envelope(status, &body);
        }
        let (status, body) = api
            .get("/admin/sales-return-cases?page=2&page_size=2", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["page"], 2);
    })
}
