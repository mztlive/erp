//! 域 D24 `supplier_catalog` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test supplier_catalog_api -- --include-ignored`。
//!
//! 跨域依赖直接经各域 Repository 种子（D09 供应商、D10 公司 SKU），
//! 与生产跨域协作规则一致（Service 只调对方 Repository）。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use database::{NoTransaction, SupplierExt};
use entities::ids::{ProductId, SupplierAccountId, UnitOfMeasureId};
use entities::supplier::{SupplierAccount, SupplierAccountData, SupplierAccountStatus};
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节）。
const TEST_JWT_SECRET: &str = "p0-5-test-secret-that-is-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("supplier_catalog_product", "list"),
    ("supplier_catalog_product", "detail"),
    ("supplier_catalog_product", "create"),
    ("supplier_catalog_product", "update"),
    ("supplier_catalog_sku", "list"),
    ("supplier_product_mapping", "list"),
    ("supplier_product_mapping", "create"),
    ("supplier_product_mapping", "approve"),
    ("supplier_offering", "list"),
    ("supplier_offering", "update"),
    ("supplier_catalog_intake_batch", "list"),
];

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

/// 种子 D09 供应商角色。
async fn seed_supplier(db: &Database) {
    let supplier = SupplierAccount::new(
        SupplierAccountId::new("sup-1"),
        SupplierAccountData {
            party_id: entities::ids::PartyId::new("pty-1"),
            supplier_no: "SUP-1001".to_string(),
            default_payment_term_id: None,
            current_commercial_profile_revision_id: None,
            status: SupplierAccountStatus::Active,
        },
        "test",
    )
    .unwrap();
    db.supplier_accounts()
        .create(&supplier, &mut NoTransaction)
        .await
        .unwrap();
}

/// 种子 D10 公司 SKU（原始文档形态；实体构造所需 Data 类型未从实体层导出）。
async fn seed_company_sku(db: &Database) -> String {
    let sku_id = format!("sku-{}", uuid::Uuid::new_v4().simple());
    db.collection::<Document>("skus")
        .insert_one(doc! {
            "_id": &sku_id,
            "id": &sku_id,
            "version": 1i64,
            "created_at": 1_700_000_000i64,
            "updated_at": 1_700_000_000i64,
            "deleted_at": 0i64,
            "status": "ENABLED",
            "current_revision_id": null,
            "created_by": "test",
            "updated_by": "test",
            "sku_no": "SKU-1001",
            "product_id": ProductId::new("prd-1").to_string(),
            "base_unit_id": UnitOfMeasureId::new("uom-1").to_string(),
            "specification_signature": "500gx2",
        })
        .await
        .unwrap();
    sku_id
}

/// 供应商商品创建请求体。
fn create_product_payload(source_reference: &str) -> Value {
    json!({
        "source_type": "MANUAL",
        "supplier_id": "sup-1",
        "source_reference": source_reference,
        "supplier_spu_code": "SPU-1001",
        "name": "慰问礼包",
        "description": "500g×2 礼盒",
        "source_product_kind": "PHYSICAL",
        "source_category": "礼盒",
        "source_brand": "华联",
        "structured_attributes": [ { "attribute_name": "口味", "attribute_value": "五香" } ],
        "media": [
            { "usage": "SPU_CAROUSEL", "url": "https://cdn.example.com/spu1.jpg" }
        ],
        "source_revision_token": "rev-1",
        "valid_from": "2026-08-01",
        "valid_to": null,
        "skus": [
            {
                "supplier_sku_code": "SKU-1001",
                "name": "慰问礼包 500g",
                "specification": "500g×2",
                "source_base_unit": "箱",
                "barcode": "6901234567890",
                "dropship_floor_price_gross": "12.0000",
                "bulk_floor_price_gross": "9.9900",
                "bulk_minimum_order_quantity": "3.000000",
                "available_quantity": "100.000000",
                "availability_status": "AVAILABLE",
                "structured_attributes": []
            }
        ],
        "idempotency_key": "create-1"
    })
}

