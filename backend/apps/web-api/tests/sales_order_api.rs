//! 域 D13 `sales_order` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）。
//! 鉴权链路与生产一致：`seed_admin_account` + 本域直接 `p` 规则
//! （casbin 的 `g(r.sub, p.sub)` 自反匹配，无需改角色）。

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
const TEST_JWT_SECRET: &str = "c-g4-sales-test-secret-that-is-at-least-32-bytes";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("sales_order", "list"),
    ("sales_order", "create"),
    ("sales_order", "detail"),
    ("sales_order", "update"),
    ("sales_order", "submit"),
    ("sales_order", "delete"),
];

/// 发送 PUT 请求（`TestApi` 只提供 GET/POST）。
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

/// 种子客户（建单依赖 D08 客户存在性校验）。
async fn seed_customer(db: &Database) -> String {
    let customer = CustomerAccount::new(
        CustomerAccountId::new("cust-c-g4-so-1"),
        CustomerAccountData {
            party_id: PartyId::new("party-c-g4-so-1"),
            customer_no: "C1001".to_string(),
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
upload_path = "c-g4-sales-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("c-g4-so-uploads-{}", uuid::Uuid::new_v4()));
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

/// 构造实物及服务销售单创建请求体（intent 可指定）。
fn order_payload(customer_id: &str, order_no: &str, intent: &str) -> Value {
    json!({
        "order_no": order_no,
        "business_type": "GOODS_SERVICE",
        "customer_id": customer_id,
        "contract_id": null,
        "settlement_party_id": "party-c-g4-so-1",
        "idempotency_key": format!("idem-{order_no}"),
        "intent": intent,
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
                        "sku_id": "sku-c-g4-1",
                        "sku_revision_id": "skurev-c-g4-1",
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

#[tokio::test]
#[ignore]
async fn happy_path_create_draft_then_detail_and_list_with_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_so_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (status, body) = api.get("/admin/sales-orders", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["items"], json!([]), "初始列表为空");
        assert_eq!(data["total"], 0);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 20);

        let (status, body) = api
            .post(
                "/admin/sales-orders",
                Some(&token),
                Some(order_payload(&customer_id, "SO-2026-0001", "SAVE_DRAFT")),
            )
            .await;
        assert_ok_envelope(status, &body);
        let created = &body["data"];
        assert_eq!(created["order_no"], "SO-2026-0001");
        assert_eq!(created["business_type"], "GOODS_SERVICE");
        assert_eq!(created["commercial_status"], "DRAFT");
        assert_eq!(created["review_status"], "NOT_SUBMITTED");
        assert_eq!(created["version"], 1);
        let order_id = created["id"].as_str().unwrap().to_string();

        let wc = &created["working_copy"];
        assert_eq!(wc["status"], "EDITING");
        assert_eq!(wc["draft_version"], 1);
        assert_eq!(wc["gross_amount"], "29.97", "表头只汇总已舍入行金额");
        assert_eq!(wc["net_amount"], "26.07");
        assert_eq!(wc["tax_amount"], "3.90");
        assert_eq!(wc["lines"].as_array().unwrap().len(), 1);
        assert_eq!(wc["lines"][0]["gross_amount"], "29.97");
        assert_eq!(wc["lines"][0]["quantity"], "3.000000");
        assert_eq!(created["submissions"].as_array().unwrap().len(), 0);
        assert_eq!(created["lines"].as_array().unwrap().len(), 1, "稳定明细行已建");

        let (status, body) = api
            .get(&format!("/admin/sales-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["order_no"], "SO-2026-0001");

        let (status, body) = api
            .get("/admin/sales-orders?order_no=SO-2026-0001", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        let item = &data["items"][0];
        for field in [
            "id",
            "order_no",
            "business_type",
            "origin_system",
            "customer_id",
            "commercial_status",
            "review_status",
            "version",
        ] {
            assert!(item.get(field).is_some(), "契约字段 {field} 必须存在: {item}");
        }
        assert_eq!(item["order_no"], "SO-2026-0001");
        assert_eq!(item["commercial_status"], "DRAFT");

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn save_working_copy_then_submit_enters_review_and_is_idempotent() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_so_submit").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (_, body) = api
            .post(
                "/admin/sales-orders",
                Some(&token),
                Some(order_payload(&customer_id, "SO-2026-0002", "SAVE_DRAFT")),
            )
            .await;
        assert_ok_envelope(200, &body);
        let order_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = put_json(
            &router,
            &format!("/admin/sales-orders/{order_id}/working-copy"),
            &token,
            json!({
                "version": 1,
                "draft": {
                    "editor_user_id": "sales-2",
                    "customer_name": "东方企业",
                    "contract_no": null,
                    "settlement_party_name": "集团结算中心",
                    "payment_term_code": "NET30",
                    "payment_term_name": "月结 30 天",
                    "invoice_type": "增值税专用发票",
                    "tax_point": "6",
                    "project_name": "中秋福利项目",
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
                                "sku_id": "sku-c-g4-1",
                                "sku_revision_id": "skurev-c-g4-1",
                                "welfare_scenario": "ANNUAL_GIFT_BAG",
                                "fulfillment_mode": "COMPANY_WAREHOUSE",
                                "fulfillment_due_at": 1800000000,
                                "quantity": "6.000000",
                                "base_unit_code": "箱",
                                "unit_price_gross": "9.9900"
                            },
                            "voucher": null
                        }
                    ]
                }
            }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["draft_version"], 2, "保存草稿草稿版本递增");
        assert_eq!(body["data"]["gross_amount"], "59.94", "行金额随草稿更新重算");
        let wc_version = body["data"]["version"].as_u64().unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/sales-orders/{order_id}/submit"),
                Some(&token),
                Some(json!({ "version": wc_version, "idempotency_key": "idem-submit-1" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let submission = &body["data"];
        assert_eq!(submission["submission_no"], 1);
        assert_eq!(submission["status"], "IN_REVIEW");
        assert_eq!(submission["gross_amount"], "59.94");
        let submission_id = submission["id"].as_str().unwrap().to_string();

        // 幂等：重复提交返回同一提交，不产生第二条正式事实
        let (status, body) = api
            .post(
                &format!("/admin/sales-orders/{order_id}/submit"),
                Some(&token),
                Some(json!({ "version": wc_version, "idempotency_key": "idem-submit-2" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["id"], submission_id, "重复提交幂等返回既有提交");

        let (status, body) = api
            .get(&format!("/admin/sales-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let detail = &body["data"];
        assert_eq!(detail["commercial_status"], "PENDING_REVIEW");
        assert_eq!(detail["review_status"], "PENDING_PROCUREMENT_CONFIRMATION");
        assert_eq!(
            detail["submissions"].as_array().unwrap().len(),
            1,
            "只产生一条提交"
        );
        assert_eq!(detail["working_copy"]["status"], "SUBMITTED", "草稿被提交锁定");
    })
}

#[tokio::test]
#[ignore]
async fn create_with_submit_intent_enters_review_immediately() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_so_submit_intent").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/sales-orders",
                Some(&token),
                Some(order_payload(&customer_id, "SO-2026-0003", "SUBMIT")),
            )
            .await;
        assert_ok_envelope(status, &body);
        let created = &body["data"];
        assert_eq!(created["commercial_status"], "PENDING_REVIEW");
        assert_eq!(created["review_status"], "PENDING_PROCUREMENT_CONFIRMATION");
        assert_eq!(created["submissions"].as_array().unwrap().len(), 1);
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_so_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/sales-orders", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_so_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/sales-orders", Some(&token)).await;
        assert_eq!(status, 403, "无 sales_order.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_unknown_sort_field_rejected() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_so_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let mut blank = order_payload(&customer_id, "SO-2026-0400", "SAVE_DRAFT");
        blank["order_no"] = json!("   ");
        let (status, body) = api.post("/admin/sales-orders", Some(&token), Some(blank)).await;
        assert_eq!(status, 400, "空白单号必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null, "失败时 data 恒为 null");

        let mut empty_lines = order_payload(&customer_id, "SO-2026-0401", "SAVE_DRAFT");
        empty_lines["draft"]["lines"] = json!([]);
        let (status, body) = api
            .post("/admin/sales-orders", Some(&token), Some(empty_lines))
            .await;
        assert_eq!(status, 400, "空明细必须 400: {body}");

        let (status, _) = api
            .get("/admin/sales-orders?sort_by=amount&sort_dir=asc", Some(&token))
            .await;
        assert_eq!(status, 400, "非法排序字段必须 400");

        let (status, _) = api.get("/admin/sales-orders?page_size=500", Some(&token)).await;
        assert_eq!(status, 400, "越界分页大小必须 400");

        // 422：serde 反序列化失败（缺必填字段）走 axum Json 拒绝
        let (status, _) = api
            .post(
                "/admin/sales-orders",
                Some(&token),
                Some(json!({ "order_no": "SO-2026-0402" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_order_no_and_stale_version_return_409() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_so_409").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (_, body) = api
            .post(
                "/admin/sales-orders",
                Some(&token),
                Some(order_payload(&customer_id, "SO-2026-0500", "SAVE_DRAFT")),
            )
            .await;
        assert_ok_envelope(200, &body);
        let order_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/sales-orders",
                Some(&token),
                Some(order_payload(&customer_id, "SO-2026-0500", "SAVE_DRAFT")),
            )
            .await;
        assert_eq!(status, 409, "重复 order_no 唯一索引冲突必须 409: {body}");

        // 陈旧版本保存草稿
        let (status, body) = put_json(
            &router,
            &format!("/admin/sales-orders/{order_id}/working-copy"),
            &token,
            json!({
                "version": 99,
                "draft": {
                    "editor_user_id": "sales-1",
                    "customer_name": "东方企业",
                    "payment_term_code": "NET30",
                    "payment_term_name": "月结 30 天",
                    "invoice_type": "增值税专用发票",
                    "tax_point": "6",
                    "lines": []
                }
            }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本保存草稿必须 409: {body}");

        // 陈旧版本提交
        let (status, body) = api
            .post(
                &format!("/admin/sales-orders/{order_id}/submit"),
                Some(&token),
                Some(json!({ "version": 99, "idempotency_key": "idem-x" })),
            )
            .await;
        assert_eq!(status, 409, "陈旧版本提交必须 409: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn void_draft_and_pagination_boundary() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g4_so_void").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let customer_id = seed_customer(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        for no in ["SO-2026-0601", "SO-2026-0602", "SO-2026-0603"] {
            let (status, body) = api
                .post(
                    "/admin/sales-orders",
                    Some(&token),
                    Some(order_payload(&customer_id, no, "SAVE_DRAFT")),
                )
                .await;
            assert_ok_envelope(status, &body);
        }

        let (status, body) = api
            .get(
                "/admin/sales-orders?page=2&page_size=2&sort_by=order_no&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 3);
        assert_eq!(data["page"], 2);
        assert_eq!(data["items"].as_array().unwrap().len(), 1, "第二页只剩一条");
        assert_eq!(data["items"][0]["order_no"], "SO-2026-0603");

        let order_id = data["items"][0]["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .post(
                &format!("/admin/sales-orders/{order_id}/void"),
                Some(&token),
                Some(json!({ "version": 1 })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["commercial_status"], "VOIDED");
        assert_eq!(body["data"]["working_copy"], Value::Null, "作废后草稿不再有效");
    })
}
