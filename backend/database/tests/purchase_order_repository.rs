//! 域 D15 `purchase_order` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test purchase_order_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::PurchaseOrderExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    ProcurementConfirmationLineId, PurchaseChangeOrderId, PurchaseChangeSubmissionId,
    PurchaseChangeSubmissionLineId, PurchaseOrderId, PurchaseOrderRevisionId, PurchaseOrderRevisionLineId,
    PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, SalesOrderId, SalesOrderRevisionLineId,
    SalesOrderSubmissionLineId, SkuId, SkuRevisionId, SupplierAccountId, SupplierCommercialProfileRevisionId,
};
use entities::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};
use entities::purchase_order::{
    FulfillmentResponsibility, PaymentTermSnapshot, ProgressStatus, PurchaseChangeOrder,
    PurchaseChangeOrderData, PurchaseChangeOrderStatus, PurchaseChangeOrderUpdate, PurchaseChangeSubmission,
    PurchaseChangeSubmissionData, PurchaseChangeSubmissionLine, PurchaseChangeSubmissionLineData,
    PurchaseLineSalesAllocation, PurchaseLineType, PurchaseOrder, PurchaseOrderData, PurchaseOrderRevision,
    PurchaseOrderRevisionData, PurchaseOrderRevisionLine, PurchaseOrderRevisionLineData, PurchaseOrderStatus,
    PurchaseOrderSubmission, PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine,
    PurchaseOrderSubmissionLineData, PurchaseOrderUpdate, PurchaseReviewStatus, PurchaseType,
    SubmissionStatus, SupplierSnapshot,
};
use futures_util::StreamExt;
use mongodb::{bson::doc, Database};
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 采购单列表筛选条件类型（经 `PurchaseOrderExt` 关联类型跨 crate 可达）。
type PurchaseOrderFilter = <Database as PurchaseOrderExt>::PurchaseOrderFilter;
/// 采购提交列表筛选条件类型。
type PurchaseOrderSubmissionFilter = <Database as PurchaseOrderExt>::PurchaseOrderSubmissionFilter;

