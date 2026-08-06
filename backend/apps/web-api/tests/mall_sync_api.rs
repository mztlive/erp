//! 域 D23 `mall_sync` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test mall_sync_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外为种子账号插入本域资源的直接
//! `p` 规则（casbin 的 `g(r.sub, p.sub)` 自反匹配），使 happy path 可鉴权通过，
//! 同时天然构造 403 用例。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use database::{CustomerExt, PartyExt, SalesOrderExt, SourceRegistryExt};
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
    ("mall_sales_sync_job", "list"),
    ("mall_sales_sync_job", "create"),
    ("mall_sales_sync_job", "detail"),
    ("mall_sales_sync_job", "complete"),
    ("mall_sales_order_snapshot", "list"),
    ("mall_sales_order_snapshot", "create"),
    ("mall_sales_sync_cursor", "detail"),
    ("mall_sales_reconciliation_job", "list"),
    ("mall_sales_reconciliation_job", "create"),
    ("mall_sales_reconciliation_item", "list"),
    ("mall_sales_reconciliation_item", "resolve"),
    ("master_mapping_task", "list"),
    ("master_mapping_task", "create"),
    ("master_mapping_task", "resolve"),
];

/// 固定时间基（秒级时间戳，测试区间使用）。
const T0: i64 = 1_754_438_400;
const T1: i64 = 1_754_450_000;

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
upload_path = "p0-5-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("p0-5-uploads-{}", uuid::Uuid::new_v4()));
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

/// 种子来源商城（D01 仓储直写）。
async fn seed_source_system(db: &Database) -> String {
    let id = entities::ids::SourceSystemId::new("sys-mall");
    let system = entities::source_registry::SourceSystem::new(
        id.clone(),
        entities::source_registry::SourceSystemData {
            code: "MALL".to_string(),
            system_type: entities::source_registry::SourceSystemType::Mall,
            name: "福利商城".to_string(),
            status: entities::source_registry::SourceSystemStatus::Active,
        },
        "test",
    )
    .unwrap();
    db.source_systems()
        .create(&system, &mut database::NoTransaction)
        .await
        .unwrap();
    id.to_string()
}

/// 种子 ERP 侧（D07 主体 + D08 客户账号 + D13 销售单），返回销售单 ID。
async fn seed_erp_sales_order(db: &Database) -> String {
    let party = entities::party::Party::new(
        entities::ids::PartyId::new("party-001"),
        entities::party::PartyData {
            party_no: "P-2026-001".to_string(),
            party_kind: entities::party::PartyKind::Enterprise,
            unified_credit_code: None,
            status: entities::party::PartyStatus::Active,
        },
        "test",
    )
    .unwrap();
    db.parties()
        .create(&party, &mut database::NoTransaction)
        .await
        .unwrap();

    let customer = entities::customer::CustomerAccount::new(
        entities::ids::CustomerAccountId::new("cust-001"),
        entities::customer::CustomerAccountData {
            party_id: entities::ids::PartyId::new("party-001"),
            customer_no: "C-2026-001".to_string(),
            default_payment_term_id: None,
            status: entities::customer::CustomerAccountStatus::Active,
        },
        "test",
    )
    .unwrap();
    db.customer_accounts()
        .create(&customer, &mut database::NoTransaction)
        .await
        .unwrap();

    let order = entities::sales_order::SalesOrder::new(
        entities::ids::SalesOrderId::new("so-001"),
        entities::sales_order::SalesOrderData {
            order_no: "SO-2026-001".to_string(),
            business_type: entities::sales_order::BusinessType::Voucher,
            origin_system: entities::sales_order::OriginSystem::Mall,
            source_identity_id: None,
            customer_id: entities::ids::CustomerAccountId::new("cust-001"),
            contract_id: None,
            settlement_party_id: entities::ids::PartyId::new("party-001"),
            source_status_code: Some("EFFECTIVE".to_string()),
        },
        "test",
    )
    .unwrap();
    db.sales_orders()
        .create(&order, &mut database::NoTransaction)
        .await
        .unwrap();
    order.base.id
}

