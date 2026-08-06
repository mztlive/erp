//! 域 D27 `projection` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test projection_api -- --include-ignored`。
//!
//! 跨域前置：投影快照由 D13 销售单当前版本与唯一卡券行派生，测试直接以实体
//! 构造方式种子 `sales_orders`/`sales_order_revisions`/`sales_order_revision_lines`/
//! `sales_order_voucher_line_revisions`。
//!
//! 外部 HTTP 调用（投影下发）不真调外部网络：handler 使用默认失败关闭连接器，
//! 覆盖「失败降级为可观测错误」路径（`inbox_message` + `integration_error_task`）；
//! mock 成功路径在 services 单测覆盖。

use std::path::PathBuf;
use std::str::FromStr;

use axum::Router;
use config::{Config, SafeConfig};
use entities::common::time::Instant;
use entities::ids::{
    CustomerAccountId, PartyId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId,
    SalesOrderRevisionLineId, SalesOrderVoucherLineRevisionId, SkuId,
};
use entities::money::{Amount, Rate, UnitPrice};
use entities::sales_order::snapshot::HeaderSnapshotData;
use entities::sales_order::{
    BusinessType, LineType, OriginSystem, RevisionSource, SalesOrder, SalesOrderData, SalesOrderRevision,
    SalesOrderRevisionData, SalesOrderRevisionLine, SalesOrderRevisionLineData,
    SalesOrderVoucherLineRevision, SalesOrderVoucherLineRevisionData,
};
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节，同时满足 config 校验与 test-support 签发要求）。
const TEST_JWT_SECRET: &str = "p0-5-test-secret-that-is-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("sales_order_projection", "list"),
    ("sales_order_projection", "detail"),
    ("sales_order_projection", "create"),
    ("sales_order_projection_revision", "create"),
    ("sales_order_projection_revision", "list"),
    ("sales_order_projection_delivery", "submit"),
    ("sales_order_projection_delivery", "list"),
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

/// 构造可复用的表头结构化快照入参。
fn header_snapshot() -> HeaderSnapshotData {
    HeaderSnapshotData {
        customer_name: "东方企业".to_string(),
        contract_no: Some("HT-2026-0088".to_string()),
        settlement_party_name: Some("集团结算中心".to_string()),
        payment_term_code: "NET30".to_string(),
        payment_term_name: "月结 30 天".to_string(),
        invoice_type: "增值税专用发票".to_string(),
        tax_point: "6".to_string(),
    }
}

