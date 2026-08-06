//! 域 D11 `warehouse` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test warehouse_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：种子账号只有 `role/admin/audit_log.list` 权限，
//! 本测试额外插入本域资源（含跨域 catalog 字典资源用于构造 SKU）的直接
//! `p` 规则，使 happy path 可鉴权通过，同时天然构造 403 用例。

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{header::AUTHORIZATION, HeaderValue, Method, Request};
use axum::Router;
use config::{Config, SafeConfig};
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use tower::ServiceExt;
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节，同时满足 config 校验与 test-support 签发要求）。
const TEST_JWT_SECRET: &str = "c-g3-wh-test-secret-that-is-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action；含跨域 catalog 构造 SKU 所需键）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("warehouse", "list"),
    ("warehouse", "create"),
    ("warehouse", "update"),
    ("warehouse_revision", "list"),
    ("warehouse_sku_policy", "list"),
    ("warehouse_sku_policy", "create"),
    ("warehouse_sku_policy", "update"),
    ("warehouse_sku_policy", "delete"),
    // 跨域构造 SKU 所需（D10 catalog 字典与商品）。
    ("product_category", "create"),
    ("product_brand", "create"),
    ("unit_of_measure", "create"),
    ("sku_attribute", "create"),
    ("sku_attribute_value", "create"),
    ("product", "create"),
    ("sku", "list"),
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

