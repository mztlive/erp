//! 域 D26 `publication` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test publication_api -- --include-ignored`。
//!
//! 跨域前置：本域创建/形成修订依赖 D10 `sku`/`sku_revision` 与
//! D24 `supplier_offering`/`supplier_offering_revision` 存在性，测试直接以实体
//! 构造方式种子这些集合。
//!
//! 外部 HTTP 调用（发布投递）不真调外部网络：handler 使用默认失败关闭连接器，
//! 覆盖「失败降级为可观测错误」路径（`inbox_message` + `integration_error_task`）；
//! mock 成功路径在 services 单测覆盖。

use std::path::PathBuf;
use std::str::FromStr;

use axum::body::Body;
use axum::http::{header::AUTHORIZATION, HeaderValue, Method, Request};
use axum::Router;
use config::{Config, SafeConfig};
use entities::catalog::sku::SkuData;
use entities::catalog::sku_revision::SkuRevisionData;
use entities::catalog::{Sku, SkuRevision};
use entities::ids::{
    ProductId, SkuId, SkuRevisionId, SupplierAccountId, SupplierCatalogSkuId, SupplierOfferingId,
    SupplierOfferingRevisionId, UnitOfMeasureId,
};
use entities::money::{Amount, Quantity, Rate, UnitPrice};
use entities::supplier_catalog::{
    AvailabilityStatus, PrefillSourceRefs, SupplierOffering, SupplierOfferingData, SupplierOfferingRevision,
    SupplierOfferingRevisionData,
};
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
    ("product_publication", "list"),
    ("product_publication", "detail"),
    ("product_publication", "create"),
    ("product_publication", "update"),
    ("product_publication_revision", "create"),
    ("product_publication_revision", "list"),
    ("product_publication_delivery", "submit"),
    ("product_publication_delivery", "list"),
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

/// 种子 D10 商品与商品版本（`skus`/`sku_revisions`）。
async fn seed_catalog(db: &Database) {
    let sku = Sku::new(
        SkuId::new("sku-pub-1"),
        SkuData {
            sku_no: "SKU-PUB-0001".to_string(),
            product_id: ProductId::new("product-1"),
            base_unit_id: UnitOfMeasureId::new("uom-1"),
            specification_signature: "sig-1".to_string(),
            status: entities::catalog::EnableStatus::Active,
        },
        "admin-1",
    )
    .expect("SKU 构造失败");
    db.collection::<Sku>("skus")
        .insert_one(&sku)
        .await
        .expect("SKU 种子失败");
    let revision = SkuRevision::new(
        SkuRevisionId::new("sku-rev-pub-1"),
        SkuRevisionData {
            sku_id: SkuId::new("sku-pub-1"),
            revision_no: 1,
            name: "福利商城卡".to_string(),
            description: None,
            specification: Some("100 元面额".to_string()),
            barcode: None,
            weight_kg: None,
            volume_m3: None,
            sales_visible_price_gross: Some(Amount::from_str("100.00").unwrap()),
            market_price: None,
            status: entities::catalog::EnableStatus::Active,
            effective_from: entities::common::time::BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            effective_to: None,
        },
    )
    .expect("SKU 修订构造失败");
    db.collection::<SkuRevision>("sku_revisions")
        .insert_one(&revision)
        .await
        .expect("SKU 修订种子失败");
}

