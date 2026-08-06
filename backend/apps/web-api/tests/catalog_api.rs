//! 域 D10 `catalog` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test catalog_api -- --include-ignored`。
//!
//! 鉴权链路与生产一致：`seed_admin_account` 种子账号只有
//! `role/admin/audit_log.list` 权限，本测试额外为种子账号插入本域资源
//! 的直接 `p` 规则（casbin 的 `g(r.sub, p.sub)` 自反匹配，无需改角色），
//! 使 happy path 可鉴权通过，同时天然构造 403 用例。

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
const TEST_JWT_SECRET: &str = "c-g3-test-secret-that-is-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("product_category", "list"),
    ("product_category", "create"),
    ("product_category", "update"),
    ("product_category", "delete"),
    ("product_brand", "list"),
    ("product_brand", "create"),
    ("product_brand", "update"),
    ("product_brand", "delete"),
    ("unit_of_measure", "list"),
    ("unit_of_measure", "create"),
    ("unit_of_measure", "update"),
    ("unit_of_measure", "delete"),
    ("sku_attribute", "list"),
    ("sku_attribute", "create"),
    ("sku_attribute", "update"),
    ("sku_attribute", "delete"),
    ("sku_attribute_value", "list"),
    ("sku_attribute_value", "create"),
    ("sku_attribute_value", "update"),
    ("sku_attribute_value", "delete"),
    ("product", "list"),
    ("product", "create"),
    ("product", "update"),
    ("product_revision", "list"),
    ("sku", "list"),
    ("sku_revision", "list"),
    ("voucher_category_profile", "list"),
    ("voucher_category_profile", "create"),
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
upload_path = "c-g3-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("c-g3-uploads-{}", uuid::Uuid::new_v4()));
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

