//! 域 D31 `mall_backfill` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控。覆盖：作业创建校验
//! （`range_end` 必须等于 `T`、禁止重叠正式批次）、START 执行（作业状态 +
//! 后台任务 + 逐项明细 + 进度统计原子写入）、幂等键、RESUME 续跑、分页与排序。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use entities::card_instance::{MallConsumptionCutover, MallConsumptionCutoverData};
use entities::common::time::Instant;
use entities::ids::MallConsumptionCutoverId;
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节）。
const TEST_JWT_SECRET: &str = "c-g10-backfill-test-secret-32-bytes-min";
/// 种子账号可访问的本域权限键（含 D29 支付接收）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("mall_order_fact", "submit"),
    ("mall_order", "list"),
    ("mall_consumption_backfill_job", "list"),
    ("mall_consumption_backfill_job", "create"),
    ("mall_consumption_backfill_job", "detail"),
    ("mall_consumption_backfill_job", "submit"),
    ("mall_consumption_backfill_item", "list"),
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
upload_path = "c-g10-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("c-g10-uploads-{}", uuid::Uuid::new_v4()));
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

/// 种子已启用的切换记录（`T` 为回填范围终点）。
async fn seed_enabled_cutover(db: &Database) -> String {
    let t = 1_750_000_100i64;
    let mut cutover = MallConsumptionCutover::new(
        MallConsumptionCutoverId::new("cutover-1"),
        MallConsumptionCutoverData {
            mall_id: "mall-a".to_string(),
            checklist_reference: None,
        },
    )
    .expect("切换实体构造失败");
    cutover
        .enable(Instant::from_unix_secs(t), "tester")
        .expect("启用切换失败");
    let id = cutover.base.id.clone();
    db.collection::<MallConsumptionCutover>("mall_consumption_cutovers")
        .insert_one(&cutover)
        .await
        .expect("切换种子写入失败");
    id
}