/// 种子 D24 供给与供给修订（`supplier_offerings`/`supplier_offering_revisions`）。
async fn seed_offering(db: &Database) -> String {
    let offering = SupplierOffering::new(
        SupplierOfferingId::new("off-pub-1"),
        SupplierOfferingData {
            sku_id: SkuId::new("sku-pub-1"),
            supplier_id: SupplierAccountId::new("sup-1"),
            supplier_catalog_sku_id: SupplierCatalogSkuId::new("cat-sku-1"),
        },
        "admin-1",
    )
    .expect("供给构造失败");
    db.collection::<SupplierOffering>("supplier_offerings")
        .insert_one(&offering)
        .await
        .expect("供给种子失败");
    let price = UnitPrice::from_str("80.0000").unwrap();
    let revision = SupplierOfferingRevision::new(
        SupplierOfferingRevisionId::new("off-rev-pub-1"),
        SupplierOfferingRevisionData {
            supplier_offering_id: offering.base.id.clone().into(),
            revision_no: 1,
            dropship_supply_price_gross: price,
            dropship_supply_price_net: price,
            bulk_supply_price_gross: price,
            bulk_supply_price_net: price,
            input_tax_rate: Rate::from_str("0.130000").unwrap(),
            dropship_express: None,
            freight_amount: None,
            service_fee_amount: None,
            bulk_minimum_order_quantity: Quantity::from_str("1.000000").unwrap(),
            supply_region: vec!["全国".to_string()],
            availability_status: AvailabilityStatus::Available,
            available_quantity: Some(Quantity::from_str("100.000000").unwrap()),
            product_capabilities: vec!["cancel".to_string()],
            valid_from: entities::common::time::BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: None,
            prefill_source_refs: PrefillSourceRefs {
                input_tax_rate: None,
                supply_region: None,
                valid_from_date: None,
                valid_from_timezone: None,
                valid_from_calendar_version: None,
            },
        },
    )
    .expect("供给修订构造失败");
    let id = revision.base.id.clone();
    db.collection::<SupplierOfferingRevision>("supplier_offering_revisions")
        .insert_one(&revision)
        .await
        .expect("供给修订种子失败");
    id
}