/// 构造可复用的采购单实体。
fn sample_order(purchase_no: &str, supplier_id: &str) -> PurchaseOrder {
    PurchaseOrder::new(
        PurchaseOrderId::new(format!("po-{purchase_no}")),
        PurchaseOrderData {
            purchase_no: purchase_no.to_string(),
            sales_order_id: SalesOrderId::new(format!("so-{supplier_id}")),
            supplier_id: SupplierAccountId::new(supplier_id),
            purchase_type: PurchaseType::Physical,
            payment_term_code: "NET-30".to_string(),
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的采购提交实体（表头金额三元组守恒）。
fn sample_submission(purchase_order_id: &PurchaseOrderId, submission_no: &str) -> PurchaseOrderSubmission {
    PurchaseOrderSubmission::new(
        PurchaseOrderSubmissionId::new(format!("sub-{submission_no}")),
        PurchaseOrderSubmissionData {
            purchase_order_id: purchase_order_id.clone(),
            submission_no: submission_no.to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            purchase_type: PurchaseType::Physical,
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            supplier_revision_id: SupplierCommercialProfileRevisionId::new("spr-1"),
            supplier_snapshot: SupplierSnapshot::new("北京华联供应商".to_string()).unwrap(),
            payment_term_snapshot: PaymentTermSnapshot::new("NET-30".to_string(), false, None, None).unwrap(),
            gross_amount: Amount::from_str("29.97").unwrap(),
            net_amount: Amount::from_str("26.07").unwrap(),
            tax_amount: Amount::from_str("3.90").unwrap(),
        },
    )
    .unwrap()
}

/// 构造可复用的采购提交行实体（商品/服务行，含销售分配）。
fn sample_submission_line(
    submission_id: &PurchaseOrderSubmissionId,
    line_no: u32,
) -> PurchaseOrderSubmissionLine {
    let (gross, net, tax) = line_amounts(
        UnitPrice::from_str("9.9900").unwrap(),
        Quantity::from_str("3.000000").unwrap(),
        Rate::from_str("0.130000").unwrap(),
    );
    PurchaseOrderSubmissionLine::new(
        PurchaseOrderSubmissionLineId::new(format!("sl-{line_no}")),
        PurchaseOrderSubmissionLineData {
            purchase_order_submission_id: submission_id.clone(),
            line_no,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
            sku_id: Some(SkuId::new("sku-1")),
            sku_revision_id: Some(SkuRevisionId::new("skur-1")),
            product_name_snapshot: Some("慰问礼包".to_string()),
            specification_snapshot: Some("500g×2".to_string()),
            quantity: Some(Quantity::from_str("3.000000").unwrap()),
            base_unit_code: Some("箱".to_string()),
            unit_cost_gross: Some(UnitPrice::from_str("9.9900").unwrap()),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            expected_delivery_date: Some(BusinessDate::from_ymd(2026, 8, 6).unwrap()),
            sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new("ssl-1")),
            allocated_quantity: Some(Quantity::from_str("3.000000").unwrap()),
        },
    )
    .unwrap()
}

/// 构造可复用的采购生效版本与版本行。
fn sample_revision(purchase_order_id: &PurchaseOrderId) -> PurchaseOrderRevision {
    PurchaseOrderRevision::new(
        PurchaseOrderRevisionId::new(format!("por-{purchase_order_id}")),
        PurchaseOrderRevisionData {
            purchase_order_id: purchase_order_id.clone(),
            revision_no: 1,
            supplier_revision_id: SupplierCommercialProfileRevisionId::new("spr-1"),
            supplier_snapshot: SupplierSnapshot::new("北京华联供应商".to_string()).unwrap(),
            payment_term_snapshot: PaymentTermSnapshot::new("NET-30".to_string(), false, None, None).unwrap(),
            gross_amount: Amount::from_str("29.97").unwrap(),
            net_amount: Amount::from_str("26.07").unwrap(),
            tax_amount: Amount::from_str("3.90").unwrap(),
            effective_at: Instant::from_unix_secs(1_700_000_000),
        },
    )
    .unwrap()
}

fn sample_revision_line(revision_id: &PurchaseOrderRevisionId) -> PurchaseOrderRevisionLine {
    let (gross, net, tax) = line_amounts(
        UnitPrice::from_str("9.9900").unwrap(),
        Quantity::from_str("3.000000").unwrap(),
        Rate::from_str("0.130000").unwrap(),
    );
    PurchaseOrderRevisionLine::new(
        PurchaseOrderRevisionLineId::new(format!("porl-{revision_id}")),
        PurchaseOrderRevisionLineData {
            purchase_order_revision_id: revision_id.clone(),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
            sku_id: Some(SkuId::new("sku-1")),
            sku_revision_id: Some(SkuRevisionId::new("skur-1")),
            product_name_snapshot: Some("慰问礼包".to_string()),
            specification_snapshot: Some("500g×2".to_string()),
            quantity: Some(Quantity::from_str("3.000000").unwrap()),
            base_unit_code: Some("箱".to_string()),
            unit_cost_gross: Some(UnitPrice::from_str("9.9900").unwrap()),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
            expected_delivery_date: Some(BusinessDate::from_ymd(2026, 8, 6).unwrap()),
        },
    )
    .unwrap()
}

/// 构造可复用的采购行→销售行分配。
fn sample_allocation(purchase_line_id: &str, sales_line_id: &str) -> PurchaseLineSalesAllocation {
    PurchaseLineSalesAllocation {
        base: entity_core::BaseModel::new(format!("alloc-{purchase_line_id}-{sales_line_id}")),
        purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new(purchase_line_id),
        sales_order_revision_line_id: SalesOrderRevisionLineId::new(sales_line_id),
        allocated_quantity: Quantity::from_str("3.000000").unwrap(),
        allocated_cost_gross: Amount::from_str("29.97").unwrap(),
        allocated_cost_net: Amount::from_str("26.07").unwrap(),
    }
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as PurchaseOrderExt>::PURCHASE_ORDERS,
        &[
            "uk_purchase_orders_purchase_no",
            "idx_purchase_orders_supplier_status",
            "idx_purchase_orders_sales_status",
        ],
    )
    .await
    .expect("purchase_orders 索引缺失");
    assert_indexes(
        db,
        <Database as PurchaseOrderExt>::PURCHASE_ORDER_SUBMISSIONS,
        &[
            "uk_purchase_order_submissions_order_no",
            "idx_purchase_order_submissions_order_status_posted",
        ],
    )
    .await
    .expect("purchase_order_submissions 索引缺失");
    assert_indexes(
        db,
        <Database as PurchaseOrderExt>::PURCHASE_ORDER_SUBMISSION_LINES,
        &["uk_purchase_order_submission_lines_order_line"],
    )
    .await
    .expect("purchase_order_submission_lines 索引缺失");
    assert_indexes(
        db,
        <Database as PurchaseOrderExt>::PURCHASE_ORDER_REVISIONS,
        &["uk_purchase_order_revisions_order_no"],
    )
    .await
    .expect("purchase_order_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as PurchaseOrderExt>::PURCHASE_ORDER_REVISION_LINES,
        &["uk_purchase_order_revision_lines_revision_line"],
    )
    .await
    .expect("purchase_order_revision_lines 索引缺失");
    assert_indexes(
        db,
        <Database as PurchaseOrderExt>::PURCHASE_LINE_SALES_ALLOCATIONS,
        &[
            "uk_purchase_line_sales_allocations_link",
            "idx_purchase_line_sales_allocations_sales_line",
        ],
    )
    .await
    .expect("purchase_line_sales_allocations 索引缺失");
    assert_indexes(
        db,
        <Database as PurchaseOrderExt>::PURCHASE_CHANGE_ORDERS,
        &["idx_purchase_change_orders_order_status"],
    )
    .await
    .expect("purchase_change_orders 索引缺失");
    assert_indexes(
        db,
        <Database as PurchaseOrderExt>::PURCHASE_CHANGE_SUBMISSIONS,
        &["uk_purchase_change_submissions_order_no"],
    )
    .await
    .expect("purchase_change_submissions 索引缺失");
    assert_indexes(
        db,
        <Database as PurchaseOrderExt>::PURCHASE_CHANGE_SUBMISSION_LINES,
        &["uk_purchase_change_submission_lines_order_line"],
    )
    .await
    .expect("purchase_change_submission_lines 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_and_read_roundtrip_preserves_decimal_amounts() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_roundtrip").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("PO-2026-0001", "sup-1");
        db.purchase_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(order.base.version, 1);

        let mut submission = sample_submission(&order.base.id.clone().into(), "SUB-01");
        submission
            .submit(Instant::from_unix_secs(1_700_000_000), " buyer-1 ")
            .unwrap();
        db.purchase_order_submissions()
            .create(&submission, &mut NoTransaction)
            .await
            .unwrap();
        let line = sample_submission_line(&submission.base.id.clone().into(), 1);
        db.purchase_order_submission_lines()
            .create(&line, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .purchase_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.purchase_no, "PO-2026-0001");
        assert_eq!(found.stable.created_by, "admin-1");
        assert_eq!(found.stable.status(), PurchaseOrderStatus::Draft);
        assert_eq!(found.review_status, PurchaseReviewStatus::Pending);
        assert_eq!(found.payment_progress, ProgressStatus::None);

        let found_submission = db
            .purchase_order_submissions()
            .find_by_id(&submission.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("提交应可读回");
        assert_eq!(found_submission.submission_no, "SUB-01");
        assert_eq!(found_submission.gross_amount, Amount::from_str("29.97").unwrap());
        assert_eq!(found_submission.net_amount, Amount::from_str("26.07").unwrap());
        assert_eq!(found_submission.tax_amount, Amount::from_str("3.90").unwrap());
        assert_eq!(found_submission.status, SubmissionStatus::Pending);
        assert_eq!(
            found_submission.submitted_at,
            Some(Instant::from_unix_secs(1_700_000_000)),
            "Instant 时间字段必须往返一致"
        );
        assert_eq!(found_submission.submitted_by.as_deref(), Some("buyer-1"));

        let found_line = db
            .purchase_order_submission_lines()
            .find_by_id(&line.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("提交行应可读回");
        assert_eq!(found_line.gross_amount, Amount::from_str("29.97").unwrap());
        assert_eq!(
            found_line.expected_delivery_date,
            Some(BusinessDate::from_ymd(2026, 8, 6).unwrap()),
            "BusinessDate 字段必须往返一致"
        );
        assert_eq!(found_line.quantity, Some(Quantity::from_str("3.000000").unwrap()));
    })
}

#[tokio::test]
#[ignore]
async fn optimistic_lock_success_and_stale_version_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_optlock").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut order = sample_order("PO-2026-0002", "sup-1");
        db.purchase_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let mut stale = order.clone();

        order
            .update(
                PurchaseOrderUpdate {
                    payment_term_code: Some("PREPAY-30".to_string()),
                    purchase_type: None,
                    fulfillment_responsibility: None,
                },
                "admin-2",
            )
            .unwrap();
        db.purchase_orders()
            .update(&mut order, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(order.base.version, 2, "乐观锁成功后 version 递增");
        assert_eq!(order.payment_term_code, "PREPAY-30");
        stale
            .update(
                PurchaseOrderUpdate {
                    payment_term_code: Some("NET-60".to_string()),
                    purchase_type: None,
                    fulfillment_responsibility: None,
                },
                "admin-3",
            )
            .unwrap();
        let error = db
            .purchase_orders()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        assert_eq!(stale.base.version, 1, "CAS 失败不得改动内存版本");
    })
}

