//! 域 D15 `purchase_order` HTTP 集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test --workspace` 跳过；CI 与验收执行
//! `cargo test -p web-api --test purchase_order_api -- --include-ignored`。
//!
//! 跨域依赖直接经各域 Repository 种子（D07 party、D09 supplier、D13 销售提交行、
//! D14 采购确认），与生产跨域协作规则一致（Service 只调对方 Repository）。

use std::path::PathBuf;

use axum::Router;
use config::{Config, SafeConfig};
use database::{NoTransaction, PartyExt, SalesOrderExt, SalesReviewExt, SupplierExt};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    PartyId, PartyRevisionId, ProcurementConfirmationId, ProcurementConfirmationLineId, SalesOrderId,
    SalesOrderSubmissionId, SalesOrderSubmissionLineId, SkuId, SkuRevisionId, SupplierAccountId,
    SupplierCapabilityRevisionId, SupplierCommercialProfileRevisionId,
};
use entities::money::{Quantity, Rate, UnitPrice};
use entities::party::{Party, PartyData, PartyKind, PartyRevision, PartyRevisionData, PartyStatus};
use entities::sales_order::{FulfillmentMode as SalesFulfillmentMode, GoodsLineFields};
use entities::sales_order::{SalesOrderSubmissionLine, SalesOrderSubmissionLineData};
use entities::sales_review::{
    FulfillmentMode, ProcurementConfirmation, ProcurementConfirmationData, ProcurementConfirmationLine,
    ProcurementConfirmationLineData,
};
use entities::supplier::{
    InvoiceType, ReconciliationCycle, SettlementMode, SupplierAccount, SupplierAccountData,
    SupplierAccountStatus, SupplierCommercialProfileRevision, SupplierCommercialProfileRevisionData,
};
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde_json::{json, Value};
use std::str::FromStr;
use test_support::{mint_jwt, require_mongo, seed_admin_account, TestApi, TestDb};
use web_api::app_state::AppState;
use web_api::core::routes;