#[tokio::test]
#[ignore]
async fn happy_path_create_product_then_list_and_detail() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_supplier(test_db.db()).await;

        let (status, body) = api.get("/admin/supplier-catalog/products", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"], json!([]));
        assert_eq!(body["data"]["total"], 0);

        let (status, body) = api
            .post(
                "/admin/supplier-catalog/products",
                Some(&token),
                Some(create_product_payload("batch-1")),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["replayed"], false);
        let product_id = body["data"]["product_id"].as_str().unwrap().to_string();
        let intake_batch_id = body["data"]["intake_batch_id"].as_str().unwrap().to_string();
        assert!(!body["data"]["sku_ids"][0].as_str().unwrap().is_empty());

        let (status, body) = api.get("/admin/supplier-catalog/products", Some(&token)).await;
        assert_ok_envelope(status, &body);
        let data = &body["data"];
        assert_eq!(data["total"], 1);
        assert_eq!(data["page"], 1);
        assert_eq!(data["page_size"], 20);
        let item = &data["items"][0];
        for field in [
            "id",
            "supplier_id",
            "source_type",
            "supplier_spu_code",
            "status",
            "name",
            "current_revision_no",
            "version",
            "created_at",
        ] {
            assert!(item.get(field).is_some(), "契约字段 {field} 必须存在: {item}");
        }
        assert_eq!(item["supplier_spu_code"], "SPU-1001");
        assert_eq!(item["name"], "慰问礼包");
        assert_eq!(item["current_revision_no"], 1);

        let (status, body) = api
            .get(
                &format!("/admin/supplier-catalog/products/{product_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        let detail = &body["data"];
        assert_eq!(detail["product"]["id"], product_id);
        assert_eq!(detail["revisions"][0]["revision_no"], 1);
        assert_eq!(detail["media"][0]["usage"], "SPU_CAROUSEL");
        assert_eq!(detail["skus"][0]["sku"]["supplier_sku_code"], "SKU-1001");
        assert_eq!(detail["skus"][0]["revisions"][0]["revision_no"], 1);

        // 入库批次可查。
        let (status, body) = api
            .get("/admin/supplier-catalog/intake-batches", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["id"], intake_batch_id);
        assert_eq!(body["data"]["items"][0]["item_count"], 1);

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn create_product_is_idempotent_by_intake_source_key() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_api_idem").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_supplier(test_db.db()).await;

        let (status, body) = api
            .post(
                "/admin/supplier-catalog/products",
                Some(&token),
                Some(create_product_payload("batch-1")),
            )
            .await;
        assert_ok_envelope(status, &body);
        let product_id = body["data"]["product_id"].as_str().unwrap().to_string();
        let intake_batch_id = body["data"]["intake_batch_id"].as_str().unwrap().to_string();

        // 同一来源键重复提交 → 幂等重放，不产生新批次/新商品。
        let (status, body) = api
            .post(
                "/admin/supplier-catalog/products",
                Some(&token),
                Some(create_product_payload("batch-1")),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["replayed"], true);
        assert_eq!(body["data"]["intake_batch_id"], intake_batch_id);

        let batch_count = test_db
            .db()
            .collection::<Document>("supplier_catalog_intake_batches")
            .count_documents(doc! { "id": { "$ne": null } })
            .await
            .unwrap();
        assert_eq!(batch_count, 1, "重复提交只产生一个入库批次");
        let product_count = test_db
            .db()
            .collection::<Document>("supplier_catalog_products")
            .count_documents(doc! { "supplier_id": "sup-1" })
            .await
            .unwrap();
        assert_eq!(product_count, 1, "重复提交只产生一个供应商 SPU");
        let _ = product_id;
    })
}

