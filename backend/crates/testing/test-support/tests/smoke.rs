//! test-support 自测：验证 `TestDb` / `require_mongo!` / 种子 / `mint_jwt` /
//! `assert_indexes` / `TestApi` 自身的可用性。
//!
//! 需要真实 MongoDB 的用例按 conventions 7.2 使用 `#[ignore]` +
//! `require_mongo!` 门控；无库环境全部跳过，`cargo test --workspace` 保持全绿。

use axum::routing::{get, post};
use axum::{Json, Router};
use mongodb::bson::{doc, Document};
use mongodb::IndexModel;
use serde_json::{json, Value};
use test_support::{assert_indexes, mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};

/// 供 `mint_jwt` 与 web-api 配置共用的测试密钥（≥32 字节）。
const TEST_SECRET: &str = "smoke-test-secret-that-is-at-least-32-bytes";

/// 验证 `TestApi` 可在不依赖 MongoDB 的情况下发送带/不带 token 的请求。
#[tokio::test]
async fn test_api_should_send_authenticated_requests_without_mongo() {
    async fn ping() -> Json<Value> {
        Json(json!({ "status": "ok" }))
    }

    async fn echo(Json(payload): Json<Value>) -> Json<Value> {
        Json(payload)
    }

    let router = Router::new().route("/ping", get(ping)).route("/echo", post(echo));
    let api = TestApi::new(router);

    let (status, body) = api.get("/ping", None).await;
    assert_eq!(status, 200);
    assert_eq!(body, json!({ "status": "ok" }));

    let (status, body) = api
        .post("/echo", Some("some-token"), Some(json!({ "hello": "world" })))
        .await;
    assert_eq!(status, 200);
    assert_eq!(body["hello"], "world");
}

/// 验证 `TestDb` + `assert_indexes` + 种子 + `mint_jwt` 完整工作流。
///
/// 需要真实 MongoDB 单节点副本集（`backend/scripts/dev-mongo.sh` 或
/// `docker compose --profile test up -d mongo`），并以
/// `ERP_TEST_MONGO_URI` 环境变量提供连接串。
#[tokio::test]
#[ignore]
async fn smoke_testdb_seed_mint_and_indexes_should_roundtrip() {
    require_mongo!(async move {
        let fixture = TestDb::new("smoke").await.expect("TestDb 创建失败");
        let db = fixture.db();

        db.collection::<Document>("accounts")
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "account": 1 })
                    .options(
                        mongodb::options::IndexOptions::builder()
                            .name("uk_smoke_account".to_string())
                            .build(),
                    )
                    .build(),
            )
            .await
            .expect("创建测试索引失败");
        assert_indexes(db, "accounts", &["_id_", "uk_smoke_account"])
            .await
            .expect("断言索引失败");

        let account_id = seed_admin_account(db).await.expect("种子账号失败");
        let token = mint_jwt(&account_id, TEST_SECRET, 3600).expect("签发 JWT 失败");
        assert_eq!(token.split('.').count(), 3);

        let rules = db
            .collection::<Document>("casbin_rules")
            .count_documents(doc! {})
            .await
            .expect("统计 Casbin 规则失败");
        assert_eq!(rules, 4, "应包含 3 条 p 权限规则与 1 条 g 绑定规则");
    });
}