/// 种子卡券销售单 + 当前版本 + 唯一卡券行（D13 集合）。
///
/// # 返回
/// 返回 `(销售单 ID, 当前版本 ID)`。
async fn seed_voucher_order(db: &Database, tag: &str) -> (String, String) {
    let mut order = SalesOrder::new(
        SalesOrderId::new(format!("so-proj-{tag}")),
        SalesOrderData {
            order_no: format!("SO-PROJ-{tag}"),
            business_type: BusinessType::Voucher,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_id: None,
            settlement_party_id: PartyId::new("party-1"),
            source_status_code: None,
        },
        "admin-1",
    )
    .expect("销售单构造失败");
    let revision = SalesOrderRevision::new(
        SalesOrderRevisionId::new(format!("so-rev-proj-{tag}")),
        SalesOrderRevisionData {
            sales_order_id: order.base.id.clone().into(),
            revision_no: 1,
            revision_source: RevisionSource::ErpApproval,
            source_snapshot_id: None,
            previous_revision_id: None,
            content_hash: format!("hash-proj-{tag}"),
            customer_revision_id: None,
            contract_revision_id: None,
            snapshot: header_snapshot(),
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: Some(SkuId::new("sku-voucher-1")),
            voucher_expiry_at: Some(Instant::from_unix_secs(1_800_000_000)),
            gross_amount: Amount::from_str("100.00").unwrap(),
            net_amount: Amount::from_str("100.00").unwrap(),
            tax_amount: Amount::from_str("0.00").unwrap(),
            effective_at: Instant::from_unix_secs(1_700_000_000),
            recorded_at: Instant::from_unix_secs(1_700_000_100),
        },
    )
    .expect("销售版本构造失败");
    let revision_id = revision.base.id.clone();
    order.stable.current_revision_id = Some(revision.base.id.clone());

    let line = SalesOrderRevisionLine::new(
        SalesOrderRevisionLineId::new(format!("so-rl-proj-{tag}")),
        SalesOrderRevisionLineData {
            sales_order_revision_id: revision.base.id.clone().into(),
            sales_order_line_id: SalesOrderLineId::new(format!("so-line-proj-{tag}")),
            line_no: 1,
            line_type: LineType::Voucher,
            gross_amount: Amount::from_str("100.00").unwrap(),
            net_amount: Amount::from_str("100.00").unwrap(),
            tax_amount: Amount::from_str("0.00").unwrap(),
            sales_tax_rate: Rate::from_str("0.000000").unwrap(),
            item_name_snapshot: "福利商城卡".to_string(),
            spec_snapshot: Some("100 元面额".to_string()),
            unit_snapshot: Some("张".to_string()),
        },
    )
    .expect("公共行版本构造失败");
    let voucher_line = SalesOrderVoucherLineRevision::new(
        SalesOrderVoucherLineRevisionId::new(format!("so-vl-proj-{tag}")),
        SalesOrderVoucherLineRevisionData {
            revision_line_id: line.base.id.clone().into(),
            face_value: Amount::from_str("100.00").unwrap(),
            card_count: 100,
            unit_price_gross: UnitPrice::from_str("100.0000").unwrap(),
            card_form: entities::sales_order::CardForm::Electronic,
        },
    )
    .expect("卡券行版本构造失败");

    db.collection::<SalesOrder>("sales_orders")
        .insert_one(&order)
        .await
        .expect("销售单种子失败");
    db.collection::<SalesOrderRevision>("sales_order_revisions")
        .insert_one(&revision)
        .await
        .expect("销售版本种子失败");
    db.collection::<SalesOrderRevisionLine>("sales_order_revision_lines")
        .insert_one(&line)
        .await
        .expect("公共行版本种子失败");
    db.collection::<SalesOrderVoucherLineRevision>("sales_order_voucher_line_revisions")
        .insert_one(&voucher_line)
        .await
        .expect("卡券行版本种子失败");
    (order.base.id.clone(), revision_id)
}

/// 种子一张非卡券销售单（建立投影应被拒）。
async fn seed_non_voucher_order(db: &Database) -> String {
    let order = SalesOrder::new(
        SalesOrderId::new("so-goods-1"),
        SalesOrderData {
            order_no: "SO-GOODS-0001".to_string(),
            business_type: BusinessType::GoodsService,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_id: None,
            settlement_party_id: PartyId::new("party-1"),
            source_status_code: None,
        },
        "admin-1",
    )
    .expect("销售单构造失败");
    let id = order.base.id.clone();
    db.collection::<SalesOrder>("sales_orders")
        .insert_one(&order)
        .await
        .expect("销售单种子失败");
    id
}

