//! 域 D21 `returns` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test returns_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。
//!
//! 本域全部集合是退货/退款/冲正事实与处理单（§4.5），**不提供软删除**；
//! 处理单（`sales_return_case`、`purchase_return_order`）复用基类乐观锁更新。

use database::repository::extensions::ReturnsExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    CustomerAccountId, CustomerReceiptId, PurchaseOrderId, PurchaseOrderRevisionLineId,
    PurchaseReturnOrderId, SalesOrderId, SalesOrderLineId, SalesReturnCaseId, SupplierPaymentId, WarehouseId,
};
use entities::money::{Amount, Quantity};
use entities::returns::{
    CaseType, CustomerRefund, CustomerRefundData, CustomerRefundStatus, PaymentReversal, PaymentReversalData,
    PurchaseReturnLine, PurchaseReturnLineData, PurchaseReturnOrder, PurchaseReturnOrderData,
    PurchaseReturnStatus, ReceiptReversal, ReceiptReversalData, ReturnMode, ReturnRoute, SalesReturnCase,
    SalesReturnCaseData, SalesReturnCaseStatus, SalesReturnLine, SalesReturnLineData, SupplierRefund,
    SupplierRefundData,
};
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 销售退货处理单列表筛选条件类型（经 `ReturnsExt` 关联类型跨 crate 可达）。
type SalesReturnCaseFilter = <Database as ReturnsExt>::SalesReturnCaseFilter;
/// 采购退货单列表筛选条件类型。
type PurchaseReturnOrderFilter = <Database as ReturnsExt>::PurchaseReturnOrderFilter;
/// 客户退款列表筛选条件类型。
type CustomerRefundFilter = <Database as ReturnsExt>::CustomerRefundFilter;

fn qty(value: &str) -> Quantity {
    Quantity::from_str(value).unwrap()
}

