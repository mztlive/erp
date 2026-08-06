//! 域 D20 `cost` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控；每个测试用独立随机库名。
//! 覆盖：401/403/400(+422)、happy path 契约形状、409（业务唯一键）、
//! 事务不变量（分配合计不等注入失败全部不可见）、分页与排序边界。

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
    ("cost_entry", "list"),
    ("cost_entry", "detail"),
    ("cost_entry", "create"),
    ("cost_allocation", "list"),
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

/// 种子一条 D13 销售单（成本归属）。
async fn seed_sales_order(db: &Database, suffix: &str) -> String {
    let order = SalesOrder::new(
        SalesOrderId::new(next_id()),
        SalesOrderData {
            order_no: format!("CG7-COST-SO-{suffix}"),
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

/// 构造成本入账请求体。
fn cost_entry_payload(sales_order_id: &str, source_document_id: &str) -> Value {
    json!({
        "cost_type": "product",
        "cost_stage": "actual",
        "cost_scope": "non_voucher_fulfillment",
        "supplier_id": "sup-cg7-1",
        "gross_amount": "113.00",
        "net_amount": "100.00",
        "tax_amount": "13.00",
        "tax_inclusion": true,
        "input_tax_rate": "0.130000",
        "occurred_at": 1754438400,
        "source_fact_type": "purchase_receipt",
        "source_document_id": source_document_id,
        "source_line_id": "line-1",
        "source_version": "v1",
        "allocations": [{
            "sales_order_id": sales_order_id,
            "allocated_gross_amount": "113.00",
            "allocated_net_amount": "100.00"
        }]
    })
}

#[tokio::test]
#[ignore]
async fn happy_path_cost_entry_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_cost_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "happy").await;

        // 初始列表为空 + 契约分页形状
        let (status, body) = api.get("/admin/cost-entries", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"], json!([]));
        assert_eq!(body["data"]["total"], 0);
        assert_eq!(body["data"]["page"], 1);
        assert_eq!(body["data"]["page_size"], 20);

        // 手工成本入账（事实 + 分配行原子可见）
        let (status, body) = api
            .post(
                "/admin/cost-entries",
                Some(&token),
                Some(cost_entry_payload(&sales_order_id, "CG7-REC-001")),
            )
            .await;
        assert_ok_envelope(status, &body);
        let entry = &body["data"];
        assert_eq!(entry["cost_type"], "product");
        assert_eq!(entry["cost_stage"], "actual");
        assert_eq!(entry["gross_amount"], "113.00");
        assert_eq!(entry["net_amount"], "100.00");
        assert_eq!(entry["tax_amount"], "13.00");
        assert_eq!(entry["allocations"][0]["sales_order_id"], sales_order_id);
        assert_eq!(entry["allocations"][0]["allocated_gross_amount"], "113.00");
        let entry_id = entry["id"].as_str().unwrap().to_string();

        // 列表 + 筛选
        let (status, body) = api
            .get("/admin/cost-entries?source_document_id=CG7-REC", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["id"], entry_id);

        // 分配列表（按成本事实筛选）
        let (status, body) = api
            .get(
                &format!("/admin/cost-allocations?cost_entry_id={entry_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["cost_entry_id"], entry_id);

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn cost_entry_requires_exact_allocation_and_rolls_back_everything() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_cost_tx").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "tx").await;

        // 注入失败：分配合计 != 事实金额 → 422，全部不可见
        let mut payload = cost_entry_payload(&sales_order_id, "CG7-REC-TX-1");
        payload["allocations"][0]["allocated_gross_amount"] = json!("100.00");
        payload["allocations"][0]["allocated_net_amount"] = json!("90.00");
        let (status, body) = api.post("/admin/cost-entries", Some(&token), Some(payload)).await;
        assert_eq!(status, 422, "分配合计必须等于事实金额: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        // 事务不变量：无任何成本事实与分配可见
        let (status, body) = api.get("/admin/cost-entries", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0, "注入失败不得留下成本事实");

        // 归属销售单不存在 → 404
        let (status, body) = api
            .post(
                "/admin/cost-entries",
                Some(&token),
                Some(cost_entry_payload("so-missing", "CG7-REC-TX-2")),
            )
            .await;
        assert_eq!(status, 404, "归属销售单不存在必须 404: {body}");

        // 合法登记后，业务唯一键重复 → 409（幂等去重）
        let (status, body) = api
            .post(
                "/admin/cost-entries",
                Some(&token),
                Some(cost_entry_payload(&sales_order_id, "CG7-REC-TX-3")),
            )
            .await;
        assert_ok_envelope(status, &body);
        let (status, body) = api
            .post(
                "/admin/cost-entries",
                Some(&token),
                Some(cost_entry_payload(&sales_order_id, "CG7-REC-TX-3")),
            )
            .await;
        assert_eq!(status, 409, "业务唯一键重复必须 409: {body}");
        let (_, body) = api.get("/admin/cost-entries", Some(&token)).await;
        assert_eq!(body["data"]["total"], 1, "重复提交只产生一条正式事实");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_sorting_auth_and_validation_bounds() {
    require_mongo!(async {
        let test_db = TestDb::new("c_g7_cost_auth").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let (status, body) = api.get("/admin/cost-entries", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");

        let test_db = TestDb::new("c_g7_cost_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let (status, body) = api.get("/admin/cost-entries", Some(&token)).await;
        assert_eq!(status, 403, "无 cost_entry.list 权限必须 403: {body}");

        let test_db = TestDb::new("c_g7_cost_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());
        let sales_order_id = seed_sales_order(test_db.db(), "page").await;
        for seq in 1..=3 {
            let payload = cost_entry_payload(&sales_order_id, &format!("CG7-REC-PAGE-{seq}"));
            let (status, body) = api.post("/admin/cost-entries", Some(&token), Some(payload)).await;
            assert_ok_envelope(status, &body);
        }

        // 非法排序字段/方向被拒
        let (status, _) = api.get("/admin/cost-entries?sort_by=hack", Some(&token)).await;
        assert_eq!(status, 400);
        let (status, _) = api.get("/admin/cost-entries?sort_dir=up", Some(&token)).await;
        assert_eq!(status, 400);

        // 边界页
        let (status, body) = api
            .get("/admin/cost-entries?page=2&page_size=2", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);

        // DTO 校验失败 → 400
        let (status, body) = api
            .post(
                "/admin/cost-entries",
                Some(&token),
                Some(json!({
                    "cost_type": "product",
                    "cost_stage": "actual",
                    "cost_scope": "non_voucher_fulfillment",
                    "gross_amount": "113.00",
                    "net_amount": "100.00",
                    "tax_amount": "13.00",
                    "tax_inclusion": true,
                    "input_tax_rate": "0.130000",
                    "occurred_at": 1754438400,
                    "source_fact_type": "  ",
                    "source_document_id": "x",
                    "source_line_id": "l",
                    "source_version": "v",
                    "allocations": []
                })),
            )
            .await;
        assert_eq!(status, 400, "空白来源类型与空分配行必须 400: {body}");
        assert_eq!(body["data"], Value::Null);
    })
}
