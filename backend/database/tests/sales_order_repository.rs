//! 域 D13 `sales_order` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test sales_order_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。
//! 提交/版本/版本行是事实类集合，不提供软删除方法；软删除仅覆盖
//! `sales_order` 主表。

use std::str::FromStr;

use database::repository::extensions::SalesOrderExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    ContractId, ContractRevisionId, CustomerAccountId, PartyId, PartyRevisionId, SalesOrderId,
    SalesOrderLineId, SalesOrderRevisionId, SalesOrderRevisionLineId, SalesOrderSubmissionId,
    SalesOrderSubmissionLineId, SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId, SkuId, SkuRevisionId,
};
use entities::money::{Amount, Quantity, Rate, UnitPrice};
use entities::sales_order::snapshot::HeaderSnapshotData;
use entities::sales_order::{
    BusinessType, CommercialStatus, GoodsLineFields, LineType, OriginSystem, ReviewStatus, RevisionSource,
    SalesOrder, SalesOrderData, SalesOrderGoodsServiceLineRevision, SalesOrderGoodsServiceLineRevisionData,
    SalesOrderLine, SalesOrderLineData, SalesOrderRevision, SalesOrderRevisionData, SalesOrderRevisionLine,
    SalesOrderRevisionLineData, SalesOrderSubmission, SalesOrderSubmissionData, SalesOrderSubmissionLine,
    SalesOrderSubmissionLineData, SalesOrderWorkingCopy, SalesOrderWorkingCopyData,
    SalesOrderWorkingCopyLine, SalesOrderWorkingCopyLineData, SubmissionStatus, WelfareScenario,
    WorkingCopyStatus, WorkingPurpose,
};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 销售单列表筛选条件类型（经 `SalesOrderExt` 关联类型跨 crate 可达）。
type SalesOrderFilter = <Database as SalesOrderExt>::SalesOrderFilter;
/// 工作副本列表筛选条件类型。
type WorkingCopyFilter = <Database as SalesOrderExt>::WorkingCopyFilter;
/// 提交历史列表筛选条件类型。
type SubmissionFilter = <Database as SalesOrderExt>::SubmissionFilter;

