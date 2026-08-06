//! 域 D33 `supplier_settlement` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test supplier_settlement_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号 + 本域直接 `p` 规则
//! （casbin `g(sub, sub)` 自反匹配）。跨域依赖数据（D32 履约订单/明细）由本测试
//! 直接种子；确认结算的应付（D19）通过接口验证，不预置。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use entities::ids::{SupplierSettlementDifferenceId, SupplierSettlementItemId};
use entities::supplier_settlement::{
    SettlementDifferenceStatus, SettlementDifferenceType, SupplierSettlementDifference,
    SupplierSettlementDifferenceData,
};
use id_generator::next_id;
use mongodb::bson::{doc, to_document, Document};
use mongodb::Database;
use serde_json::{json, Value};
use std::str::FromStr;
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节，同时满足 config 校验与 test-support 签发要求）。
const TEST_JWT_SECRET: &str = "c11-stl-test-secret-that-is-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("supplier_settlement_statement", "list"),
    ("supplier_settlement_statement", "detail"),
    ("supplier_settlement_statement", "create"),
    ("supplier_settlement_statement", "submit"),
    ("supplier_settlement_statement", "confirm"),
    ("supplier_settlement_statement", "void"),
    ("supplier_settlement_item", "list"),
    ("supplier_settlement_difference", "list"),
    ("supplier_settlement_difference", "update"),
];
/// 种子供应商与履约单据标识。
const SUPPLIER_ID: &str = "supplier-1";
/// `payable_accounts` 集合名。
const PAYABLE_ACCOUNTS: &str = "payable_accounts";
/// `payable_entries` 集合名。
const PAYABLE_ENTRIES: &str = "payable_entries";
/// `supplier_settlement_differences` 集合名。
const SETTLEMENT_DIFFERENCES: &str = "supplier_settlement_differences";

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