#[tokio::test]
#[ignore]
async fn soft_delete_and_restore_purchase_order() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_softdel").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut order = sample_order("PO-2026-0003", "sup-1");
        db.purchase_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();

        db.purchase_orders()
            .soft_delete(&mut order, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .purchase_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.purchase_orders()
            .restore(&mut order, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .purchase_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn unique_purchase_no_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("PO-2026-0004", "sup-1");
        db.purchase_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();

        let mut duplicate = sample_order("PO-2026-0004", "sup-2");
        duplicate.base.id = "po-dup".to_string();
        let error = db
            .purchase_orders()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复采购单号必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let found = db
            .purchase_orders()
            .find_by_purchase_no("PO-2026-0004", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按单号应命中首个写入");
        assert_eq!(found.stable.created_by, "admin-1");
    })
}

#[tokio::test]
#[ignore]
async fn submission_order_no_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_sub_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("PO-2026-0005", "sup-1");
        db.purchase_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let order_id = order.base.id.clone().into();

        let submission = sample_submission(&order_id, "SUB-01");
        db.purchase_order_submissions()
            .create(&submission, &mut NoTransaction)
            .await
            .unwrap();

        let mut duplicate = sample_submission(&order_id, "SUB-01");
        duplicate.base.id = "sub-dup".to_string();
        let error = db
            .purchase_order_submissions()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一采购单重复提交序号必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let other_order = sample_order("PO-2026-0006", "sup-1");
        let other_id = other_order.base.id.clone().into();
        let same_no_other_order = sample_submission(&other_id, "SUB-01");
        db.purchase_order_submissions()
            .create(&same_no_other_order, &mut NoTransaction)
            .await
            .unwrap();
    })
}

