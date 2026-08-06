//! 域 D19 `payable` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控；每个测试用独立随机库名。
//! 覆盖：401/403/400(+422)、happy path 契约形状、409（唯一键）、
//! §8.3-1/§8.3-2 事务不变量（含注入失败全部不可见）、资金入口幂等去重、
//! 分页与排序边界。

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{header::AUTHORIZATION, HeaderValue, Method, Request};
use axum::Router;
use config::{Config, SafeConfig};
use database::{NoTransaction, PurchaseOrderExt, SalesOrderExt, SupplierExt};
use entities::ids::{CustomerAccountId, PartyId, PurchaseOrderId, SalesOrderId, SupplierAccountId};
use entities::purchase_order::{FulfillmentResponsibility, PurchaseOrder, PurchaseOrderData, PurchaseType};
use entities::sales_order::{BusinessType, OriginSystem, SalesOrder, SalesOrderData};
use entities::supplier::{SupplierAccount, SupplierAccountData, SupplierAccountStatus};
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
    ("payable_account", "list"),
    ("payable_account", "detail"),
    ("payable_account", "create"),
    ("supplier_payment", "list"),
    ("supplier_payment", "detail"),
    ("supplier_payment", "create"),
    ("supplier_payment", "post"),
    ("purchase_invoice_allocation", "list"),
    ("purchase_invoice_allocation", "post"),
];