/// 测试 JWT 密钥（≥32 字节）。
const TEST_JWT_SECRET: &str = "p0-5-test-secret-that-is-at-least-32-bytes-long";
/// 种子账号可访问的本域权限键（resource, action）。
const DOMAIN_PERMISSIONS: &[(&str, &str)] = &[
    ("purchase_order", "list"),
    ("purchase_order", "detail"),
    ("purchase_order", "create"),
    ("purchase_order", "update"),
    ("purchase_order", "submit"),
    ("purchase_order", "approve"),
    ("purchase_order", "reject"),
    ("purchase_change_order", "create"),
    ("purchase_change_order", "submit"),
    ("purchase_change_order", "post"),
    ("purchase_change_order", "list"),
    ("purchase_change_order", "detail"),
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

/// 已种子的跨域依赖（供应商、主体、采购确认、销售提交行）。
struct SeededContext {
    /// 采购确认（已通过）。
    pub confirmation_id: String,
    /// 采购确认分行。
    pub confirmation_line_id: String,
    /// 销售提交行。
    pub sales_submission_line_id: String,
}

/// 种子跨域依赖（D07 主体、D09 供应商、D13 销售提交行、D14 采购确认）。
async fn seed_context(db: &Database) -> SeededContext {
    // D07：企业主体 + 当前主体修订（供应商名称来源）。
    let party = Party::new(
        PartyId::new(next_id()),
        PartyData {
            party_no: "P-1001".to_string(),
            party_kind: PartyKind::Enterprise,
            unified_credit_code: None,
            status: PartyStatus::Active,
        },
        "test",
    )
    .unwrap();
    let party_revision = PartyRevision::new(
        PartyRevisionId::new(next_id()),
        PartyRevisionData {
            revision_no: 1,
            party_id: party.base.id.clone().into(),
            legal_name: "测试供应商".to_string(),
            short_name: None,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            effective_to: None,
            change_reason: "期初".to_string(),
        },
    )
    .unwrap();
    let mut party_mut = party.clone();
    party_mut.stable.current_revision_id = Some(party_revision.base.id.clone());
    db.parties().create(&party_mut, &mut NoTransaction).await.unwrap();
    db.party_revisions()
        .create(&party_revision, &mut NoTransaction)
        .await
        .unwrap();

    // D09：供应商角色 + 商务结算版本（付款条件快照 NET-30）。
    let profile = SupplierCommercialProfileRevision::new(
        SupplierCommercialProfileRevisionId::new(next_id()),
        SupplierCommercialProfileRevisionData {
            supplier_id: SupplierAccountId::new("sup-1"),
            revision_no: 1,
            settlement_mode: SettlementMode::PayAfterUse,
            reconciliation_cycle: ReconciliationCycle::Monthly,
            payment_term_snapshot: "NET-30".to_string(),
            invoice_type: InvoiceType::VatSpecial,
            invoice_tax_rate: Rate::from_str("0.13").unwrap(),
            signing_entity_party_id: party.base.id.clone().into(),
            payment_entity_party_id: party.base.id.clone().into(),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: None,
            change_reason: "期初".to_string(),
        },
    )
    .unwrap();
    let supplier = SupplierAccount::new(
        SupplierAccountId::new("sup-1"),
        SupplierAccountData {
            party_id: party.base.id.clone().into(),
            supplier_no: "SUP-1001".to_string(),
            default_payment_term_id: Some("NET-30".to_string()),
            current_commercial_profile_revision_id: Some(profile.base.id.clone().into()),
            status: SupplierAccountStatus::Active,
        },
        "test",
    )
    .unwrap();
    db.supplier()
        .create_supplier_with_initial_profile(&supplier, &profile, &mut NoTransaction)
        .await
        .unwrap();

    // D13：销售提交行（商品名/规格/单位/SKU 快照来源）。
    let sales_line = SalesOrderSubmissionLine::new(
        SalesOrderSubmissionLineId::new(next_id()),
        SalesOrderSubmissionId::new("sso-1"),
        SalesOrderSubmissionLineData {
            sales_order_line_id: entities::ids::SalesOrderLineId::new("sol-1"),
            line_no: 1,
            line_type: entities::sales_order::LineType::GoodsService,
            sales_tax_rate: Rate::from_str("0.13").unwrap(),
            item_name_snapshot: "慰问礼包".to_string(),
            spec_snapshot: Some("500g×2".to_string()),
            unit_snapshot: Some("箱".to_string()),
            goods: Some(GoodsLineFields {
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: SkuRevisionId::new("skur-1"),
                welfare_scenario: None,
                fulfillment_mode: SalesFulfillmentMode::CompanyWarehouse,
                fulfillment_due_at: Instant::now(),
                quantity: Quantity::from_str("3.000000").unwrap(),
                base_unit_code: "箱".to_string(),
                unit_price_gross: UnitPrice::from_str("9.9900").unwrap(),
            }),
            voucher: None,
        },
    )
    .unwrap();
    db.sales_order_submission_lines()
        .create(&sales_line, &mut NoTransaction)
        .await
        .unwrap();

    // D14：采购确认（已通过）与分行。
    let confirmation = ProcurementConfirmation::new(
        ProcurementConfirmationId::new(next_id()),
        ProcurementConfirmationData {
            sales_order_id: SalesOrderId::new("so-1"),
            submission_id: SalesOrderSubmissionId::new("sso-1"),
            reject_reason_code: None,
            comment: None,
        },
        "buyer-1",
    )
    .unwrap();
    let mut confirmation = confirmation;
    confirmation.approve("buyer-1", Instant::now()).unwrap();
    let confirmation_line = ProcurementConfirmationLine::new(
        ProcurementConfirmationLineId::new(next_id()),
        ProcurementConfirmationLineData {
            procurement_confirmation_id: confirmation.base.id.clone().into(),
            line_no: 1,
            sales_order_submission_line_id: sales_line.base.id.clone().into(),
            supplier_id: SupplierAccountId::new("sup-1"),
            confirmed_quantity: Quantity::from_str("3.000000").unwrap(),
            latest_cost_gross: UnitPrice::from_str("9.9900").unwrap(),
            input_tax_rate: Rate::from_str("0.13").unwrap(),
            expected_delivery_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
            fulfillment_mode: FulfillmentMode::CompanyWarehouse,
            supplier_capability_revision_id: SupplierCapabilityRevisionId::new("cap-1"),
        },
    )
    .unwrap();
    let confirmation_id = confirmation.base.id.clone();
    let line_id = confirmation_line.base.id.clone();
    db.procurement_confirmations()
        .create(&confirmation, &mut NoTransaction)
        .await
        .unwrap();
    db.procurement_confirmation_lines()
        .create(&confirmation_line, &mut NoTransaction)
        .await
        .unwrap();

    SeededContext {
        confirmation_id,
        confirmation_line_id: line_id,
        sales_submission_line_id: sales_line.base.id.clone(),
    }
}

/// 生成采购单完整保存行（与种子确认分行对齐）。
fn draft_line_payload(ctx: &SeededContext, line_id: Option<&str>) -> Value {
    json!({
        "line_type": "ITEM_SERVICE",
        "procurement_confirmation_line_id": ctx.confirmation_line_id,
        "sku_id": "sku-1",
        "sku_revision_id": "skur-1",
        "product_name": "慰问礼包",
        "specification": "500g×2",
        "quantity": "3.000000",
        "base_unit_code": "箱",
        "unit_cost_gross": "9.9900",
        "input_tax_rate": "0.13",
        "expected_delivery_date": "2026-08-06",
        "sales_order_submission_line_id": ctx.sales_submission_line_id,
        "allocated_quantity": "3.000000",
        "line_id": line_id,
    })
}

/// 创建采购单并返回 id。
async fn create_purchase_order(api: &TestApi, token: &str, ctx: &SeededContext) -> String {
    let (status, body) = api
        .post(
            "/admin/purchase-orders",
            Some(token),
            Some(json!({
                "basis_id": ctx.confirmation_id,
                "purchase_type": "PHYSICAL",
                "payment_term_code": "NET-30",
                "idempotency_key": "create-1",
            })),
        )
        .await;
    assert_ok_envelope(status, &body);
    assert_eq!(body["data"]["replayed"], false);
    body["data"]["purchase_order_id"].as_str().unwrap().to_string()
}

fn next_id() -> String {
    use id_generator::next_id as nid;
    nid()
}

#[tokio::test]
#[ignore]
async fn happy_path_create_save_submit_approve_flow() {
    require_mongo!(async {
        let test_db = TestDb::new("po_api_happy").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, upload_path) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let ctx = seed_context(test_db.db()).await;

        let (status, body) = api.get("/admin/purchase-orders", Some(&token)).await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["items"], json!([]));
        assert_eq!(body["data"]["total"], 0);

        let order_id = create_purchase_order(&api, &token, &ctx).await;

        let (status, body) = api
            .get(&format!("/admin/purchase-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        let detail = &body["data"];
        assert_eq!(detail["status"], "DRAFT");
        assert_eq!(detail["content_source"], "DRAFT");
        assert_eq!(detail["supplier_name"], "测试供应商");
        let line = &detail["lines"][0];
        for field in [
            "line_id",
            "line_no",
            "line_type",
            "product_name",
            "quantity",
            "gross_amount",
            "net_amount",
            "tax_amount",
        ] {
            assert!(line.get(field).is_some(), "契约字段 {field} 必须存在: {line}");
        }
        assert_eq!(line["gross_amount"], "29.97", "行金额 9.99×3 逐行舍入");

        let (status, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/draft"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": detail["version"],
                    "payment_term_code": "NET-30",
                    "lines": [draft_line_payload(&ctx, None)],
                    "idempotency_key": "save-1",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["totals"]["gross"], "29.97");

        let (status, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/submit"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": body["data"]["lock_version"],
                    "idempotency_key": "submit-1",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let submission_id = body["data"]["submission_id"].as_str().unwrap().to_string();
        let work_item_id = body["data"]["work_item_id"].as_str().unwrap().to_string();
        let lock_version = body["data"]["lock_version"].as_u64().unwrap();
        assert!(body["data"]["submission_no"]
            .as_str()
            .unwrap()
            .starts_with("SUB-"));

        let (status, body) = api
            .get(&format!("/admin/purchase-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "PENDING_FINANCE_REVIEW");
        assert_eq!(body["data"]["content_source"], "SUBMISSION");

        let (status, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/review/approve"),
                Some(&token),
                Some(json!({
                    "submission_id": submission_id,
                    "work_item_id": work_item_id,
                    "expected_lock_version": lock_version,
                    "comment": "金额核对无误",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["review_result"], "APPROVED");
        let revision_id = body["data"]["revision_id"].as_str().unwrap().to_string();
        assert_eq!(body["data"]["revision_no"], 1);
        assert!(body["data"]["payable_entry_id"].as_str().is_some());

        let db = test_db.db();
        // §8.1.4：版本、版本行、应付分录、成本事实、待办多集合同时生效。
        let revision = db
            .collection::<Document>("purchase_order_revisions")
            .find_one(doc! { "id": &revision_id })
            .await
            .unwrap()
            .expect("生效版本必须存在");
        assert_eq!(revision.get_str("purchase_order_id").unwrap(), order_id);
        let revision_lines = db
            .collection::<Document>("purchase_order_revision_lines")
            .find_one(doc! { "purchase_order_revision_id": revision_id })
            .await
            .unwrap()
            .expect("版本行必须存在");
        assert_eq!(revision_lines.get_str("line_type").unwrap(), "ITEM_SERVICE");
        let payable_entries = db
            .collection::<Document>("payable_entries")
            .find_one(doc! { "source_document_id": &order_id })
            .await
            .unwrap()
            .expect("应付分录必须存在");
        assert_eq!(payable_entries.get_str("entry_type").unwrap(), "original");
        let cost_entries = db
            .collection::<Document>("cost_entries")
            .find_one(doc! { "source_document_id": &order_id })
            .await
            .unwrap()
            .expect("CONFIRMED 成本事实必须存在");
        assert_eq!(cost_entries.get_str("cost_stage").unwrap(), "confirmed");
        let work_item = db
            .collection::<Document>("work_items")
            .find_one(doc! { "id": work_item_id })
            .await
            .unwrap()
            .expect("审核待办必须存在");
        assert_eq!(work_item.get_str("status").unwrap(), "COMPLETED");

        let (status, body) = api
            .get(&format!("/admin/purchase-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "EFFECTIVE");
        assert_eq!(body["data"]["review_status"], "APPROVED");
        assert_eq!(body["data"]["content_source"], "REVISION");
        assert_eq!(body["data"]["revision_no"], 1);
        assert_eq!(body["data"]["totals"]["gross"], "29.97");

        let _ = tokio::fs::remove_dir_all(upload_path).await;
    })
}

#[tokio::test]
#[ignore]
async fn review_approve_rolls_back_all_collections_when_payable_duplicate() {
    require_mongo!(async {
        let test_db = TestDb::new("po_api_tx").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let ctx = seed_context(test_db.db()).await;

        let order_id = create_purchase_order(&api, &token, &ctx).await;
        let (_, body) = api
            .get(&format!("/admin/purchase-orders/{order_id}"), Some(&token))
            .await;
        let (_, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/draft"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": body["data"]["version"],
                    "payment_term_code": "NET-30",
                    "lines": [draft_line_payload(&ctx, None)],
                    "idempotency_key": "save-1",
                })),
            )
            .await;
        let (_, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/submit"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": body["data"]["lock_version"],
                    "idempotency_key": "submit-1",
                })),
            )
            .await;
        let submission_id = body["data"]["submission_id"].as_str().unwrap().to_string();
        let work_item_id = body["data"]["work_item_id"].as_str().unwrap().to_string();
        let lock_version = body["data"]["lock_version"].as_u64().unwrap();

        // 注入失败：预插同键应付子账（source_type + source_document_id 唯一），
        // 审核事务内子账写入触发唯一索引冲突 → 整个事务回滚。
        let db = test_db.db();
        let payable_account_id = next_id();
        db.collection::<Document>("payable_accounts")
            .insert_one(doc! {
                "_id": &payable_account_id,
                "id": &payable_account_id,
                "version": 1i64,
                "created_at": 1_700_000_000i64,
                "updated_at": 1_700_000_000i64,
                "deleted_at": 0i64,
                "status": "ACTIVE",
                "current_revision_id": null,
                "created_by": "test",
                "updated_by": "test",
                "source_document_id": &order_id,
                "supplier_id": "sup-1",
                "source_type": "purchase_order",
                "gross_total": "29.97",
                "settled_total": "0.00",
                "open_total": "29.97",
                "invoiceable_total": "29.97",
                "invoiced_total": "0.00",
                "open_invoiceable_total": "29.97",
            })
            .await
            .unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/review/approve"),
                Some(&token),
                Some(json!({
                    "submission_id": submission_id,
                    "work_item_id": work_item_id,
                    "expected_lock_version": lock_version,
                    "comment": "触发回滚",
                })),
            )
            .await;
        assert_eq!(status, 409, "唯一索引冲突必须 409: {body}");
        assert_eq!(body["success"], false);

        // 注入失败后：版本、版本行、成本、待办全部不可见，采购状态不变。
        let revision = db
            .collection::<Document>("purchase_order_revisions")
            .find_one(doc! { "purchase_order_id": &order_id })
            .await
            .unwrap();
        assert!(revision.is_none(), "回滚后不得留下生效版本");
        let cost = db
            .collection::<Document>("cost_entries")
            .find_one(doc! { "source_document_id": &order_id })
            .await
            .unwrap();
        assert!(cost.is_none(), "回滚后不得留下成本事实");
        let payable_entry = db
            .collection::<Document>("payable_entries")
            .find_one(doc! { "source_document_id": &order_id })
            .await
            .unwrap();
        assert!(payable_entry.is_none(), "回滚后不得留下应付分录");
        let order = db
            .collection::<Document>("purchase_orders")
            .find_one(doc! { "id": &order_id })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(order.get_str("status").unwrap(), "PENDING_FINANCE_REVIEW");
        assert_eq!(order.get_str("review_status").unwrap(), "PENDING");
    })
}

#[tokio::test]
#[ignore]
async fn submit_is_idempotent_by_state_machine() {
    require_mongo!(async {
        let test_db = TestDb::new("po_api_idem").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let ctx = seed_context(test_db.db()).await;

        let order_id = create_purchase_order(&api, &token, &ctx).await;
        let (_, body) = api
            .get(&format!("/admin/purchase-orders/{order_id}"), Some(&token))
            .await;
        let (_, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/draft"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": body["data"]["version"],
                    "lines": [draft_line_payload(&ctx, None)],
                    "idempotency_key": "save-1",
                })),
            )
            .await;

        let (status, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/submit"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": body["data"]["lock_version"],
                    "idempotency_key": "submit-1",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);

        let (status, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/submit"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": 2,
                    "idempotency_key": "submit-1",
                })),
            )
            .await;
        assert_eq!(status, 409, "重复提交必须 409: {body}");

        // 只产生一条正式事实。
        let formal = test_db
            .db()
            .collection::<Document>("purchase_order_submissions")
            .count_documents(doc! { "purchase_order_id": &order_id, "submission_no": { "$regex": "^SUB-" } })
            .await
            .unwrap();
        assert_eq!(formal, 1, "重复提交只产生一条正式提交");
    })
}