/// 构造最小 AppState（默认配置 + 临时上传目录）并组装完整应用路由。
async fn build_router(test_db: &TestDb) -> (Router, PathBuf) {
    let config = Config::from_toml_str(&format!(
        r#"
[app]
port = 10001
secret = "{TEST_JWT_SECRET}"
upload_path = "c11-stl-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("c11-stl-uploads-{}", uuid::Uuid::new_v4()));
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

/// 种子供应商履约订单与履约明细（D32 原始文档，仅承载本域校验所需字段）。
async fn seed_fulfillment(db: &Database) -> (String, String) {
    let order_id = next_id();
    let item_id = next_id();
    db.collection::<Document>("supplier_fulfillment_orders")
        .insert_one(doc! { "id": &order_id, "version": 1, "created_at": 0, "updated_at": 0, "deleted_at": 0 })
        .await
        .expect("插入履约订单种子失败");
    db.collection::<Document>("supplier_fulfillment_items")
        .insert_one(
            doc! { "id": &item_id, "version": 1, "created_at": 0, "updated_at": 0, "deleted_at": 0, "supplier_fulfillment_order_id": &order_id })
        .await
        .expect("插入履约明细种子失败");
    (order_id, item_id)
}

/// 构造创建结算单请求体（1 行明细：ERP 计算金额 100.00 = 100 + 10 + 5 − 15）。
fn create_statement_body(order_id: &str, item_id: &str, statement_no: &str) -> Value {
    json!({
        "statement_no": statement_no,
        "supplier_id": SUPPLIER_ID,
        "period_start": "2026-07-01",
        "period_end": "2026-07-31",
        "external_bill_no": "BILL-2026-07",
        "external_bill_version": "1",
        "items": [{
            "supplier_fulfillment_order_id": order_id,
            "supplier_fulfillment_item_id": item_id,
            "order_amount": "100.00",
            "freight_amount": "10.00",
            "service_fee_amount": "5.00",
            "refund_amount": "15.00",
            "supplier_billed_amount": "99.50"
        }]
    })
}

/// 创建结算单并断言成功，返回结算单视图。
async fn create_statement(api: &TestApi, token: &str, body: Value) -> Value {
    let (status, body) = api
        .post("/admin/supplier-settlement-statements", Some(token), Some(body))
        .await;
    assert_ok_envelope(status, &body);
    body["data"].clone()
}

/// 为结算明细种子一条未解决差异（D33 原始实体文档）。
async fn seed_open_difference(db: &Database, statement_item_id: &str) {
    let difference = SupplierSettlementDifference::new(
        SupplierSettlementDifferenceId::new(next_id()),
        SupplierSettlementDifferenceData {
            statement_item_id: SupplierSettlementItemId::new(statement_item_id),
            difference_type: SettlementDifferenceType::Amount,
            difference_amount: entities::money::Amount::from_str("12.00").unwrap(),
            status: SettlementDifferenceStatus::Pending,
            resolution: None,
            resolved_by: None,
            resolved_at: None,
        },
    )
    .expect("差异种子构造失败");
    db.collection::<Document>(SETTLEMENT_DIFFERENCES)
        .insert_one(to_document(&difference).unwrap())
        .await
        .expect("插入差异种子失败");
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("ss_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/supplier-settlement-statements", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("ss_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .get("/admin/supplier-settlement-statements", Some(&token))
            .await;
        assert_eq!(
            status, 403,
            "无 supplier_settlement_statement.list 权限必须 403: {body}"
        );
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("ss_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let mut body = create_statement_body("order-1", "item-1", "ST-400");
        body["statement_no"] = json!("   ");
        let (status, body) = api
            .post("/admin/supplier-settlement-statements", Some(&token), Some(body))
            .await;
        assert_eq!(status, 400, "空白结算单号必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null, "失败时 data 恒为 null");

        let (status, _) = api
            .post(
                "/admin/supplier-settlement-statements",
                Some(&token),
                Some(json!({ "statement_no": "ST-400" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn happy_path_create_then_detail_and_list_with_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("ss_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (order_id, item_id) = seed_fulfillment(test_db.db()).await;
        let created = create_statement(
            &api,
            &token,
            create_statement_body(&order_id, &item_id, "ST-2026-001"),
        )
        .await;
        assert_eq!(created["statement_no"], "ST-2026-001");
        assert_eq!(created["status"], "DRAFT");
        assert_eq!(created["erp_amount"], "100.00", "表头 ERP 金额由明细派生");
        assert_eq!(created["supplier_amount"], "99.50");
        assert_eq!(created["difference_amount"], "-0.50", "差异 = 供应商 − ERP");
        assert_eq!(created["period_start"], "2026-07-01");
        assert_eq!(created["external_bill_no"], "BILL-2026-07");
        assert!(created["version"].as_u64().unwrap() >= 1);
        assert!(!created["prepared_by"].as_str().unwrap().is_empty());

        let statement_id = created["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .get(
                &format!("/admin/supplier-settlement-statements/{statement_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let detail = &body["data"];
        assert_eq!(detail["statement"]["status"], "DRAFT");
        assert_eq!(detail["items"].as_array().unwrap().len(), 1);
        let item = &detail["items"][0];
        assert_eq!(item["erp_calculated_amount"], "100.00");
        assert_eq!(item["order_amount"], "100.00");
        assert_eq!(item["supplier_billed_amount"], "99.50");

        let (status, body) = api
            .get(
                "/admin/supplier-settlement-statements?status=DRAFT&page_size=1",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 1);
        let row = &data["items"][0];
        for field in [
            "id",
            "statement_no",
            "supplier_id",
            "period_start",
            "period_end",
            "erp_amount",
            "supplier_amount",
            "difference_amount",
            "status",
            "prepared_by",
            "version",
            "created_at",
        ] {
            assert!(row.get(field).is_some(), "契约字段 {field} 必须存在: {row}");
        }

        let (status, body) = api
            .get(
                &format!("/admin/supplier-settlement-items?statement_id={statement_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn create_statement_is_idempotent_by_statement_no() {
    require_mongo!(async {
        let test_db = TestDb::new("ss_api_idem").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (order_id, item_id) = seed_fulfillment(test_db.db()).await;
        let first = create_statement(
            &api,
            &token,
            create_statement_body(&order_id, &item_id, "ST-IDEM"),
        )
        .await;
        let second = create_statement(
            &api,
            &token,
            create_statement_body(&order_id, &item_id, "ST-IDEM"),
        )
        .await;
        assert_eq!(first["id"], second["id"], "重复创建必须返回原结算单");
        assert_eq!(first["version"], second["version"], "幂等命中不得推进版本");

        let item_count = test_db
            .db()
            .collection::<Document>("supplier_settlement_items")
            .count_documents(doc! {})
            .await
            .expect("统计结算明细失败");
        assert_eq!(item_count, 1, "重复创建只产生一份结算明细");
    })
}

#[tokio::test]
#[ignore]
async fn submit_review_requires_resolved_differences_then_confirm_forms_payable() {
    require_mongo!(async {
        let test_db = TestDb::new("ss_api_confirm").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (order_id, item_id) = seed_fulfillment(test_db.db()).await;
        let statement = create_statement(
            &api,
            &token,
            create_statement_body(&order_id, &item_id, "ST-CONF"),
        )
        .await;
        let statement_id = statement["id"].as_str().unwrap().to_string();
        let version = statement["version"].as_u64().unwrap();

        let (_, detail) = api
            .get(
                &format!("/admin/supplier-settlement-statements/{statement_id}"),
                Some(&token),
            )
            .await;
        let statement_item_id = detail["data"]["items"][0]["id"].as_str().unwrap().to_string();
        seed_open_difference(test_db.db(), &statement_item_id).await;

        let (status, body) = api
            .post(
                &format!("/admin/supplier-settlement-statements/{statement_id}/submit-review"),
                Some(&token),
                Some(json!({ "version": version })),
            )
            .await;
        assert_eq!(status, 422, "存在未解决差异时提交复核必须 422: {body}");

        let (_, diff_page) = api
            .get(
                &format!("/admin/supplier-settlement-differences?statement_item_id={statement_item_id}"),
                Some(&token),
            )
            .await;
        let difference_id = diff_page["data"]["items"][0]["id"].as_str().unwrap().to_string();
        let difference_version = diff_page["data"]["items"][0]["version"].as_u64().unwrap();
        let (status, body) = api
            .post(
                &format!("/admin/supplier-settlement-differences/{difference_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": difference_version,
                    "status": "COMPENSATED",
                    "resolution": "已按账单补偿",
                    "resolved_by": "复核人-b",
                    "resolved_at": 1753000000,
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "COMPENSATED");

        let (status, body) = api
            .post(
                &format!("/admin/supplier-settlement-statements/{statement_id}/submit-review"),
                Some(&token),
                Some(json!({ "version": version })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "PENDING_REVIEW");
        let review_version = body["data"]["version"].as_u64().unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/supplier-settlement-statements/{statement_id}/confirm"),
                Some(&token),
                Some(json!({ "version": review_version, "reviewed_by": "复核人-b" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let confirmed = &body["data"];
        assert_eq!(confirmed["status"], "CONFIRMED");
        assert!(confirmed["confirmed_at"].as_i64().is_some());
        assert!(!confirmed["payable_account_id"].as_str().unwrap().is_empty());

        let account_count = test_db
            .db()
            .collection::<Document>(PAYABLE_ACCOUNTS)
            .count_documents(doc! { "source_type": "supplier_settlement" })
            .await
            .expect("统计应付账户失败");
        assert_eq!(account_count, 1, "确认必须形成一条结算单应付账户");
        let entry_count = test_db
            .db()
            .collection::<Document>(PAYABLE_ENTRIES)
            .count_documents(doc! { "source_document_id": "ST-CONF" })
            .await
            .expect("统计应付分录失败");
        assert_eq!(entry_count, 1, "确认必须形成一条原始应付分录");
        let account = test_db
            .db()
            .collection::<Document>(PAYABLE_ACCOUNTS)
            .find_one(doc! { "source_document_id": "ST-CONF" })
            .await
            .expect("查询应付账户失败")
            .expect("应付账户必须存在");
        assert_eq!(account.get_str("supplier_id").unwrap(), SUPPLIER_ID);

        let (status, body) = api
            .post(
                &format!("/admin/supplier-settlement-statements/{statement_id}/confirm"),
                Some(&token),
                Some(json!({ "version": review_version, "reviewed_by": "复核人-b" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let account_count = test_db
            .db()
            .collection::<Document>(PAYABLE_ACCOUNTS)
            .count_documents(doc! { "source_type": "supplier_settlement" })
            .await
            .expect("统计应付账户失败");
        assert_eq!(account_count, 1, "重复确认只形成一条应付事实");

        let (status, _) = api
            .post(
                &format!("/admin/supplier-settlement-statements/{statement_id}/confirm"),
                Some(&token),
                Some(json!({ "version": 1, "reviewed_by": "复核人-b" })),
            )
            .await;
        assert_eq!(status, 409, "陈旧版本确认必须 409");
    })
}

#[tokio::test]
#[ignore]
async fn confirm_transaction_invariant_rolls_back_on_injected_failure() {
    require_mongo!(async {
        let test_db = TestDb::new("ss_api_confirm_tx").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (order_id, item_id) = seed_fulfillment(test_db.db()).await;
        let statement = create_statement(
            &api,
            &token,
            create_statement_body(&order_id, &item_id, "ST-CONFTX"),
        )
        .await;
        let statement_id = statement["id"].as_str().unwrap().to_string();
        let version = statement["version"].as_u64().unwrap();
        let (status, body) = api
            .post(
                &format!("/admin/supplier-settlement-statements/{statement_id}/submit-review"),
                Some(&token),
                Some(json!({ "version": version })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let review_version = body["data"]["version"].as_u64().unwrap();

        // 注入失败：预置同 (source_type, source_document_id) 应付账户，
        // 事务内唯一索引冲突使「结算状态 + 应付账户 + 原始分录」整体回滚。
        test_db
            .db()
            .collection::<Document>(PAYABLE_ACCOUNTS)
            .insert_one(doc! {
                "id": next_id(),
                "version": 1, "created_at": 0, "updated_at": 0, "deleted_at": 0,
                "status": "open",
                "current_revision_id": next_id(),
                "lock_version": 1,
                "created_by": "seed",
                "updated_by": "seed",
                "source_document_id": "ST-CONFTX",
                "supplier_id": SUPPLIER_ID,
                "source_type": "supplier_settlement",
                "gross_total": 100.0,
                "settled_total": 0.0,
                "open_total": 100.0,
                "invoiceable_total": 100.0,
                "invoiced_total": 0.0,
                "open_invoiceable_total": 100.0,
            })
            .await
            .expect("预置冲突应付账户失败");

        let (status, _) = api
            .post(
                &format!("/admin/supplier-settlement-statements/{statement_id}/confirm"),
                Some(&token),
                Some(json!({ "version": review_version, "reviewed_by": "复核人-b" })),
            )
            .await;
        assert_eq!(status, 409, "应付账户唯一键冲突必须 409");

        let (_, detail) = api
            .get(
                &format!("/admin/supplier-settlement-statements/{statement_id}"),
                Some(&token),
            )
            .await;
        assert_eq!(
            detail["data"]["statement"]["status"], "PENDING_REVIEW",
            "注入失败后结算状态必须保持原状"
        );
        assert_eq!(
            detail["data"]["statement"]["payable_account_id"],
            Value::Null,
            "注入失败后不得写入应付账户引用"
        );
        let entry_count = test_db
            .db()
            .collection::<Document>(PAYABLE_ENTRIES)
            .count_documents(doc! {})
            .await
            .expect("统计应付分录失败");
        assert_eq!(entry_count, 0, "注入失败后不得留下应付分录");
    })
}

#[tokio::test]
#[ignore]
async fn void_statement_is_idempotent_and_terminal() {
    require_mongo!(async {
        let test_db = TestDb::new("ss_api_void").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (order_id, item_id) = seed_fulfillment(test_db.db()).await;
        let statement = create_statement(
            &api,
            &token,
            create_statement_body(&order_id, &item_id, "ST-VOID"),
        )
        .await;
        let statement_id = statement["id"].as_str().unwrap().to_string();
        let version = statement["version"].as_u64().unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/supplier-settlement-statements/{statement_id}/void"),
                Some(&token),
                Some(json!({ "version": version, "reason": "期间选择错误" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "VOIDED");

        let (status, body) = api
            .post(
                &format!("/admin/supplier-settlement-statements/{statement_id}/void"),
                Some(&token),
                Some(json!({ "version": version, "reason": "重复作废" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "VOIDED", "重复作废返回原结果");

        let (status, _) = api
            .post(
                &format!("/admin/supplier-settlement-statements/{statement_id}/submit-review"),
                Some(&token),
                Some(json!({ "version": version + 1 })),
            )
            .await;
        assert_eq!(status, 422, "已作废终态不可再推进");
    })
}

#[tokio::test]
#[ignore]
async fn difference_resolve_enforces_trio_and_version() {
    require_mongo!(async {
        let test_db = TestDb::new("ss_api_diff").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (order_id, item_id) = seed_fulfillment(test_db.db()).await;
        let statement = create_statement(
            &api,
            &token,
            create_statement_body(&order_id, &item_id, "ST-DIFF"),
        )
        .await;
        let statement_id = statement["id"].as_str().unwrap().to_string();
        let (_, detail) = api
            .get(
                &format!("/admin/supplier-settlement-statements/{statement_id}"),
                Some(&token),
            )
            .await;
        let statement_item_id = detail["data"]["items"][0]["id"].as_str().unwrap().to_string();
        seed_open_difference(test_db.db(), &statement_item_id).await;

        let (_, diff_page) = api
            .get(
                &format!("/admin/supplier-settlement-differences?statement_item_id={statement_item_id}"),
                Some(&token),
            )
            .await;
        let difference_id = diff_page["data"]["items"][0]["id"].as_str().unwrap().to_string();
        let difference_version = diff_page["data"]["items"][0]["version"].as_u64().unwrap();

        let (status, _) = api
            .post(
                &format!("/admin/supplier-settlement-differences/{difference_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": difference_version,
                    "status": "CLOSED",
                })),
            )
            .await;
        assert_eq!(status, 422, "关闭必须填写处理结果三元组");

        let (status, body) = api
            .post(
                &format!("/admin/supplier-settlement-differences/{difference_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": difference_version,
                    "status": "ERP_ACKNOWLEDGED",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "ERP_ACKNOWLEDGED");

        let (status, _) = api
            .post(
                &format!("/admin/supplier-settlement-differences/{difference_id}/resolve"),
                Some(&token),
                Some(json!({
                    "version": difference_version,
                    "status": "CLOSED",
                    "resolution": "关闭",
                    "resolved_by": "财务-1",
                    "resolved_at": 1753000000,
                })),
            )
            .await;
        assert_eq!(status, 409, "陈旧版本处理必须 409");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_boundaries_are_enforced() {
    require_mongo!(async {
        let test_db = TestDb::new("ss_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (order_id, item_id) = seed_fulfillment(test_db.db()).await;
        for statement_no in ["ST-P1", "ST-P2", "ST-P3"] {
            create_statement(
                &api,
                &token,
                create_statement_body(&order_id, &item_id, statement_no),
            )
            .await;
        }

        let (status, body) = api
            .get(
                "/admin/supplier-settlement-statements?page_size=1&page=2&sort_by=period_start&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["page"], 2);

        let (status, body) = api
            .get(
                "/admin/supplier-settlement-statements?sort_by=status",
                Some(&token),
            )
            .await;
        assert_eq!(status, 400, "白名单外排序字段必须 400: {body}");

        let (status, _) = api
            .get("/admin/supplier-settlement-statements?page_size=0", Some(&token))
            .await;
        assert_eq!(status, 400, "非法分页大小必须 400");
    })
}