#[tokio::test]
#[ignore]
async fn revise_product_appends_revision_and_conflicts_on_stale_version() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_api_revise").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_supplier(test_db.db()).await;

        let (_, body) = api
            .post(
                "/admin/supplier-catalog/products",
                Some(&token),
                Some(create_product_payload("batch-1")),
            )
            .await;
        let product_id = body["data"]["product_id"].as_str().unwrap().to_string();

        // 期望修订号不匹配 → 409。
        let (status, body) = api
            .post(
                &format!("/admin/supplier-catalog/products/{product_id}/revisions"),
                Some(&token),
                Some(json!({
                    "expected_revision_no": 5,
                    "supplier_spu_code": "SPU-1001",
                    "name": "慰问礼包新版",
                    "source_product_kind": "PHYSICAL",
                    "skus": [{
                        "supplier_sku_code": "SKU-1001",
                        "name": "慰问礼包 500g",
                        "specification": "500g×2",
                        "source_base_unit": "箱",
                        "dropship_floor_price_gross": "11.5000",
                        "bulk_floor_price_gross": "9.4900",
                        "bulk_minimum_order_quantity": "3.000000",
                        "availability_status": "AVAILABLE",
                        "structured_attributes": []
                    }],
                    "change_reason": "成本调整",
                    "idempotency_key": "rev-1",
                })),
            )
            .await;
        assert_eq!(status, 409, "期望修订号不一致必须 409: {body}");

        let (status, body) = api
            .post(
                &format!("/admin/supplier-catalog/products/{product_id}/revisions"),
                Some(&token),
                Some(json!({
                    "expected_revision_no": 1,
                    "supplier_spu_code": "SPU-1001",
                    "name": "慰问礼包新版",
                    "source_product_kind": "PHYSICAL",
                    "skus": [{
                        "supplier_sku_code": "SKU-1001",
                        "name": "慰问礼包 500g",
                        "specification": "500g×2",
                        "source_base_unit": "箱",
                        "dropship_floor_price_gross": "11.5000",
                        "bulk_floor_price_gross": "9.4900",
                        "bulk_minimum_order_quantity": "3.000000",
                        "availability_status": "AVAILABLE",
                        "structured_attributes": []
                    }],
                    "change_reason": "成本调整",
                    "idempotency_key": "rev-1",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["revision_no"], 2);

        let (status, body) = api
            .get(
                &format!("/admin/supplier-catalog/products/{product_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["product"]["current_revision_no"], 2);
        assert_eq!(body["data"]["revisions"][0]["name"], "慰问礼包新版");
        assert_eq!(
            body["data"]["revisions"].as_array().unwrap().len(),
            2,
            "只追加不覆盖"
        );
    })
}

#[tokio::test]
#[ignore]
async fn mapping_create_approve_and_offering_revise_flow() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_api_pool").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        seed_supplier(test_db.db()).await;
        let company_sku_id = seed_company_sku(test_db.db()).await;

        let (_, body) = api
            .post(
                "/admin/supplier-catalog/products",
                Some(&token),
                Some(create_product_payload("batch-1")),
            )
            .await;
        let supplier_sku_id = body["data"]["sku_ids"][0].as_str().unwrap().to_string();

        // 创建映射（PENDING）。
        let (status, body) = api
            .post(
                "/admin/supplier-catalog/mappings",
                Some(&token),
                Some(json!({
                    "supplier_catalog_sku_id": supplier_sku_id,
                    "sku_id": company_sku_id,
                    "reason": "规格一致，入池",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "PENDING");
        let mapping_id = body["data"]["mapping_id"].as_str().unwrap().to_string();

        // 确认映射 + 双价供给（入池）。
        let (status, body) = api
            .post(
                &format!("/admin/supplier-catalog/mappings/{mapping_id}/approve"),
                Some(&token),
                Some(json!({
                    "expected_version": 1,
                    "dropship_supply_price_gross": "11.5000",
                    "bulk_supply_price_gross": "9.4900",
                    "input_tax_rate": "0.13",
                    "bulk_minimum_order_quantity": "3.000000",
                    "supply_region": ["全国"],
                    "valid_from": "2026-08-01",
                    "dropship_express": "顺丰包邮",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "ACTIVE");
        let offering_id = body["data"]["offering_id"].as_str().unwrap().to_string();
        assert_eq!(body["data"]["offering_revision_no"], 1);

        // 映射与供给列表可见。
        let (status, body) = api
            .get(
                &format!("/admin/supplier-catalog/mappings?supplier_catalog_sku_id={supplier_sku_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        assert_eq!(body["data"]["items"][0]["status"], "ACTIVE");

        let (status, body) = api
            .get(
                &format!("/admin/supplier-catalog/offerings?sku_id={company_sku_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["total"], 1);
        let offering = &body["data"]["items"][0];
        assert_eq!(offering["id"], offering_id);
        assert_eq!(offering["dropship_supply_price_gross"], "11.5000");
        assert_eq!(offering["bulk_supply_price_gross"], "9.4900");
        assert_eq!(offering["input_tax_rate"], "0.13");
        assert_eq!(offering["status"], "ACTIVE");

        // 供给修订（暂停）。
        let (status, body) = api
            .post(
                &format!("/admin/supplier-catalog/offerings/{offering_id}/revisions"),
                Some(&token),
                Some(json!({
                    "expected_revision_no": 1,
                    "dropship_supply_price_gross": "11.5000",
                    "bulk_supply_price_gross": "9.4900",
                    "input_tax_rate": "0.13",
                    "bulk_minimum_order_quantity": "3.000000",
                    "supply_region": ["全国"],
                    "valid_from": "2026-08-01",
                    "status": "PAUSED",
                    "change_reason": "库存调整暂停",
                    "idempotency_key": "off-1",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["revision_no"], 2);
        assert_eq!(body["data"]["status"], "PAUSED");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/supplier-catalog/products", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/supplier-catalog/products", Some(&token)).await;
        assert_eq!(
            status, 403,
            "无 supplier_catalog_product.list 权限必须 403: {body}"
        );
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400_and_422() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/supplier-catalog/products",
                Some(&token),
                Some(json!({
                    "source_type": "MANUAL",
                    "supplier_id": "sup-1",
                    "supplier_spu_code": "  ",
                    "name": "空白编码",
                    "skus": [{ "supplier_sku_code": "S1", "name": "n", "specification": "s", "availability_status": "AVAILABLE", "structured_attributes": [] }],
                    "idempotency_key": "k-1",
                })),
            )
            .await;
        assert_eq!(status, 400, "空白 SPU 编码必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let (status, body) = api
            .post(
                "/admin/supplier-catalog/products",
                Some(&token),
                Some(json!({
                    "source_type": "MANUAL",
                    "supplier_id": "sup-1",
                    "supplier_spu_code": "SPU-1",
                    "name": "无SKU",
                    "skus": [],
                    "idempotency_key": "k-2",
                })),
            )
            .await;
        assert_eq!(status, 400, "空 SKU 集合必须 400: {body}");

        let (status, _) = api
            .post(
                "/admin/supplier-catalog/products",
                Some(&token),
                Some(json!({ "supplier_id": "sup-1" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sorting_boundaries() {
    require_mongo!(async {
        let test_db = TestDb::new("sc_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .get(
                "/admin/supplier-catalog/products?page=2&page_size=10&sort_by=supplier_spu_code&sort_dir=asc",
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["page"], 2);
        assert_eq!(body["data"]["page_size"], 10);
        assert_eq!(body["data"]["total"], 0);

        let (status, body) = api
            .get("/admin/supplier-catalog/products?sort_by=price", Some(&token))
            .await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");

        let (status, body) = api
            .get("/admin/supplier-catalog/skus?page_size=500", Some(&token))
            .await;
        assert_eq!(status, 400, "超界分页大小必须 400: {body}");
    })
}
