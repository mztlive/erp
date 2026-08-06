//! 域 D09 `supplier` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）。
//! 鉴权链路与生产一致：种子账号只有 `role/admin/audit_log.list` 权限，
//! 本测试额外为种子账号插入本域直接 `p` 规则，天然构造 403 用例。

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{header::AUTHORIZATION, HeaderValue, Method, Request};
use axum::Router;
use config::{Config, SafeConfig};
use database::{NoTransaction, PartyExt};
use entities::party::{Party, PartyData, PartyId, PartyKind, PartyStatus};
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use tower::ServiceExt;
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节，同时满足 config 校验与 test-support 签发要求）。
const TEST_JWT_SECRET: &str = "p0-5-test-secret-that-is-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("supplier", "list"),
    ("supplier", "create"),
    ("supplier", "detail"),
    ("supplier", "update"),
    ("supplier", "delete"),
    ("supplier_commercial_profile", "list"),
    ("supplier_commercial_profile", "create"),
    ("supplier_capability", "list"),
    ("supplier_capability", "create"),
    ("supplier_capability", "update"),
    ("supplier_qualification", "list"),
    ("supplier_qualification", "create"),
    ("supplier_qualification", "update"),
    ("supplier_rating", "list"),
    ("supplier_rating", "create"),
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

/// 构造最小 AppState 并组装完整应用路由。
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

/// 直接经仓储写入一个主体（测试前置数据）。
async fn seed_party(db: &Database, party_no: &str) -> String {
    let id = PartyId::new(format!("party-{party_no}"));
    let party = Party::new(
        id.clone(),
        PartyData {
            party_no: party_no.to_string(),
            party_kind: PartyKind::Enterprise,
            unified_credit_code: None,
            status: PartyStatus::Active,
        },
        "seed",
    )
    .unwrap();
    db.parties().create(&party, &mut NoTransaction).await.unwrap();
    id.to_string()
}

/// 准备带权限的测试环境，返回 `(router, api, token)`。
async fn setup(test_db: &TestDb) -> (Router, TestApi, String) {
    database::ensure_indexes(test_db.db()).await.unwrap();
    let account_id = seed_admin_account(test_db.db()).await.unwrap();
    grant_domain_permissions(test_db.db(), &account_id).await;
    let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
    let (router, _) = build_router(test_db).await;
    let api = TestApi::new(router.clone());
    (router, api, token)
}

