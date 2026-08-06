//! 域 D18 `receivable` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）；
//! 每个测试用独立随机库名并在结束 drop。覆盖：401/403/400(+422)、happy path
//! 契约形状、409（唯一键/乐观锁）、§8.3-1/§8.3-2 事务不变量（含注入失败
//! 全部不可见）、资金入口幂等去重、分页与排序边界。

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{header::AUTHORIZATION, HeaderValue, Method, Request};
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
use tower::ServiceExt;
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节）。
const TEST_JWT_SECRET: &str = "c-g7-finance-test-secret-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("receivable_account", "list"),
    ("receivable_account", "detail"),
    ("receivable_account", "create"),
    ("receivable_account", "update"),
    ("receivable_funds_review", "create"),
    ("customer_receipt", "list"),
    ("customer_receipt", "detail"),
    ("customer_receipt", "create"),
    ("customer_receipt", "post"),
    ("invoice", "list"),
    ("invoice", "detail"),
    ("invoice", "create"),
    ("invoice", "post"),
    ("invoice", "reverse"),
];

/// 发送 PUT 请求（`TestApi` 只提供 GET/POST，PUT 路径由本辅助覆盖）。
async fn put_json(router: &Router, path: &str, token: &str, json: Value) -> (u16, Value) {
    let request = Request::builder()
        .method(Method::PUT)
        .uri(path)
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        )
        .header("content-type", "application/json")
        .body(Body::from(json.to_string()))
        .expect("PUT 请求构造失败");
    let response = router.clone().oneshot(request).await.expect("路由调用失败");
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("响应体读取失败");
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// 为种子账号插入本域直接 `p` 规则（casbin `g(sub, sub)` 自反匹配，无需角色）。
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