/// 构造可复用的销售退货处理单。
fn sample_case(no: &str) -> SalesReturnCase {
    SalesReturnCase::new(
        SalesReturnCaseId::new(format!("src-{no}")),
        SalesReturnCaseData {
            return_no: no.to_string(),
            sales_order_id: SalesOrderId::new("so-1"),
            acceptance_id: None,
            case_type: CaseType::Return,
            reason: "商品破损".to_string(),
            discovered_at: Instant::from_unix_secs(1_700_000_000),
            return_route: ReturnRoute::CompanyWarehouse,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的销售退货明细。
fn sample_sales_line(case_id: &SalesReturnCaseId) -> SalesReturnLine {
    SalesReturnLine::new(
        entities::ids::SalesReturnLineId::new(format!("srl-{case_id}")),
        SalesReturnLineData {
            sales_return_case_id: case_id.clone(),
            sales_order_line_id: SalesOrderLineId::new("so-1-l1"),
            requested_quantity: qty("10.000000"),
            received_quantity: None,
            quality_result: None,
            restockable_quantity: None,
        },
    )
    .unwrap()
}

/// 构造可复用的采购退货单。
fn sample_purchase_order(no: &str) -> PurchaseReturnOrder {
    PurchaseReturnOrder::new(
        PurchaseReturnOrderId::new(format!("pro-{no}")),
        PurchaseReturnOrderData {
            purchase_return_no: no.to_string(),
            purchase_order_id: PurchaseOrderId::new("po-1"),
            sales_return_case_id: Some(SalesReturnCaseId::new("src-1")),
            return_mode: ReturnMode::CompanyWarehouseToSupplier,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的采购退货明细。
fn sample_purchase_line(order_id: &PurchaseReturnOrderId) -> PurchaseReturnLine {
    PurchaseReturnLine::new(
        entities::ids::PurchaseReturnLineId::new(format!("prl-{order_id}")),
        PurchaseReturnLineData {
            purchase_return_order_id: order_id.clone(),
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("po-1-r1-l1"),
            return_quantity: qty("5.000000"),
            warehouse_id: Some(WarehouseId::new("wh-1")),
        },
    )
    .unwrap()
}

/// 构造可复用的客户退款（原回款引用）。
fn sample_customer_refund(no: &str) -> CustomerRefund {
    CustomerRefund::new(
        entities::ids::CustomerRefundId::new(format!("crf-{no}")),
        CustomerRefundData {
            refund_no: no.to_string(),
            sales_return_case_id: Some(SalesReturnCaseId::new("src-1")),
            customer_id: CustomerAccountId::new("cust-1"),
            original_receipt_id: Some(CustomerReceiptId::new("cr-1")),
            original_receivable_entry_id: None,
            reason_code: Some("QUALITY".to_string()),
            reason_text: "商品破损退款".to_string(),
            amount: Amount::from_str("1000.00").unwrap(),
            handled_by: "handler-1".to_string(),
            reviewed_by: "reviewer-1".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            evidence_attachment_id: None,
        },
    )
    .unwrap()
}

/// 构造可复用的供应商退款（原付款引用）。
fn sample_supplier_refund(no: &str) -> SupplierRefund {
    SupplierRefund::new(
        entities::ids::SupplierRefundId::new(format!("srf-{no}")),
        SupplierRefundData {
            refund_no: no.to_string(),
            purchase_return_order_id: Some(PurchaseReturnOrderId::new("pro-1")),
            supplier_id: entities::ids::SupplierAccountId::new("sup-1"),
            original_payment_id: Some(SupplierPaymentId::new("sp-1")),
            original_payable_entry_id: None,
            reason_code: Some("OVERPAY".to_string()),
            reason_text: "错付款退回".to_string(),
            amount: Amount::from_str("1000.00").unwrap(),
            handled_by: "handler-1".to_string(),
            reviewed_by: "reviewer-1".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            evidence_attachment_id: None,
        },
    )
    .unwrap()
}

/// 构造可复用的回款冲正单。
fn sample_receipt_reversal(no: &str) -> ReceiptReversal {
    ReceiptReversal::new(
        entities::ids::ReceiptReversalId::new(format!("rr-{no}")),
        ReceiptReversalData {
            reversal_no: no.to_string(),
            original_customer_receipt_id: CustomerReceiptId::new("cr-1"),
            reason_code: Some("WRONG_ACCOUNT".to_string()),
            reason_text: "错记回款冲正".to_string(),
            amount: Amount::from_str("1000.00").unwrap(),
            handled_by: "handler-1".to_string(),
            reviewed_by: "reviewer-1".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            evidence_attachment_id: None,
        },
    )
    .unwrap()
}

/// 构造可复用的付款冲正单。
fn sample_payment_reversal(no: &str) -> PaymentReversal {
    PaymentReversal::new(
        entities::ids::PaymentReversalId::new(format!("prr-{no}")),
        PaymentReversalData {
            reversal_no: no.to_string(),
            original_supplier_payment_id: SupplierPaymentId::new("sp-1"),
            reason_code: Some("WRONG_ACCOUNT".to_string()),
            reason_text: "错付款冲正".to_string(),
            amount: Amount::from_str("1000.00").unwrap(),
            handled_by: "handler-1".to_string(),
            reviewed_by: "reviewer-1".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            evidence_attachment_id: None,
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as ReturnsExt>::SALES_RETURN_CASES,
        &[
            "uk_sales_return_cases_no",
            "idx_sales_return_cases_order_status",
            "idx_sales_return_cases_status",
        ],
    )
    .await
    .expect("sales_return_cases 索引缺失");
    assert_indexes(
        db,
        <Database as ReturnsExt>::SALES_RETURN_LINES,
        &["idx_sales_return_lines_case", "idx_sales_return_lines_order_line"],
    )
    .await
    .expect("sales_return_lines 索引缺失");
    assert_indexes(
        db,
        <Database as ReturnsExt>::PURCHASE_RETURN_ORDERS,
        &[
            "uk_purchase_return_orders_no",
            "idx_purchase_return_orders_po_status",
        ],
    )
    .await
    .expect("purchase_return_orders 索引缺失");
    assert_indexes(
        db,
        <Database as ReturnsExt>::PURCHASE_RETURN_LINES,
        &[
            "idx_purchase_return_lines_order",
            "idx_purchase_return_lines_rev_line",
        ],
    )
    .await
    .expect("purchase_return_lines 索引缺失");
    assert_indexes(
        db,
        <Database as ReturnsExt>::CUSTOMER_REFUNDS,
        &[
            "uk_customer_refunds_no",
            "idx_customer_refunds_customer_status",
            "idx_customer_refunds_original",
        ],
    )
    .await
    .expect("customer_refunds 索引缺失");
    assert_indexes(
        db,
        <Database as ReturnsExt>::SUPPLIER_REFUNDS,
        &[
            "uk_supplier_refunds_no",
            "idx_supplier_refunds_supplier_status",
            "idx_supplier_refunds_original",
        ],
    )
    .await
    .expect("supplier_refunds 索引缺失");
    assert_indexes(
        db,
        <Database as ReturnsExt>::RECEIPT_REVERSALS,
        &["uk_receipt_reversals_no", "idx_receipt_reversals_original"],
    )
    .await
    .expect("receipt_reversals 索引缺失");
    assert_indexes(
        db,
        <Database as ReturnsExt>::PAYMENT_REVERSALS,
        &["uk_payment_reversals_no", "idx_payment_reversals_original"],
    )
    .await
    .expect("payment_reversals 索引缺失");
}

#[tokio::test]
#[ignore]
async fn case_create_read_roundtrip_preserves_fields_and_quantity() {
    require_mongo!(async {
        let test_db = TestDb::new("ret_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let case = sample_case("RT-1");
        let case_id: SalesReturnCaseId = case.base.id.clone().into();
        db.sales_return_cases()
            .create(&case, &mut NoTransaction)
            .await
            .unwrap();
        let line = sample_sales_line(&case_id);
        db.sales_return_lines()
            .create(&line, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .sales_return_cases()
            .find_by_id(&case.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.return_no, "RT-1");
        assert_eq!(found.reason, "商品破损");
        assert_eq!(found.stable.status(), SalesReturnCaseStatus::Draft);
        assert_eq!(
            found.discovered_at,
            Instant::from_unix_secs(1_700_000_000),
            "发现时间必须往返一致"
        );

        let found_line = db
            .sales_return_lines()
            .find_by_id(&line.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("明细应可读回");
        assert_eq!(found_line.requested_quantity, qty("10.000000"));
        assert!(found_line.received_quantity.is_none());
    })
}

#[tokio::test]
#[ignore]
async fn unique_numbers_conflict_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("ret_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let case = sample_case("RT-1");
        db.sales_return_cases()
            .create(&case, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_case = sample_case("RT-1");
        let error = db
            .sales_return_cases()
            .create(&duplicate_case, &mut NoTransaction)
            .await
            .expect_err("重复退货处理号必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let order = sample_purchase_order("PRT-1");
        db.purchase_return_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_order = sample_purchase_order("PRT-1");
        let error = db
            .purchase_return_orders()
            .create(&duplicate_order, &mut NoTransaction)
            .await
            .expect_err("重复采购退货单号必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let refund = sample_customer_refund("RF-1");
        db.customer_refunds()
            .create(&refund, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_refund = sample_customer_refund("RF-1");
        let error = db
            .customer_refunds()
            .create(&duplicate_refund, &mut NoTransaction)
            .await
            .expect_err("重复退款单号必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let reversal = sample_receipt_reversal("RR-1");
        db.receipt_reversals()
            .create(&reversal, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_reversal = sample_receipt_reversal("RR-1");
        let error = db
            .receipt_reversals()
            .create(&duplicate_reversal, &mut NoTransaction)
            .await
            .expect_err("重复冲正单号必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));
    })
}

#[tokio::test]
#[ignore]
async fn optimistic_lock_success_increments_version_and_stale_fails() {
    require_mongo!(async {
        let test_db = TestDb::new("ret_optlock").await.unwrap();
        let db = test_db.db();

        let mut case = sample_case("RT-1");
        db.sales_return_cases()
            .create(&case, &mut NoTransaction)
            .await
            .unwrap();

        let mut stale = case.clone();
        case.update(
            entities::returns::SalesReturnCaseUpdate {
                reason: Some(" 客户拒收 ".to_string()),
                status: Some(SalesReturnCaseStatus::Processing),
                ..Default::default()
            },
            "admin-2",
        )
        .unwrap();
        db.sales_return_cases()
            .update(&mut case, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(case.base.version, 2, "乐观锁成功后 version 递增");

        stale
            .update(
                entities::returns::SalesReturnCaseUpdate {
                    reason: Some("陈旧版本更新".to_string()),
                    ..Default::default()
                },
                "admin-3",
            )
            .unwrap();
        let error = db
            .sales_return_cases()
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
async fn sales_multi_step_commits_and_rolls_back_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("ret_sales_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let case = sample_case("RT-1");
        let line = sample_sales_line(&case.base.id.clone().into());
        let db_clone = db.clone();
        let case_for_tx = case.clone();
        let line_for_tx = line.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .returns()
                        .create_sales_return_with_line(&case_for_tx, &line_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");
        assert!(db
            .sales_return_cases()
            .find_by_id(&case.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
        assert!(db
            .sales_return_lines()
            .find_by_id(&line.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());

        let case2 = sample_case("RT-2");
        let line2 = sample_sales_line(&case2.base.id.clone().into());
        let db_clone = db.clone();
        let case_for_tx = case2.clone();
        let line_for_tx = line2.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .returns()
                        .create_sales_return_with_line(&case_for_tx, &line_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");
        assert!(
            db.sales_return_cases()
                .find_by_id(&case2.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后处理单不得残留"
        );
        assert!(
            db.sales_return_lines()
                .find_by_id(&line2.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后明细不得残留"
        );
    })
}

#[tokio::test]
#[ignore]
async fn purchase_multi_step_commits_and_rolls_back_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("ret_purchase_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_purchase_order("PRT-1");
        let line = sample_purchase_line(&order.base.id.clone().into());
        let db_clone = db.clone();
        let order_for_tx = order.clone();
        let line_for_tx = line.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .returns()
                        .create_purchase_return_with_line(&order_for_tx, &line_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");
        assert!(db
            .purchase_return_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
        assert!(db
            .purchase_return_lines()
            .find_by_id(&line.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());

        let order2 = sample_purchase_order("PRT-2");
        let line2 = sample_purchase_line(&order2.base.id.clone().into());
        let db_clone = db.clone();
        let order_for_tx = order2.clone();
        let line_for_tx = line2.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .returns()
                        .create_purchase_return_with_line(&order_for_tx, &line_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");
        assert!(
            db.purchase_return_orders()
                .find_by_id(&order2.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后退货单不得残留"
        );
        assert!(
            db.purchase_return_lines()
                .find_by_id(&line2.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后明细不得残留"
        );
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_without_transaction_is_predictable() {
    require_mongo!(async {
        let test_db = TestDb::new("ret_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let case = sample_case("RT-1");
        let line = sample_sales_line(&case.base.id.clone().into());
        db.returns()
            .create_sales_return_with_line(&case, &line, &mut NoTransaction)
            .await
            .expect("NoTransaction 下每笔写入各自自动提交");
        assert!(db
            .sales_return_cases()
            .find_by_id(&case.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
        assert!(
            db.sales_return_lines()
                .find_by_id(&line.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "无事务时两笔均自动提交可见（Service 必须传事务保证原子性）"
        );

        let duplicate_case = sample_case("RT-1");
        let mut dangling_line = sample_sales_line(&duplicate_case.base.id.clone().into());
        dangling_line.base.id = "dangling-line".to_string();
        let error = db
            .returns()
            .create_sales_return_with_line(&duplicate_case, &dangling_line, &mut NoTransaction)
            .await
            .expect_err("首笔写入违反编号唯一索引时透出 DuplicateKey");
        assert!(matches!(error, database::Error::DuplicateKey(_)));
        assert!(
            db.sales_return_lines()
                .find_by_id(&dangling_line.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "首笔失败时后续写入不执行，无可预期的半成品残留"
        );
    })
}

#[tokio::test]
#[ignore]
async fn case_list_respects_pagination_sort_whitelist_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("ret_case_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        for seq in 1..=3 {
            let mut case = sample_case(&format!("RT-2026-{seq}"));
            case.discovered_at = Instant::from_unix_secs(1_700_000_000 + seq);
            db.sales_return_cases()
                .create(&case, &mut NoTransaction)
                .await
                .unwrap();
        }

        let filter = SalesReturnCaseFilter {
            return_no: Some("RT-2026".to_string()),
            sales_order_id: Some(SalesOrderId::new("so-1")),
            status: None,
            page: 2,
            page_size: 2,
            sort_by: Some("discovered_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .sales_return_cases()
            .search_sales_return_cases(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 1, "第二页只应剩一条");
        assert_eq!(page.items[0].return_no, "RT-2026-3");

        let whitelist_fallback = SalesReturnCaseFilter {
            return_no: None,
            sales_order_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: Some("$where".to_string()),
            sort_ascending: false,
        };
        let page = db
            .sales_return_cases()
            .search_sales_return_cases(&whitelist_fallback, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 3, "非白名单排序回退 created_at 降序");
        let row = &page.items[0];
        assert_eq!(row.sales_order_id, "so-1");
        assert_eq!(row.case_type, CaseType::Return);
        assert_eq!(row.return_route, ReturnRoute::CompanyWarehouse);
        assert_eq!(row.stable.status(), SalesReturnCaseStatus::Draft);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let found = db
            .sales_return_cases()
            .find_by_return_no("RT-2026-2", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按处理号应可读回");
        assert_eq!(found.reason, "商品破损");
    })
}

#[tokio::test]
#[ignore]
async fn refund_and_return_order_lists_filter_and_project() {
    require_mongo!(async {
        let test_db = TestDb::new("ret_refund_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_purchase_order("PRT-2026-001");
        db.purchase_return_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        db.purchase_return_orders()
            .create(&sample_purchase_order("PRT-2026-002"), &mut NoTransaction)
            .await
            .unwrap();

        let mut posted = sample_customer_refund("RF-2026-001");
        posted.transition(CustomerRefundStatus::PendingReview).unwrap();
        posted.transition(CustomerRefundStatus::Posted).unwrap();
        db.customer_refunds()
            .create(&posted, &mut NoTransaction)
            .await
            .unwrap();
        db.customer_refunds()
            .create(&sample_customer_refund("RF-2026-002"), &mut NoTransaction)
            .await
            .unwrap();

        let po_filter = PurchaseReturnOrderFilter {
            purchase_return_no: Some("PRT-2026".to_string()),
            purchase_order_id: Some(PurchaseOrderId::new("po-1")),
            status: None,
            page: 1,
            page_size: 20,
            sort_by: Some("purchase_return_no".to_string()),
            sort_ascending: true,
        };
        let page = db
            .purchase_return_orders()
            .search_purchase_return_orders(&po_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2);
        let row = &page.items[0];
        assert_eq!(row.purchase_return_no, "PRT-2026-001");
        assert_eq!(row.return_mode, ReturnMode::CompanyWarehouseToSupplier);
        assert_eq!(row.stable.status(), PurchaseReturnStatus::Draft);

        let refund_filter = CustomerRefundFilter {
            refund_no: Some("RF-2026".to_string()),
            customer_id: Some(CustomerAccountId::new("cust-1")),
            status: Some(CustomerRefundStatus::Posted),
            page: 1,
            page_size: 20,
            sort_by: Some("occurred_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .customer_refunds()
            .search_customer_refunds(&refund_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        let row = &page.items[0];
        assert_eq!(row.refund_no, "RF-2026-001");
        assert_eq!(row.status, CustomerRefundStatus::Posted);
        assert_eq!(row.customer_id, "cust-1");
        assert_eq!(row.reason_text, "商品破损退款");
    })
}

#[tokio::test]
#[ignore]
async fn batch_queries_avoid_n_plus_one() {
    require_mongo!(async {
        let test_db = TestDb::new("ret_batch").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let case1 = sample_case("RT-1");
        let case1_id: SalesReturnCaseId = case1.base.id.clone().into();
        let case2 = sample_case("RT-2");
        let case2_id: SalesReturnCaseId = case2.base.id.clone().into();
        let line1 = sample_sales_line(&case1_id);
        let line2 = sample_sales_line(&case2_id);
        db.sales_return_cases()
            .create(&case1, &mut NoTransaction)
            .await
            .unwrap();
        db.sales_return_cases()
            .create(&case2, &mut NoTransaction)
            .await
            .unwrap();
        db.sales_return_lines()
            .create(&line1, &mut NoTransaction)
            .await
            .unwrap();
        db.sales_return_lines()
            .create(&line2, &mut NoTransaction)
            .await
            .unwrap();

        let order = sample_purchase_order("PRT-1");
        let order_id: PurchaseReturnOrderId = order.base.id.clone().into();
        let purchase_line = sample_purchase_line(&order_id);
        db.purchase_return_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        db.purchase_return_lines()
            .create(&purchase_line, &mut NoTransaction)
            .await
            .unwrap();

        let refund = sample_customer_refund("RF-1");
        let supplier_refund = sample_supplier_refund("SRF-1");
        db.customer_refunds()
            .create(&refund, &mut NoTransaction)
            .await
            .unwrap();
        db.supplier_refunds()
            .create(&supplier_refund, &mut NoTransaction)
            .await
            .unwrap();
        let receipt_reversal = sample_receipt_reversal("RR-1");
        let payment_reversal = sample_payment_reversal("PRR-1");
        db.receipt_reversals()
            .create(&receipt_reversal, &mut NoTransaction)
            .await
            .unwrap();
        db.payment_reversals()
            .create(&payment_reversal, &mut NoTransaction)
            .await
            .unwrap();

        let lines = db
            .sales_return_lines()
            .find_lines_by_cases(&[case1_id.clone(), case2_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(lines.len(), 2, "$in 一次取回，不得 N+1");

        let by_order_lines = db
            .sales_return_lines()
            .find_lines_by_order_lines(&[SalesOrderLineId::new("so-1-l1")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_order_lines.len(), 2);

        let purchase_lines = db
            .purchase_return_lines()
            .find_lines_by_orders(&[order_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(purchase_lines.len(), 1);
        assert_eq!(purchase_lines[0].return_quantity, qty("5.000000"));

        let customer_refunds = db
            .customer_refunds()
            .find_refunds_by_originals(&[CustomerReceiptId::new("cr-1")], &[], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(customer_refunds.len(), 1);

        let supplier_refunds = db
            .supplier_refunds()
            .find_refunds_by_originals(&[SupplierPaymentId::new("sp-1")], &[], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(supplier_refunds.len(), 1);
        assert_amount_fidelity(supplier_refunds[0].amount, "1000.00");

        let reversals = db
            .receipt_reversals()
            .find_reversals_by_receipts(&[CustomerReceiptId::new("cr-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(reversals.len(), 1);

        let payment_reversals = db
            .payment_reversals()
            .find_reversals_by_payments(&[SupplierPaymentId::new("sp-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(payment_reversals.len(), 1);
    })
}

/// 断言金额 Decimal128 往返保真（原值、小数位逐字一致）。
fn assert_amount_fidelity(actual: Amount, expected: &str) {
    assert_eq!(
        actual.to_decimal(),
        Amount::from_str(expected).unwrap().to_decimal()
    );
    assert_eq!(actual.to_decimal().to_string(), expected);
}