/// 构造最小 AppState 并组装完整应用路由。
async fn build_router(test_db: &TestDb) -> (Router, PathBuf) {
    let config = Config::from_toml_str(&format!(
        r#"
[app]
port = 10001
secret = "{TEST_JWT_SECRET}"
upload_path = "projection-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("projection-uploads-{}", uuid::Uuid::new_v4()));
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

/// 组装「建库 + 索引 + 种子账号 + 权限 + token + 路由」的公共前置。
async fn setup(prefix: &str) -> (TestDb, String, Router) {
    let test_db = TestDb::new(prefix).await.unwrap();
    database::ensure_indexes(test_db.db()).await.unwrap();
    let account_id = seed_admin_account(test_db.db()).await.unwrap();
    grant_domain_permissions(test_db.db(), &account_id).await;
    let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
    let (router, _) = build_router(&test_db).await;
    (test_db, token, router)
}

/// 建立投影并返回响应视图（供后续用例复用）。
async fn create_projection(api: &TestApi, token: &str, sales_order_id: &str) -> Value {
    let (status, body) = api
        .post(
            "/admin/sales-order-projections",
            Some(token),
            Some(json!({
                "sales_order_id": sales_order_id,
                "target_mall_id": "mall-proj-1",
                "customer_external_identity": "mall-customer-001",
                "voucher_category_external_identity": "mall-voucher-001"
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    body["data"].clone()
}

#[tokio::test]
#[ignore]
async fn happy_path_create_projection_revision_deliver_with_contract_shape() {
    require_mongo!(async {
        let (test_db, token, router) = setup("proj_api_happy").await;
        let (sales_order_id, _) = seed_voucher_order(test_db.db(), "1").await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/sales-order-projections", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"], json!([]));

        let projection = create_projection(&api, &token, &sales_order_id).await;
        let projection_id = projection["id"].as_str().unwrap();
        for field in ["id", "sales_order_id", "target_mall_id", "created_at"] {
            assert!(
                projection.get(field).is_some(),
                "契约字段 {field} 必须存在: {projection}"
            );
        }
        assert_eq!(projection["sales_order_id"], sales_order_id);
        assert_eq!(projection["target_mall_id"], "mall-proj-1");
        assert!(
            projection["current_acked_revision_id"].is_null(),
            "初始无商城确认版本"
        );

        let (status, body) = api
            .get(
                &format!("/admin/sales-order-projections/{projection_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["sales_order_id"], sales_order_id);

        let (status, body) = api
            .get(
                &format!("/admin/sales-order-projections/{projection_id}/revisions"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let revisions = body["data"].as_array().unwrap();
        assert_eq!(revisions.len(), 1);
        let first = &revisions[0];
        assert_eq!(first["revision_no"], 1);
        assert_eq!(
            first["projection_source"], "cutover_snapshot",
            "首版投影为存量单切换快照"
        );
        assert_eq!(first["customer_external_identity"], "mall-customer-001");
        assert_eq!(
            first["face_value"], "100.00",
            "面额由 ERP 卡券行派生且按字符串序列化"
        );
        assert_eq!(first["card_count"], 100, "卡张数由 ERP 卡券行派生");
        assert_eq!(first["card_form"], "electronic");

        let (status, body) = api
            .get("/admin/sales-order-projection-deliveries", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["status"], "pending_send");

        let (status, body) = api
            .post(
                &format!("/admin/sales-order-projections/{projection_id}/revisions/1/deliver"),
                Some(&token),
                Some(json!({ "idempotency_key": "deliver-001" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let result = &body["data"];
        assert_eq!(
            result["delivery_status"], "failed",
            "默认连接器失败关闭 → 下发失败"
        );
        assert!(!result["error_task_id"].as_str().unwrap().is_empty());

        let inbox_count = test_db
            .db()
            .collection::<Document>("inbox_messages")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(inbox_count, 1, "一次下发只落一条消息信封");
        let inbox = test_db
            .db()
            .collection::<Document>("inbox_messages")
            .find_one(doc! {})
            .await
            .unwrap()
            .expect("消息信封必须存在");
        assert_eq!(inbox.get_str("status").unwrap(), "failed");

        let task_count = test_db
            .db()
            .collection::<Document>("integration_error_tasks")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(task_count, 1, "失败必须创建一条集成错误任务");

        let (status, body) = api
            .get(
                &format!("/admin/sales-order-projections/{projection_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert!(
            body["data"]["current_acked_revision_id"].is_null(),
            "商城未确认不得推进确认版本（§6.16）"
        );
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("proj_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/sales-order-projections", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("proj_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/sales-order-projections", Some(&token)).await;
        assert_eq!(status, 403, "无 sales_order_projection.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_identity_and_non_voucher_order_return_400_and_404() {
    require_mongo!(async {
        let (test_db, token, router) = setup("proj_api_400").await;
        let (sales_order_id, _) = seed_voucher_order(test_db.db(), "2").await;
        seed_non_voucher_order(test_db.db()).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/sales-order-projections",
                Some(&token),
                Some(json!({
                    "sales_order_id": "so-missing",
                    "target_mall_id": "mall-proj-1",
                    "customer_external_identity": "mall-customer-001",
                    "voucher_category_external_identity": "mall-voucher-001"
                })),
            )
            .await;
        assert_eq!(status, 404, "不存在的销售单必须 404: {body}");

        let (status, body) = api
            .post(
                "/admin/sales-order-projections",
                Some(&token),
                Some(json!({
                    "sales_order_id": "so-goods-1",
                    "target_mall_id": "mall-proj-1",
                    "customer_external_identity": "mall-customer-001",
                    "voucher_category_external_identity": "mall-voucher-001"
                })),
            )
            .await;
        assert_eq!(status, 400, "非卡券销售单无法建立投影必须 400: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = api
            .post(
                "/admin/sales-order-projections",
                Some(&token),
                Some(json!({
                    "sales_order_id": sales_order_id,
                    "target_mall_id": "mall-proj-1",
                    "customer_external_identity": "   ",
                    "voucher_category_external_identity": "mall-voucher-001"
                })),
            )
            .await;
        assert_eq!(status, 400, "空白商城客户标识必须 400: {body}");
        assert_eq!(body["data"], Value::Null);
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_projection_and_duplicate_revision_return_409() {
    require_mongo!(async {
        let (test_db, token, router) = setup("proj_api_409").await;
        let (sales_order_id, _) = seed_voucher_order(test_db.db(), "3").await;
        let api = TestApi::new(router);

        let _ = create_projection(&api, &token, &sales_order_id).await;
        let (status, body) = api
            .post(
                "/admin/sales-order-projections",
                Some(&token),
                Some(json!({
                    "sales_order_id": sales_order_id,
                    "target_mall_id": "mall-proj-1",
                    "customer_external_identity": "mall-customer-001",
                    "voucher_category_external_identity": "mall-voucher-001"
                })),
            )
            .await;
        assert_eq!(
            status, 409,
            "(sales_order_id, target_mall_id) 唯一冲突必须 409: {body}"
        );
        assert_eq!(body["success"], false);

        let projection = create_projection(&api, &token, &sales_order_id).await;
        let projection_id = projection["id"].as_str().unwrap();
        let (status, body) = api
            .post(
                &format!("/admin/sales-order-projections/{projection_id}/revisions"),
                Some(&token),
                Some(json!({
                    "customer_external_identity": "mall-customer-001",
                    "voucher_category_external_identity": "mall-voucher-001"
                })),
            )
            .await;
        assert_eq!(
            status, 409,
            "同一 (sales_order_revision_id, target_mall_id) 重复推进必须 409: {body}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn advance_revision_uses_erp_revision_source_and_blocks_inflight_delivery() {
    require_mongo!(async {
        let (test_db, token, router) = setup("proj_api_advance").await;
        let (sales_order_id, _) = seed_voucher_order(test_db.db(), "4").await;
        let api = TestApi::new(router);

        let projection = create_projection(&api, &token, &sales_order_id).await;
        let projection_id = projection["id"].as_str().unwrap();

        // 推进销售版本 2 并更新销售单当前版本引用。
        let new_revision = SalesOrderRevision::new(
            SalesOrderRevisionId::new("so-rev-proj-4-v2"),
            SalesOrderRevisionData {
                sales_order_id: SalesOrderId::new(sales_order_id.clone()),
                revision_no: 2,
                revision_source: RevisionSource::SalesChange,
                source_snapshot_id: None,
                previous_revision_id: Some(SalesOrderRevisionId::new("so-rev-proj-4")),
                content_hash: "hash-proj-4-v2".to_string(),
                customer_revision_id: None,
                contract_revision_id: None,
                snapshot: header_snapshot(),
                project_name: None,
                business_remark: None,
                voucher_category_sku_id: Some(SkuId::new("sku-voucher-1")),
                voucher_expiry_at: Some(Instant::from_unix_secs(1_800_000_000)),
                gross_amount: Amount::from_str("120.00").unwrap(),
                net_amount: Amount::from_str("120.00").unwrap(),
                tax_amount: Amount::from_str("0.00").unwrap(),
                effective_at: Instant::from_unix_secs(1_700_000_200),
                recorded_at: Instant::from_unix_secs(1_700_000_300),
            },
        )
        .expect("销售版本 2 构造失败");
        test_db
            .db()
            .collection::<SalesOrderRevision>("sales_order_revisions")
            .insert_one(&new_revision)
            .await
            .expect("销售版本 2 种子失败");
        let mut order_doc = test_db
            .db()
            .collection::<Document>("sales_orders")
            .find_one(doc! { "id": &sales_order_id })
            .await
            .unwrap()
            .expect("销售单必须存在");
        order_doc.insert("current_revision_id", new_revision.base.id.clone());
        test_db
            .db()
            .collection::<Document>("sales_orders")
            .replace_one(doc! { "id": &sales_order_id }, order_doc)
            .await
            .unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/sales-order-projections/{projection_id}/revisions"),
                Some(&token),
                Some(json!({
                    "customer_external_identity": "mall-customer-001",
                    "voucher_category_external_identity": "mall-voucher-001"
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let revision = &body["data"];
        assert_eq!(revision["revision_no"], 2);
        assert_eq!(
            revision["projection_source"], "erp_revision",
            "后续版本来源为 ERP 销售版本"
        );
        assert_eq!(revision["sales_order_revision_id"], "so-rev-proj-4-v2");
        assert_eq!(revision["face_value"], "100.00");
        assert_eq!(revision["card_count"], 100);

        let (status, body) = api
            .post(
                &format!("/admin/sales-order-projections/{projection_id}/revisions/2/deliver"),
                Some(&token),
                Some(json!({ "idempotency_key": "deliver-adv" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["delivery_status"], "failed");

        let (status, body) = api
            .post(
                &format!("/admin/sales-order-projections/{projection_id}/revisions/2/deliver"),
                Some(&token),
                Some(json!({ "idempotency_key": "deliver-adv-2" })),
            )
            .await;
        assert_eq!(status, 409, "未确认版本重复下发必须 409: {body}");

        let delivery_count = test_db
            .db()
            .collection::<Document>("sales_order_projection_deliveries")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(delivery_count, 2, "两个版本各一条下发记录，重复提交不新增");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_boundaries_are_enforced() {
    require_mongo!(async {
        let (test_db, token, router) = setup("proj_api_page").await;
        let (sales_order_id, _) = seed_voucher_order(test_db.db(), "5").await;
        let api = TestApi::new(router);

        let (status, body) = api
            .get(
                &format!("/admin/sales-order-projections?sales_order_id={sales_order_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0, "按销售单筛选初始为空");

        let (status, _) = api
            .get("/admin/sales-order-projections?page=0", Some(&token))
            .await;
        assert_eq!(status, 400, "页码 0 必须 400");

        let (status, body) = api
            .get("/admin/sales-order-projections?sort_by=magic", Some(&token))
            .await;
        assert_eq!(status, 400, "白名单外排序字段必须 400: {body}");

        let (status, _) = api
            .get(
                "/admin/sales-order-projection-deliveries?page_size=0",
                Some(&token),
            )
            .await;
        assert_eq!(status, 400, "分页大小非法必须 400");
    })
}