/// 建立「分类 + 品牌 + 计量单位 + 规格属性 + 属性值」字典基座（happy path 前置）。
async fn seed_catalog_dictionaries(api: &TestApi, token: &str) -> Value {
    let (status, body) = api
        .post(
            "/admin/product-categories",
            Some(token),
            Some(json!({ "category_code": "CAT-001", "name": "食品分类", "product_kind": "PHYSICAL" })),
        )
        .await;
    assert_ok_envelope(status, &body);
    let category_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = api
        .post(
            "/admin/product-brands",
            Some(token),
            Some(json!({ "brand_code": "BR-001", "name": "山姆自营" })),
        )
        .await;
    assert_ok_envelope(status, &body);
    let brand_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = api
        .post(
            "/admin/unit-of-measures",
            Some(token),
            Some(json!({ "unit_code": "KG", "name": "千克", "symbol": "kg", "quantity_scale": 3 })),
        )
        .await;
    assert_ok_envelope(status, &body);
    let unit_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = api
        .post(
            "/admin/sku-attributes",
            Some(token),
            Some(json!({ "attribute_code": "SIZE", "name": "尺码", "value_type": "enum" })),
        )
        .await;
    assert_ok_envelope(status, &body);
    let attribute_id = body["data"]["id"].as_str().unwrap().to_string();

    for (code, display) in [("L", "大号"), ("M", "中号")] {
        let (status, body) = api
            .post(
                "/admin/sku-attribute-values",
                Some(token),
                Some(json!({
                    "attribute_id": attribute_id,
                    "value_code": code,
                    "display_value": display,
                    "sort_order": 0,
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
    }

    json!({
        "category_id": category_id,
        "brand_id": brand_id,
        "unit_id": unit_id,
        "attribute_id": attribute_id,
    })
}

/// 构造商品创建请求体（基于字典基座，可覆盖 skus）。
fn product_payload(product_no: &str, dictionaries: &Value, kind: &str, skus: Value) -> Value {
    json!({
        "product_no": product_no,
        "product_kind": kind,
        "name": format!("商品 {product_no}"),
        "category_id": dictionaries["category_id"],
        "brand_id": dictionaries["brand_id"],
        "effective_from": "2026-01-01",
        "skus": skus,
    })
}

/// 为规格编辑 PUT 补上必填的 `version` 与 `status`（UpdateProductRequest 形态）。
fn spec_edit_payload(product_no: &str, dictionaries: &Value, skus: Value, version: u64) -> Value {
    let mut payload = product_payload(product_no, dictionaries, "PHYSICAL", skus);
    payload["version"] = json!(version);
    payload["status"] = json!("active");
    payload
}

/// 构造一个带规格的 SKU 行。
fn spec_sku(sku_no: &str, unit_id: &str, size: &str, price: &str) -> Value {
    json!({
        "sku_no": sku_no,
        "base_unit_id": unit_id,
        "sales_visible_price_gross": price,
        "spec_entries": [{
            "attribute_code": "SIZE",
            "attribute_value_code": size,
        }],
    })
}

#[tokio::test]
#[ignore]
async fn happy_path_dictionary_crud_and_product_create_with_contract_shape() {
    require_mongo!(async {
        let test_db = TestDb::new("catalog_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        // 初始列表为空，契约形状固定。
        let (status, body) = api.get("/admin/product-categories", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["items"], json!([]), "初始列表为空");
        assert_eq!(data["total"], 0);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 20);

        let dictionaries = seed_catalog_dictionaries(&api, &token).await;

        // 商品创建：SPU + 修订 + SKU + 规格值原子写入。
        let (status, body) = api
            .post(
                "/admin/products",
                Some(&token),
                Some(product_payload(
                    "P-001",
                    &dictionaries,
                    "PHYSICAL",
                    json!([spec_sku(
                        "SKU-001",
                        dictionaries["unit_id"].as_str().unwrap(),
                        "L",
                        "99.90"
                    )]),
                )),
            )
            .await;
        assert_ok_envelope(status, &body);
        let product = &body["data"];
        assert_eq!(product["product_no"], "P-001");
        assert_eq!(product["product_kind"], "PHYSICAL");
        assert_eq!(product["status"], "active");
        assert_eq!(product["version"], 1);
        let product_id = product["id"].as_str().unwrap().to_string();

        // SKU 列表：签名规范化（SIZE=L），契约字段齐全。
        let (status, body) = api
            .get(&format!("/admin/skus?product_id={product_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        let sku = &data["items"][0];
        for field in [
            "id",
            "sku_no",
            "product_id",
            "base_unit_id",
            "specification_signature",
            "status",
            "version",
        ] {
            assert!(sku.get(field).is_some(), "契约字段 {field} 必须存在: {sku}");
        }
        assert_eq!(sku["specification_signature"], "SIZE=L");
        let sku_id = sku["id"].as_str().unwrap().to_string();

        // SKU 修订：价格字符串形态、revision_no=1。
        let (status, body) = api
            .get(&format!("/admin/sku-revisions?sku_id={sku_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        assert_eq!(data["items"][0]["revision_no"], 1);
        assert_eq!(data["items"][0]["sales_visible_price_gross"], "99.90");
        assert_eq!(data["items"][0]["name"], "商品 P-001");

        // 商品修订：revision_no=1。
        let (status, body) = api
            .get(
                &format!("/admin/product-revisions?product_id={product_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["revision_no"], 1);

        // 字典乐观锁更新：版本递增；陈旧版本 409。
        let category_id = dictionaries["category_id"].as_str().unwrap().to_string();
        let (status, body) = put_json(
            &router,
            &format!("/admin/product-categories/{category_id}"),
            &token,
            json!({ "version": 1, "name": "食品分类（修订）" }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["version"], 2);

        let (status, body) = put_json(
            &router,
            &format!("/admin/product-categories/{category_id}"),
            &token,
            json!({ "version": 1, "name": "陈旧版本" }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本更新必须 409: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn product_spec_edit_keeps_reactivates_and_disables_skus_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("catalog_api_spec").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let dictionaries = seed_catalog_dictionaries(&api, &token).await;
        let unit_id = dictionaries["unit_id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/products",
                Some(&token),
                Some(product_payload(
                    "P-100",
                    &dictionaries,
                    "PHYSICAL",
                    json!([spec_sku("SKU-100-L", &unit_id, "L", "99.90")]),
                )),
            )
            .await;
        assert_ok_envelope(status, &body);
        let product_id = body["data"]["id"].as_str().unwrap().to_string();

        // 规格编辑：保留 SIZE=L（追加修订）、新增 SIZE=M（新 SKU）。
        let (status, body) = put_json(
            &router,
            &format!("/admin/products/{product_id}"),
            &token,
            spec_edit_payload(
                "P-100",
                &dictionaries,
                json!([
                    spec_sku("SKU-100-L", &unit_id, "L", "88.00"),
                    spec_sku("SKU-100-M", &unit_id, "M", "78.00"),
                ]),
                1,
            ),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["version"], 2, "SPU 版本递增");

        let (status, body) = api
            .get(&format!("/admin/skus?product_id={product_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2, "保留 + 新增 = 2 个 SKU");
        let l_sku = body["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["specification_signature"] == "SIZE=L")
            .expect("SIZE=L SKU 必须保留")
            .clone();
        let m_sku = body["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["specification_signature"] == "SIZE=M")
            .expect("SIZE=M SKU 必须新增")
            .clone();
        let l_sku_id = l_sku["id"].as_str().unwrap().to_string();
        let m_sku_id = m_sku["id"].as_str().unwrap().to_string();

        // 保留 SKU 追加修订：revision_no=2 且新价格生效；原 sku_id 不变。
        let (status, body) = api
            .get(&format!("/admin/sku-revisions?sku_id={l_sku_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 2);
        assert_eq!(body["data"]["items"][0]["revision_no"], 1);
        assert_eq!(body["data"]["items"][0]["sales_visible_price_gross"], "99.90");
        assert_eq!(body["data"]["items"][1]["revision_no"], 2);
        assert_eq!(body["data"]["items"][1]["sales_visible_price_gross"], "88.00");

        let (status, body) = api
            .get(&format!("/admin/sku-revisions?sku_id={m_sku_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1, "新 SKU 只有首个修订");

        // 移除 SIZE=M：旧 SKU 转为停用（保留历史），L 保留。
        let (status, body) = put_json(
            &router,
            &format!("/admin/products/{product_id}"),
            &token,
            spec_edit_payload(
                "P-100",
                &dictionaries,
                json!([spec_sku("SKU-100-L", &unit_id, "L", "88.00")]),
                2,
            ),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["version"], 3);

        let (status, body) = api
            .get(&format!("/admin/skus?product_id={product_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let items = body["data"]["items"].as_array().unwrap();
        let m = items.iter().find(|item| item["id"] == m_sku_id).unwrap();
        assert_eq!(m["status"], "disabled", "移除签名的 SKU 必须转为停用");

        // 重新启用：同一签名（SIZE=M）再次出现 → 复用原 sku_id 并置 Active。
        let (status, body) = put_json(
            &router,
            &format!("/admin/products/{product_id}"),
            &token,
            spec_edit_payload(
                "P-100",
                &dictionaries,
                json!([
                    spec_sku("SKU-100-L", &unit_id, "L", "88.00"),
                    spec_sku("SKU-100-M", &unit_id, "M", "78.00"),
                ]),
                3,
            ),
        )
        .await;
        assert_ok_envelope(status, &body);
        let (status, body) = api
            .get(&format!("/admin/skus?product_id={product_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let items = body["data"]["items"].as_array().unwrap();
        let m = items.iter().find(|item| item["id"] == m_sku_id).unwrap();
        assert_eq!(m["status"], "active", "历史停用签名重新出现必须显式重新启用");
        assert_eq!(body["data"]["total"], 2, "不得创建第二个同签名 SKU");
    })
}

#[tokio::test]
#[ignore]
async fn product_create_rolls_back_atomically_on_unique_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("catalog_api_tx").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let dictionaries = seed_catalog_dictionaries(&api, &token).await;
        let unit_id = dictionaries["unit_id"].as_str().unwrap().to_string();

        // 两个 SKU 使用相同 sku_no：第二笔写入命中唯一索引 → 整事务回滚。
        let (status, body) = api
            .post(
                "/admin/products",
                Some(&token),
                Some(product_payload(
                    "P-TX-001",
                    &dictionaries,
                    "PHYSICAL",
                    json!([
                        spec_sku("SKU-DUP", &unit_id, "L", "99.90"),
                        spec_sku("SKU-DUP", &unit_id, "M", "89.90"),
                    ]),
                )),
            )
            .await;
        assert_eq!(status, 409, "唯一约束冲突必须 409: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        // 事务不变量：注入失败后全部不可见（products/skus 均为空）。
        let (status, body) = api.get("/admin/products", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0, "SPU 必须不可见");
        let (status, body) = api.get("/admin/skus", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0, "SKU 必须不可见");
    })
}

#[tokio::test]
#[ignore]
async fn barcode_conflict_and_media_missing_are_rejected() {
    require_mongo!(async {
        let test_db = TestDb::new("catalog_api_barcode").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let dictionaries = seed_catalog_dictionaries(&api, &token).await;
        let unit_id = dictionaries["unit_id"].as_str().unwrap().to_string();

        // 第一个商品带条码 6901234567890。
        let mut sku = spec_sku("SKU-BC-1", &unit_id, "L", "99.90");
        sku["barcode"] = json!("6901234567890");
        let (status, body) = api
            .post(
                "/admin/products",
                Some(&token),
                Some(product_payload("P-BC-1", &dictionaries, "PHYSICAL", json!([sku]))),
            )
            .await;
        assert_ok_envelope(status, &body);

        // 第二个商品复用同一在用品条码 → 422 阻断（转人工，不自动合并）。
        let mut sku2 = spec_sku("SKU-BC-2", &unit_id, "L", "99.90");
        sku2["barcode"] = json!("6901234567890");
        let (status, body) = api
            .post(
                "/admin/products",
                Some(&token),
                Some(product_payload(
                    "P-BC-2",
                    &dictionaries,
                    "PHYSICAL",
                    json!([sku2]),
                )),
            )
            .await;
        assert_eq!(status, 422, "条码冲突必须 422: {body}");
        assert_eq!(body["success"], false);

        // 媒体引用不存在的 file_asset（D05 跨域 Repository 校验）→ 404。
        let (status, body) = api
            .post(
                "/admin/products",
                Some(&token),
                Some(json!({
                    "product_no": "P-MEDIA-1",
                    "product_kind": "PHYSICAL",
                    "name": "带媒体商品",
                    "category_id": dictionaries["category_id"],
                    "brand_id": dictionaries["brand_id"],
                    "effective_from": "2026-01-01",
                    "carousel_media": [{
                        "file_asset_id": "asset-does-not-exist",
                        "sort_order": 0,
                    }],
                    "skus": json!([spec_sku("SKU-MEDIA-1", &unit_id, "L", "99.90")]),
                })),
            )
            .await;
        assert_eq!(status, 404, "媒体文件不存在必须 404: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn category_tree_guards_cycle_and_delete_with_children() {
    require_mongo!(async {
        let test_db = TestDb::new("catalog_api_tree").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        let (status, body) = api
            .post(
                "/admin/product-categories",
                Some(&token),
                Some(json!({ "category_code": "CAT-ROOT", "name": "根分类", "product_kind": "PHYSICAL" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let root_id = body["data"]["id"].as_str().unwrap().to_string();

        let (status, body) = api
            .post(
                "/admin/product-categories",
                Some(&token),
                Some(json!({
                    "category_code": "CAT-CHILD",
                    "parent_category_id": root_id,
                    "name": "子分类",
                    "product_kind": "PHYSICAL",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let child_id = body["data"]["id"].as_str().unwrap().to_string();

        // 把根分类移动到自己的子分类之下 → 422 成环。
        let (status, body) = put_json(
            &router,
            &format!("/admin/product-categories/{root_id}/parent"),
            &token,
            json!({ "version": 1, "parent_category_id": child_id }),
        )
        .await;
        assert_eq!(status, 422, "移动形成环必须 422: {body}");

        // 删除带子分类的节点 → 422。
        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/admin/product-categories/{root_id}"))
            .header(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
            )
            .body(Body::empty())
            .expect("DELETE 请求构造失败");
        let response = router.clone().oneshot(request).await.expect("路由调用失败");
        assert_eq!(response.status().as_u16(), 422, "存在子分类必须拒绝删除");

        // 按父分类筛选根节点：`parent_category_id=root`。
        let (status, body) = api
            .get("/admin/product-categories?parent_category_id=root", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["category_code"], "CAT-ROOT");
    })
}

#[tokio::test]
#[ignore]
async fn voucher_category_profile_requires_voucher_sku() {
    require_mongo!(async {
        let test_db = TestDb::new("catalog_api_voucher").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let dictionaries = seed_catalog_dictionaries(&api, &token).await;
        let unit_id = dictionaries["unit_id"].as_str().unwrap().to_string();

        // PHYSICAL 商品 + 无规格 SKU。
        let (status, body) = api
            .post(
                "/admin/products",
                Some(&token),
                Some(product_payload(
                    "P-V-1",
                    &dictionaries,
                    "PHYSICAL",
                    json!([spec_sku("SKU-V-1", &unit_id, "L", "99.90")]),
                )),
            )
            .await;
        assert_ok_envelope(status, &body);
        let physical_sku_id = {
            let (_, body) = api.get("/admin/skus", Some(&token)).await;
            body["data"]["items"][0]["id"].as_str().unwrap().to_string()
        };

        // 物理 SKU 不能建立卡券类目 → 422。
        let (status, body) = api
            .post(
                "/admin/voucher-category-profiles",
                Some(&token),
                Some(json!({ "sku_id": physical_sku_id, "description": "物理商品卡券类目" })),
            )
            .await;
        assert_eq!(status, 422, "非 VOUCHER 类型必须 422: {body}");

        // VOUCHER 商品（无规格 SKU）→ 200；分类必须允许 VOUCHER 类型。
        let (status, body) = api
            .post(
                "/admin/product-categories",
                Some(&token),
                Some(json!({ "category_code": "CAT-V", "name": "卡券类目分类", "product_kind": "VOUCHER" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let voucher_category_id = body["data"]["id"].as_str().unwrap().to_string();
        let (status, body) = api
            .post(
                "/admin/products",
                Some(&token),
                Some(json!({
                    "product_no": "P-V-2",
                    "product_kind": "VOUCHER",
                    "name": "卡券类目商品",
                    "category_id": voucher_category_id,
                    "brand_id": dictionaries["brand_id"],
                    "effective_from": "2026-01-01",
                    "skus": json!([json!({
                        "sku_no": "SKU-V-2",
                        "base_unit_id": unit_id,
                    })]),
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let voucher_sku_id = {
            let (_, body) = api.get("/admin/skus?sku_no=SKU-V-2", Some(&token)).await;
            body["data"]["items"][0]["id"].as_str().unwrap().to_string()
        };
        let (status, body) = api
            .post(
                "/admin/voucher-category-profiles",
                Some(&token),
                Some(json!({ "sku_id": voucher_sku_id, "description": "中国通卡券类目" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["revision_no"], 1);
        assert_eq!(body["data"]["description"], "中国通卡券类目");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sort_guards_apply() {
    require_mongo!(async {
        let test_db = TestDb::new("catalog_api_paging").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .get("/admin/product-brands?sort_by=unknown_field", Some(&token))
            .await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let (status, _) = api.get("/admin/product-brands?sort_dir=up", Some(&token)).await;
        assert_eq!(status, 400, "非法排序方向必须 400");

        let (status, _) = api
            .get("/admin/product-brands?page=0&page_size=1000", Some(&token))
            .await;
        assert_eq!(status, 400, "越界分页参数必须 400");

        // 合法边界：空页返回空 items 与正确 total。
        let (status, body) = api
            .get("/admin/product-brands?page=2&page_size=5", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["page"], 2);
        assert_eq!(body["data"]["page_size"], 5);
        assert_eq!(body["data"]["items"], json!([]));
        assert_eq!(body["data"]["total"], 0);

        // 422：缺必填字段与非法枚举走 axum Json 拒绝（与 D01 测试同形态）。
        let (status, _) = api
            .post(
                "/admin/product-brands",
                Some(&token),
                Some(json!({ "brand_code": "BR-X" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        let (status, _) = api
            .post(
                "/admin/product-brands",
                Some(&token),
                Some(json!({ "brand_code": "BR-X", "name": "x", "status": "MARS" })),
            )
            .await;
        assert_eq!(status, 422, "非法枚举值走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("catalog_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/products", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("catalog_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        // 种子账号只有 role/admin/audit_log.list 权限，本域权限未授予 → 403。
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/products", Some(&token)).await;
        assert_eq!(status, 403, "无 product.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn validation_and_conflict_cases_for_dictionaries() {
    require_mongo!(async {
        let test_db = TestDb::new("catalog_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router.clone());

        // 空白品牌代码 → 400。
        let (status, body) = api
            .post(
                "/admin/product-brands",
                Some(&token),
                Some(json!({ "brand_code": "  ", "name": "空代码" })),
            )
            .await;
        assert_eq!(status, 400, "空白 brand_code 必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        // 计量单位小数位越界 → 400。
        let (status, body) = api
            .post(
                "/admin/unit-of-measures",
                Some(&token),
                Some(json!({ "unit_code": "KG", "name": "千克", "symbol": "kg", "quantity_scale": 9 })),
            )
            .await;
        assert_eq!(status, 400, "quantity_scale 越界必须 400: {body}");

        // 重复单位代码（唯一索引）→ 409。
        let (status, body) = api
            .post(
                "/admin/unit-of-measures",
                Some(&token),
                Some(json!({ "unit_code": "KG", "name": "千克", "symbol": "kg", "quantity_scale": 3 })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let (status, body) = api
            .post(
                "/admin/unit-of-measures",
                Some(&token),
                Some(json!({ "unit_code": "KG", "name": "重复单位", "symbol": "kg", "quantity_scale": 3 })),
            )
            .await;
        assert_eq!(status, 409, "重复 unit_code 唯一索引冲突必须 409: {body}");

        // 规格属性值引用不存在的属性 → 404。
        let (status, body) = api
            .post(
                "/admin/sku-attribute-values",
                Some(&token),
                Some(json!({ "attribute_id": "attr-missing", "value_code": "L", "display_value": "大号", "sort_order": 0 })),
            )
            .await;
        assert_eq!(status, 404, "属性不存在必须 404: {body}");

        // 软删除后列表不可见；重复代码可重新使用（软删除不占唯一空间语义由索引决定）。
        let (status, body) = api
            .post(
                "/admin/product-brands",
                Some(&token),
                Some(json!({ "brand_code": "BR-DEL", "name": "待删除品牌" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let brand_id = body["data"]["id"].as_str().unwrap().to_string();
        let request = Request::builder()
            .method(Method::DELETE)
            .uri(format!("/admin/product-brands/{brand_id}"))
            .header(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
            )
            .body(Body::empty())
            .expect("DELETE 请求构造失败");
        let response = router.clone().oneshot(request).await.expect("路由调用失败");
        assert_eq!(response.status().as_u16(), 200, "品牌软删除必须成功");
        let (status, body) = api.get("/admin/product-brands", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 0, "软删除后列表不可见");
    })
}