#[tokio::test]
#[ignore]
async fn list_search_respects_filters_pagination_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut supplier_two = sample_order("PO-2026-0011", "sup-2");
        supplier_two.purchase_type = PurchaseType::Service;
        db.purchase_orders()
            .create(&sample_order("PO-2026-0010", "sup-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.purchase_orders()
            .create(&supplier_two, &mut NoTransaction)
            .await
            .unwrap();
        db.purchase_orders()
            .create(&sample_order("PO-2026-0012", "sup-1"), &mut NoTransaction)
            .await
            .unwrap();

        let filter = PurchaseOrderFilter {
            purchase_no: None,
            sales_order_id: None,
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            status: Some(PurchaseOrderStatus::Draft),
            page: 1,
            page_size: 1,
            sort_by: Some("purchase_no".to_string()),
            sort_ascending: true,
        };
        let page = db
            .purchase_orders()
            .search_purchase_orders(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "sup-1 且草稿共两条");
        assert_eq!(page.items.len(), 1, "单页一条");
        let row = &page.items[0];
        assert_eq!(row.purchase_no, "PO-2026-0010", "按单号升序第一页应为最小单号");
        assert_eq!(row.supplier_id, SupplierAccountId::new("sup-1"));
        assert_eq!(row.status, PurchaseOrderStatus::Draft);
        assert_eq!(row.purchase_type, PurchaseType::Physical);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let page_two = db
            .purchase_orders()
            .search_purchase_orders(&PurchaseOrderFilter { page: 2, ..filter }, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page_two.items.len(), 1, "第二页一条");
        assert_eq!(page_two.items[0].purchase_no, "PO-2026-0012");

        let raw = db
            .collection::<mongodb::bson::Document>("purchase_orders")
            .find_one(doc! { "purchase_no": "PO-2026-0012" })
            .await
            .unwrap()
            .expect("原始文档应存在");
        assert!(
            raw.contains_key("payment_term_code"),
            "完整文档应包含列表投影外的字段（用于对照投影字段集）"
        );
        let mut cursor = db
            .collection::<mongodb::bson::Document>("purchase_orders")
            .find(doc! { "purchase_no": "PO-2026-0012" })
            .projection(doc! {
                "id": 1,
                "purchase_no": 1,
                "sales_order_id": 1,
                "supplier_id": 1,
                "purchase_type": 1,
                "status": 1,
                "review_status": 1,
                "payment_progress": 1,
                "invoice_progress": 1,
                "fulfillment_progress": 1,
                "current_submission_id": 1,
                "current_revision_id": 1,
                "version": 1,
                "created_at": 1,
            })
            .await
            .unwrap();
        let projected = cursor.next().await.unwrap().unwrap();
        let keys: Vec<&str> = projected.keys().map(String::as_str).collect();
        assert!(
            keys.contains(&"purchase_no") && keys.contains(&"current_submission_id"),
            "投影必须包含列表字段"
        );
        assert!(
            !keys.contains(&"payment_term_code"),
            "投影必须排除列表不需要的整文档字段"
        );
        assert!(!keys.contains(&"stable"), "StableBase 必须扁平化而非嵌套");
    })
}