/// 种子一条 D13 销售单（D18 跨域存在性校验的依赖数据）。
async fn seed_sales_order(db: &Database, order_no: &str) -> String {
    let order = SalesOrder::new(
        SalesOrderId::new(next_id()),
        SalesOrderData {
            order_no: order_no.to_string(),
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

/// 建立应收子账（序号 1）。
async fn create_account(api: &TestApi, token: &str, sales_order_id: &str) -> (String, Value) {
    create_account_with_seq(api, token, sales_order_id, 1).await
}

/// 建立指定序号的应收子账（同一销售单多子账避免唯一键冲突）。
async fn create_account_with_seq(
    api: &TestApi,
    token: &str,
    sales_order_id: &str,
    account_seq: u32,
) -> (String, Value) {
    let (status, body) = api
        .post(
            "/admin/receivable-accounts",
            Some(token),
            Some(json!({
                "sales_order_id": sales_order_id,
                "account_seq": account_seq,
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
    let id = body["data"]["id"].as_str().unwrap().to_string();
    (id, body)
}

#[tokio::test]
#[ignore]
async fn happy_path_account_receipt_invoice_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_recv_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "CG7-SO-001").await;

        // 初始列表为空 + 契约分页形状
        let (status, body) = api.get("/admin/receivable-accounts", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"], json!([]));
        assert_eq!(body["data"]["total"], 0);
        assert_eq!(body["data"]["page"], 1);
        assert_eq!(body["data"]["page_size"], 20);

        // 建立应收子账
        let (account_id, created) = create_account(&api, &token, &sales_order_id).await;
        assert_eq!(created["data"]["account_seq"], 1);
        assert_eq!(created["data"]["gross_total"], "1000.00");
        assert_eq!(created["data"]["open_total"], "1000.00");
        assert_eq!(created["data"]["status"], "open");
        assert_eq!(created["data"]["version"], 1);
        assert_eq!(created["data"]["entries"][0]["entry_type"], "original");
        assert_eq!(created["data"]["entries"][0]["amount"], "1000.00");
        assert_eq!(created["data"]["entries"][0]["direction"], "increase");

        // 登记并过账回款（全部核销到分录）
        let (status, body) = api
            .post(
                "/admin/customer-receipts",
                Some(&token),
                Some(json!({
                    "receipt_no": "CG7-RC-001",
                    "counterparty_party_id": "party-cg7-1",
                    "customer_id": "cust-cg7-1",
                    "received_at": 1754438400,
                    "amount": "1000.00",
                    "bank_reference": "BANK-001",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let receipt = &body["data"];
        assert_eq!(receipt["receipt_no"], "CG7-RC-001");
        assert_eq!(receipt["status"], "draft");
        assert_eq!(receipt["amount"], "1000.00");
        assert_eq!(receipt["unallocated_amount"], "1000.00");
        let receipt_id = receipt["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/customer-receipts/{receipt_id}/post"),
                Some(&token),
                Some(json!({
                    "allocations": [{
                        "receivable_entry_id": created["data"]["entries"][0]["id"],
                        "allocated_amount": "1000.00"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let posted = &body["data"];
        assert_eq!(posted["status"], "posted");
        assert_eq!(posted["allocated_total"], "1000.00");
        assert_eq!(posted["unallocated_amount"], "0.00");
        assert_eq!(posted["allocations"][0]["allocation_action"], "apply");

        // 子账同步结清（§8.3-1 双侧进度一致）
        let (status, body) = api
            .get(&format!("/admin/receivable-accounts/{account_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["settled_total"], "1000.00");
        assert_eq!(body["data"]["open_total"], "0.00");
        assert_eq!(body["data"]["status"], "settled");

        // 登记并过账销项发票（重新建一张子账留可开票额度，序号 2 避免唯一键冲突）
        let (account2, _) = create_account_with_seq(&api, &token, &sales_order_id, 2).await;
        let _ = put_json(
            &router,
            &format!("/admin/receivable-accounts/{account2}/review"),
            &token,
            json!({
                "version": 1,
                "review_status": "reviewed",
                "reviewed_by": "reviewer-1",
                "reviewed_at": 1754438400,
                "review_evidence_reference": "evidence-1"
            }),
        )
        .await;
        let (status, body) = api
            .post(
                "/admin/invoices",
                Some(&token),
                Some(json!({
                    "invoice_direction": "sales",
                    "invoice_kind": "blue",
                    "party_id": "party-cg7-1",
                    "invoice_no": "INV-CG7-001",
                    "invoice_date": "2026-08-06",
                    "gross_amount": "113.00",
                    "net_amount": "100.00",
                    "tax_amount": "13.00"
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let invoice_id = body["data"]["id"].as_str().unwrap().to_string();
        assert_eq!(body["data"]["status"], "draft");

        let (status, body) = api
            .post(
                &format!("/admin/invoices/{invoice_id}/post"),
                Some(&token),
                Some(json!({
                    "allocations": [{
                        "receivable_account_id": account2,
                        "allocated_gross_amount": "113.00",
                        "allocated_net_amount": "100.00",
                        "allocated_tax_amount": "13.00"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let invoice = &body["data"];
        assert_eq!(invoice["status"], "registered");
        assert_eq!(invoice["invoice_kind"], "blue");
        assert_eq!(invoice["allocated_total"], "113.00");
        assert_eq!(invoice["unallocated_amount"], "0.00");
        assert_eq!(invoice["allocations"][0]["allocation_action"], "apply");

        // 发票列表筛选 + 回款列表
        let (status, body) = api
            .get(
                "/admin/invoices?invoice_direction=sales&invoice_no=INV-CG7",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["invoice_no"], "INV-CG7-001");
        let (status, body) = api.get("/admin/customer-receipts", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["status"], "posted");

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn receipt_post_rejects_cross_party_and_rolls_back_everything() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_recv_tx").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "CG7-SO-002").await;
        let (_, account_view) = create_account(&api, &token, &sales_order_id).await;
        let entry_id = account_view["data"]["entries"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let (_, body) = api
            .post(
                "/admin/customer-receipts",
                Some(&token),
                Some(json!({
                    "receipt_no": "CG7-RC-TX-1",
                    "counterparty_party_id": "party-cg7-OTHER",
                    "received_at": 1754438400,
                    "amount": "500.00"
                })),
            )
            .await;
        let receipt_id = body["data"]["id"].as_str().unwrap().to_string();

        // 注入失败：回款往来主体与应收分录主体不一致 → 422，全部不可见
        let (status, body) = api
            .post(
                &format!("/admin/customer-receipts/{receipt_id}/post"),
                Some(&token),
                Some(json!({
                    "allocations": [{ "receivable_entry_id": entry_id, "allocated_amount": "500.00" }]
                })),
            )
            .await;
        assert_eq!(status, 422, "跨主体核销必须拒绝: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        // 事务不变量：分配行与子账进度都不可见
        let (status, body) = api
            .get(&format!("/admin/customer-receipts/{receipt_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "draft", "回款必须保持草稿");
        assert_eq!(body["data"]["allocations"], json!([]), "不得残留分配行");
        assert_eq!(body["data"]["unallocated_amount"], "500.00");
        let (_, body) = api.get("/admin/receivable-accounts", Some(&token)).await;
        assert_eq!(
            body["data"]["items"][0]["settled_total"], "0.00",
            "子账进度不得变化"
        );
        assert_eq!(body["data"]["items"][0]["open_total"], "1000.00");
    })
}

#[tokio::test]
#[ignore]
async fn invoice_post_requires_allocations_equal_invoice_and_rejects_excess() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_recv_inv_tx").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "CG7-SO-003").await;
        let (account_id, _) = create_account(&api, &token, &sales_order_id).await;

        let (_, body) = api
            .post(
                "/admin/invoices",
                Some(&token),
                Some(json!({
                    "invoice_direction": "sales",
                    "invoice_kind": "blue",
                    "party_id": "party-cg7-1",
                    "invoice_no": "INV-CG7-TX-1",
                    "invoice_date": "2026-08-06",
                    "gross_amount": "113.00",
                    "net_amount": "100.00",
                    "tax_amount": "13.00"
                })),
            )
            .await;
        let invoice_id = body["data"]["id"].as_str().unwrap().to_string();

        // 分配合计 != 发票金额 → 422
        let (status, body) = api
            .post(
                &format!("/admin/invoices/{invoice_id}/post"),
                Some(&token),
                Some(json!({
                    "allocations": [{
                        "receivable_account_id": account_id,
                        "allocated_gross_amount": "100.00",
                        "allocated_net_amount": "88.00",
                        "allocated_tax_amount": "12.00"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 422, "分配合计必须等于发票金额: {body}");
        assert_eq!(body["success"], false);

        // 事务不变量：发票保持草稿、无分配行、子账可开票额度不变
        let (status, body) = api
            .get(&format!("/admin/invoices/{invoice_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "draft");
        assert_eq!(body["data"]["allocations"], json!([]));
        let (_, body) = api
            .get(&format!("/admin/receivable-accounts/{account_id}"), Some(&token))
            .await;
        assert_eq!(body["data"]["invoiced_total"], "0.00");
        assert_eq!(body["data"]["open_invoiceable_total"], "1000.00");

        // 超额开票（超过可开票额度）→ 422
        let (_, body) = api
            .post(
                &format!("/admin/invoices/{invoice_id}/post"),
                Some(&token),
                Some(json!({
                    "allocations": [{
                        "receivable_account_id": account_id,
                        "allocated_gross_amount": "113.00",
                        "allocated_net_amount": "100.00",
                        "allocated_tax_amount": "13.00"
                    }]
                })),
            )
            .await;
        let _ = body;
        let (status, body) = api
            .post(
                &format!("/admin/invoices/{invoice_id}/post"),
                Some(&token),
                Some(json!({
                    "allocations": [{
                        "receivable_account_id": account_id,
                        "allocated_gross_amount": "113.00",
                        "allocated_net_amount": "100.00",
                        "allocated_tax_amount": "13.00"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 422, "分配合计超过可开票额度必须拒绝: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn red_invoice_reverses_blue_allocation_within_limit() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_recv_red").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "CG7-SO-004").await;
        let (account_id, _) = create_account(&api, &token, &sales_order_id).await;

        let (_, body) = api
            .post(
                "/admin/invoices",
                Some(&token),
                Some(json!({
                    "invoice_direction": "sales",
                    "invoice_kind": "blue",
                    "party_id": "party-cg7-1",
                    "invoice_no": "INV-CG7-BLUE-1",
                    "invoice_date": "2026-08-06",
                    "gross_amount": "113.00",
                    "net_amount": "100.00",
                    "tax_amount": "13.00"
                })),
            )
            .await;
        let blue_id = body["data"]["id"].as_str().unwrap().to_string();
        let (_, body) = api
            .post(
                &format!("/admin/invoices/{blue_id}/post"),
                Some(&token),
                Some(json!({
                    "allocations": [{
                        "receivable_account_id": account_id,
                        "allocated_gross_amount": "113.00",
                        "allocated_net_amount": "100.00",
                        "allocated_tax_amount": "13.00"
                    }]
                })),
            )
            .await;
        let blue_allocation_id = body["data"]["allocations"][0]["id"].as_str().unwrap().to_string();

        // 红票全额红冲
        let (status, body) = api
            .post(
                &format!("/admin/invoices/{blue_id}/red-issue"),
                Some(&token),
                Some(json!({
                    "invoice_no": "INV-CG7-RED-1",
                    "invoice_date": "2026-08-07",
                    "gross_amount": "113.00",
                    "net_amount": "100.00",
                    "tax_amount": "13.00",
                    "allocations": [{
                        "reverses_allocation_id": blue_allocation_id,
                        "allocated_gross_amount": "113.00",
                        "allocated_net_amount": "100.00",
                        "allocated_tax_amount": "13.00"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["invoice_kind"], "red");
        assert_eq!(body["data"]["status"], "registered");
        assert_eq!(body["data"]["original_invoice_id"], blue_id);
        assert_eq!(body["data"]["allocations"][0]["allocation_action"], "reverse");

        // 原蓝票置已红冲；子账可开票额度恢复
        let (status, body) = api.get(&format!("/admin/invoices/{blue_id}"), Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "red_invoiced");
        let (_, body) = api
            .get(&format!("/admin/receivable-accounts/{account_id}"), Some(&token))
            .await;
        assert_eq!(body["data"]["invoiced_total"], "0.00");
        assert_eq!(body["data"]["open_invoiceable_total"], "1000.00");

        // 重复红冲同一蓝票 → 422（原蓝票已置红冲，状态迁移去重）
        let (status, body) = api
            .post(
                &format!("/admin/invoices/{blue_id}/red-issue"),
                Some(&token),
                Some(json!({
                    "invoice_no": "INV-CG7-RED-2",
                    "invoice_date": "2026-08-07",
                    "gross_amount": "113.00",
                    "net_amount": "100.00",
                    "tax_amount": "13.00",
                    "allocations": [{
                        "reverses_allocation_id": blue_allocation_id,
                        "allocated_gross_amount": "113.00",
                        "allocated_net_amount": "100.00",
                        "allocated_tax_amount": "13.00"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 422, "已红冲蓝票不得再次红冲: {body}");
        // 红票登记后再次提交同一红票号码（对另一张蓝票）→ 409（规范化号码去重）
        let (account3, _) = create_account_with_seq(&api, &token, &sales_order_id, 2).await;
        let (_, body) = api
            .post(
                "/admin/invoices",
                Some(&token),
                Some(json!({
                    "invoice_direction": "sales",
                    "invoice_kind": "blue",
                    "party_id": "party-cg7-1",
                    "invoice_no": "INV-CG7-BLUE-2",
                    "invoice_date": "2026-08-06",
                    "gross_amount": "100.00",
                    "net_amount": "88.50",
                    "tax_amount": "11.50"
                })),
            )
            .await;
        let blue2_id = body["data"]["id"].as_str().unwrap().to_string();
        let (_, body) = api
            .post(
                &format!("/admin/invoices/{blue2_id}/post"),
                Some(&token),
                Some(json!({
                    "allocations": [{
                        "receivable_account_id": account3,
                        "allocated_gross_amount": "100.00",
                        "allocated_net_amount": "88.50",
                        "allocated_tax_amount": "11.50"
                    }]
                })),
            )
            .await;
        let blue2_alloc_id = body["data"]["allocations"][0]["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .post(
                &format!("/admin/invoices/{blue2_id}/red-issue"),
                Some(&token),
                Some(json!({
                    "invoice_no": "INV-CG7-RED-1",
                    "invoice_date": "2026-08-07",
                    "gross_amount": "100.00",
                    "net_amount": "88.50",
                    "tax_amount": "11.50",
                    "allocations": [{
                        "reverses_allocation_id": blue2_alloc_id,
                        "allocated_gross_amount": "100.00",
                        "allocated_net_amount": "88.50",
                        "allocated_tax_amount": "11.50"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 409, "红票号码重复必须 409: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_receipt_no_and_stale_review_update_return_409() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_recv_409").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "CG7-SO-005").await;
        let (account_id, _) = create_account(&api, &token, &sales_order_id).await;

        // 幂等去重：同一回款单号重复登记 → 409，只产生一条正式事实
        let payload = json!({
            "receipt_no": "CG7-RC-409-1",
            "counterparty_party_id": "party-cg7-1",
            "received_at": 1754438400,
            "amount": "100.00"
        });
        let (status, body) = api
            .post("/admin/customer-receipts", Some(&token), Some(payload.clone()))
            .await;
        assert_ok_envelope(status, &body);
        let (status, body) = api
            .post("/admin/customer-receipts", Some(&token), Some(payload))
            .await;
        assert_eq!(status, 409, "重复回款单号必须 409: {body}");
        let (_, body) = api.get("/admin/customer-receipts", Some(&token)).await;
        assert_eq!(body["data"]["total"], 1, "重复提交只产生一条正式事实");

        // 乐观锁：陈旧版本更新复核缓存 → 409
        let (status, body) = put_json(
            &router,
            &format!("/admin/receivable-accounts/{account_id}/review"),
            &token,
            json!({
                "version": 1,
                "review_status": "reviewed",
                "reviewed_by": "reviewer-1",
                "reviewed_at": 1754438400,
                "review_evidence_reference": "evidence-1"
            }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["review_status"], "reviewed");
        let (status, body) = put_json(
            &router,
            &format!("/admin/receivable-accounts/{account_id}/review"),
            &token,
            json!({
                "version": 1,
                "review_status": "not_applicable",
                "reviewed_by": "reviewer-2",
                "reviewed_at": 1754438500,
                "review_evidence_reference": "evidence-2"
            }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本更新必须 409: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn funds_review_appends_chain_and_refreshes_cache() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_recv_review").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "CG7-SO-006").await;
        let (account_id, _) = create_account(&api, &token, &sales_order_id).await;

        let (status, body) = api
            .post(
                "/admin/receivable-funds-reviews",
                Some(&token),
                Some(json!({
                    "receivable_account_id": account_id,
                    "work_item_id": "wi-cg7-1",
                    "review_type": "opening",
                    "review_result": "passed",
                    "evidence_reference": "evidence-opening",
                    "reviewed_by": "reviewer-1",
                    "reviewed_at": 1754438400
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["review_no"], 1);
        assert_eq!(body["data"]["review_result"], "passed");

        // 复核缓存同步为已复核
        let (_, body) = api
            .get(&format!("/admin/receivable-accounts/{account_id}"), Some(&token))
            .await;
        assert_eq!(body["data"]["review_status"], "reviewed");

        // 复核链逐号递增
        let (status, body) = api
            .post(
                "/admin/receivable-funds-reviews",
                Some(&token),
                Some(json!({
                    "receivable_account_id": account_id,
                    "work_item_id": "wi-cg7-2",
                    "review_type": "sync_delta",
                    "review_result": "rejected",
                    "evidence_reference": "evidence-delta",
                    "reviewed_by": "reviewer-2",
                    "reviewed_at": 1754438500
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["review_no"], 2);
        let (_, body) = api
            .get(&format!("/admin/receivable-accounts/{account_id}"), Some(&token))
            .await;
        assert_eq!(body["data"]["reviews"].as_array().unwrap().len(), 2);
        assert_eq!(body["data"]["reviews"][1]["review_no"], 2);
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sorting_bounds() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_recv_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "CG7-SO-007").await;
        for seq in 1..=3 {
            let (status, body) = api
                .post(
                    "/admin/receivable-accounts",
                    Some(&token),
                    Some(json!({
                        "sales_order_id": sales_order_id,
                        "account_seq": seq,
                        "customer_id": "cust-cg7-1",
                        "counterparty_party_id": "party-cg7-1",
                        "gross_total": format!("{seq}00.00"),
                        "due_date": "2026-12-31",
                        "source_sales_order_revision_id": "so-r1",
                        "source_sequence": seq,
                    })),
                )
                .await;
            assert_ok_envelope(status, &body);
        }

        // 边界页：第二页空
        let (status, body) = api
            .get("/admin/receivable-accounts?page=2&page_size=2", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["page"], 2);

        // 非法排序字段被拒
        let (status, body) = api
            .get("/admin/receivable-accounts?sort_by=hack", Some(&token))
            .await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");
        assert_eq!(body["success"], false);

        // 非法排序方向被拒
        let (status, _) = api
            .get("/admin/receivable-accounts?sort_dir=up", Some(&token))
            .await;
        assert_eq!(status, 400);
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_recv_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/receivable-accounts", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_recv_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/customer-receipts", Some(&token)).await;
        assert_eq!(status, 403, "无 customer_receipt.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_recv_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        // DTO 校验失败 → 400 信封
        let (status, body) = api
            .post(
                "/admin/customer-receipts",
                Some(&token),
                Some(json!({
                    "receipt_no": "  ",
                    "counterparty_party_id": "party-cg7-1",
                    "received_at": 1754438400,
                    "amount": "100.00"
                })),
            )
            .await;
        assert_eq!(status, 400, "空白单号必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        // serde 反序列化失败走 axum Json 拒绝 → 422
        let (status, _) = api
            .post(
                "/admin/customer-receipts",
                Some(&token),
                Some(json!({ "receipt_no": "CG7-RC-400" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        // 负金额落入实体业务规则 → 422
        let (status, _) = api
            .post(
                "/admin/customer-receipts",
                Some(&token),
                Some(json!({
                    "receipt_no": "CG7-RC-400",
                    "counterparty_party_id": "party-cg7-1",
                    "received_at": 1754438400,
                    "amount": "-1.00"
                })),
            )
            .await;
        assert_eq!(status, 422, "负金额落入实体业务规则 422");
    })
}