/// 创建供应商的请求体（含首版商务结算版本）。
fn create_supplier_body(party_id: &str, supplier_no: &str, entity: &str) -> Value {
    json!({
        "party_id": party_id,
        "supplier_no": supplier_no,
        "settlement_mode": "prepayment",
        "reconciliation_cycle": "monthly",
        "payment_term_snapshot": "PREPAY_30",
        "invoice_type": "vat_special",
        "invoice_tax_rate": "0.13",
        "signing_entity_party_id": party_id,
        "payment_entity_party_id": party_id,
        "valid_from": "2026-01-01",
        "valid_to": "2026-12-31",
        "change_reason": "首次建档",
        "signing_entity": entity,
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("supplier_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/suppliers", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("supplier_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/suppliers", Some(&token)).await;
        assert_eq!(status, 403, "无 supplier.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("supplier_api_400").await.unwrap();
        let (_, api, token) = setup(&test_db).await;
        let party_id = seed_party(test_db.db(), "P-S400").await;

        let (status, body) = api
            .post(
                "/admin/suppliers",
                Some(&token),
                Some(json!({
                    "party_id": party_id,
                    "supplier_no": "  ",
                    "settlement_mode": "prepayment",
                    "reconciliation_cycle": "monthly",
                    "payment_term_snapshot": "PREPAY_30",
                    "invoice_type": "vat_special",
                    "invoice_tax_rate": "0.13",
                    "signing_entity_party_id": party_id,
                    "payment_entity_party_id": party_id,
                    "valid_from": "2026-01-01",
                    "change_reason": "x",
                })),
            )
            .await;
        assert_eq!(status, 400, "空白 supplier_no 必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let (status, _) = api
            .post(
                "/admin/suppliers",
                Some(&token),
                Some(json!({ "supplier_no": "S-1" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        let (status, _) = api
            .post(
                "/admin/suppliers",
                Some(&token),
                Some(json!({
                    "party_id": party_id,
                    "supplier_no": "S-1",
                    "settlement_mode": "MARS",
                    "reconciliation_cycle": "monthly",
                    "payment_term_snapshot": "PREPAY_30",
                    "invoice_type": "vat_special",
                    "invoice_tax_rate": "0.13",
                    "signing_entity_party_id": party_id,
                    "payment_entity_party_id": party_id,
                    "valid_from": "2026-01-01",
                    "change_reason": "x",
                })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn happy_path_create_supplier_with_profile_capability_qualification_rating() {
    require_mongo!(async {
        let test_db = TestDb::new("supplier_api_happy").await.unwrap();
        let (_, api, token) = setup(&test_db).await;
        let party_id = seed_party(test_db.db(), "P-SH1").await;

        let (status, body) = api
            .post(
                "/admin/suppliers",
                Some(&token),
                Some(create_supplier_body(
                    &party_id,
                    "S-2026-001",
                    "上海示例科技有限公司",
                )),
            )
            .await;
        assert_ok_envelope(status, &body);
        let created = &body["data"];
        assert_eq!(created["supplier_no"], "S-2026-001");
        assert_eq!(created["party_id"], party_id);
        assert_eq!(created["status"], "active");
        assert_eq!(created["version"], 1);
        assert!(!created["current_commercial_profile_revision_id"].is_null());
        let supplier_id = created["id"].as_str().unwrap().to_string();

        // 详情：当前商务版本快照（含税率）。
        let (status, body) = api
            .get(&format!("/admin/suppliers/{supplier_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let detail = &body["data"];
        assert_eq!(detail["party_no"], "P-SH1");
        let profile = &detail["current_profile"];
        assert_eq!(profile["revision_no"], 1);
        assert_eq!(profile["settlement_mode"], "prepayment");
        assert_eq!(profile["invoice_tax_rate"], "0.130000");

        // 商务版本列表。
        let (status, body) = api
            .get(
                &format!("/admin/suppliers/{supplier_id}/commercial-profiles"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["revision_no"], 1);

        // 能力 + 首版能力修订。
        let (status, body) = api
            .post(
                &format!("/admin/suppliers/{supplier_id}/capabilities"),
                Some(&token),
                Some(json!({
                    "capability_code": "physical",
                    "service_region": "华东",
                    "owner_user_id": "buyer-1",
                    "valid_from": "2026-01-01",
                    "valid_to": "2026-12-31",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["capability_code"], "physical");
        assert_eq!(body["data"]["status"], "active");
        let capability_id = body["data"]["id"].as_str().unwrap().to_string();

        // 资质 + 首版修订 + 适用能力关联。
        let (status, body) = api
            .post(
                &format!("/admin/suppliers/{supplier_id}/qualifications"),
                Some(&token),
                Some(json!({
                    "qualification_type": "certificate",
                    "certificate_no": "ZZ-2026-001",
                    "issuer": "示例发证机构",
                    "valid_from": "2026-01-01",
                    "valid_to": "2026-12-31",
                    "capability_ids": [capability_id],
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["certificate_no"], "ZZ-2026-001");
        assert_eq!(body["data"]["status"], "active");

        // 评估版本（首版可携带期初评分）。
        let (status, body) = api
            .post(
                &format!("/admin/suppliers/{supplier_id}/ratings"),
                Some(&token),
                Some(json!({
                    "initial_score": 80,
                    "rating": "B",
                    "current_score": 85,
                    "valid_from": "2026-01-01",
                    "valid_to": "2026-12-31",
                    "change_reason": "首次合作评估",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["revision_no"], 1);
        assert_eq!(body["data"]["initial_score"], 80);
        assert_eq!(body["data"]["rating"], "B");

        let (status, body) = api
            .get(&format!("/admin/suppliers/{supplier_id}/ratings"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_supplier_no_or_party_returns_409_and_missing_party_404() {
    require_mongo!(async {
        let test_db = TestDb::new("supplier_api_dup").await.unwrap();
        let (_, api, token) = setup(&test_db).await;
        let party_id = seed_party(test_db.db(), "P-SD1").await;

        let (status, body) = api
            .post(
                "/admin/suppliers",
                Some(&token),
                Some(create_supplier_body("party-missing", "S-2026-099", "x")),
            )
            .await;
        assert_eq!(status, 404, "共用主体不存在必须 404: {body}");

        let (_, body) = api
            .post(
                "/admin/suppliers",
                Some(&token),
                Some(create_supplier_body(&party_id, "S-2026-001", "甲")),
            )
            .await;
        assert_ok_envelope(200, &body);

        let (status, body) = api
            .post(
                "/admin/suppliers",
                Some(&token),
                Some(create_supplier_body(&party_id, "S-2026-002", "乙")),
            )
            .await;
        assert_eq!(status, 409, "一个 party 最多一个有效供应商角色必须 409: {body}");

        let party2 = seed_party(test_db.db(), "P-SD2").await;
        let (status, body) = api
            .post(
                "/admin/suppliers",
                Some(&token),
                Some(create_supplier_body(&party2, "S-2026-001", "丙")),
            )
            .await;
        assert_eq!(status, 409, "重复 supplier_no 唯一索引冲突必须 409: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn overlapping_commercial_profile_rolls_back_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("supplier_api_invariant").await.unwrap();
        let (_, api, token) = setup(&test_db).await;
        let party_id = seed_party(test_db.db(), "P-SI1").await;

        let (_, body) = api
            .post(
                "/admin/suppliers",
                Some(&token),
                Some(create_supplier_body(&party_id, "S-2026-010", "甲")),
            )
            .await;
        assert_ok_envelope(200, &body);
        let supplier_id = body["data"]["id"].as_str().unwrap().to_string();

        // 新商务版本窗口与首版（2026-01-01 ~ 2026-12-31）重叠 → 409 且整体回滚。
        let (status, body) = api
            .post(
                &format!("/admin/suppliers/{supplier_id}/commercial-profiles"),
                Some(&token),
                Some(json!({
                    "settlement_mode": "pay_after_use",
                    "reconciliation_cycle": "monthly",
                    "payment_term_snapshot": "NET30",
                    "invoice_type": "vat_special",
                    "invoice_tax_rate": "0.13",
                    "signing_entity_party_id": party_id,
                    "payment_entity_party_id": party_id,
                    "valid_from": "2026-06-01",
                    "valid_to": "2027-05-31",
                    "change_reason": "重叠测试",
                })),
            )
            .await;
        assert_eq!(status, 409, "重叠商务版本必须 409: {body}");

        let (status, body) = api
            .get(
                &format!("/admin/suppliers/{supplier_id}/commercial-profiles"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1, "事务回滚后版本链保持原状");
        let (status, body) = api
            .get(&format!("/admin/suppliers/{supplier_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["current_profile"]["revision_no"], 1,
            "生效指针未被部分推进"
        );
    })
}

#[tokio::test]
#[ignore]
async fn qualification_validates_capability_ownership_and_attachment() {
    require_mongo!(async {
        let test_db = TestDb::new("supplier_api_qual").await.unwrap();
        let (_, api, token) = setup(&test_db).await;
        let party1 = seed_party(test_db.db(), "P-SQ1").await;
        let party2 = seed_party(test_db.db(), "P-SQ2").await;

        let (_, body) = api
            .post(
                "/admin/suppliers",
                Some(&token),
                Some(create_supplier_body(&party1, "S-2026-020", "甲")),
            )
            .await;
        assert_ok_envelope(200, &body);
        let supplier1 = body["data"]["id"].as_str().unwrap().to_string();
        let (_, body) = api
            .post(
                "/admin/suppliers",
                Some(&token),
                Some(create_supplier_body(&party2, "S-2026-021", "乙")),
            )
            .await;
        assert_ok_envelope(200, &body);
        let supplier2 = body["data"]["id"].as_str().unwrap().to_string();

        let (_, body) = api
            .post(
                &format!("/admin/suppliers/{supplier1}/capabilities"),
                Some(&token),
                Some(json!({
                    "capability_code": "physical",
                    "owner_user_id": "buyer-1",
                    "valid_from": "2026-01-01",
                })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let capability1 = body["data"]["id"].as_str().unwrap().to_string();

        // 引用其他供应商的能力 → 404。
        let (status, body) = api
            .post(
                &format!("/admin/suppliers/{supplier2}/qualifications"),
                Some(&token),
                Some(json!({
                    "qualification_type": "certificate",
                    "certificate_no": "ZZ-OTHER",
                    "valid_from": "2026-01-01",
                    "capability_ids": [capability1],
                })),
            )
            .await;
        assert_eq!(status, 404, "能力不属于该供应商必须 404: {body}");

        // 引用不存在的附件 → 404（D05 跨域校验）。
        let (status, body) = api
            .post(
                &format!("/admin/suppliers/{supplier1}/qualifications"),
                Some(&token),
                Some(json!({
                    "qualification_type": "certificate",
                    "certificate_no": "ZZ-ATT",
                    "valid_from": "2026-01-01",
                    "attachment_id": "file-does-not-exist",
                    "capability_ids": [capability1],
                })),
            )
            .await;
        assert_eq!(status, 404, "附件不存在必须 404: {body}");

        // 无能力引用也可建资质。
        let (status, body) = api
            .post(
                &format!("/admin/suppliers/{supplier1}/qualifications"),
                Some(&token),
                Some(json!({
                    "qualification_type": "certificate",
                    "certificate_no": "ZZ-2026-020",
                    "valid_from": "2026-01-01",
                    "capability_ids": [],
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_update_returns_409() {
    require_mongo!(async {
        let test_db = TestDb::new("supplier_api_409").await.unwrap();
        let (router, api, token) = setup(&test_db).await;
        let party_id = seed_party(test_db.db(), "P-S409").await;

        let (_, body) = api
            .post(
                "/admin/suppliers",
                Some(&token),
                Some(create_supplier_body(&party_id, "S-2026-030", "甲")),
            )
            .await;
        assert_ok_envelope(200, &body);
        let supplier_id = body["data"]["id"].as_str().unwrap().to_string();

        let (_, body) = api
            .post(
                &format!("/admin/suppliers/{supplier_id}/capabilities"),
                Some(&token),
                Some(json!({
                    "capability_code": "api",
                    "owner_user_id": "buyer-1",
                    "valid_from": "2026-01-01",
                })),
            )
            .await;
        assert_ok_envelope(200, &body);
        let capability_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = put_json(
            &router,
            &format!("/admin/supplier-capabilities/{capability_id}"),
            &token,
            json!({ "version": 1, "owner_user_id": "buyer-2" }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["version"], 2, "更新成功版本递增");
        assert_eq!(body["data"]["owner_user_id"], "buyer-2");

        let (status, body) = put_json(
            &router,
            &format!("/admin/supplier-capabilities/{capability_id}"),
            &token,
            json!({ "version": 1, "owner_user_id": "buyer-3" }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本更新必须 409: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_are_validated() {
    require_mongo!(async {
        let test_db = TestDb::new("supplier_api_page").await.unwrap();
        let (_, api, token) = setup(&test_db).await;

        for (i, party_no) in ["P-SP1", "P-SP2", "P-SP3"].iter().enumerate() {
            let party_id = seed_party(test_db.db(), party_no).await;
            let (status, body) = api
                .post(
                    "/admin/suppliers",
                    Some(&token),
                    Some(create_supplier_body(
                        &party_id,
                        &format!("S-2026-04{i}"),
                        "示例公司",
                    )),
                )
                .await;
            assert_ok_envelope(status, &body);
        }

        let (status, body) = api
            .get(
                "/admin/suppliers?page=2&page_size=2&sort_by=supplier_no&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 3);
        assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["items"][0]["supplier_no"], "S-2026-042");

        let (status, body) = api.get("/admin/suppliers?sort_by=hacked", Some(&token)).await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");
    })
}