/// 构造可复用的销售单实体。
fn sample_order(order_no: &str) -> SalesOrder {
    SalesOrder::new(
        SalesOrderId::new(format!("order-{order_no}")),
        SalesOrderData {
            order_no: order_no.to_string(),
            business_type: BusinessType::GoodsService,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_id: Some(ContractId::new("contract-1")),
            settlement_party_id: PartyId::new("party-1"),
            source_status_code: None,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的稳定明细行。
fn sample_line(order: &SalesOrder, line_no: u32) -> SalesOrderLine {
    SalesOrderLine::new(
        SalesOrderLineId::new(format!("line-{}-{line_no}", order.base.id)),
        order.base.id.clone().into(),
        SalesOrderLineData { line_no },
    )
    .unwrap()
}

/// 构造可复用的表头结构化快照入参。
fn header_snapshot() -> HeaderSnapshotData {
    HeaderSnapshotData {
        customer_name: "东方企业".to_string(),
        contract_no: Some("HT-2026-0088".to_string()),
        settlement_party_name: Some("集团结算中心".to_string()),
        payment_term_code: "NET30".to_string(),
        payment_term_name: "月结 30 天".to_string(),
        invoice_type: "增值税专用发票".to_string(),
        tax_point: "6".to_string(),
    }
}

/// 构造可复用的实物及服务行字段组。
fn goods_fields() -> GoodsLineFields {
    GoodsLineFields {
        sku_id: SkuId::new("sku-1"),
        sku_revision_id: SkuRevisionId::new("skurev-1"),
        welfare_scenario: Some(WelfareScenario::AnnualGiftBag),
        fulfillment_mode: entities::sales_order::FulfillmentMode::CompanyWarehouse,
        fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
        quantity: Quantity::from_str("3.000000").unwrap(),
        base_unit_code: "箱".to_string(),
        unit_price_gross: UnitPrice::from_str("9.9900").unwrap(),
    }
}

/// 构造可复用的工作副本行创建数据。
fn working_copy_line_data(line_no: u32) -> SalesOrderWorkingCopyLineData {
    SalesOrderWorkingCopyLineData {
        sales_order_line_id: SalesOrderLineId::new(format!("line-{line_no}")),
        line_no,
        line_type: LineType::GoodsService,
        sales_tax_rate: Rate::from_str("0.130000").unwrap(),
        item_name_snapshot: "年货礼盒".to_string(),
        spec_snapshot: None,
        unit_snapshot: None,
        goods: Some(goods_fields()),
        voucher: None,
    }
}

/// 构造可复用的工作副本实体（首次提交目的）。
fn sample_working_copy(order: &SalesOrder, tag: &str) -> SalesOrderWorkingCopy {
    SalesOrderWorkingCopy::new(
        SalesOrderWorkingCopyId::new(format!("wc-{}-{tag}", order.order_no)),
        SalesOrderWorkingCopyData {
            sales_order_id: order.base.id.clone().into(),
            working_purpose: WorkingPurpose::FirstSubmission,
            sales_change_order_id: None,
            base_revision_id: None,
            draft_version: 1,
            content_hash: "hash-abc123".to_string(),
            editor_user_id: "editor-1".to_string(),
            business_type: BusinessType::GoodsService,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_id: Some(ContractId::new("contract-1")),
            settlement_party_id: PartyId::new("party-1"),
            snapshot: header_snapshot(),
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: Amount::from_str("29.97").unwrap(),
            net_amount: Amount::from_str("26.07").unwrap(),
            tax_amount: Amount::from_str("3.90").unwrap(),
            lines: vec![working_copy_line_data(1)],
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的工作副本行实体。
fn sample_working_copy_line(working_copy: &SalesOrderWorkingCopy, line_no: u32) -> SalesOrderWorkingCopyLine {
    SalesOrderWorkingCopyLine::new(
        SalesOrderWorkingCopyLineId::new(format!("wcl-{}-{line_no}", working_copy.base.id)),
        working_copy.base.id.clone().into(),
        working_copy_line_data(line_no),
    )
    .unwrap()
}

/// 构造可复用的提交快照（复制自工作副本）。
fn sample_submission(working_copy: &SalesOrderWorkingCopy, submission_no: u32) -> SalesOrderSubmission {
    SalesOrderSubmission::new(
        SalesOrderSubmissionId::new(format!("sub-{}-{submission_no}", working_copy.base.id)),
        SalesOrderSubmissionData {
            sales_order_id: working_copy.sales_order_id.clone(),
            submission_no,
            working_copy_id: working_copy.base.id.clone().into(),
            working_copy_version: working_copy.draft_version,
            business_type: working_copy.business_type,
            customer_id: working_copy.customer_id.clone(),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
            settlement_party_id: working_copy.settlement_party_id.clone(),
            snapshot: header_snapshot(),
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: working_copy.gross_amount,
            net_amount: working_copy.net_amount,
            tax_amount: working_copy.tax_amount,
            submitted_at: Instant::from_unix_secs(1_800_000_000),
            submitted_by: "editor-1".to_string(),
            lines: vec![SalesOrderSubmissionLineData {
                sales_order_line_id: SalesOrderLineId::new("line-1"),
                line_no: 1,
                line_type: LineType::GoodsService,
                sales_tax_rate: Rate::from_str("0.130000").unwrap(),
                item_name_snapshot: "年货礼盒".to_string(),
                spec_snapshot: None,
                unit_snapshot: None,
                goods: Some(goods_fields()),
                voucher: None,
            }],
        },
    )
    .unwrap()
}

/// 构造可复用的提交行实体。
fn sample_submission_line(submission: &SalesOrderSubmission, line_no: u32) -> SalesOrderSubmissionLine {
    SalesOrderSubmissionLine::new(
        SalesOrderSubmissionLineId::new(format!("sl-{}-{line_no}", submission.base.id)),
        submission.base.id.clone().into(),
        SalesOrderSubmissionLineData {
            sales_order_line_id: SalesOrderLineId::new("line-1"),
            line_no,
            line_type: LineType::GoodsService,
            sales_tax_rate: Rate::from_str("0.130000").unwrap(),
            item_name_snapshot: "年货礼盒".to_string(),
            spec_snapshot: None,
            unit_snapshot: None,
            goods: Some(goods_fields()),
            voucher: None,
        },
    )
    .unwrap()
}

/// 构造可复用的正式版本头。
fn sample_revision(order: &SalesOrder, revision_no: u32) -> SalesOrderRevision {
    SalesOrderRevision::new(
        SalesOrderRevisionId::new(format!("rev-{}-{revision_no}", order.order_no)),
        SalesOrderRevisionData {
            sales_order_id: order.base.id.clone().into(),
            revision_no,
            revision_source: RevisionSource::ErpApproval,
            source_snapshot_id: None,
            previous_revision_id: None,
            content_hash: "hash-abc123".to_string(),
            customer_revision_id: Some(PartyRevisionId::new("party-rev-1")),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
            snapshot: header_snapshot(),
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: Amount::from_str("29.97").unwrap(),
            net_amount: Amount::from_str("26.07").unwrap(),
            tax_amount: Amount::from_str("3.90").unwrap(),
            effective_at: Instant::from_unix_secs(1_800_000_000),
            recorded_at: Instant::from_unix_secs(1_800_000_100),
        },
    )
    .unwrap()
}

/// 构造可复用的公共行版本。
fn sample_revision_line(
    revision: &SalesOrderRevision,
    order_line_id: &SalesOrderLineId,
    line_no: u32,
) -> SalesOrderRevisionLine {
    SalesOrderRevisionLine::new(
        SalesOrderRevisionLineId::new(format!("rl-{}-{line_no}", revision.base.id)),
        SalesOrderRevisionLineData {
            sales_order_revision_id: revision.base.id.clone().into(),
            sales_order_line_id: order_line_id.clone(),
            line_no,
            line_type: LineType::GoodsService,
            gross_amount: Amount::from_str("29.97").unwrap(),
            net_amount: Amount::from_str("26.07").unwrap(),
            tax_amount: Amount::from_str("3.90").unwrap(),
            sales_tax_rate: Rate::from_str("0.130000").unwrap(),
            item_name_snapshot: "年货礼盒".to_string(),
            spec_snapshot: None,
            unit_snapshot: None,
        },
    )
    .unwrap()
}

/// 构造可复用的实物及服务行版本。
fn sample_goods_line_revision(revision_line: &SalesOrderRevisionLine) -> SalesOrderGoodsServiceLineRevision {
    SalesOrderGoodsServiceLineRevision::new(
        entities::ids::SalesOrderGoodsServiceLineRevisionId::new(format!("gs-{}", revision_line.base.id)),
        SalesOrderGoodsServiceLineRevisionData {
            revision_line_id: revision_line.base.id.clone().into(),
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: Some(WelfareScenario::AnnualGiftBag),
            fulfillment_mode: entities::sales_order::FulfillmentMode::CompanyWarehouse,
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: Quantity::from_str("3.000000").unwrap(),
            base_unit_code: "箱".to_string(),
            unit_price_gross: UnitPrice::from_str("9.9900").unwrap(),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as SalesOrderExt>::SALES_ORDERS,
        &[
            "uk_sales_orders_order_no",
            "idx_sales_orders_customer_status_created",
            "idx_sales_orders_review_status_created",
        ],
    )
    .await
    .expect("sales_orders 索引缺失");
    assert_indexes(
        db,
        <Database as SalesOrderExt>::SALES_ORDER_LINES,
        &["uk_sales_order_lines_order_line"],
    )
    .await
    .expect("sales_order_lines 索引缺失");
    assert_indexes(
        db,
        <Database as SalesOrderExt>::SALES_ORDER_WORKING_COPIES,
        &[
            "uk_sales_order_working_copies_active_per_purpose",
            "idx_sales_order_working_copies_order_purpose",
            "idx_sales_order_working_copies_status_updated",
        ],
    )
    .await
    .expect("sales_order_working_copies 索引缺失");
    assert_indexes(
        db,
        <Database as SalesOrderExt>::SALES_ORDER_WORKING_COPY_LINES,
        &["uk_sales_order_working_copy_lines_copy_line"],
    )
    .await
    .expect("sales_order_working_copy_lines 索引缺失");
    assert_indexes(
        db,
        <Database as SalesOrderExt>::SALES_ORDER_SUBMISSIONS,
        &[
            "uk_sales_order_submissions_order_submission_no",
            "idx_sales_order_submissions_order_submitted",
            "idx_sales_order_submissions_status_created",
        ],
    )
    .await
    .expect("sales_order_submissions 索引缺失");
    assert_indexes(
        db,
        <Database as SalesOrderExt>::SALES_ORDER_SUBMISSION_LINES,
        &["uk_sales_order_submission_lines_submission_line"],
    )
    .await
    .expect("sales_order_submission_lines 索引缺失");
    assert_indexes(
        db,
        <Database as SalesOrderExt>::SALES_ORDER_REVISIONS,
        &[
            "uk_sales_order_revisions_order_revision_no",
            "idx_sales_order_revisions_order_content_hash",
            "idx_sales_order_revisions_order_effective",
        ],
    )
    .await
    .expect("sales_order_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as SalesOrderExt>::SALES_ORDER_REVISION_LINES,
        &[
            "uk_sales_order_revision_lines_revision_line",
            "idx_sales_order_revision_lines_due",
        ],
    )
    .await
    .expect("sales_order_revision_lines 索引缺失");
    assert_indexes(
        db,
        <Database as SalesOrderExt>::SALES_ORDER_GOODS_SERVICE_LINE_REVISIONS,
        &["uk_sales_order_goods_service_line_revisions_line"],
    )
    .await
    .expect("goods_service_line_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as SalesOrderExt>::SALES_ORDER_VOUCHER_LINE_REVISIONS,
        &["uk_sales_order_voucher_line_revisions_line"],
    )
    .await
    .expect("voucher_line_revisions 索引缺失");
}

/// 在事务内创建销售单与稳定明细行。
async fn create_order_with_lines(test_db: &TestDb, db: &Database, order: &mut SalesOrder) -> SalesOrderLine {
    let line = sample_line(order, 1);
    let db_clone = db.clone();
    let order_for_tx = order.clone();
    let line_for_tx = line.clone();
    *order = test_db
        .client()
        .with_transaction::<_, SalesOrder, database::Error>(move |session| {
            Box::pin(async move {
                db_clone.sales_orders().create(&order_for_tx, session).await?;
                db_clone.sales_order_lines().create(&line_for_tx, session).await?;
                Ok(order_for_tx)
            })
        })
        .await
        .expect("事务提交应成功");
    line
}

#[tokio::test]
#[ignore]
async fn create_order_read_roundtrip_with_decimal_amounts() {
    require_mongo!(async {
        let test_db = TestDb::new("so_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut order = sample_order("SO-2026-7001");
        let line = create_order_with_lines(&test_db, db, &mut order).await;

        let found = db
            .sales_orders()
            .find_by_order_no("SO-2026-7001", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按单号应可读回");
        assert_eq!(found.customer_id, CustomerAccountId::new("cust-1"));
        assert_eq!(found.commercial_status, CommercialStatus::Draft);

        let lines = db
            .sales_order_lines()
            .list_lines_by_order(&found.base.id.clone().into(), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].line_no, 1);
        assert_eq!(lines[0].base.id, line.base.id);

        let revision = sample_revision(&found, 1);
        let revision_line = sample_revision_line(&revision, &line.base.id.clone().into(), 1);
        let goods_line = sample_goods_line_revision(&revision_line);
        let db_clone = db.clone();
        let mut order_for_tx = found.clone();
        order_for_tx
            .submit_for_review("admin-1")
            .and_then(|()| order_for_tx.transition_review(ReviewStatus::Approved, "reviewer"))
            .and_then(|()| order_for_tx.approve(Instant::from_unix_secs(1_800_000_000), "reviewer"))
            .unwrap();
        order_for_tx.attach_revision(&revision.base.id, "reviewer");
        let revision_for_tx = revision.clone();
        let revision_line_for_tx = revision_line.clone();
        let goods_line_for_tx = goods_line.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .sales_order()
                        .formalize_submission(
                            &mut order_for_tx,
                            &revision_for_tx,
                            &[revision_line_for_tx],
                            &[goods_line_for_tx],
                            &[],
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let rev = db
            .sales_order_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("版本应可读回");
        assert_eq!(
            rev.gross_amount.to_decimal(),
            Amount::from_str("29.97").unwrap().to_decimal(),
            "Decimal128 金额往返一致"
        );
        assert_eq!(rev.revision.revision_no, 1);
        assert_eq!(rev.content_hash, "hash-abc123");

        let revision_lines = db
            .sales_order_revision_lines()
            .list_lines_by_revision(&rev.base.id.clone().into(), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(revision_lines.len(), 1);
        let goods = db
            .sales_order_goods_service_line_revisions()
            .list_by_revision_line_ids(&[revision_lines[0].base.id.clone().into()], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(goods.len(), 1);
        assert_eq!(goods[0].sku_id, SkuId::new("sku-1"));

        let updated = db
            .sales_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("主表应推进到生效态");
        assert_eq!(updated.commercial_status, CommercialStatus::Effective);
        assert_eq!(
            updated.stable.current_revision_id.as_deref(),
            Some(revision.base.id.as_str())
        );
    })
}

#[tokio::test]
#[ignore]
async fn optimistic_lock_guards_sales_order_updates() {
    require_mongo!(async {
        let test_db = TestDb::new("so_optlock").await.unwrap();
        let db = test_db.db();

        let mut order = sample_order("SO-2026-7002");
        db.sales_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();

        order
            .update(
                entities::sales_order::SalesOrderUpdate {
                    customer_id: Some(CustomerAccountId::new("cust-2")),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();
        db.sales_orders()
            .update(&mut order, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(order.base.version, 2, "乐观锁成功后 version 递增");

        let mut stale = order.clone();
        let mut live = order.clone();
        live.update(
            entities::sales_order::SalesOrderUpdate {
                customer_id: Some(CustomerAccountId::new("cust-3")),
                ..Default::default()
            },
            "admin-3",
        )
        .unwrap();
        db.sales_orders()
            .update(&mut live, &mut NoTransaction)
            .await
            .unwrap();

        stale
            .update(
                entities::sales_order::SalesOrderUpdate {
                    customer_id: Some(CustomerAccountId::new("cust-4")),
                    ..Default::default()
                },
                "admin-4",
            )
            .unwrap();
        let error = db
            .sales_orders()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        assert_eq!(stale.base.version, 2, "CAS 失败不得改动内存版本");
    })
}

#[tokio::test]
#[ignore]
async fn sales_order_soft_delete_and_restore_keep_order_no() {
    require_mongo!(async {
        let test_db = TestDb::new("so_softdel").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut order = sample_order("SO-2026-7003");
        db.sales_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();

        db.sales_orders()
            .soft_delete(&mut order, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .sales_orders()
            .find_by_order_no("SO-2026-7003", &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按单号不可见");

        db.sales_orders()
            .restore(&mut order, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .sales_orders()
            .find_by_order_no("SO-2026-7003", &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按单号重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn unique_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("so_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut order = sample_order("SO-2026-7004");
        let line = create_order_with_lines(&test_db, db, &mut order).await;

        let duplicate_order = sample_order("SO-2026-7004");
        let error = db
            .sales_orders()
            .create(&duplicate_order, &mut NoTransaction)
            .await
            .expect_err("重复 order_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let duplicate_line = sample_line(&order, 1);
        let error = db
            .sales_order_lines()
            .create(&duplicate_line, &mut NoTransaction)
            .await
            .expect_err("重复 (sales_order_id, line_no) 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
        assert_eq!(line.line_no, 1);
    })
}

#[tokio::test]
#[ignore]
async fn partial_unique_limits_active_working_copy_per_purpose() {
    require_mongo!(async {
        let test_db = TestDb::new("so_wc_unique").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("SO-2026-7005");
        db.sales_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();

        let first = sample_working_copy(&order, "a");
        db.sales_order_working_copies()
            .create(&first, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_working_copy(&order, "b");
        let error = db
            .sales_order_working_copies()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一销售单与编辑目的的第二个有效工作副本必须被拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let mut abandoned = sample_working_copy(&order, "c");
        abandoned.abandon().unwrap();
        db.sales_order_working_copies()
            .create(&abandoned, &mut NoTransaction)
            .await
            .expect("已放弃草稿不参与有效唯一，可保留历史");

        let mut first_mut = first.clone();
        first_mut.submit().unwrap();
        let db_clone = db.clone();
        let mut first_for_tx = first_mut.clone();
        let version_after = test_db
            .client()
            .with_transaction::<_, u64, database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .sales_order_working_copies()
                        .update(&mut first_for_tx, session)
                        .await?;
                    Ok(first_for_tx.base.version)
                })
            })
            .await
            .expect("提交草稿应成功");
        assert_eq!(version_after, 2);

        let after_submit = sample_working_copy(&order, "d");
        db.sales_order_working_copies()
            .create(&after_submit, &mut NoTransaction)
            .await
            .expect("已提交的旧草稿不再占用有效名额，新草稿可创建");
    })
}

#[tokio::test]
#[ignore]
async fn submit_working_copy_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("so_submit_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("SO-2026-7006");
        db.sales_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let mut working_copy = sample_working_copy(&order, "a");
        db.sales_order_working_copies()
            .create(&working_copy, &mut NoTransaction)
            .await
            .unwrap();
        let wc_line = sample_working_copy_line(&working_copy, 1);
        db.sales_order_working_copy_lines()
            .create(&wc_line, &mut NoTransaction)
            .await
            .unwrap();

        let submission = sample_submission(&working_copy, 1);
        let submission_line = sample_submission_line(&submission, 1);
        working_copy.submit().unwrap();

        let db_clone = db.clone();
        let mut wc_for_tx = working_copy.clone();
        let submission_for_tx = submission.clone();
        let line_for_tx = submission_line.clone();
        let version_after = test_db
            .client()
            .with_transaction::<_, u64, database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .sales_order()
                        .submit_working_copy(&mut wc_for_tx, &submission_for_tx, &[line_for_tx], session)
                        .await?;
                    Ok(wc_for_tx.base.version)
                })
            })
            .await
            .expect("事务提交应成功");
        assert_eq!(version_after, 2, "草稿锁定写入递增版本");

        let stored = db
            .sales_order_submissions()
            .find_by_id(&submission.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("提交快照应可见");
        assert_eq!(stored.stable.status(), SubmissionStatus::InReview);
        assert_eq!(
            stored.gross_amount.to_decimal(),
            Amount::from_str("29.97").unwrap().to_decimal()
        );

        let lines = db
            .sales_order_submission_lines()
            .list_lines_by_submissions(&[submission.base.id.clone().into()], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].line_no, 1);

        let locked = db
            .sales_order_working_copies()
            .find_by_id(&working_copy.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("工作副本应仍可读");
        assert_eq!(
            locked.stable.status(),
            WorkingCopyStatus::Submitted,
            "提交后草稿锁定"
        );
    })
}

#[tokio::test]
#[ignore]
async fn submit_working_copy_conflict_rolls_back_whole_batch() {
    require_mongo!(async {
        let test_db = TestDb::new("so_submit_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("SO-2026-7007");
        db.sales_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let mut working_copy = sample_working_copy(&order, "a");
        db.sales_order_working_copies()
            .create(&working_copy, &mut NoTransaction)
            .await
            .unwrap();

        let submission = sample_submission(&working_copy, 1);
        let submission_line = sample_submission_line(&submission, 1);

        let mut stale = working_copy.clone();
        working_copy.save_draft("hash-new", "editor-2").unwrap();
        db.sales_order_working_copies()
            .update(&mut working_copy, &mut NoTransaction)
            .await
            .unwrap();
        stale.submit().unwrap();

        let db_clone = db.clone();
        let mut stale_for_tx = stale.clone();
        let submission_for_tx = submission.clone();
        let line_for_tx = submission_line.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .sales_order()
                        .submit_working_copy(&mut stale_for_tx, &submission_for_tx, &[line_for_tx], session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(result.is_err(), "陈旧草稿版本提交必须被 CAS 拒绝");

        let stored = db
            .sales_order_submissions()
            .find_by_id(&submission.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(stored.is_none(), "冲突回滚后提交头不得残留");
        let lines = db
            .sales_order_submission_lines()
            .find_many(
                mongodb::bson::doc! { "submission_id": submission.base.id },
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(lines.is_empty(), "冲突回滚后提交明细不得残留");
        let locked = db
            .sales_order_working_copies()
            .find_by_id(&working_copy.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("草稿不受影响");
        assert_eq!(locked.stable.status(), WorkingCopyStatus::Editing);
        assert_eq!(locked.base.version, 2, "草稿保持并发保存后的版本");
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_submission_and_lines() {
    require_mongo!(async {
        let test_db = TestDb::new("so_submit_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("SO-2026-7008");
        db.sales_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
        let mut working_copy = sample_working_copy(&order, "a");
        db.sales_order_working_copies()
            .create(&working_copy, &mut NoTransaction)
            .await
            .unwrap();

        let submission = sample_submission(&working_copy, 1);
        let submission_line = sample_submission_line(&submission, 1);
        working_copy.submit().unwrap();

        let db_clone = db.clone();
        let mut wc_for_tx = working_copy.clone();
        let submission_for_tx = submission.clone();
        let line_for_tx = submission_line.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .sales_order()
                        .submit_working_copy(&mut wc_for_tx, &submission_for_tx, &[line_for_tx], session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let stored = db
            .sales_order_submissions()
            .find_by_id(&submission.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(stored.is_none(), "回滚后提交头不得残留");
        let locked = db
            .sales_order_working_copies()
            .find_by_id(&working_copy.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("草稿不受影响");
        assert_eq!(
            locked.stable.status(),
            WorkingCopyStatus::Editing,
            "回滚后草稿未被锁定"
        );
    })
}

#[tokio::test]
#[ignore]
async fn formalize_submission_conflict_rolls_back_whole_batch() {
    require_mongo!(async {
        let test_db = TestDb::new("so_formalize_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut order = sample_order("SO-2026-7009");
        let line = create_order_with_lines(&test_db, db, &mut order).await;

        let revision = sample_revision(&order, 1);
        let revision_line = sample_revision_line(&revision, &line.base.id.clone().into(), 1);
        let goods_line = sample_goods_line_revision(&revision_line);

        let stale = order.clone();
        order
            .update(
                entities::sales_order::SalesOrderUpdate {
                    customer_id: Some(CustomerAccountId::new("cust-9")),
                    ..Default::default()
                },
                "admin-9",
            )
            .unwrap();
        db.sales_orders()
            .update(&mut order, &mut NoTransaction)
            .await
            .unwrap();

        let db_clone = db.clone();
        let mut stale_for_tx = stale.clone();
        let revision_for_tx = revision.clone();
        let revision_line_for_tx = revision_line.clone();
        let goods_line_for_tx = goods_line.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    stale_for_tx
                        .submit_for_review("admin-1")
                        .and_then(|()| stale_for_tx.transition_review(ReviewStatus::Approved, "reviewer"))
                        .and_then(|()| {
                            stale_for_tx.approve(Instant::from_unix_secs(1_800_000_000), "reviewer")
                        })
                        .unwrap();
                    stale_for_tx.attach_revision(&revision_for_tx.base.id, "reviewer");
                    db_clone
                        .sales_order()
                        .formalize_submission(
                            &mut stale_for_tx,
                            &revision_for_tx,
                            &[revision_line_for_tx],
                            &[goods_line_for_tx],
                            &[],
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(result.is_err(), "陈旧销售单版本生效必须被 CAS 拒绝");

        let rev = db
            .sales_order_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(rev.is_none(), "冲突回滚后版本头不得残留");
        let revision_lines = db
            .sales_order_revision_lines()
            .list_lines_by_revision(&revision.base.id.clone().into(), &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_lines.is_empty(), "冲突回滚后版本行不得残留");
        let goods = db
            .sales_order_goods_service_line_revisions()
            .list_by_revision_line_ids(&[revision_line.base.id.clone().into()], &mut NoTransaction)
            .await
            .unwrap();
        assert!(goods.is_empty(), "冲突回滚后子类型行不得残留");
        let stored = db
            .sales_orders()
            .find_by_id(&order.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("主表不受影响");
        assert_eq!(stored.commercial_status, CommercialStatus::Draft);
    })
}

#[tokio::test]
#[ignore]
async fn projection_list_search_respects_filters_and_pagination() {
    require_mongo!(async {
        let test_db = TestDb::new("so_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut orders = Vec::new();
        for no in ["SO-2026-8001", "SO-2026-8002", "SO-2026-8003"] {
            let mut order = sample_order(no);
            db.sales_orders()
                .create(&order, &mut NoTransaction)
                .await
                .unwrap();
            if no == "SO-2026-8002" {
                order.submit_for_review("admin-1").unwrap();
                db.sales_orders()
                    .update(&mut order, &mut NoTransaction)
                    .await
                    .unwrap();
            }
            orders.push(order);
        }

        let filter = SalesOrderFilter {
            order_no: Some("SO-2026-8".to_string()),
            customer_id: Some("cust-1".to_string()),
            commercial_status: Some(CommercialStatus::Draft),
            review_status: None,
            business_type: None,
            page: 1,
            page_size: 2,
            sort_by: Some("order_no".to_string()),
            sort_ascending: true,
        };
        let page = db
            .sales_orders()
            .search_sales_orders(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "草稿且匹配前缀只有两条");
        assert_eq!(page.items.len(), 2, "分页边界：首页两条");
        assert_eq!(page.items[0].order_no, "SO-2026-8001", "按单号升序");
        let row = &page.items[0];
        assert_eq!(row.customer_id, "cust-1");
        assert_eq!(row.commercial_status, CommercialStatus::Draft);
        assert_eq!(row.business_type, BusinessType::GoodsService);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);
        assert!(row.effective_at.is_none());

        let second = SalesOrderFilter {
            order_no: None,
            customer_id: None,
            commercial_status: None,
            review_status: None,
            business_type: None,
            page: 2,
            page_size: 2,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let tail = db
            .sales_orders()
            .search_sales_orders(&second, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(tail.total, 3);
        assert_eq!(tail.items.len(), 1, "分页边界：第二页一条");

        let review_track = SalesOrderFilter {
            order_no: None,
            customer_id: None,
            commercial_status: None,
            review_status: Some(ReviewStatus::PendingProcurementConfirmation),
            business_type: None,
            page: 1,
            page_size: 10,
            sort_by: None,
            sort_ascending: false,
        };
        let pending = db
            .sales_orders()
            .search_sales_orders(&review_track, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(pending.total, 1, "审核轨筛选命中一条");
        assert_eq!(pending.items[0].order_no, "SO-2026-8002");
    })
}

#[tokio::test]
#[ignore]
async fn working_copy_status_filter_and_submission_history_queries() {
    require_mongo!(async {
        let test_db = TestDb::new("so_history").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let order = sample_order("SO-2026-8004");
        db.sales_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();

        let working_copy = sample_working_copy(&order, "a");
        db.sales_order_working_copies()
            .create(&working_copy, &mut NoTransaction)
            .await
            .unwrap();
        let wc_line = sample_working_copy_line(&working_copy, 1);
        db.sales_order_working_copy_lines()
            .create(&wc_line, &mut NoTransaction)
            .await
            .unwrap();

        let active = db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(
                &working_copy.sales_order_id.clone(),
                WorkingPurpose::FirstSubmission,
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .expect("有效工作副本应命中");
        assert_eq!(active.base.id, working_copy.base.id);

        let mut first_submission = sample_submission(&working_copy, 1);
        let mut second_submission = sample_submission(&working_copy, 2);
        first_submission.base.created_at = 1_700_000_000;
        second_submission.base.created_at = 1_700_000_100;
        let first_line = sample_submission_line(&first_submission, 1);
        let second_line = sample_submission_line(&second_submission, 1);
        let db_clone = db.clone();
        let mut wc_for_tx = working_copy.clone();
        let first_for_tx = first_submission.clone();
        let first_line_for_tx = first_line.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    wc_for_tx.submit().unwrap();
                    db_clone
                        .sales_order()
                        .submit_working_copy(&mut wc_for_tx, &first_for_tx, &[first_line_for_tx], session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");
        db.sales_order_submissions()
            .create(&second_submission, &mut NoTransaction)
            .await
            .unwrap();
        db.sales_order_submission_lines()
            .create(&second_line, &mut NoTransaction)
            .await
            .unwrap();

        let filter = WorkingCopyFilter {
            sales_order_id: Some(order.base.id.clone().into()),
            working_purpose: Some(WorkingPurpose::FirstSubmission),
            status: Some(WorkingCopyStatus::Submitted),
            page: 1,
            page_size: 10,
            sort_by: None,
            sort_ascending: false,
        };
        let page = db
            .sales_order_working_copies()
            .search_working_copies(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "已提交草稿状态筛选命中一条");
        assert_eq!(page.items[0].status, WorkingCopyStatus::Submitted);

        let history = db
            .sales_order_submissions()
            .search_submissions(
                &SubmissionFilter {
                    sales_order_id: Some(order.base.id.clone().into()),
                    status: None,
                    page: 1,
                    page_size: 10,
                    sort_by: Some("created_at".to_string()),
                    sort_ascending: true,
                },
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(history.total, 2, "提交历史含两笔");
        assert_eq!(history.items[0].submission_no, 1);
        assert_eq!(
            history.items[0].gross_amount.to_decimal(),
            Amount::from_str("29.97").unwrap().to_decimal(),
            "投影行金额保持 Decimal128 原样"
        );

        let lines = db
            .sales_order_submission_lines()
            .list_lines_by_submissions(
                &[
                    first_submission.base.id.clone().into(),
                    second_submission.base.id.clone().into(),
                ],
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(lines.len(), 2, "批量 $in 一次取回两个提交的明细");

        let by_no = db
            .sales_order_submissions()
            .find_by_order_and_no(&order.base.id.clone().into(), 2, &mut NoTransaction)
            .await
            .unwrap()
            .expect("按 (销售单, 提交序号) 应命中");
        assert_eq!(by_no.base.id, second_submission.base.id);
    })
}