/// 构造最小 AppState 并组装完整应用路由。
async fn build_router(test_db: &TestDb) -> (Router, PathBuf) {
    let config = Config::from_toml_str(&format!(
        r#"
[app]
port = 10001
secret = "{TEST_JWT_SECRET}"
upload_path = "publication-test-uploads"
file_base_url = "http://localhost:10001"

[database]
uri = "mongodb://localhost:27017"
db_name = "test"
"#
    ))
    .expect("测试配置必须合法");
    let upload_path = std::env::temp_dir().join(format!("publication-uploads-{}", uuid::Uuid::new_v4()));
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

/// 组装「建库 + 索引 + 种子账号 + 权限 + token + 路由 + 跨域种子」的公共前置。
async fn setup(prefix: &str) -> (TestDb, String, Router) {
    let test_db = TestDb::new(prefix).await.unwrap();
    database::ensure_indexes(test_db.db()).await.unwrap();
    let account_id = seed_admin_account(test_db.db()).await.unwrap();
    grant_domain_permissions(test_db.db(), &account_id).await;
    let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
    let (router, _) = build_router(&test_db).await;
    (test_db, token, router)
}

/// 创建发布并返回响应视图（供后续用例复用）。
async fn create_publication(api: &TestApi, token: &str) -> Value {
    let (status, body) = api
        .post(
            "/admin/product-publications",
            Some(token),
            Some(json!({
                "sku_id": "sku-pub-1",
                "target_mall_id": "mall-pub-1"
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    body["data"].clone()
}

/// 形成发布修订（含一张主图媒体）。
async fn create_revision(api: &TestApi, token: &str, publication_id: &str) -> Value {
    let (status, body) = api
        .post(
            &format!("/admin/product-publications/{publication_id}/revisions"),
            Some(token),
            Some(json!({
                "sku_revision_id": "sku-rev-pub-1",
                "supplier_offering_revision_id": "off-rev-pub-1",
                "category_id": "cat-1",
                "name": "福利商城卡",
                "specification": "100 元面额",
                "sales_description": "员工福利采购",
                "minimum_purchase_quantity": "1.000000",
                "sales_price_gross": "100.00",
                "sales_tax_rate": "0.130000",
                "base_unit_code": "张",
                "sales_region": "全国",
                "sale_status": "on_sale",
                "product_capabilities": ["cancel", "refund"],
                "valid_from": 1700000000,
                "valid_to": 1800000000,
                "media": [
                    { "file_asset_id": "file-1", "media_role": "main", "sort_no": 1, "alt_text": "卡面主图" },
                    { "file_asset_id": "file-2", "media_role": "carousel", "sort_no": 1, "alt_text": null }
                ]
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    body["data"].clone()
}

#[tokio::test]
#[ignore]
async fn happy_path_publication_revision_deliver_with_contract_shape() {
    require_mongo!(async {
        let (test_db, token, router) = setup("pub_api_happy").await;
        seed_catalog(test_db.db()).await;
        seed_offering(test_db.db()).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/product-publications", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"], json!([]));

        let publication = create_publication(&api, &token).await;
        let publication_id = publication["id"].as_str().unwrap();
        for field in ["id", "sku_id", "target_mall_id", "status", "created_at"] {
            assert!(
                publication.get(field).is_some(),
                "契约字段 {field} 必须存在: {publication}"
            );
        }
        assert_eq!(publication["status"], "draft");
        assert_eq!(publication["sku_id"], "sku-pub-1");
        assert_eq!(publication["target_mall_id"], "mall-pub-1");
        assert_eq!(publication["version"], 1);

        let revision = create_revision(&api, &token, publication_id).await;
        assert_eq!(revision["revision_no"], 1, "首个修订序号为 1");
        assert_eq!(revision["name"], "福利商城卡");
        assert_eq!(revision["sales_price_gross"], "100.00", "金额按字符串序列化");
        assert_eq!(revision["sale_status"], "on_sale");
        assert!(revision["id"].as_str().unwrap().len() > 0);

        let (status, body) = api
            .get(
                &format!("/admin/product-publications/{publication_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["status"], "pending_publish",
            "形成修订后发布进入待发布"
        );

        let (status, body) = api
            .get(
                &format!("/admin/product-publications/{publication_id}/revisions"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"][0]["revision_no"], 1);

        let revision_id = revision["id"].as_str().unwrap();
        let (status, body) = api
            .get(
                &format!("/admin/product-publication-revisions/{revision_id}/media"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let media = body["data"].as_array().unwrap();
        assert_eq!(media.len(), 2);
        assert!(media.iter().any(|m| m["media_role"] == "main"));

        let (status, body) = api
            .post(
                &format!("/admin/product-publications/{publication_id}/revisions/1/deliver"),
                Some(&token),
                Some(json!({ "idempotency_key": "deliver-001" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let result = &body["data"];
        assert_eq!(
            result["delivery_status"], "failed",
            "默认连接器失败关闭 → 投递失败"
        );
        assert!(result["error_task_id"].as_str().unwrap().len() > 0);
        assert!(!result["inbox_message_id"].as_str().unwrap().is_empty());

        let (status, body) = api
            .get(
                &format!("/admin/product-publications/{publication_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(
            body["data"]["status"], "pending_publish",
            "商城未确认不得推进为商城生效（§6.15）"
        );

        let (status, body) = api
            .get("/admin/product-publication-deliveries", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["delivery_status"], "failed");

        let inbox_count = test_db
            .db()
            .collection::<Document>("inbox_messages")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(inbox_count, 1, "一次投递只落一条消息信封");
        let inbox = test_db
            .db()
            .collection::<Document>("inbox_messages")
            .find_one(doc! {})
            .await
            .unwrap()
            .expect("消息信封必须存在");
        assert_eq!(inbox.get_str("status").unwrap(), "failed");
        assert_eq!(inbox.get_str("message_type").unwrap(), "MALL_ACTION_REQUEST");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("pub_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/product-publications", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("pub_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/product-publications", Some(&token)).await;
        assert_eq!(status, 403, "无 product_publication.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_body_and_missing_main_image_return_400() {
    require_mongo!(async {
        let (test_db, token, router) = setup("pub_api_400").await;
        seed_catalog(test_db.db()).await;
        seed_offering(test_db.db()).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/product-publications",
                Some(&token),
                Some(json!({ "sku_id": "sku-pub-1" })),
            )
            .await;
        assert_eq!(status, 422, "缺少 target_mall_id 走 axum Json 拒绝: {body}");

        let (status, body) = api
            .post(
                "/admin/product-publications",
                Some(&token),
                Some(json!({
                    "sku_id": "sku-missing",
                    "target_mall_id": "mall-pub-1"
                })),
            )
            .await;
        assert_eq!(status, 404, "不存在的 SKU 必须 404: {body}");

        let publication = create_publication(&api, &token).await;
        let publication_id = publication["id"].as_str().unwrap();
        let (status, body) = api
            .post(
                &format!("/admin/product-publications/{publication_id}/revisions"),
                Some(&token),
                Some(json!({
                    "sku_revision_id": "sku-rev-pub-1",
                    "supplier_offering_revision_id": "off-rev-pub-1",
                    "category_id": "cat-1",
                    "name": "福利商城卡",
                    "sales_description": "员工福利采购",
                    "minimum_purchase_quantity": "1.000000",
                    "sales_price_gross": "100.00",
                    "sales_tax_rate": "0.130000",
                    "base_unit_code": "张",
                    "sale_status": "on_sale",
                    "product_capabilities": [],
                    "valid_from": 1700000000,
                    "media": [
                        { "file_asset_id": "file-1", "media_role": "carousel", "sort_no": 1, "alt_text": null }
                    ]
                })),
            )
            .await;
        assert_eq!(status, 400, "缺少主图媒体必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_publication_and_inflight_delivery_return_409() {
    require_mongo!(async {
        let (test_db, token, router) = setup("pub_api_409").await;
        seed_catalog(test_db.db()).await;
        seed_offering(test_db.db()).await;
        let api = TestApi::new(router);

        let _ = create_publication(&api, &token).await;
        let (status, body) = api
            .post(
                "/admin/product-publications",
                Some(&token),
                Some(json!({
                    "sku_id": "sku-pub-1",
                    "target_mall_id": "mall-pub-1"
                })),
            )
            .await;
        assert_eq!(
            status, 409,
            "(sku_id, target_mall_id) 唯一索引冲突必须 409: {body}"
        );
        assert_eq!(body["success"], false);

        let publication = create_publication(&api, &token).await;
        let publication_id = publication["id"].as_str().unwrap();
        let _ = create_revision(&api, &token, publication_id).await;

        let (status, body) = api
            .post(
                &format!("/admin/product-publications/{publication_id}/revisions/1/deliver"),
                Some(&token),
                Some(json!({ "idempotency_key": "deliver-409" })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["delivery_status"], "failed");

        let (status, body) = api
            .post(
                &format!("/admin/product-publications/{publication_id}/revisions/1/deliver"),
                Some(&token),
                Some(json!({ "idempotency_key": "deliver-409-b" })),
            )
            .await;
        assert_eq!(status, 409, "未确认版本重复投递必须 409: {body}");

        let delivery_count = test_db
            .db()
            .collection::<Document>("product_publication_deliveries")
            .count_documents(doc! {})
            .await
            .unwrap();
        assert_eq!(delivery_count, 1, "重复投递只产生一条投递记录");
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_update_and_sort_boundary_are_enforced() {
    require_mongo!(async {
        let (test_db, token, router) = setup("pub_api_update").await;
        seed_catalog(test_db.db()).await;
        let api = TestApi::new(router.clone());

        let publication = create_publication(&api, &token).await;
        let publication_id = publication["id"].as_str().unwrap();

        let (status, body) = put_json(
            &router,
            &format!("/admin/product-publications/{publication_id}"),
            &token,
            json!({ "version": 1, "status": "paused" }),
        )
        .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "paused");
        assert_eq!(body["data"]["version"], 2);

        let (status, body) = put_json(
            &router,
            &format!("/admin/product-publications/{publication_id}"),
            &token,
            json!({ "version": 1, "status": "draft" }),
        )
        .await;
        assert_eq!(status, 409, "陈旧版本更新必须 409: {body}");

        let (status, body) = api
            .get("/admin/product-publications?sort_by=evil_field", Some(&token))
            .await;
        assert_eq!(status, 400, "白名单外排序字段必须 400: {body}");

        let (status, _) = api
            .get(
                "/admin/product-publication-deliveries?page_size=1000",
                Some(&token),
            )
            .await;
        assert_eq!(status, 400, "分页大小超界必须 400");
    })
}