#[tokio::test]
#[ignore]
async fn list_search_whitelists_sort_and_matches_regex() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_sort_regex").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.purchase_orders()
            .create(&sample_order("PO-2026-0021", "sup-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.purchase_orders()
            .create(&sample_order("PO-2026-0022", "sup-1"), &mut NoTransaction)
            .await
            .unwrap();

        let bogus_sort = PurchaseOrderFilter {
            purchase_no: Some("po-2026".to_string()),
            sales_order_id: None,
            supplier_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: Some("arbitrary_field".to_string()),
            sort_ascending: true,
        };
        let page = db
            .purchase_orders()
            .search_purchase_orders(&bogus_sort, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "单号正则（忽略大小写）命中两条");
        assert_eq!(
            page.items[0].purchase_no, "PO-2026-0021",
            "未知排序字段必须回退 created_at 升序（先建先出）"
        );
    })
}

#[tokio::test]
#[ignore]
async fn submission_queue_list_projects_amounts() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_queue").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("PO-2026-0031", "sup-1");
        db.purchase_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let order_id = order.base.id.clone().into();

        let mut pending = sample_submission(&order_id, "SUB-01");
        pending
            .submit(Instant::from_unix_secs(1_700_000_000), "buyer-1")
            .unwrap();
        db.purchase_order_submissions()
            .create(&pending, &mut NoTransaction)
            .await
            .unwrap();
        let mut rejected = sample_submission(&order_id, "SUB-02");
        rejected
            .submit(Instant::from_unix_secs(1_700_000_100), "buyer-1")
            .unwrap();
        rejected.mark_reviewed(false).unwrap();
        db.purchase_order_submissions()
            .create(&rejected, &mut NoTransaction)
            .await
            .unwrap();

        let filter = PurchaseOrderSubmissionFilter {
            purchase_order_id: Some(order_id.clone()),
            supplier_id: None,
            status: Some(SubmissionStatus::Pending),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let page = db
            .purchase_order_submissions()
            .search_purchase_order_submissions(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "仅待审核提交入队");
        let row = &page.items[0];
        assert_eq!(row.submission_no, "SUB-01");
        assert_eq!(row.status, SubmissionStatus::Pending);
        assert_eq!(row.gross_amount, Amount::from_str("29.97").unwrap());
        assert_eq!(row.net_amount, Amount::from_str("26.07").unwrap());
        assert_eq!(row.tax_amount, Amount::from_str("3.90").unwrap());
        assert_eq!(row.submitted_at, Some(Instant::from_unix_secs(1_700_000_000)));
    })
}