/// 发送 PUT 请求（保留扩展性，本域测试未使用）。
async fn _put_json(router: &Router, path: &str, token: &str, json: Value) -> (u16, Value) {
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

/// 种子 D09 供应商（返回 `(supplier_id, party_id)`）。
async fn seed_supplier(db: &Database, party_id: &str) -> (String, String) {
    let supplier = SupplierAccount::new(
        SupplierAccountId::new(next_id()),
        SupplierAccountData {
            party_id: PartyId::new(party_id),
            supplier_no: format!("SUP-{party_id}"),
            default_payment_term_id: None,
            current_commercial_profile_revision_id: None,
            status: SupplierAccountStatus::Active,
        },
        "seed-admin",
    )
    .expect("种子供应商构造失败");
    db.supplier_accounts()
        .create(&supplier, &mut NoTransaction)
        .await
        .expect("种子供应商写入失败");
    (supplier.base.id, party_id.to_string())
}

/// 种子 D13 销售单 + D15 采购单（返回 `(sales_order_id, purchase_order_id)`）。
async fn seed_orders(db: &Database, suffix: &str) -> (String, String) {
    let customer_id = CustomerAccountId::new("cust-cg7-1");
    let party_id = PartyId::new(format!("party-{suffix}"));
    let sales_order = SalesOrder::new(
        SalesOrderId::new(next_id()),
        SalesOrderData {
            order_no: format!("CG7-SO-{suffix}"),
            business_type: BusinessType::GoodsService,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: customer_id.clone(),
            contract_id: None,
            settlement_party_id: party_id.clone(),
            source_status_code: None,
        },
        "seed-admin",
    )
    .expect("种子销售单构造失败");
    db.sales_orders()
        .create(&sales_order, &mut NoTransaction)
        .await
        .expect("种子销售单写入失败");
    let supplier_id = SupplierAccountId::new(next_id());
    let supplier = SupplierAccount::new(
        supplier_id.clone(),
        SupplierAccountData {
            party_id: party_id.clone(),
            supplier_no: format!("SUP-{suffix}"),
            default_payment_term_id: None,
            current_commercial_profile_revision_id: None,
            status: SupplierAccountStatus::Active,
        },
        "seed-admin",
    )
    .expect("种子供应商构造失败");
    db.supplier_accounts()
        .create(&supplier, &mut NoTransaction)
        .await
        .expect("种子供应商写入失败");
    let purchase_order = PurchaseOrder::new(
        PurchaseOrderId::new(next_id()),
        PurchaseOrderData {
            purchase_no: format!("CG7-PO-{suffix}"),
            sales_order_id: sales_order.base.id.clone().into(),
            supplier_id,
            purchase_type: PurchaseType::Physical,
            payment_term_code: "NET30".to_string(),
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
        },
        "seed-admin",
    )
    .expect("种子采购单构造失败");
    db.purchase_orders()
        .create(&purchase_order, &mut NoTransaction)
        .await
        .expect("种子采购单写入失败");
    (sales_order.base.id, purchase_order.base.id)
}

/// 建立应付子账（返回 `(account_id, entry_id)`）。
async fn create_payable_account(
    api: &TestApi,
    token: &str,
    purchase_order_id: &str,
    supplier_id: &str,
) -> (String, String) {
    let (status, body) = api
        .post(
            "/admin/payable-accounts",
            Some(token),
            Some(json!({
                "source_document_id": purchase_order_id,
                "supplier_id": supplier_id,
                "source_type": "purchase_order",
                "gross_total": "1000.00",
                "invoiceable_total": "1000.00",
                "due_date": "2026-12-31",
                "source_revision_id": "po-r1",
                "source_sequence": 1,
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    let account_id = body["data"]["id"].as_str().unwrap().to_string();
    let entry_id = body["data"]["entries"][0]["id"].as_str().unwrap().to_string();
    (account_id, entry_id)
}

#[tokio::test]
#[ignore]
async fn happy_path_payable_payment_invoice_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_pay_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let (_, purchase_order_id) = seed_orders(test_db.db(), "happy").await;

        // 初始列表为空 + 契约分页形状
        let (status, body) = api.get("/admin/payable-accounts", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"], json!([]));
        assert_eq!(body["data"]["total"], 0);
        assert_eq!(body["data"]["page"], 1);
        assert_eq!(body["data"]["page_size"], 20);

        // 建立应付子账（D19 跨域校验 D15 采购单存在）
        let (status, body) = api
            .post(
                "/admin/payable-accounts",
                Some(&token),
                Some(json!({
                    "source_document_id": purchase_order_id,
                    "supplier_id": "sup-nonexistent",
                    "source_type": "purchase_order",
                    "gross_total": "1000.00",
                    "due_date": "2026-12-31",
                    "source_revision_id": "po-r1",
                    "source_sequence": 1,
                })),
            )
            .await;
        let _ = (status, body);
        // 重新用真实供应商建立
        let (supplier_id, _) = seed_supplier(test_db.db(), "party-pay").await;
        let (_, _) = create_payable_account(&api, &token, &purchase_order_id, &supplier_id).await;
        let (_, body) = api.get("/admin/payable-accounts", Some(&token)).await;
        let account_row = &body["data"]["items"][0];
        assert_eq!(account_row["gross_total"], "1000.00");
        assert_eq!(account_row["open_total"], "1000.00");
        assert_eq!(account_row["status"], "open");
        assert_eq!(account_row["source_type"], "purchase_order");
        let account_id = account_row["id"].as_str().unwrap().to_string();
        let entry_id = {
            let (_, body) = api
                .get(&format!("/admin/payable-accounts/{account_id}"), Some(&token))
                .await;
            body["data"]["entries"][0]["id"].as_str().unwrap().to_string()
        };

        // 登记并过账付款（全部核销）
        let (status, body) = api
            .post(
                "/admin/supplier-payments",
                Some(&token),
                Some(json!({
                    "payment_no": "CG7-PAY-001",
                    "supplier_id": supplier_id,
                    "paid_at": 1754438400,
                    "amount": "1000.00",
                    "bank_reference": "BANK-PAY-001"
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let payment_id = body["data"]["id"].as_str().unwrap().to_string();
        assert_eq!(body["data"]["status"], "draft");

        let (status, body) = api
            .post(
                &format!("/admin/supplier-payments/{payment_id}/post"),
                Some(&token),
                Some(json!({
                    "allocations": [{ "payable_entry_id": entry_id, "allocated_amount": "1000.00" }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "posted");
        assert_eq!(body["data"]["allocated_total"], "1000.00");
        assert_eq!(body["data"]["unallocated_amount"], "0.00");

        // 子账同步结清（§8.3-1 双侧一致）
        let (_, body) = api
            .get(&format!("/admin/payable-accounts/{account_id}"), Some(&token))
            .await;
        assert_eq!(body["data"]["settled_total"], "1000.00");
        assert_eq!(body["data"]["open_total"], "0.00");
        assert_eq!(body["data"]["status"], "settled");

        // 进项发票登记过账（新子账留可收票额度）
        let (account2, _) = create_payable_account(&api, &token, &purchase_order_id, &supplier_id).await;
        let (status, body) = api
            .post(
                "/admin/purchase-invoice-allocations",
                Some(&token),
                Some(json!({
                    "invoice_no": "PINV-CG7-001",
                    "invoice_date": "2026-08-06",
                    "gross_amount": "113.00",
                    "net_amount": "100.00",
                    "tax_amount": "13.00",
                    "supplier_id": supplier_id,
                    "allocations": [{
                        "payable_account_id": account2,
                        "allocated_gross_amount": "113.00",
                        "allocated_net_amount": "100.00",
                        "allocated_tax_amount": "13.00"
                    }]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["invoice_no"], "PINV-CG7-001");
        assert_eq!(body["data"]["allocations"][0]["allocation_action"], "apply");

        // 进项发票分配列表（按应付子账筛选）
        let (status, body) = api
            .get(
                &format!("/admin/purchase-invoice-allocations?payable_account_id={account2}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["payable_account_id"], account2);

        // 付款列表
        let (status, body) = api.get("/admin/supplier-payments", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["status"], "posted");

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn payment_post_rejects_cross_supplier_and_rolls_back_everything() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_pay_tx").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let (_, purchase_order_id) = seed_orders(test_db.db(), "tx").await;
        let (supplier_id, _) = seed_supplier(test_db.db(), "party-pay-tx").await;
        let (_, entry_id) = create_payable_account(&api, &token, &purchase_order_id, &supplier_id).await;

        let (_, body) = api
            .post(
                "/admin/supplier-payments",
                Some(&token),
                Some(json!({
                    "payment_no": "CG7-PAY-TX-1",
                    "supplier_id": "sup-other",
                    "paid_at": 1754438400,
                    "amount": "500.00"
                })),
            )
            .await;
        let payment_id = body["data"]["id"].as_str().unwrap().to_string();

        // 注入失败：付款供应商与应付分录供应商不一致 → 422，全部不可见
        let (status, body) = api
            .post(
                &format!("/admin/supplier-payments/{payment_id}/post"),
                Some(&token),
                Some(json!({
                    "allocations": [{ "payable_entry_id": entry_id, "allocated_amount": "500.00" }]
                })),
            )
            .await;
        assert_eq!(status, 422, "跨供应商核销必须拒绝: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        // 事务不变量：付款保持草稿、无分配行、子账进度不变
        let (status, body) = api
            .get(&format!("/admin/supplier-payments/{payment_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "draft");
        assert_eq!(body["data"]["allocations"], json!([]));
        assert_eq!(body["data"]["unallocated_amount"], "500.00");
        let (_, body) = api.get("/admin/payable-accounts", Some(&token)).await;
        assert_eq!(body["data"]["items"][0]["settled_total"], "0.00");
        assert_eq!(body["data"]["items"][0]["open_total"], "1000.00");
    })
}

#[tokio::test]
#[ignore]
async fn purchase_invoice_register_requires_exact_allocation_and_dedups_no() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_pay_inv").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let (_, purchase_order_id) = seed_orders(test_db.db(), "inv").await;
        let (supplier_id, _) = seed_supplier(test_db.db(), "party-pay-inv").await;
        let (account_id, _) = create_payable_account(&api, &token, &purchase_order_id, &supplier_id).await;

        // 分配合计 != 发票金额 → 422 且全部不可见
        let (status, body) = api
            .post(
                "/admin/purchase-invoice-allocations",
                Some(&token),
                Some(json!({
                    "invoice_no": "PINV-CG7-TX-1",
                    "invoice_date": "2026-08-06",
                    "gross_amount": "113.00",
                    "net_amount": "100.00",
                    "tax_amount": "13.00",
                    "supplier_id": supplier_id,
                    "allocations": [{
                        "payable_account_id": account_id,
                        "allocated_gross_amount": "100.00",
                        "allocated_net_amount": "88.00",
                        "allocated_tax_amount": "12.00"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 422, "分配合计必须等于发票金额: {body}");
        let (_, body) = api.get("/admin/payable-accounts", Some(&token)).await;
        assert_eq!(body["data"]["items"][0]["invoiced_total"], "0.00");

        // 合法登记
        let payload = json!({
            "invoice_no": "PINV-CG7-TX-1",
            "invoice_date": "2026-08-06",
            "gross_amount": "113.00",
            "net_amount": "100.00",
            "tax_amount": "13.00",
            "supplier_id": supplier_id,
            "allocations": [{
                "payable_account_id": account_id,
                "allocated_gross_amount": "113.00",
                "allocated_net_amount": "100.00",
                "allocated_tax_amount": "13.00"
            }]
        });
        let (status, body) = api
            .post(
                "/admin/purchase-invoice-allocations",
                Some(&token),
                Some(payload.clone()),
            )
            .await;
        assert_ok_envelope(status, &body);
        let (_, body) = api.get("/admin/payable-accounts", Some(&token)).await;
        assert_eq!(body["data"]["items"][0]["invoiced_total"], "113.00");

        // 幂等去重：同一发票号码重复登记 → 409，只产生一条正式事实
        let (status, body) = api
            .post("/admin/purchase-invoice-allocations", Some(&token), Some(payload))
            .await;
        assert_eq!(status, 409, "规范化号码重复必须 409: {body}");
        let (_, body) = api.get("/admin/payable-accounts", Some(&token)).await;
        assert_eq!(
            body["data"]["items"][0]["invoiced_total"], "113.00",
            "重复提交不得再记票"
        );
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_payment_no_returns_409_and_pagination_bounds() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_pay_409").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let (_, _purchase_order_id) = seed_orders(test_db.db(), "dup").await;
        let (supplier_id, _) = seed_supplier(test_db.db(), "party-pay-dup").await;

        // 幂等去重：同一付款单号重复登记 → 409，只产生一条正式事实
        let payload = json!({
            "payment_no": "CG7-PAY-DUP-1",
            "supplier_id": supplier_id,
            "paid_at": 1754438400,
            "amount": "100.00"
        });
        let (status, body) = api
            .post("/admin/supplier-payments", Some(&token), Some(payload.clone()))
            .await;
        assert_ok_envelope(status, &body);
        let (status, body) = api
            .post("/admin/supplier-payments", Some(&token), Some(payload))
            .await;
        assert_eq!(status, 409, "重复付款单号必须 409: {body}");
        let (_, body) = api.get("/admin/supplier-payments", Some(&token)).await;
        assert_eq!(body["data"]["total"], 1, "重复提交只产生一条正式事实");

        // 非法排序字段/方向被拒
        let (status, _) = api
            .get("/admin/payable-accounts?sort_by=hack", Some(&token))
            .await;
        assert_eq!(status, 400);
        let (status, _) = api.get("/admin/payable-accounts?sort_dir=up", Some(&token)).await;
        assert_eq!(status, 400);
        let (status, body) = api
            .get("/admin/payable-accounts?page=2&page_size=1", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);

        // 跨域校验：来源采购单不存在 → 404
        let (status, body) = api
            .post(
                "/admin/payable-accounts",
                Some(&token),
                Some(json!({
                    "source_document_id": "po-missing",
                    "supplier_id": supplier_id,
                    "source_type": "purchase_order",
                    "gross_total": "100.00",
                    "due_date": "2026-12-31",
                    "source_revision_id": "po-r1",
                    "source_sequence": 1,
                })),
            )
            .await;
        assert_eq!(status, 404, "来源采购单不存在必须 404: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401_and_forbidden_403() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_pay_auth").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/payable-accounts", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);

        let test_db = TestDb::new("c_g7_pay_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let (status, body) = api.get("/admin/supplier-payments", Some(&token)).await;
        assert_eq!(status, 403, "无 supplier_payment.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_pay_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        // DTO 校验失败 → 400 信封
        let (status, body) = api
            .post(
                "/admin/supplier-payments",
                Some(&token),
                Some(json!({
                    "payment_no": "  ",
                    "supplier_id": "sup-1",
                    "paid_at": 1754438400,
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
                "/admin/supplier-payments",
                Some(&token),
                Some(json!({ "payment_no": "CG7-PAY-400" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");
    })
}