/// 经 D29 接口接收一笔 `T` 前支付（occurred_at 落在回填范围内）。
async fn seed_payment_fact(api: &TestApi, token: &str) {
    let (status, body) = api
        .post(
            "/admin/mall-order-facts",
            Some(token),
            Some(json!({
                "mall_id": "mall-a",
                "source_event_id": "evt-pay-h1",
                "inbox_message_id": "inbox-pay-h1",
                "business_fact_key": "mall-a:PAYMENT:H1:v1",
                "fact_type": "PAYMENT_SUCCEEDED",
                "external_order_no": "H1",
                "external_order_version": "v1",
                "occurred_at": 1_749_000_000,
                "received_at": 1_749_000_010,
                "data_source": "history_backfill",
                "payment": {
                    "mall_user_ref": "user-1",
                    "ordered_at": 1_748_999_900,
                    "gross_amount": "50.00",
                    "discount_amount": "0.00",
                    "freight_amount": "0.00",
                    "paid_amount": "50.00",
                    "items": [{
                        "external_item_id": "item-1",
                        "name_snapshot": "历史商品",
                        "quantity": "1.000000",
                        "unit_price_gross": "50.0000",
                        "allocated_discount_amount": "0.00",
                        "allocated_freight_amount": "0.00",
                        "sales_tax_rate": "0.130000",
                        "cost_snapshot_total": "40.00",
                        "cost_tax_inclusion": true,
                        "cost_input_tax_rate": "0.130000"
                    }],
                    "payment_sources": [
                        { "source_no": 1, "source_type": "WECHAT", "amount": "50.00", "wechat_payment_ref": "wx-h1" }
                    ],
                    "funding_allocations": [
                        { "external_item_id": "item-1", "source_no": 1, "allocated_payment_amount": "50.00" }
                    ]
                }
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
}

#[tokio::test]
#[ignore]
async fn backfill_job_lifecycle_start_writes_job_items_and_background_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("backfill_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let cutover_id = seed_enabled_cutover(test_db.db()).await;
        seed_payment_fact(&api, &token).await;

        // 创建回填任务：range_end 必须等于 T。
        let (status, body) = api
            .post(
                "/admin/mall-consumption-backfill-jobs",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "cutover_id": cutover_id,
                    "range_start": 1_740_000_000,
                    "range_end": 1_750_000_100,
                    "total_count": 1,
                    "total_amount": "50.00"
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let job = &body["data"];
        assert_eq!(job["status"], "pending");
        assert_eq!(job["range_end"], 1_750_000_100);
        let job_id = job["id"].as_str().unwrap().to_string();
        let version = job["version"].as_u64().unwrap();

        let (status, body) = api
            .get("/admin/mall-consumption-backfill-jobs", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);

        // 列表投影不含进度，详情含明细总数。
        let (status, body) = api
            .get(
                &format!("/admin/mall-consumption-backfill-jobs/{job_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["item_total_count"], 0);

        // START：同步执行回填（作业状态 + 后台任务 + 明细 + 进度原子写入）。
        let (status, body) = api
            .post(
                &format!("/admin/mall-consumption-backfill-jobs/{job_id}/commands"),
                Some(&token),
                Some(json!({
                    "command": "START",
                    "version": version,
                    "operation_id": "op-1",
                    "idempotency_key": "idem-start-1"
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "COMMITTED");

        let (status, body) = api
            .get(
                &format!("/admin/mall-consumption-backfill-jobs/{job_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["job"]["status"], "completed");
        assert_eq!(body["data"]["job"]["actual_count"], 1);
        assert_eq!(body["data"]["job"]["none_count"], 0);
        assert_eq!(body["data"]["job"]["deduplicated_count"], 0);
        assert_eq!(body["data"]["item_total_count"], 1);

        // 明细契约形状。
        let (status, body) = api
            .get(
                &format!("/admin/mall-consumption-backfill-items?job_id={job_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        let item = &body["data"]["items"][0];
        assert_eq!(item["result"], "new");
        assert_eq!(item["cost_basis"], "ACTUAL");
        assert_eq!(item["business_fact_key"], "mall-a:PAYMENT:H1:v1");

        // D04 后台任务登记。
        let background = test_db
            .db()
            .collection::<Document>("background_jobs")
            .count_documents(doc! { "domain_job_type": "mall_consumption_backfill" })
            .await
            .unwrap();
        assert_eq!(background, 1);
    })
}

#[tokio::test]
#[ignore]
async fn backfill_start_is_idempotent_by_idempotency_key() {
    require_mongo!(async {
        let test_db = TestDb::new("backfill_api_idem").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let cutover_id = seed_enabled_cutover(test_db.db()).await;
        seed_payment_fact(&api, &token).await;

        let (_, body) = api
            .post(
                "/admin/mall-consumption-backfill-jobs",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "cutover_id": cutover_id,
                    "range_start": 1_740_000_000,
                    "range_end": 1_750_000_100,
                    "total_count": 1,
                    "total_amount": "50.00"
                })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let job_id = body["data"]["id"].as_str().unwrap().to_string();
        let version = body["data"]["version"].as_u64().unwrap();
        let command = json!({
            "command": "START",
            "version": version,
            "operation_id": "op-1",
            "idempotency_key": "idem-start-1"
        });

        let (status, body) = api
            .post(
                &format!("/admin/mall-consumption-backfill-jobs/{job_id}/commands"),
                Some(&token),
                Some(command.clone()),
            )
            .await;
        assert_ok_envelope(status, &body);

        // 同一幂等键重复提交 → 命中既有后台任务，不重复执行。
        let (status, body) = api
            .post(
                &format!("/admin/mall-consumption-backfill-jobs/{job_id}/commands"),
                Some(&token),
                Some(command),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert!(body["data"]["next_step"].as_str().unwrap().contains("重复提交"));

        let items = test_db
            .db()
            .collection::<Document>("mall_consumption_backfill_items")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(items, 1, "重复提交不重复建明细");
    })
}

#[tokio::test]
#[ignore]
async fn resume_after_failure_continues_along_original_scope() {
    require_mongo!(async {
        let test_db = TestDb::new("backfill_api_resume").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let cutover_id = seed_enabled_cutover(test_db.db()).await;
        seed_payment_fact(&api, &token).await;

        let (_, body) = api
            .post(
                "/admin/mall-consumption-backfill-jobs",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "cutover_id": cutover_id,
                    "range_start": 1_740_000_000,
                    "range_end": 1_750_000_100,
                    "total_count": 1,
                    "total_amount": "50.00"
                })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let job_id = body["data"]["id"].as_str().unwrap().to_string();

        // 模拟运行中断：直接推进为失败（原任务续跑路径）。
        let now = Instant::now().unix_secs();
        test_db
            .db()
            .collection::<Document>("mall_consumption_backfill_jobs")
            .update_one(
                doc! { "id": job_id.clone() },
                doc! { "$set": { "status": "failed", "updated_at": now } },
            )
            .await
            .unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/mall-consumption-backfill-jobs/{job_id}/commands"),
                Some(&token),
                Some(json!({
                    "command": "RESUME",
                    "version": 2,
                    "operation_id": "op-resume-1",
                    "idempotency_key": "idem-resume-1"
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "COMMITTED");

        let (status, body) = api
            .get(
                &format!("/admin/mall-consumption-backfill-jobs/{job_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["job"]["status"], "completed");
        assert_eq!(body["data"]["job"]["actual_count"], 1);

        // 续跑沿用原任务与原明细键，不制造第二份明细。
        let items = test_db
            .db()
            .collection::<Document>("mall_consumption_backfill_items")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(items, 1);

        // 已完成任务不可再 START。
        let (status, body) = api
            .post(
                &format!("/admin/mall-consumption-backfill-jobs/{job_id}/commands"),
                Some(&token),
                Some(json!({
                    "command": "START",
                    "version": 3,
                    "operation_id": "op-again",
                    "idempotency_key": "idem-again"
                })),
            )
            .await;
        assert_eq!(status, 422, "已完成任务禁止重复启动: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn create_validation_rejects_bad_range_and_overlap() {
    require_mongo!(async {
        let test_db = TestDb::new("backfill_api_validate").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let cutover_id = seed_enabled_cutover(test_db.db()).await;
        seed_payment_fact(&api, &token).await;

        // range_end != T → 422。
        let (status, body) = api
            .post(
                "/admin/mall-consumption-backfill-jobs",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "cutover_id": cutover_id,
                    "range_start": 1_740_000_000,
                    "range_end": 1_750_000_200,
                    "total_count": 1,
                    "total_amount": "50.00"
                })),
            )
            .await;
        assert_eq!(status, 422, "range_end 必须等于 T: {body}");

        // 不存在的切换 → 404。
        let (status, body) = api
            .post(
                "/admin/mall-consumption-backfill-jobs",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "cutover_id": "cutover-missing",
                    "range_start": 1_740_000_000,
                    "range_end": 1_750_000_100,
                    "total_count": 1,
                    "total_amount": "50.00"
                })),
            )
            .await;
        assert_eq!(status, 404, "切换不存在必须 404: {body}");

        // 合法创建后，重叠正式批次被拒。
        let (status, body) = api
            .post(
                "/admin/mall-consumption-backfill-jobs",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "cutover_id": cutover_id,
                    "range_start": 1_740_000_000,
                    "range_end": 1_750_000_100,
                    "total_count": 1,
                    "total_amount": "50.00"
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let job_id = body["data"]["id"].as_str().unwrap().to_string();
        let version = body["data"]["version"].as_u64().unwrap();
        let (status, _) = api
            .post(
                &format!("/admin/mall-consumption-backfill-jobs/{job_id}/commands"),
                Some(&token),
                Some(json!({
                    "command": "START",
                    "version": version,
                    "operation_id": "op-1",
                    "idempotency_key": "idem-1"
                })),
            )
            .await;
        assert_ok_envelope(status, &body);

        let (status, body) = api
            .post(
                "/admin/mall-consumption-backfill-jobs",
                Some(&token),
                Some(json!({
                    "mall_id": "mall-a",
                    "cutover_id": cutover_id,
                    "range_start": 1_740_000_000,
                    "range_end": 1_750_000_100,
                    "total_count": 1,
                    "total_amount": "50.00"
                })),
            )
            .await;
        assert_eq!(status, 422, "重叠正式批次必须被拒: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_forbidden_and_pagination_bounds() {
    require_mongo!(async {
        let test_db = TestDb::new("backfill_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let (status, _) = api.get("/admin/mall-consumption-backfill-jobs", None).await;
        assert_eq!(status, 401);

        let test_db = TestDb::new("backfill_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let (status, body) = api
            .get("/admin/mall-consumption-backfill-items", Some(&token))
            .await;
        assert_eq!(status, 403, "无权限必须 403: {body}");

        let test_db = TestDb::new("backfill_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        // 空白必填 → 400。
        let (status, body) = api
            .post(
                "/admin/mall-consumption-backfill-jobs",
                Some(&token),
                Some(json!({
                    "mall_id": " ",
                    "cutover_id": "c",
                    "range_start": 1,
                    "range_end": 2,
                    "total_count": 1,
                    "total_amount": "1.00"
                })),
            )
            .await;
        assert_eq!(status, 400, "空白商城必须 400: {body}");

        // 非法排序 → 400；分页越界 → 400。
        let (status, body) = api
            .get("/admin/mall-consumption-backfill-jobs?sort_by=evil", Some(&token))
            .await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");
        let (status, body) = api
            .get("/admin/mall-consumption-backfill-items?page_size=0", Some(&token))
            .await;
        assert_eq!(status, 400, "分页大小必须 ≥1: {body}");
    })
}