#[tokio::test]
#[ignore]
async fn allocation_bidirectional_batch_queries() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_alloc").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.purchase_line_sales_allocations()
            .create(&sample_allocation("porl-1", "sorl-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.purchase_line_sales_allocations()
            .create(&sample_allocation("porl-2", "sorl-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.purchase_line_sales_allocations()
            .create(&sample_allocation("porl-3", "sorl-2"), &mut NoTransaction)
            .await
            .unwrap();

        let by_sales = db
            .purchase_line_sales_allocations()
            .find_by_sales_revision_line_ids(&[SalesOrderRevisionLineId::new("sorl-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_sales.len(), 2, "同一销售行被两个采购行分配");

        let by_purchase = db
            .purchase_line_sales_allocations()
            .find_by_purchase_revision_line_ids(
                &[
                    PurchaseOrderRevisionLineId::new("porl-1"),
                    PurchaseOrderRevisionLineId::new("porl-3"),
                ],
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(by_purchase.len(), 2, "两个采购行各一条分配");

        let empty = db
            .purchase_line_sales_allocations()
            .find_by_purchase_revision_line_ids(&[], &mut NoTransaction)
            .await
            .unwrap();
        assert!(empty.is_empty(), "空 ID 集合直接返回空结果");
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_submission_commits_atomically_in_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("PO-2026-0041", "sup-1");
        db.purchase_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let order_id = order.base.id.clone().into();
        let submission = sample_submission(&order_id, "SUB-01");
        let lines = vec![sample_submission_line(&submission.base.id.clone().into(), 1)];

        let db_clone = db.clone();
        let mut order_for_tx = order.clone();
        let submission_for_tx = submission.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    order_for_tx
                        .submit_for_review(submission_for_tx.base.id.clone(), "buyer-1")
                        .expect("草稿可提交");
                    db_clone
                        .purchase_order()
                        .create_purchase_submission(&mut order_for_tx, &submission_for_tx, &lines, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let order_found = db
            .purchase_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("采购单应可见");
        assert_eq!(
            order_found.stable.status(),
            PurchaseOrderStatus::PendingFinanceReview
        );
        assert_eq!(
            order_found.current_submission_id.as_deref(),
            Some(submission.base.id.as_str())
        );
        let submission_found = db
            .purchase_order_submissions()
            .find_by_id(&submission.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(submission_found.is_some(), "事务提交后提交必须可见");
        let lines_found = db
            .purchase_order_submission_lines()
            .find_lines_by_submission_ids(&[submission.base.id.clone().into()], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(lines_found.len(), 1, "事务提交后明细必须可见");
        assert_eq!(lines_found[0].line_no, 1);
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_conflict_rolls_back_whole_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_tx_rollback").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("PO-2026-0042", "sup-1");
        db.purchase_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let order_id = order.base.id.clone().into();
        let submission = sample_submission(&order_id, "SUB-01");
        let lines = vec![sample_submission_line(&submission.base.id.clone().into(), 1)];

        let mut stale = order.clone();
        stale
            .update(
                PurchaseOrderUpdate {
                    payment_term_code: Some("NET-60".to_string()),
                    purchase_type: None,
                    fulfillment_responsibility: None,
                },
                "admin-2",
            )
            .unwrap();
        db.purchase_orders()
            .update(&mut stale, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(stale.base.version, 2);

        let db_clone = db.clone();
        let submission_for_tx = submission.clone();
        let mut order_for_tx = order.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    order_for_tx
                        .submit_for_review(submission_for_tx.base.id.clone(), "buyer-1")
                        .expect("草稿可提交");
                    let error = db_clone
                        .purchase_order()
                        .create_purchase_submission(&mut order_for_tx, &submission_for_tx, &lines, session)
                        .await
                        .expect_err("陈旧版本 CAS 必须失败");
                    assert!(
                        matches!(error, database::Error::OptimisticLockingError),
                        "期望 OptimisticLockingError，实际为 {error:?}"
                    );
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let submission_found = db
            .purchase_order_submissions()
            .find_by_id(&submission.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(submission_found.is_none(), "回滚后提交不得残留");
        let lines_found = db
            .purchase_order_submission_lines()
            .find_lines_by_submission_ids(&[submission.base.id.clone().into()], &mut NoTransaction)
            .await
            .unwrap();
        assert!(lines_found.is_empty(), "回滚后明细不得残留");

        let order_found = db
            .purchase_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("采购单仍应存在");
        assert_eq!(order_found.base.version, 2, "主表保持事务外最后版本");
        assert_eq!(
            order_found.stable.status(),
            PurchaseOrderStatus::Draft,
            "主表状态不得被半成品改动"
        );
        assert_eq!(order_found.payment_term_code, "NET-60");
        assert!(order_found.current_submission_id.is_none());
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_without_transaction_is_predictable() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_no_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("PO-2026-0043", "sup-1");
        db.purchase_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let order_id = order.base.id.clone().into();
        let submission = sample_submission(&order_id, "SUB-01");
        let lines = vec![sample_submission_line(&submission.base.id.clone().into(), 1)];

        let mut stale = order.clone();
        stale
            .update(
                PurchaseOrderUpdate {
                    payment_term_code: Some("NET-60".to_string()),
                    purchase_type: None,
                    fulfillment_responsibility: None,
                },
                "admin-2",
            )
            .unwrap();
        db.purchase_orders()
            .update(&mut stale, &mut NoTransaction)
            .await
            .unwrap();

        let mut order_no_tx = order.clone();
        order_no_tx
            .submit_for_review(submission.base.id.clone(), "buyer-1")
            .unwrap();
        let error = db
            .purchase_order()
            .create_purchase_submission(&mut order_no_tx, &submission, &lines, &mut NoTransaction)
            .await
            .expect_err("无事务时陈旧版本 CAS 同样失败");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );

        let submission_found = db
            .purchase_order_submissions()
            .find_by_id(&submission.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            submission_found.is_some(),
            "NoTransaction 下提交已自动提交（可预期半成品）"
        );
        let order_found = db
            .purchase_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("采购单仍应存在");
        assert_eq!(
            order_found.stable.status(),
            PurchaseOrderStatus::Draft,
            "主表 CAS 失败，指针未被更新"
        );
    })
}

#[tokio::test]
#[ignore]
async fn effective_revision_creation_is_atomic_with_lines() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_revision_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("PO-2026-0044", "sup-1");
        db.purchase_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let order_id = order.base.id.clone().into();
        let revision = sample_revision(&order_id);
        let revision_id = revision.base.id.clone().into();
        let line = sample_revision_line(&revision_id);

        let db_clone = db.clone();
        let revision_for_tx = revision.clone();
        let line_for_tx = line.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .purchase_order()
                        .create_effective_revision(&revision_for_tx, &[line_for_tx], session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let revision_found = db
            .purchase_order_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("版本应可见");
        assert_eq!(revision_found.revision.revision_no, 1);
        assert_eq!(
            revision_found.effective_at,
            Instant::from_unix_secs(1_700_000_000)
        );

        let lines_found = db
            .purchase_order_revision_lines()
            .find_lines_by_revision_ids(&[revision_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(lines_found.len(), 1, "版本明细必须原子可见");
        assert_eq!(lines_found[0].gross_amount, Amount::from_str("29.97").unwrap());

        let mut duplicate = sample_revision(&order_id);
        duplicate.base.id = "por-dup".to_string();
        let db_clone = db.clone();
        let duplicate_for_tx = duplicate.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .purchase_order()
                        .create_effective_revision(&duplicate_for_tx, &[], session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(result.is_err(), "版本号重复必须整体回滚");
        let duplicate_found = db
            .purchase_order_revisions()
            .find_by_id(&duplicate.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(duplicate_found.is_none(), "重复版本号写入不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn change_order_crud_and_multi_step_change_submission() {
    require_mongo!(async {
        let test_db = TestDb::new("po_repo_change").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("PO-2026-0045", "sup-1");
        db.purchase_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let order_id = order.base.id.clone().into();
        let revision = sample_revision(&order_id);
        db.purchase_order_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .unwrap();

        let mut change_order = PurchaseChangeOrder::new(
            PurchaseChangeOrderId::new("pco-1"),
            PurchaseChangeOrderData {
                purchase_order_id: order_id.clone(),
                base_revision_id: revision.base.id.clone().into(),
                reason: "成本上涨调整".to_string(),
            },
            "admin-1",
        )
        .unwrap();
        db.purchase_change_orders()
            .create(&change_order, &mut NoTransaction)
            .await
            .unwrap();

        let change_submission = PurchaseChangeSubmission::new(
            PurchaseChangeSubmissionId::new("pcs-1"),
            PurchaseChangeSubmissionData {
                purchase_change_order_id: change_order.base.id.clone().into(),
                submission_no: "CS-01".to_string(),
                base_revision_id: revision.base.id.clone().into(),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                supplier_revision_id: SupplierCommercialProfileRevisionId::new("spr-1"),
                supplier_snapshot: SupplierSnapshot::new("北京华联供应商".to_string()).unwrap(),
                payment_term_snapshot: PaymentTermSnapshot::new("NET-30".to_string(), false, None, None)
                    .unwrap(),
                gross_amount: Amount::from_str("29.97").unwrap(),
                net_amount: Amount::from_str("26.07").unwrap(),
                tax_amount: Amount::from_str("3.90").unwrap(),
            },
        )
        .unwrap();
        let change_submission_id = change_submission.base.id.clone();
        let (gross, net, tax) = line_amounts(
            UnitPrice::from_str("9.9900").unwrap(),
            Quantity::from_str("3.000000").unwrap(),
            Rate::from_str("0.130000").unwrap(),
        );
        let change_line = PurchaseChangeSubmissionLine::new(
            PurchaseChangeSubmissionLineId::new("pcsl-1"),
            PurchaseChangeSubmissionLineData {
                purchase_change_submission_id: change_submission_id.clone().into(),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: Some(SkuRevisionId::new("skur-1")),
                product_name_snapshot: Some("慰问礼包".to_string()),
                specification_snapshot: Some("500g×2".to_string()),
                quantity: Some(Quantity::from_str("3.000000").unwrap()),
                base_unit_code: Some("箱".to_string()),
                unit_cost_gross: Some(UnitPrice::from_str("9.9900").unwrap()),
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
                input_tax_rate: Some(Rate::from_str("0.130000").unwrap()),
                expected_delivery_date: Some(BusinessDate::from_ymd(2026, 8, 6).unwrap()),
                sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new("ssl-1")),
                allocated_quantity: Some(Quantity::from_str("3.000000").unwrap()),
            },
        )
        .unwrap();

        change_order
            .update(
                PurchaseChangeOrderUpdate {
                    current_submission_id: Some(change_submission.base.id.clone().into()),
                    target_content_hash: Some("hash-1".to_string()),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();

        let db_clone = db.clone();
        let mut change_for_tx = change_order.clone();
        let submission_for_tx = change_submission.clone();
        let line_for_tx = change_line.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .purchase_order()
                        .create_change_submission(
                            &mut change_for_tx,
                            &submission_for_tx,
                            &[line_for_tx],
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let change_found = db
            .purchase_change_orders()
            .find_by_id(&change_order.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("变更单应可见");
        assert_eq!(change_found.stable.status(), PurchaseChangeOrderStatus::Draft);
        assert_eq!(change_found.current_submission_id.as_deref(), Some("pcs-1"));
        assert_eq!(change_found.target_content_hash.as_deref(), Some("hash-1"));

        let by_order = db
            .purchase_change_orders()
            .find_by_purchase_order_id(&order_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_order.len(), 1, "按采购单取回变更单");

        let change_lines = db
            .purchase_change_submission_lines()
            .find_lines_by_submission_ids(&[change_submission_id.clone().into()], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(change_lines.len(), 1, "变更提交明细必须可见");

        let duplicate = PurchaseChangeSubmission::new(
            PurchaseChangeSubmissionId::new("pcs-dup"),
            PurchaseChangeSubmissionData {
                purchase_change_order_id: change_order.base.id.clone().into(),
                submission_no: "CS-01".to_string(),
                base_revision_id: revision.base.id.clone().into(),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                supplier_revision_id: SupplierCommercialProfileRevisionId::new("spr-1"),
                supplier_snapshot: SupplierSnapshot::new("北京华联供应商".to_string()).unwrap(),
                payment_term_snapshot: PaymentTermSnapshot::new("NET-30".to_string(), false, None, None)
                    .unwrap(),
                gross_amount: Amount::from_str("29.97").unwrap(),
                net_amount: Amount::from_str("26.07").unwrap(),
                tax_amount: Amount::from_str("3.90").unwrap(),
            },
        )
        .unwrap();
        let error = db
            .purchase_change_submissions()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一变更单重复提交序号必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}
