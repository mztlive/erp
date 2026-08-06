//! 域 D14 `sales_review` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 覆盖第 8 章事务不变量（数据模型）：
//! - §8.1.1 采购确认通过：版本 + 销售状态 + 应收 + 待办 + 审计单事务生效，
//!   注入失败全部不可见，重复通过幂等；
//! - §8.1.2（本批部分）卡券运营审批通过：形成版本与应收、销售状态生效；
//! - §8.1.3（本批部分）销售变更生效：追加新版本与应收差额，基准版本防并发覆盖。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控；鉴权链路与生产一致。

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{header::AUTHORIZATION, HeaderValue, Method, Request};
use axum::Router;
use config::{Config, SafeConfig};
use database::{CustomerExt, NoTransaction};
use entities::customer::{CustomerAccount, CustomerAccountData, CustomerAccountId, CustomerAccountStatus};
use entities::ids::PartyId;
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use tower::ServiceExt;
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节）。
const TEST_JWT_SECRET: &str = "c-g4-sales-review-test-secret-32-bytes-long";
/// 种子账号可访问的权限键（覆盖本域全部接口 + 建单链路）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("sales_order", "list"),
    ("sales_order", "create"),
    ("sales_order", "detail"),
    ("sales_order", "update"),
    ("sales_order", "submit"),
    ("sales_order", "delete"),
    ("procurement_confirmation", "list"),
    ("procurement_confirmation", "detail"),
    ("procurement_confirmation", "update"),
    ("procurement_confirmation", "approve"),
    ("procurement_confirmation", "reject"),
    ("sales_order_review", "list"),
    ("sales_order_review", "approve"),
    ("sales_order_review", "reject"),
    ("sales_change_order", "list"),
    ("sales_change_order", "detail"),
    ("sales_change_order", "create"),
    ("sales_change_order", "submit"),
    ("sales_change_order", "approve"),
    ("sales_change_order", "reject"),
    ("sales_change_order", "delete"),
];

/// 发送 PUT 请求。
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

/// 种子客户。
async fn seed_customer(db: &Database) -> String {
    let customer = CustomerAccount::new(
        CustomerAccountId::new("cust-c-g4-sr-1"),
        CustomerAccountData {
            party_id: PartyId::new("party-c-g4-sr-1"),
            customer_no: "C2001".to_string(),
            default_payment_term_id: None,
            status: CustomerAccountStatus::Active,
        },
        "seed",
    )
    .unwrap();
    db.customer_accounts()
        .create(&customer, &mut NoTransaction)
        .await
        .unwrap();
    customer.base.id
}