/// 创建基线作业并完成（快照落盘 + 成功完成 + 水位建立），返回 (job_id, batch_id)。
async fn run_baseline(_db: &Database, api: &TestApi, token: &str, suffix: &str) -> (String, String) {
    let (_, body) = api
        .post(
            "/admin/mall-sales-sync-jobs",
            Some(token),
            Some(json!({
                "source_system_id": "sys-mall",
                "job_type": "baseline",
                "range_start": T0,
                "range_end": T0 + 3600,
            })),
        )
        .await;
    assert_ok_envelope(200, &body);
    let job_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = api
        .post(
            "/admin/mall-sales-order-snapshots",
            Some(token),
            Some(json!({
                "sync_job_id": job_id,
                "items": [
                    {
                        "external_order_no": format!("SO-BASE-{suffix}"),
                        "source_updated_at": T0,
                        "content_hash": "sha256:base",
                        "source_status_code": "EFFECTIVE",
                        "normalized_snapshot": format!(r#"{{"order":"SO-BASE-{suffix}"}}"#)
                    }
                ]
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    assert_eq!(body["data"]["accepted"], 1);

    let (status, body) = api
        .post(
            &format!("/admin/mall-sales-sync-jobs/{job_id}/complete"),
            Some(token),
            Some(json!({ "outcome": "success" })),
        )
        .await;
    assert_ok_envelope(status, &body);
    assert_eq!(body["data"]["status"], "success");

    let (status, body) = api
        .get(
            "/admin/mall-sales-sync-cursors?source_system_id=sys-mall",
            Some(token),
        )
        .await;
    assert_ok_envelope(status, &body);
    assert_eq!(
        body["data"]["high_water_updated_at"], T0,
        "期初基线完成后的水位初值取基线拉取开始时间"
    );
    (job_id, body["data"]["id"].as_str().unwrap().to_string())
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/mall-sales-sync-jobs", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/mall-sales-sync-jobs", Some(&token)).await;
        assert_eq!(status, 403, "无 mall_sales_sync_job.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/mall-sales-order-snapshots",
                Some(&token),
                Some(json!({ "sync_job_id": "j-1", "items": [] })),
            )
            .await;
        assert_eq!(status, 400, "空快照列表必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let (_, body) = api
            .post(
                "/admin/mall-sales-sync-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-mall",
                    "job_type": "single_order_backfill",
                })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let running_job_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/mall-sales-order-snapshots",
                Some(&token),
                Some(json!({
                    "sync_job_id": running_job_id,
                    "items": [{
                        "external_order_no": "  ",
                        "source_updated_at": T0,
                        "source_status_code": "EFFECTIVE",
                        "normalized_snapshot": "{}"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 400, "空白来源单号必须 400: {body}");

        let (status, _) = api
            .post(
                "/admin/mall-sales-sync-jobs",
                Some(&token),
                Some(json!({ "source_system_id": "sys-mall" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn happy_path_sync_snapshot_complete_cursor_and_idempotent_reingest() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        seed_source_system(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (_, body) = api
            .post(
                "/admin/mall-sales-sync-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-mall",
                    "job_type": "baseline",
                    "range_start": T0,
                    "range_end": T0 + 3600,
                })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let job = &body["data"];
        for field in [
            "id",
            "source_system_id",
            "job_type",
            "range_start",
            "range_end",
            "started_at",
            "status",
            "page_count",
            "item_count",
            "error_count",
            "version",
            "created_at",
        ] {
            assert!(job.get(field).is_some(), "契约字段 {field} 必须存在: {job}");
        }
        assert_eq!(job["job_type"], "baseline");
        assert_eq!(job["status"], "running");
        let job_id = job["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/mall-sales-order-snapshots",
                Some(&token),
                Some(json!({
                    "sync_job_id": job_id,
                    "items": [
                        {
                            "external_order_no": "SO-100",
                            "source_updated_at": T0,
                            "content_hash": "sha256:100",
                            "source_status_code": "EFFECTIVE",
                            "normalized_snapshot": "{\"order\":\"SO-100\"}"
                        },
                        {
                            "external_order_no": "SO-101",
                            "source_updated_at": T0,
                            "content_hash": "sha256:101",
                            "source_status_code": "EFFECTIVE",
                            "normalized_snapshot": "{\"order\":\"SO-101\"}"
                        }
                    ]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["accepted"], 2);
        assert_eq!(body["data"]["skipped"], 0);
        assert_eq!(body["data"]["snapshot_ids"].as_array().unwrap().len(), 2);

        let (status, body) = api
            .post(
                "/admin/mall-sales-order-snapshots",
                Some(&token),
                Some(json!({
                    "sync_job_id": job_id,
                    "items": [
                        {
                            "external_order_no": "SO-100",
                            "source_updated_at": T0,
                            "content_hash": "sha256:100",
                            "source_status_code": "EFFECTIVE",
                            "normalized_snapshot": "{\"order\":\"SO-100\"}"
                        }
                    ]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["accepted"], 0, "重复推送不产生重复快照");
        assert_eq!(body["data"]["skipped"], 1);

        let (status, body) = api.get("/admin/mall-sales-order-snapshots", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2, "重复落盘后快照总数不变");
        let snapshot = &body["data"]["items"][0];
        for field in [
            "id",
            "source_system_id",
            "external_order_no",
            "source_updated_at",
            "source_status_code",
            "observed_at",
            "mapping_status",
            "sync_job_id",
        ] {
            assert!(
                snapshot.get(field).is_some(),
                "契约字段 {field} 必须存在: {snapshot}"
            );
        }
        assert_eq!(snapshot["mapping_status"], "pending");

        let (status, body) = api
            .post(
                &format!("/admin/mall-sales-sync-jobs/{job_id}/complete"),
                Some(&token),
                Some(json!({ "outcome": "success" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "success");
        assert_eq!(body["data"]["item_count"], 2);

        let (status, body) = api
            .get(
                "/admin/mall-sales-sync-cursors?source_system_id=sys-mall",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let cursor = &body["data"];
        assert_eq!(cursor["high_water_updated_at"], T0, "基线完成水位取拉取开始时间");
        assert_eq!(cursor["last_success_job_id"], job_id);

        let (status, body) = api
            .post(
                &format!("/admin/mall-sales-sync-jobs/{job_id}/complete"),
                Some(&token),
                Some(json!({ "outcome": "success" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "success", "相同终态重复提交按幂等返回");

        let (status, body) = api
            .get(&format!("/admin/mall-sales-sync-jobs/{job_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "success");

        let (status, body) = api.get("/admin/mall-sales-sync-jobs", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["job_type"], "baseline");
    })
}

#[tokio::test]
#[ignore]
async fn create_sync_job_validates_source_and_rejects_concurrent_incremental() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_api_job").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/mall-sales-sync-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-missing",
                    "job_type": "incremental",
                    "range_start": T0,
                    "range_end": T1,
                })),
            )
            .await;
        assert_eq!(status, 404, "来源商城不存在必须 404: {body}");
        assert_eq!(body["success"], false);

        seed_source_system(test_db.db()).await;
        let (status, body) = api
            .post(
                "/admin/mall-sales-sync-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-mall",
                    "job_type": "incremental",
                    "range_start": T0,
                    "range_end": T1,
                })),
            )
            .await;
        assert_ok_envelope(status, &body);

        let (status, body) = api
            .post(
                "/admin/mall-sales-sync-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-mall",
                    "job_type": "incremental",
                    "range_start": T1,
                    "range_end": T1 + 3600,
                })),
            )
            .await;
        assert_eq!(status, 409, "同一商城并发增量任务必须 409: {body}");

        let (status, body) = api
            .post(
                "/admin/mall-sales-sync-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-mall",
                    "job_type": "single_order_backfill",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["job_type"], "single_order_backfill");
    })
}

#[tokio::test]
#[ignore]
async fn failure_does_not_advance_watermark_and_retry_succeeds() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_api_water").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        seed_source_system(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        run_baseline(test_db.db(), &api, &token, "W").await;

        let (_, body) = api
            .post(
                "/admin/mall-sales-sync-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-mall",
                    "job_type": "incremental",
                    "range_start": T0 + 3600,
                    "range_end": T0 + 7200,
                })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let failed_job_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                &format!("/admin/mall-sales-sync-jobs/{failed_job_id}/complete"),
                Some(&token),
                Some(json!({ "outcome": "partial_failure" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "partial_failure");

        let (status, body) = api
            .get(
                "/admin/mall-sales-sync-cursors?source_system_id=sys-mall",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["high_water_updated_at"], T0,
            "部分失败水位不前移（§8.4 第 2 条）"
        );

        let (_, body) = api
            .post(
                "/admin/mall-sales-sync-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-mall",
                    "job_type": "incremental",
                    "range_start": T0 + 3600,
                    "range_end": T0 + 7200,
                })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let retry_job_id = body["data"]["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .post(
                &format!("/admin/mall-sales-sync-jobs/{retry_job_id}/complete"),
                Some(&token),
                Some(json!({ "outcome": "success" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "success");

        let (status, body) = api
            .get(
                "/admin/mall-sales-sync-cursors?source_system_id=sys-mall",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["high_water_updated_at"],
            T0 + 7200,
            "重试成功后水位前移到区间止"
        );

        let (status, body) = api
            .post(
                &format!("/admin/mall-sales-sync-jobs/{failed_job_id}/complete"),
                Some(&token),
                Some(json!({ "outcome": "failed" })),
            )
            .await;
        assert_eq!(status, 409, "已终态且结果不一致必须 409: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn snapshot_ingest_rejects_unknown_and_non_running_job() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_api_ingest").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        seed_source_system(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/mall-sales-order-snapshots",
                Some(&token),
                Some(json!({
                    "sync_job_id": "job-missing",
                    "items": [{
                        "external_order_no": "SO-X",
                        "source_updated_at": T0,
                        "source_status_code": "EFFECTIVE",
                        "normalized_snapshot": "{}"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 404, "未知作业必须 404: {body}");

        let (_, body) = api
            .post(
                "/admin/mall-sales-sync-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-mall",
                    "job_type": "single_order_backfill",
                })),
            )
            .await;
        let job_id = body["data"]["id"].as_str().unwrap().to_string();
        let (_, body) = api
            .post(
                &format!("/admin/mall-sales-sync-jobs/{job_id}/complete"),
                Some(&token),
                Some(json!({ "outcome": "failed" })),
            )
            .await;
        assert_eq!(body["data"]["status"], "failed");

        let (status, body) = api
            .post(
                "/admin/mall-sales-order-snapshots",
                Some(&token),
                Some(json!({
                    "sync_job_id": job_id,
                    "items": [{
                        "external_order_no": "SO-Y",
                        "source_updated_at": T0,
                        "source_status_code": "EFFECTIVE",
                        "normalized_snapshot": "{}"
                    }]
                })),
            )
            .await;
        assert_eq!(status, 422, "非运行中作业禁止落盘快照: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn reconciliation_job_validates_erp_sides_is_idempotent_and_items_resolve() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_api_recon").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        seed_source_system(test_db.db()).await;
        let sales_order_id = seed_erp_sales_order(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/mall-sales-reconciliation-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-mall",
                    "job_no": "REC-2026-06",
                    "source_list_as_of": T1,
                    "source_count": 2,
                    "erp_count": 1,
                    "items": [
                        {
                            "external_order_no": "SO-100",
                            "source_status_code": "EFFECTIVE",
                            "source_updated_at": T0,
                            "source_content_hash": "sha256:mall",
                            "sales_order_id": sales_order_id,
                            "erp_revision_id": "rev-1",
                            "erp_content_hash": "sha256:erp",
                            "difference_type": "status_difference"
                        }
                    ]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let job = &body["data"];
        assert_eq!(job["job_no"], "REC-2026-06");
        assert_eq!(job["status"], "has_difference");
        assert_eq!(job["difference_count"], 1);
        let job_id = job["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/mall-sales-reconciliation-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-mall",
                    "job_no": "REC-2026-06",
                    "source_list_as_of": T1,
                    "source_count": 2,
                    "erp_count": 1,
                    "items": [
                        {
                            "external_order_no": "SO-100",
                            "source_status_code": "EFFECTIVE",
                            "source_updated_at": T0,
                            "sales_order_id": sales_order_id,
                            "difference_type": "status_difference"
                        }
                    ]
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["id"].as_str().unwrap(),
            job_id,
            "核对批次号重复按幂等返回既有作业"
        );

        let (status, body) = api
            .post(
                "/admin/mall-sales-reconciliation-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-mall",
                    "job_no": "REC-2026-07",
                    "source_list_as_of": T1,
                    "source_count": 1,
                    "erp_count": 0,
                    "items": [
                        {
                            "external_order_no": "SO-MISSING-ERP",
                            "source_status_code": "EFFECTIVE",
                            "source_updated_at": T0,
                            "sales_order_id": "so-not-exists",
                            "difference_type": "status_difference"
                        }
                    ]
                })),
            )
            .await;
        assert_eq!(status, 404, "ERP 销售单不存在必须 404: {body}");

        let (status, body) = api
            .get("/admin/mall-sales-reconciliation-jobs", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["status"], "has_difference");

        let (status, body) = api
            .get(
                &format!("/admin/mall-sales-reconciliation-jobs/{job_id}/items"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        let item_id = body["data"]["items"][0]["id"].as_str().unwrap().to_string();
        assert_eq!(body["data"]["items"][0]["status"], "pending");
        assert_eq!(body["data"]["items"][0]["difference_type"], "status_difference");

        let (status, body) = api
            .post(
                &format!("/admin/mall-sales-reconciliation-items/{item_id}/resolve"),
                Some(&token),
                Some(json!({ "kind": "resolve", "resolution": "补拉后一致，保留 ERP 版本" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "resolved");
        assert_eq!(body["data"]["resolution"], "补拉后一致，保留 ERP 版本");

        let (status, body) = api
            .post(
                &format!("/admin/mall-sales-reconciliation-items/{item_id}/resolve"),
                Some(&token),
                Some(json!({ "kind": "resolve", "resolution": "补拉后一致，保留 ERP 版本" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "resolved", "已终态明细重复处理按幂等返回");

        let (status, body) = api
            .post(
                &format!("/admin/mall-sales-reconciliation-items/{item_id}/resolve"),
                Some(&token),
                Some(json!({ "kind": "resolve" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["status"], "resolved",
            "已终态明细重复处理按幂等返回（缺省结论不阻断）"
        );
    })
}

#[tokio::test]
#[ignore]
async fn mapping_task_duplicate_pending_conflicts_and_resolve_is_idempotent() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_api_mapping").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        seed_source_system(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (_, body) = api
            .post(
                "/admin/mall-sales-sync-jobs",
                Some(&token),
                Some(json!({
                    "source_system_id": "sys-mall",
                    "job_type": "single_order_backfill",
                })),
            )
            .await;
        let job_id = body["data"]["id"].as_str().unwrap().to_string();
        let (_, body) = api
            .post(
                "/admin/mall-sales-order-snapshots",
                Some(&token),
                Some(json!({
                    "sync_job_id": job_id,
                    "items": [{
                        "external_order_no": "SO-MAP",
                        "source_updated_at": T0,
                        "source_status_code": "EFFECTIVE",
                        "normalized_snapshot": "{\"customer\":\"C-MALL-1\"}"
                    }]
                })),
            )
            .await;
        let snapshot_id = body["data"]["snapshot_ids"][0].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/master-mapping-tasks",
                Some(&token),
                Some(json!({
                    "source_snapshot_id": snapshot_id,
                    "mapping_type": "customer",
                    "owner_role": "销售",
                    "owner_user_id": "user-sales-1"
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let task = &body["data"];
        for field in [
            "id",
            "source_snapshot_id",
            "mapping_type",
            "status",
            "owner_role",
            "version",
            "created_at",
        ] {
            assert!(task.get(field).is_some(), "契约字段 {field} 必须存在: {task}");
        }
        assert_eq!(task["mapping_type"], "customer");
        assert_eq!(task["status"], "pending");
        let task_id = task["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/master-mapping-tasks",
                Some(&token),
                Some(json!({
                    "source_snapshot_id": snapshot_id,
                    "mapping_type": "customer",
                    "owner_role": "销售",
                })),
            )
            .await;
        assert_eq!(status, 409, "同一快照同类进行中任务必须 409: {body}");

        let (status, body) = api
            .post(
                "/admin/master-mapping-tasks",
                Some(&token),
                Some(json!({
                    "source_snapshot_id": "snap-missing",
                    "mapping_type": "contract",
                    "owner_role": "销售",
                })),
            )
            .await;
        assert_eq!(status, 404, "快照不存在必须 404: {body}");

        let (status, body) = api.get("/admin/master-mapping-tasks", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);

        let (status, body) = api
            .post(
                &format!("/admin/master-mapping-tasks/{task_id}/resolve"),
                Some(&token),
                Some(json!({ "kind": "resolved", "resolution": "映射到客户 C-1" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "resolved");

        let (status, body) = api
            .post(
                &format!("/admin/master-mapping-tasks/{task_id}/resolve"),
                Some(&token),
                Some(json!({ "kind": "resolved", "resolution": "映射到客户 C-1" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "resolved", "已终态任务重复处理按幂等返回");

        let (status, body) = api
            .post(
                &format!("/admin/master-mapping-tasks/{task_id}/resolve"),
                Some(&token),
                Some(json!({ "kind": "resolved", "resolution": "  " })),
            )
            .await;
        assert_eq!(status, 400, "空白处理结论必须 400: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_boundaries() {
    require_mongo!(async {
        let test_db = TestDb::new("mall_sync_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        seed_source_system(test_db.db()).await;
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        for _index in 0..3 {
            let (_, body) = api
                .post(
                    "/admin/mall-sales-sync-jobs",
                    Some(&token),
                    Some(json!({
                        "source_system_id": "sys-mall",
                        "job_type": "single_order_backfill",
                    })),
                )
                .await;
            assert_ok_envelope(200, &body);
            let job_id = body["data"]["id"].as_str().unwrap().to_string();
            let _ = job_id;
        }

        let (status, body) = api
            .get("/admin/mall-sales-sync-jobs?page=1&page_size=2", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 2);

        let (status, body) = api
            .get("/admin/mall-sales-sync-jobs?page=2&page_size=2", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["items"].as_array().unwrap().len(),
            1,
            "边界页返回剩余条数"
        );

        let (status, body) = api
            .get(
                "/admin/mall-sales-sync-jobs?sort_by=id&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_eq!(status, 400, "排序字段不在白名单必须 400: {body}");
        assert_eq!(body["success"], false);

        let (status, body) = api
            .get("/admin/mall-sales-sync-jobs?page_size=0", Some(&token))
            .await;
        assert_eq!(status, 400, "分页大小非法必须 400: {body}");
    })
}