#[tokio::test]
#[ignore]
async fn unauthenticated_request_returns_401() {
    require_mongo!(async {
        let test_db = TestDb::new("po_api_401").await.unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/purchase-orders", None).await;
        assert_eq!(status, 401, "未带 token 必须 401: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn account_without_domain_permission_returns_403() {
    require_mongo!(async {
        let test_db = TestDb::new("po_api_403").await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api.get("/admin/purchase-orders", Some(&token)).await;
        assert_eq!(status, 403, "无 purchase_order.list 权限必须 403: {body}");
        assert_eq!(body["success"], false);
    })
}

#[tokio::test]
#[ignore]
async fn invalid_request_body_returns_400() {
    require_mongo!(async {
        let test_db = TestDb::new("po_api_400").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .post(
                "/admin/purchase-orders",
                Some(&token),
                Some(json!({
                    "basis_id": "   ",
                    "purchase_type": "PHYSICAL",
                    "payment_term_code": "NET-30",
                    "idempotency_key": "k-1",
                })),
            )
            .await;
        assert_eq!(status, 400, "空白 basis_id 必须 400: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);

        let (status, _) = api
            .post(
                "/admin/purchase-orders",
                Some(&token),
                Some(json!({ "basis_id": "b-1" })),
            )
            .await;
        assert_eq!(status, 422, "缺少必填字段走 axum Json 拒绝");

        let (status, body) = api
            .get(
                "/admin/purchase-orders?sort_by=amount&sort_dir=desc",
                Some(&token),
            )
            .await;
        assert_eq!(status, 400, "非法排序字段必须 400: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_save_draft_returns_409() {
    require_mongo!(async {
        let test_db = TestDb::new("po_api_409").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let ctx = seed_context(test_db.db()).await;

        let order_id = create_purchase_order(&api, &token, &ctx).await;

        let (status, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/draft"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": 99,
                    "lines": [draft_line_payload(&ctx, None)],
                    "idempotency_key": "save-1",
                })),
            )
            .await;
        assert_eq!(status, 409, "陈旧版本保存必须 409: {body}");
        assert_eq!(body["success"], false);
        assert_eq!(body["data"], Value::Null);
    })
}

#[tokio::test]
#[ignore]
async fn pagination_and_sorting_boundaries() {
    require_mongo!(async {
        let test_db = TestDb::new("po_api_page").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);

        let (status, body) = api
            .get("/admin/purchase-orders?page=2&page_size=10", Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["page"], 2);
        assert_eq!(body["data"]["page_size"], 10);
        assert_eq!(body["data"]["total"], 0);

        let (status, body) = api
            .get("/admin/purchase-orders?page_size=1000", Some(&token))
            .await;
        assert_eq!(status, 400, "超界分页大小必须 400: {body}");

        let (status, body) = api.get("/admin/purchase-orders?page=0", Some(&token)).await;
        assert_eq!(status, 400, "非法页码必须 400: {body}");
    })
}

#[tokio::test]
#[ignore]
async fn reject_returns_order_to_draft() {
    require_mongo!(async {
        let test_db = TestDb::new("po_api_reject").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let ctx = seed_context(test_db.db()).await;

        let order_id = create_purchase_order(&api, &token, &ctx).await;
        let (_, body) = api
            .get(&format!("/admin/purchase-orders/{order_id}"), Some(&token))
            .await;
        let (_, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/draft"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": body["data"]["version"],
                    "lines": [draft_line_payload(&ctx, None)],
                    "idempotency_key": "save-1",
                })),
            )
            .await;
        let (_, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/submit"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": body["data"]["lock_version"],
                    "idempotency_key": "submit-1",
                })),
            )
            .await;
        let submission_id = body["data"]["submission_id"].as_str().unwrap().to_string();
        let work_item_id = body["data"]["work_item_id"].as_str().unwrap().to_string();
        let lock_version = body["data"]["lock_version"].as_u64().unwrap();

        let (status, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/review/reject"),
                Some(&token),
                Some(json!({
                    "submission_id": submission_id,
                    "work_item_id": work_item_id,
                    "expected_lock_version": lock_version,
                    "reason_code": "COST_TAX",
                    "comment": "成本与确认不一致",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["review_result"], "REJECTED");

        let (status, body) = api
            .get(&format!("/admin/purchase-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "DRAFT");
        assert_eq!(body["data"]["review_status"], "REJECTED");
    })
}

#[tokio::test]
#[ignore]
async fn change_order_start_submit_effect_flow() {
    require_mongo!(async {
        let test_db = TestDb::new("po_api_change").await.unwrap();
        database::ensure_indexes(test_db.db()).await.unwrap();
        let account_id = seed_admin_account(test_db.db()).await.unwrap();
        grant_domain_permissions(test_db.db(), &account_id).await;
        let token = mint_jwt(&account_id, TEST_JWT_SECRET, 3600).unwrap();
        let (router, _) = build_router(&test_db).await;
        let api = TestApi::new(router);
        let ctx = seed_context(test_db.db()).await;

        // 先走到 EFFECTIVE（简化：复用 create+submit+approve）。
        let order_id = create_purchase_order(&api, &token, &ctx).await;
        let (_, body) = api
            .get(&format!("/admin/purchase-orders/{order_id}"), Some(&token))
            .await;
        let (_, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/draft"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": body["data"]["version"],
                    "lines": [draft_line_payload(&ctx, None)],
                    "idempotency_key": "save-1",
                })),
            )
            .await;
        let (_, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/submit"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": body["data"]["lock_version"],
                    "idempotency_key": "submit-1",
                })),
            )
            .await;
        let submission_id = body["data"]["submission_id"].as_str().unwrap().to_string();
        let work_item_id = body["data"]["work_item_id"].as_str().unwrap().to_string();
        let lock_version = body["data"]["lock_version"].as_u64().unwrap();
        api.post(
            &format!("/admin/purchase-orders/{order_id}/review/approve"),
            Some(&token),
            Some(json!({
                "submission_id": submission_id,
                "work_item_id": work_item_id,
                "expected_lock_version": lock_version,
            })),
        )
        .await;

        // 发起变更。
        let (status, body) = api
            .post(
                &format!("/admin/purchase-orders/{order_id}/changes"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": 3,
                    "reason": "成本上涨调整",
                    "idempotency_key": "change-1",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let change_id = body["data"]["change_id"].as_str().unwrap().to_string();
        assert_eq!(body["data"]["base_revision_no"], 1);

        // 提交目标内容（数量 3 → 4，单价不变）。
        let mut line = draft_line_payload(&ctx, None);
        line["quantity"] = json!("4.000000");
        line["allocated_quantity"] = json!("4.000000");
        let (status, body) = api
            .post(
                &format!("/admin/purchase-change-orders/{change_id}/submit"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": 1,
                    "payment_term_code": "NET-30",
                    "lines": [line],
                    "idempotency_key": "change-submit-1",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        let change_submission_id = body["data"]["submission_id"].as_str().unwrap().to_string();
        let change_lock_version = body["data"]["lock_version"].as_u64().unwrap();

        // 生效（§8.1.3：新版本 + 应付差额 + 指针推进）。
        let (status, body) = api
            .post(
                &format!("/admin/purchase-change-orders/{change_id}/effect"),
                Some(&token),
                Some(json!({
                    "expected_lock_version": change_lock_version,
                    "submission_id": change_submission_id,
                    "idempotency_key": "change-effect-1",
                })),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["revision_no"], 2);
        assert!(body["data"]["payable_delta_entry_id"].as_str().is_some());

        let (status, body) = api
            .get(
                &format!("/admin/purchase-change-orders/{change_id}"),
                Some(&token),
            )
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["status"], "EFFECTIVE");
        assert!(body["data"]["effective_revision_id"].as_str().is_some());

        let (status, body) = api
            .get(&format!("/admin/purchase-orders/{order_id}"), Some(&token))
            .await;
        assert_ok_envelope(status, &body);
        assert_eq!(body["data"]["revision_no"], 2);
        assert_eq!(body["data"]["totals"]["gross"], "39.96", "4×9.99 变更后行汇总");
    })
}