/// 构造最小 AppState 并组装完整应用路由。
async fn build_router(test_db: &TestDb) -> (Router, PathBuf) {
    let config = Config::from_toml_str(&format!(
        r#"
[app]
port = 10001
secret = "{TEST_JWT_SECRET}"
upload_path = "c-g4-sr-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("c-g4-sr-uploads-{}", uuid::Uuid::new_v4()));
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

/// 实物及服务销售单创建请求（行含税 29.97 = 3 × 9.99）。
fn order_payload(customer_id: &str, order_no: &str) -> Value {
    json!({
        "order_no": order_no,
        "business_type": "GOODS_SERVICE",
        "customer_id": customer_id,
        "contract_id": null,
        "settlement_party_id": "party-c-g4-sr-1",
        "idempotency_key": format!("idem-{order_no}"),
        "intent": "SAVE_DRAFT",
        "draft": {
            "editor_user_id": "sales-1",
            "customer_name": "东方企业",
            "contract_no": null,
            "settlement_party_name": "集团结算中心",
            "payment_term_code": "NET30",
            "payment_term_name": "月结 30 天",
            "invoice_type": "增值税专用发票",
            "tax_point": "6",
            "project_name": "端午福利项目",
            "business_remark": null,
            "voucher_category_sku_id": null,
            "voucher_expiry_at": null,
            "lines": [
                {
                    "line_no": 1,
                    "line_type": "GOODS_SERVICE",
                    "sales_tax_rate": "0.130000",
                    "item_name_snapshot": "年货礼盒",
                    "spec_snapshot": "10kg",
                    "unit_snapshot": "箱",
                    "goods": {
                        "sku_id": "sku-c-g4-sr-1",
                        "sku_revision_id": "skurev-c-g4-sr-1",
                        "welfare_scenario": "ANNUAL_GIFT_BAG",
                        "fulfillment_mode": "COMPANY_WAREHOUSE",
                        "fulfillment_due_at": 1800000000,
                        "quantity": "3.000000",
                        "base_unit_code": "箱",
                        "unit_price_gross": "9.9900"
                    },
                    "voucher": null
                }
            ]
        }
    })
}

/// 变更目标草稿（数量 4 → 含税 39.96，相对原 29.97 差额 +9.99）。
fn change_draft_payload() -> Value {
    json!({
        "editor_user_id": "sales-1",
        "customer_name": "东方企业",
        "contract_no": null,
        "settlement_party_name": "集团结算中心",
        "payment_term_code": "NET30",
        "payment_term_name": "月结 30 天",
        "invoice_type": "增值税专用发票",
        "tax_point": "6",
        "project_name": "追加数量",
        "business_remark": null,
        "voucher_category_sku_id": null,
        "voucher_expiry_at": null,
        "lines": [
            {
                "line_no": 1,
                "line_type": "GOODS_SERVICE",
                "sales_tax_rate": "0.130000",
                "item_name_snapshot": "年货礼盒",
                "spec_snapshot": "10kg",
                "unit_snapshot": "箱",
                "goods": {
                    "sku_id": "sku-c-g4-sr-1",
                    "sku_revision_id": "skurev-c-g4-sr-1",
                    "welfare_scenario": "ANNUAL_GIFT_BAG",
                    "fulfillment_mode": "COMPANY_WAREHOUSE",
                    "fulfillment_due_at": 1800000000,
                    "quantity": "4.000000",
                    "base_unit_code": "箱",
                    "unit_price_gross": "9.9900"
                },
                "voucher": null
            }
        ]
    })
}

/// 建单 + 提交，返回 `(订单 ID, 提交 ID, 确认批次 ID, 工作副本版本)`。
async fn create_submitted_goods_order(
    api: &TestApi,
    token: &str,
    order_no: &str,
    customer_id: &str,
) -> (String, String, String, u64) {
    let (_, body) = api
        .post(
            "/admin/sales-orders",
            Some(token),
            Some(order_payload(customer_id, order_no)),
        )
        .await;
    assert_ok_envelope(200, &body);
    let order_id = body["data"]["id"].as_str().unwrap().to_string();
    let wc_version = body["data"]["working_copy"]["version"].as_u64().unwrap();

    let (_, body) = api
        .post(
            &format!("/admin/sales-orders/{order_id}/submit"),
            Some(token),
            Some(json!({ "version": wc_version, "idempotency_key": format!("idem-sub-{order_no}") })),
        )
        .await;
    assert_ok_envelope(200, &body);
    let submission_id = body["data"]["id"].as_str().unwrap().to_string();

    let (_, body) = api
        .get(
            &format!("/admin/procurement-confirmations?submission_id={submission_id}"),
            Some(token),
        )
        .await;
    assert_ok_envelope(200, &body);
    assert_eq!(body["data"]["total"], 1, "提交后必须生成采购确认批次");
    let confirmation_id = body["data"]["items"][0]["id"].as_str().unwrap().to_string();
    (order_id, submission_id, confirmation_id, wc_version)
}

/// 保存采购确认分行（覆盖承诺数量 3 箱）。
async fn save_confirmation_lines(
    router: &Router,
    token: &str,
    confirmation_id: &str,
    submission_line_id: &str,
) -> u64 {
    let (_, body) = api_detail(
        router,
        token,
        &format!("/admin/procurement-confirmations/{confirmation_id}"),
    )
    .await;
    assert_ok_envelope(200, &body);
    let version = body["data"]["version"].as_u64().unwrap();

    let (status, body) = put_json(
        router,
        &format!("/admin/procurement-confirmations/{confirmation_id}/lines"),
        token,
        json!({
            "version": version,
            "lines": [
                {
                    "line_no": 1,
                    "sales_order_submission_line_id": submission_line_id,
                    "supplier_id": "sup-c-g4-sr-1",
                    "confirmed_quantity": "3.000000",
                    "latest_cost_gross": "8.5000",
                    "input_tax_rate": "0.130000",
                    "expected_delivery_date": "2026-09-30",
                    "fulfillment_mode": "COMPANY_WAREHOUSE",
                    "supplier_capability_revision_id": "cap-c-g4-sr-1"
                }
            ]
        }),
    )
    .await;
    assert_ok_envelope(status, &body);
    body["data"]["version"].as_u64().unwrap()
}

/// GET 详情辅助（TestApi 已提供，此处仅为语义化封装）。
async fn api_detail(router: &Router, token: &str, path: &str) -> (u16, Value) {
    let request = Request::builder()
        .method(Method::GET)
        .uri(path)
        .header(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        )
        .body(Body::empty())
        .expect("GET 请求构造失败");
    let response = router.clone().oneshot(request).await.expect("路由调用失败");
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("响应体读取失败");
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// 取提交明细行 ID（提交后从订单详情或提交集合查询）。
async fn first_submission_line_id(db: &Database, submission_id: &str) -> String {
    let collection = db.collection::<Document>("sales_order_submission_lines");
    let doc = collection
        .find_one(doc! { "submission_id": submission_id })
        .await
        .unwrap()
        .expect("提交明细必须存在");
    doc.get_str("id").unwrap().to_string()
}

/// 计数辅助。
async fn count(db: &Database, collection: &str) -> u64 {
    db.collection::<Document>(collection)
        .count_documents(doc! {})
        .await
        .unwrap()
}

/// 条件计数辅助。
async fn count_where(db: &Database, collection: &str, filter: Document) -> u64 {
    db.collection::<Document>(collection)
        .count_documents(filter)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore]
async fn section_811_procurement_approval_forms_all_facts_in_one_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_sr_811").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (order_id, submission_id, confirmation_id, _) =
            create_submitted_goods_order(&api, &token, "SO-2026-8101", &customer_id).await;
        let submission_line_id = first_submission_line_id(test_db.db(), &submission_id).await;
        save_confirmation_lines(&router, &token, &confirmation_id, &submission_line_id).await;

        let (status, body) = api
            .post(
                &format!("/admin/procurement-confirmations/{confirmation_id}/approve"),
                Some(&token),
                Some(json!({ "idempotency_key": "idem-approve-8101" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let decision = &body["data"];
        assert_eq!(decision["status"], "APPROVED");
        assert_eq!(decision["confirmation_id"], confirmation_id);
        assert_eq!(decision["sales_order_id"], order_id);
        let revision_id = decision["revision_id"].as_str().unwrap().to_string();
        let account_id = decision["receivable_account_id"].as_str().unwrap().to_string();
        assert!(!revision_id.is_empty());
        assert!(!account_id.is_empty());

        // 五类事实同时生效
        let (_, body) = api
            .get(&format!("/admin/sales-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(200, &body);
        let detail = &body["data"];
        assert_eq!(detail["commercial_status"], "EFFECTIVE");
        assert_eq!(detail["review_status"], "APPROVED");
        assert_eq!(detail["current_revision_id"], revision_id);
        assert_eq!(detail["revisions"].as_array().unwrap().len(), 1);
        assert_eq!(detail["revisions"][0]["revision_no"], 1);

        // 应收原始分录
        let entries = test_db
            .db()
            .collection::<Document>("receivable_entries")
            .find_one(doc! { "source_revision_id": &revision_id })
            .await
            .unwrap()
            .expect("应收原始分录必须存在");
        assert_eq!(entries.get_str("source_fact_type").unwrap(), "SALES_ORDER");
        assert_eq!(entries.get_str("entry_type").unwrap(), "original");
        assert_eq!(entries.get_str("direction").unwrap(), "increase");
        let accounts = test_db
            .db()
            .collection::<Document>("receivable_accounts")
            .find_one(doc! { "id": &account_id })
            .await
            .unwrap()
            .expect("应收往来子账必须存在");
        assert_eq!(accounts.get_str("sales_order_id").unwrap(), order_id);

        // 确认批次已通过
        let (_, body) = api
            .get(
                &format!("/admin/procurement-confirmations/{confirmation_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(200, &body);
        assert_eq!(body["data"]["status"], "APPROVED");
        assert!(body["data"]["handled_by"].is_string());

        // 待办：确认待办完成 + 采购单创建待办派发
        let work_items = test_db.db().collection::<Document>("work_items");
        assert_eq!(
            count(test_db.db(), "work_items").await,
            2,
            "确认待办 + 采购创建待办各一条"
        );
        let completed = work_items
            .find_one(doc! {
                "business_object_type": "procurement_confirmation",
                "business_object_id": &confirmation_id,
                "status": "COMPLETED"
            })
            .await
            .unwrap()
            .expect("确认待办必须已完成");
        assert_eq!(completed.get_str("business_object_id").unwrap(), confirmation_id);
        let creation = work_items
            .find_one(doc! {
                "business_object_type": "purchase_order_creation",
                "business_object_id": &confirmation_id,
                "status": "UNCLAIMED"
            })
            .await
            .unwrap()
            .expect("§8.1.1 生成后续采购待办");
        assert_eq!(
            creation.get_str("completion_action").unwrap(),
            "CREATE_PURCHASE_ORDER"
        );

        // 审计
        assert_eq!(
            count(test_db.db(), "audit_logs").await,
            4,
            "建单 + 保存分行 + 提交 + 通过各写审计"
        );
        let audit = test_db
            .db()
            .collection::<Document>("audit_logs")
            .find_one(doc! { "action": "procurement_confirmation.approve", "resource_id": &order_id })
            .await
            .unwrap()
            .expect("§8.1.1 写审计");
        assert!(audit.get_bool("success").unwrap());
    })
}

#[tokio::test]
#[ignore]
async fn section_811_approve_is_idempotent_and_rolls_back_entire_transaction_on_failure() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_sr_811_tx").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (_order_id, submission_id, confirmation_id, _) =
            create_submitted_goods_order(&api, &token, "SO-2026-8102", &customer_id).await;
        let submission_line_id = first_submission_line_id(test_db.db(), &submission_id).await;
        save_confirmation_lines(&router, &token, &confirmation_id, &submission_line_id).await;

        // 幂等：重复通过返回既有结果，不产生第二条正式事实
        let (_, body) = api
            .post(
                &format!("/admin/procurement-confirmations/{confirmation_id}/approve"),
                Some(&token),
                Some(json!({ "idempotency_key": "idem-approve-a" })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let revision_id = body["data"]["revision_id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/procurement-confirmations/{confirmation_id}/approve"),
                Some(&token),
                Some(json!({ "idempotency_key": "idem-approve-b" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["revision_id"], revision_id,
            "重复通过幂等返回既有版本"
        );
        assert_eq!(
            count(test_db.db(), "sales_order_revisions").await,
            1,
            "只产生一个版本"
        );
        assert_eq!(
            count(test_db.db(), "receivable_entries").await,
            1,
            "只产生一条应收分录"
        );

        // 注入失败：预置同 (订单, 版本号) 冲突文档 → 事务内唯一索引冲突 → 全部不可见
        let order2_no = "SO-2026-8103";
        let (order2_id, submission_id2, confirmation2_id, _) =
            create_submitted_goods_order(&api, &token, order2_no, &customer_id).await;
        let submission_line2 = first_submission_line_id(test_db.db(), &submission_id2).await;
        save_confirmation_lines(&router, &token, &confirmation2_id, &submission_line2).await;
        test_db
            .db()
            .collection::<Document>("sales_order_revisions")
            .insert_one(doc! { "sales_order_id": &order2_id, "revision_no": 1_i32, "conflict": "injected" })
            .await
            .unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/procurement-confirmations/{confirmation2_id}/approve"),
                Some(&token),
                Some(json!({ "idempotency_key": "idem-approve-c" })),
            )
            .await;
        assert_eq!(status, 409, "注入冲突必须 409: {body}");

        // 注入失败后全部不可见
        assert_eq!(
            count(test_db.db(), "sales_order_revisions").await,
            1,
            "失败事务不得留下版本"
        );
        let (_, body) = api
            .get(&format!("/admin/sales-orders/{order2_id}"), Some(&token))
            .await;
        assert_ok_envelope(200, &body);
        assert_eq!(
            body["data"]["commercial_status"], "PENDING_REVIEW",
            "销售状态未推进"
        );
        assert_eq!(body["data"]["review_status"], "PENDING_PROCUREMENT_CONFIRMATION");
        let accounts_for_order2 = count_where(
            test_db.db(),
            "receivable_accounts",
            doc! { "sales_order_id": &order2_id },
        )
        .await;
        assert_eq!(accounts_for_order2, 0, "失败事务不得留下应收子账");
        let entries_for_order2 = test_db
            .db()
            .collection::<Document>("receivable_entries")
            .find_one(doc! { "source_document_id": &order2_id })
            .await
            .unwrap();
        assert!(entries_for_order2.is_none(), "失败事务不得留下应收分录");
        let active_work_items = test_db
            .db()
            .collection::<Document>("work_items")
            .find_one(doc! {
                "business_object_type": "procurement_confirmation",
                "business_object_id": &confirmation2_id,
                "status": { "$in": ["UNCLAIMED", "IN_PROGRESS"] }
            })
            .await
            .unwrap()
            .expect("失败事务不得完成确认待办");
        assert_eq!(active_work_items.get_str("status").unwrap(), "UNCLAIMED");
        let creation_item = test_db
            .db()
            .collection::<Document>("work_items")
            .find_one(
doc! { "business_object_type": "purchase_order_creation", "business_object_id": &confirmation2_id }
)
            .await
            .unwrap();
        assert!(creation_item.is_none(), "失败事务不得派发采购待办");
        let audit = test_db
            .db()
            .collection::<Document>("audit_logs")
            .find_one(doc! { "resource_id": &order2_id })
            .await
            .unwrap();
        assert!(audit.is_none(), "失败事务不得写审计");
    })
}

#[tokio::test]
#[ignore]
async fn section_811_reject_returns_order_to_draft_and_incomplete_coverage_rejected() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_sr_811_rej").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (order_id, submission_id, confirmation_id, _) =
            create_submitted_goods_order(&api, &token, "SO-2026-8104", &customer_id).await;
        let submission_line_id = first_submission_line_id(test_db.db(), &submission_id).await;

        // 覆盖不足：未保存任何分行直接通过 → 400
        let (status, body) = api
            .post(
                &format!("/admin/procurement-confirmations/{confirmation_id}/approve"),
                Some(&token),
                Some(json!({ "idempotency_key": "idem-approve-x" })),
            )
            .await;
        assert_eq!(status, 400, "覆盖不足必须 400: {body}");
        assert_eq!(body["success"], false);

        // 分行确认数量不足（2 < 承诺 3）→ 400
        let (_, body) = api_detail(
            &router,
            &token,
            &format!("/admin/procurement-confirmations/{confirmation_id}"),
        )
        .await;
        let version = body["data"]["version"].as_u64().unwrap();
        let (status, body) = put_json(
            &router,
            &format!("/admin/procurement-confirmations/{confirmation_id}/lines"),
            &token,
            json!({
                "version": version,
                "lines": [
                    {
                        "line_no": 1,
                        "sales_order_submission_line_id": submission_line_id,
                        "supplier_id": "sup-c-g4-sr-1",
                        "confirmed_quantity": "2.000000",
                        "latest_cost_gross": "8.5000",
                        "input_tax_rate": "0.130000",
                        "expected_delivery_date": "2026-09-30",
                        "fulfillment_mode": "COMPANY_WAREHOUSE",
                        "supplier_capability_revision_id": "cap-c-g4-sr-1"
                    }
                ]
            }),
        )
        .await;
        assert_ok_envelope(status, &body);
        let (status, body) = api
            .post(
                &format!("/admin/procurement-confirmations/{confirmation_id}/approve"),
                Some(&token),
                Some(json!({ "idempotency_key": "idem-approve-y" })),
            )
            .await;
        assert_eq!(status, 400, "确认数量不足必须 400: {body}");

        // 驳回 → 销售单回草稿，确认批次 REJECTED
        let (status, body) = api
            .post(
                &format!("/admin/procurement-confirmations/{confirmation_id}/reject"),
                Some(&token),
                Some(json!({
                    "reject_reason_code": "CANNOT_FULFILL",
                    "comment": "供应商无法履约",
                    "idempotency_key": "idem-reject-8104"
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "REJECTED");

        let (_, body) = api
            .get(&format!("/admin/sales-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(200, &body);
        assert_eq!(body["data"]["commercial_status"], "DRAFT", "驳回后销售单回到草稿");
        assert_eq!(body["data"]["review_status"], "NOT_SUBMITTED");
        assert_eq!(body["data"]["submissions"][0]["status"], "REJECTED");

        let completed = test_db
            .db()
            .collection::<Document>("work_items")
            .find_one(
doc! { "business_object_type": "procurement_confirmation", "business_object_id": &confirmation_id }
)
            .await
            .unwrap()
            .expect("确认待办必须存在");
        assert_eq!(
            completed.get_str("status").unwrap(),
            "COMPLETED",
            "驳回完成确认待办"
        );
    })
}

#[tokio::test]
#[ignore]
async fn sales_change_order_full_flow_effective_with_receivable_delta() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_sr_813").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        // 先通过采购确认让销售单生效
        let (order_id, submission_id, confirmation_id, _) =
            create_submitted_goods_order(&api, &token, "SO-2026-8131", &customer_id).await;
        let submission_line_id = first_submission_line_id(test_db.db(), &submission_id).await;
        save_confirmation_lines(&router, &token, &confirmation_id, &submission_line_id).await;
        let (_, body) = api
            .post(
                &format!("/admin/procurement-confirmations/{confirmation_id}/approve"),
                Some(&token),
                Some(json!({ "idempotency_key": "idem-approve-8131" })),
            )
            .await;
        assert_ok_envelope(200, &body);

        // 发起变更（草稿）
        let (status, body) = api
            .post(
                "/admin/sales-change-orders",
                Some(&token),
                Some(json!({
                    "sales_order_id": order_id,
                    "change_type": "QUANTITY",
                    "reason": "客户要求追加数量",
                    "idempotency_key": "idem-change-8131",
                    "draft": change_draft_payload()
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "DRAFT");
        let change_id = body["data"]["id"].as_str().unwrap().to_string();
        let change_version = body["data"]["version"].as_u64().unwrap();

        // 发起影响确认
        let (status, body) = api
            .post(
                &format!("/admin/sales-change-orders/{change_id}/submit-impact"),
                Some(&token),
                Some(json!({ "version": change_version, "idempotency_key": "idem-impact-8131" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "PENDING_IMPACT_CONFIRMATION");
        assert!(body["data"]["current_submission_id"].is_string());

        // 通过影响确认 → 待财务复核
        let (status, body) = api
            .post(
                &format!("/admin/sales-change-orders/{change_id}/impact-confirm"),
                Some(&token),
                Some(json!({ "decision_reason": "采购可履约", "idempotency_key": "idem-ic-8131" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "PENDING_FINANCE_REVIEW");

        // 财务复核通过 → §8.1.3 生效：新版本 + 应收差额
        let (status, body) = api
            .post(
                &format!("/admin/sales-change-orders/{change_id}/finance-confirm"),
                Some(&token),
                Some(json!({ "decision_reason": "财务同意", "idempotency_key": "idem-fc-8131" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "EFFECTIVE");
        assert!(body["data"]["effective_revision_id"].is_string());
        let new_revision_id = body["data"]["effective_revision_id"]
            .as_str()
            .unwrap()
            .to_string();

        let (_, body) = api
            .get(&format!("/admin/sales-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(200, &body);
        let detail = &body["data"];
        assert_eq!(
            detail["current_revision_id"], new_revision_id,
            "销售单切换到新版本"
        );
        assert_eq!(
            detail["revisions"].as_array().unwrap().len(),
            2,
            "追加版本不改写旧版本"
        );
        assert_eq!(detail["revisions"][0]["revision_no"], 2);

        let delta = test_db
            .db()
            .collection::<Document>("receivable_entries")
            .find_one(doc! { "source_revision_id": &new_revision_id, "entry_type": "sales_change_delta" })
            .await
            .unwrap()
            .expect("变更生效必须追加应收差额分录");
        assert_eq!(delta.get_str("direction").unwrap(), "increase", "差额为正 → 增加");
        assert_eq!(count(test_db.db(), "sales_order_revisions").await, 2);
        assert_eq!(count(test_db.db(), "sales_change_orders").await, 1);
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_and_forbidden_requests() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_sr_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/procurement-confirmations", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403_and_invalid_body_400() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_sr_403").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (status, body) = api.get("/admin/procurement-confirmations", Some(&token)).await;
        assert_eq!(
            status, 403,
            "无 procurement_confirmation.list 权限必须 403: {body}"
        );

        // 授权后校验 400（空原因创建变更单）
        grant_domain_permissions(test_db.db(), &account_id).await;
        let (status, body) = api
            .post(
                "/admin/sales-change-orders",
                Some(&token),
                Some(json!({
                    "sales_order_id": "o-1",
                    "change_type": "QUANTITY",
                    "reason": "  ",
                    "idempotency_key": "idem-1",
                    "draft": {
                        "editor_user_id": "sales-1",
                        "customer_name": "东方企业",
                        "payment_term_code": "NET30",
                        "payment_term_name": "月结 30 天",
                        "invoice_type": "增值税专用发票",
                        "tax_point": "6",
                        "lines": []
                    }
                })),
            )
            .await;
        assert_eq!(status, 400, "空白变更原因必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        // 列表排序白名单
        let (status, _) = api
            .get("/admin/sales-order-reviews?sort_by=name", Some(&token))
            .await;
        assert_eq!(status, 400, "非法排序字段必须 400");

        // 422：缺必填字段
        let (status, _) = api
            .post(
                "/admin/sales-change-orders",
                Some(&token),
                Some(json!({ "change_type": "QUANTITY" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        // 未生效销售单发起变更 → 业务校验失败
        let (status, body) = api
            .post(
                "/admin/sales-change-orders",
                Some(&token),
                Some(json!({
                    "sales_order_id": "o-nonexistent",
                    "change_type": "QUANTITY",
                    "reason": "测试",
                    "idempotency_key": "idem-2",
                    "draft": change_draft_payload()
                })),
            )
            .await;
        assert_eq!(status, 404, "销售单不存在必须 404: {body}");
        assert!(!customer_id.is_empty());
    })
}