/// 构造最小 AppState（默认配置 + 临时上传目录）并组装完整应用路由。
async fn build_router(test_db: &TestDb) -> (Router, PathBuf) {
    let config = Config::from_toml_str(&format!(
        r#"
[app]
port = 10001
secret = "{TEST_JWT_SECRET}"
upload_path = "c-g3-wh-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("c-g3-wh-uploads-{}", uuid::Uuid::new_v4()));
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

/// 构造仓库创建请求体。
fn warehouse_payload(warehouse_code: &str, name: &str) -> Value {
    json!({
        "warehouse_code": warehouse_code,
        "name": name,
        "address": "北京市朝阳区望京街道 1 号",
        "contact": "张三 13900000000",
        "effective_from": "2026-01-01",
        "change_reason": "期初建仓",
    })
}

/// 经 catalog API 构造一个带规格 SKU 的商品并返回 sku_id（跨域数据依赖）。
async fn seed_product_sku(api: &TestApi, token: &str) -> String {
    let (status, body) = api
        .post(
            "/admin/product-categories",
            Some(token),
            Some(json!({ "category_code": "CAT-WH", "name": "仓库测试分类", "product_kind": "PHYSICAL" })),
        )
        .await;
    assert_ok_envelope(status, &body);
    let category_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = api
        .post(
            "/admin/product-brands",
            Some(token),
            Some(json!({ "brand_code": "BR-WH", "name": "仓库测试品牌" })),
        )
        .await;
    assert_ok_envelope(status, &body);
    let brand_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = api
        .post(
            "/admin/unit-of-measures",
            Some(token),
            Some(json!({ "unit_code": "PCS", "name": "件", "symbol": "件", "quantity_scale": 0 })),
        )
        .await;
    assert_ok_envelope(status, &body);
    let unit_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = api
        .post(
            "/admin/products",
            Some(token),
            Some(json!({
                "product_no": "P-WH-001",
                "product_kind": "PHYSICAL",
                "name": "仓库测试商品",
                "category_id": category_id,
                "brand_id": brand_id,
                "effective_from": "2026-01-01",
                "skus": json!([json!({
                    "sku_no": "SKU-WH-001",
                    "base_unit_id": unit_id,
                })]),
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    let (status, body) = api.get("/admin/skus?sku_no=SKU-WH-001", Some(token)).await;
    assert_ok_envelope(status, &body);
    body["data"]["items"][0]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore]
async fn happy_path_warehouse_create_revision_append_and_policy_crud() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (status, body) = api.get("/admin/warehouses", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["items"], json!([]), "初始列表为空");
        assert_eq!(data["total"], 0);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 20);

        // 创建仓库：稳定身份 + 首个修订原子写入。
        let (status, body) = api
            .post(
                "/admin/warehouses",
                Some(&token),
                Some(warehouse_payload("WH-BJ-001", "北京一号仓")),
            )
            .await;
        assert_ok_envelope(status, &body);
        let warehouse = &body["data"];
        assert_eq!(warehouse["warehouse_code"], "WH-BJ-001");
        assert_eq!(warehouse["status"], "active");
        assert_eq!(warehouse["version"], 1);
        let warehouse_id = warehouse["id"].as_str().unwrap().to_string();

        // 仓库修订列表：不暴露敏感字段，首个修订落位。
        let (status, body) = api
            .get(
                &format!("/admin/warehouse-revisions?warehouse_id={warehouse_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        assert_eq!(data["items"][0]["revision_no"], 1);
        assert_eq!(data["items"][0]["name"], "北京一号仓");
        assert_eq!(data["items"][0]["change_reason"], "期初建仓");
        for field in ["address", "contact", "encrypted", "fingerprint"] {
            assert!(
                data["items"][0].get(field).is_none(),
                "敏感字段 {field} 不得出现在列表: {data}"
            );
        }

        // 更新仓库：追加修订（revision_no=2）并更新稳定身份，版本递增。
        let (status, body) = put_json(
            &router,
            &format!("/admin/warehouses/{warehouse_id}"),
            &token,
            json!({
                "version": 1,
                "name": "北京一号仓（扩仓）",
                "address": "北京市朝阳区望京街道 2 号",
                "contact": "李四 13800000000",
                "effective_from": "2026-02-01",
                "change_reason": "仓库扩建",
                "status": "active",
            }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["version"], 2, "更新成功版本递增");

        let (status, body) = api
            .get(
                &format!("/admin/warehouse-revisions?warehouse_id={warehouse_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2, "追加写入修订");
        assert_eq!(body["data"]["items"][1]["revision_no"], 2);
        assert_eq!(body["data"]["items"][1]["name"], "北京一号仓（扩仓）");

        // 陈旧版本更新 → 409。
        let (status, body) = put_json(
            &router,
            &format!("/admin/warehouses/{warehouse_id}"),
            &token,
            json!({
                "version": 1,
                "name": "陈旧版本",
                "address": "x",
                "contact": "x",
                "effective_from": "2026-03-01",
                "change_reason": "陈旧",
                "status": "active",
            }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本更新必须 409: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        // 仓库-SKU 预警策略：创建 + 列表 + 更新 + 删除。
        let sku_id = seed_product_sku(&api, &token).await;
        let (status, body) = api
            .post(
                "/admin/warehouse-sku-policies",
                Some(&token),
                Some(json!({
                    "warehouse_id": warehouse_id,
                    "sku_id": sku_id,
                    "minimum_available_quantity": "10.000000",
                    "effective_from": "2026-01-01",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let policy = &body["data"];
        assert_eq!(policy["minimum_available_quantity"], "10.000000");
        assert_eq!(policy["warehouse_id"], warehouse_id);
        assert_eq!(policy["sku_id"], sku_id);
        let policy_id = policy["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .get(
                &format!("/admin/warehouse-sku-policies?warehouse_id={warehouse_id}&sku_id={sku_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);

        let (status, body) = put_json(
            &router,
            &format!("/admin/warehouse-sku-policies/{policy_id}"),
            &token,
            json!({ "version": 1, "minimum_available_quantity": "5.000000" }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["minimum_available_quantity"], "5.000000");
        assert_eq!(body["data"]["version"], 2);

        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/admin/warehouse-sku-policies/{policy_id}"))
            .header(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
            )
            .body(Body::empty())
            .expect("DELETE 请求构造失败");
        let response = router.clone().oneshot(request).await.expect("路由调用失败");
        assert_eq!(response.status().as_u16(), 200, "策略软删除必须成功");
        let (status, body) = api
            .get(
                &format!("/admin/warehouse-sku-policies?warehouse_id={warehouse_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0, "软删除后列表不可见");

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn warehouse_sku_policy_rejects_overlapping_windows() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_api_overlap").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (_, body) = api
            .post(
                "/admin/warehouses",
                Some(&token),
                Some(warehouse_payload("WH-SZ-001", "深圳仓")),
            )
            .await;
        assert_ok_envelope(200, &body);
        let warehouse_id = body["data"]["id"].as_str().unwrap().to_string();
        let sku_id = seed_product_sku(&api, &token).await;

        let (status, body) = api
            .post(
                "/admin/warehouse-sku-policies",
                Some(&token),
                Some(json!({
                    "warehouse_id": warehouse_id,
                    "sku_id": sku_id,
                    "minimum_available_quantity": "10.000000",
                    "effective_from": "2026-01-01",
                    "effective_to": "2026-03-01",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);

        // 与既有区间重叠（2026-02-01 落在 [2026-01-01, 2026-03-01) 内）→ 422。
        let (status, body) = api
            .post(
                "/admin/warehouse-sku-policies",
                Some(&token),
                Some(json!({
                    "warehouse_id": warehouse_id,
                    "sku_id": sku_id,
                    "minimum_available_quantity": "8.000000",
                    "effective_from": "2026-02-01",
                    "effective_to": "2026-04-01",
                })),
            )
            .await;
        assert_eq!(status, 422, "启用区间重叠必须 422: {body}");
        assert_eq!(body["success"], false);

        // 相邻区间（结束日 = 开始日，半开区间）不重叠 → 200。
        let (status, body) = api
            .post(
                "/admin/warehouse-sku-policies",
                Some(&token),
                Some(json!({
                    "warehouse_id": warehouse_id,
                    "sku_id": sku_id,
                    "minimum_available_quantity": "8.000000",
                    "effective_from": "2026-03-01",
                    "effective_to": "2026-06-01",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["effective_from"], "2026-03-01");

        // 引用不存在的 SKU（跨域 Repository 校验）→ 404。
        let (status, body) = api
            .post(
                "/admin/warehouse-sku-policies",
                Some(&token),
                Some(json!({
                    "warehouse_id": warehouse_id,
                    "sku_id": "sku-does-not-exist",
                    "minimum_available_quantity": "1.000000",
                    "effective_from": "2026-09-01",
                })),
            )
            .await;
        assert_eq!(status, 404, "SKU 不存在必须 404: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn warehouse_validation_and_conflict_cases() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        // 空白地址 → 400。
        let (status, body) = api
            .post(
                "/admin/warehouses",
                Some(&token),
                Some(json!({
                    "warehouse_code": "WH-X",
                    "name": "空地址仓",
                    "address": "  ",
                    "contact": "张三",
                    "effective_from": "2026-01-01",
                    "change_reason": "期初建仓",
                })),
            )
            .await;
        assert_eq!(status, 400, "空白地址必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        // 生效区间倒挂（entities 校验）→ 422。
        let (status, body) = api
            .post(
                "/admin/warehouses",
                Some(&token),
                Some(json!({
                    "warehouse_code": "WH-X",
                    "name": "倒挂仓",
                    "address": "地址",
                    "contact": "张三",
                    "effective_from": "2026-03-01",
                    "effective_to": "2026-02-01",
                    "change_reason": "期初建仓",
                })),
            )
            .await;
        assert_eq!(status, 422, "生效区间倒挂必须 422: {body}");

        // 重复 warehouse_code（唯一索引）→ 409。
        let (status, body) = api
            .post(
                "/admin/warehouses",
                Some(&token),
                Some(warehouse_payload("WH-DUP-001", "一号仓")),
            )
            .await;
        assert_ok_envelope(status, &body);
        let (status, body) = api
            .post(
                "/admin/warehouses",
                Some(&token),
                Some(warehouse_payload("WH-DUP-001", "重复仓")),
            )
            .await;
        assert_eq!(status, 409, "重复 warehouse_code 必须 409: {body}");

        // 非法排序字段 → 400；缺必填字段 → 422。
        let (status, body) = api
            .get("/admin/warehouses?sort_by=unknown_field", Some(&token))
            .await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");

        let (status, _) = api
            .post(
                "/admin/warehouses",
                Some(&token),
                Some(json!({ "warehouse_code": "WH-X" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/warehouses", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("wh_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        // 种子账号只有 role/admin/audit_log.list 权限，本域权限未授予 → 403。
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/warehouses", Some(&token)).await;
        assert_eq!(status, 403, "无 warehouse.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}
