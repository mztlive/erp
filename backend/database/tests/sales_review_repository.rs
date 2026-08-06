//! 域 D14 `sales_review` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test sales_review_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。
//! 审批/确认/复核记录与变更提交是历史与事实类对象，不提供软删除方法；
//! 软删除仅覆盖 `sales_change_order`。

use std::str::FromStr;

use database::repository::extensions::SalesReviewExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    ProcurementConfirmationId, ProcurementConfirmationLineId, SalesChangeOrderId, SalesChangeSubmissionId,
    SalesChangeSubmissionLineId, SalesOrderId, SalesOrderReviewId, SalesOrderRevisionId,
    SalesOrderSubmissionId, SalesOrderSubmissionLineId, SalesOrderWorkingCopyId, SupplierAccountId,
    SupplierCapabilityRevisionId,
};
use entities::money::{Quantity, Rate, UnitPrice};
use entities::sales_review::{
    ProcurementConfirmation, ProcurementConfirmationData, ProcurementConfirmationLine,
    ProcurementConfirmationLineData, ProcurementConfirmationStatus, SalesChangeOrder, SalesChangeOrderData,
    SalesChangeOrderStatus, SalesChangeSubmission, SalesChangeSubmissionData, SalesChangeSubmissionLine,
    SalesChangeSubmissionLineData, SalesChangeType, SalesOrderReview, SalesOrderReviewData, SalesReviewStage,
    SalesReviewStatus,
};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 审批记录列表筛选条件类型（经 `SalesReviewExt` 关联类型跨 crate 可达）。
type SalesOrderReviewFilter = <Database as SalesReviewExt>::SalesOrderReviewFilter;
/// 采购确认列表筛选条件类型。
type ProcurementConfirmationFilter = <Database as SalesReviewExt>::ProcurementConfirmationFilter;
/// 销售变更单列表筛选条件类型。
type SalesChangeOrderFilter = <Database as SalesReviewExt>::SalesChangeOrderFilter;

/// 构造可复用的销售审批记录实体。
fn sample_review(sales_order_id: &str, tag: &str) -> SalesOrderReview {
    SalesOrderReview::new(
        SalesOrderReviewId::new(format!("review-{tag}")),
        SalesOrderReviewData {
            sales_order_id: SalesOrderId::new(sales_order_id),
            submission_id: SalesOrderSubmissionId::new(format!("sub-{tag}")),
            review_stage: SalesReviewStage::SalesLeader,
        },
        "system-1",
    )
    .unwrap()
}

/// 构造可复用的采购确认实体（待处理）。
fn sample_confirmation(sales_order_id: &str, submission_id: &str) -> ProcurementConfirmation {
    ProcurementConfirmation::new(
        ProcurementConfirmationId::new(format!("confirm-{submission_id}")),
        ProcurementConfirmationData {
            sales_order_id: SalesOrderId::new(sales_order_id),
            submission_id: SalesOrderSubmissionId::new(submission_id),
            reject_reason_code: None,
            comment: None,
        },
        "system-1",
    )
    .unwrap()
}

/// 构造可复用的采购确认分行实体。
fn sample_confirmation_line(
    confirmation: &ProcurementConfirmation,
    line_no: u32,
    submission_line_id: &str,
) -> ProcurementConfirmationLine {
    ProcurementConfirmationLine::new(
        ProcurementConfirmationLineId::new(format!("confirm-line-{}-{line_no}", confirmation.base.id)),
        ProcurementConfirmationLineData {
            procurement_confirmation_id: confirmation.base.id.clone().into(),
            line_no,
            sales_order_submission_line_id: SalesOrderSubmissionLineId::new(submission_line_id),
            supplier_id: SupplierAccountId::new("supplier-1"),
            confirmed_quantity: Quantity::from_str("3.000000").unwrap(),
            latest_cost_gross: UnitPrice::from_str("8.5000").unwrap(),
            input_tax_rate: Rate::from_str("0.130000").unwrap(),
            expected_delivery_date: BusinessDate::from_ymd(2026, 8, 31).unwrap(),
            fulfillment_mode: entities::sales_review::FulfillmentMode::CompanyWarehouse,
            supplier_capability_revision_id: SupplierCapabilityRevisionId::new("cap-rev-1"),
        },
    )
    .unwrap()
}

/// 构造可复用的销售变更单实体（草稿）。
fn sample_change_order(sales_order_id: &str, tag: &str) -> SalesChangeOrder {
    SalesChangeOrder::new(
        SalesChangeOrderId::new(format!("change-{tag}")),
        SalesChangeOrderData {
            sales_order_id: SalesOrderId::new(sales_order_id),
            base_revision_id: SalesOrderRevisionId::new(format!("rev-{tag}")),
            change_type: SalesChangeType::Quantity,
            reason: "客户要求追加数量".to_string(),
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的销售变更提交实体。
fn sample_change_submission(change_order: &SalesChangeOrder, submission_no: u32) -> SalesChangeSubmission {
    SalesChangeSubmission::new(
        SalesChangeSubmissionId::new(format!("change-sub-{}-{submission_no}", change_order.base.id)),
        SalesChangeSubmissionData {
            sales_change_order_id: change_order.base.id.clone().into(),
            submission_no,
            base_revision_id: change_order.base_revision_id.clone(),
            sales_order_id: change_order.sales_order_id.clone(),
            working_copy_id: SalesOrderWorkingCopyId::new("wc-1"),
            working_copy_version: 1,
            business_type: entities::sales_review::BusinessType::GoodsService,
            customer_id: entities::ids::CustomerAccountId::new("cust-1"),
            contract_revision_id: None,
            settlement_party_id: entities::ids::PartyId::new("party-1"),
            snapshot: entities::sales_review::snapshot::HeaderSnapshotData {
                customer_name: "东方企业".to_string(),
                contract_no: None,
                settlement_party_name: Some("集团结算中心".to_string()),
                payment_term_code: "NET30".to_string(),
                payment_term_name: "月结 30 天".to_string(),
                invoice_type: "增值税专用发票".to_string(),
                tax_point: "6".to_string(),
            },
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: entities::money::Amount::from_str("29.97").unwrap(),
            net_amount: entities::money::Amount::from_str("26.07").unwrap(),
            tax_amount: entities::money::Amount::from_str("3.90").unwrap(),
            submitted_at: Instant::from_unix_secs(1_800_000_000),
            submitted_by: "editor-1".to_string(),
            lines: vec![SalesChangeSubmissionLineData {
                sales_order_line_id: entities::ids::SalesOrderLineId::new("line-1"),
                line_no: 1,
                line_type: entities::sales_review::LineType::GoodsService,
                sales_tax_rate: Rate::from_str("0.130000").unwrap(),
                item_name_snapshot: "年货礼盒".to_string(),
                spec_snapshot: None,
                unit_snapshot: None,
                goods: Some(entities::sales_review::GoodsLineFields {
                    sku_id: entities::ids::SkuId::new("sku-1"),
                    sku_revision_id: entities::ids::SkuRevisionId::new("skurev-1"),
                    welfare_scenario: Some(entities::sales_review::WelfareScenario::AnnualGiftBag),
                    fulfillment_mode: entities::sales_review::FulfillmentMode::CompanyWarehouse,
                    fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                    quantity: Quantity::from_str("3.000000").unwrap(),
                    base_unit_code: "箱".to_string(),
                    unit_price_gross: UnitPrice::from_str("9.9900").unwrap(),
                }),
                voucher: None,
            }],
        },
    )
    .unwrap()
}

/// 构造可复用的销售变更提交行实体。
fn sample_change_submission_line(
    submission: &SalesChangeSubmission,
    line_no: u32,
) -> SalesChangeSubmissionLine {
    SalesChangeSubmissionLine::new(
        SalesChangeSubmissionLineId::new(format!("change-sl-{}-{line_no}", submission.base.id)),
        submission.base.id.clone().into(),
        SalesChangeSubmissionLineData {
            sales_order_line_id: entities::ids::SalesOrderLineId::new("line-1"),
            line_no,
            line_type: entities::sales_review::LineType::GoodsService,
            sales_tax_rate: Rate::from_str("0.130000").unwrap(),
            item_name_snapshot: "年货礼盒".to_string(),
            spec_snapshot: None,
            unit_snapshot: None,
            goods: Some(entities::sales_review::GoodsLineFields {
                sku_id: entities::ids::SkuId::new("sku-1"),
                sku_revision_id: entities::ids::SkuRevisionId::new("skurev-1"),
                welfare_scenario: Some(entities::sales_review::WelfareScenario::AnnualGiftBag),
                fulfillment_mode: entities::sales_review::FulfillmentMode::CompanyWarehouse,
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                quantity: Quantity::from_str("3.000000").unwrap(),
                base_unit_code: "箱".to_string(),
                unit_price_gross: UnitPrice::from_str("9.9900").unwrap(),
            }),
            voucher: None,
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as SalesReviewExt>::SALES_ORDER_REVIEWS,
        &[
            "uk_sales_order_reviews_submission_stage",
            "idx_sales_order_reviews_pending_role_created",
        ],
    )
    .await
    .expect("sales_order_reviews 索引缺失");
    assert_indexes(
        db,
        <Database as SalesReviewExt>::PROCUREMENT_CONFIRMATIONS,
        &[
            "uk_procurement_confirmations_pending_per_submission",
            "idx_procurement_confirmations_pending_created",
        ],
    )
    .await
    .expect("procurement_confirmations 索引缺失");
    assert_indexes(
        db,
        <Database as SalesReviewExt>::PROCUREMENT_CONFIRMATION_LINES,
        &["uk_procurement_confirmation_lines_confirmation_line"],
    )
    .await
    .expect("procurement_confirmation_lines 索引缺失");
    assert_indexes(
        db,
        <Database as SalesReviewExt>::SALES_CHANGE_ORDERS,
        &[
            "uk_sales_change_orders_active_per_order_base",
            "idx_sales_change_orders_order_status",
        ],
    )
    .await
    .expect("sales_change_orders 索引缺失");
    assert_indexes(
        db,
        <Database as SalesReviewExt>::SALES_CHANGE_SUBMISSIONS,
        &[
            "uk_sales_change_submissions_order_submission_no",
            "idx_sales_change_submissions_order_submitted",
        ],
    )
    .await
    .expect("sales_change_submissions 索引缺失");
    assert_indexes(
        db,
        <Database as SalesReviewExt>::SALES_CHANGE_SUBMISSION_LINES,
        &["uk_sales_change_submission_lines_submission_line"],
    )
    .await
    .expect("sales_change_submission_lines 索引缺失");
    assert_indexes(
        db,
        <Database as SalesReviewExt>::SALES_CHANGE_REVIEWS,
        &[
            "uk_sales_change_reviews_submission_stage",
            "idx_sales_change_reviews_pending_role_created",
        ],
    )
    .await
    .expect("sales_change_reviews 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_review_and_confirmation_roundtrip_with_decimal_amounts() {
    require_mongo!(async {
        let test_db = TestDb::new("sr_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let review = sample_review("o-1", "1");
        db.sales_order_reviews()
            .create(&review, &mut NoTransaction)
            .await
            .unwrap();

        let found = db
            .sales_order_reviews()
            .find_by_submission_and_stage(
                &review.submission_id.clone(),
                SalesReviewStage::SalesLeader,
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .expect("按 (提交, 阶段) 应可读回");
        assert_eq!(found.stable.status(), SalesReviewStatus::Pending);
        assert_eq!(found.sales_order_id, SalesOrderId::new("o-1"));

        let confirmation = sample_confirmation("o-1", "sub-1");
        let line = sample_confirmation_line(&confirmation, 1, "sl-1");
        let db_clone = db.clone();
        let confirmation_for_tx = confirmation.clone();
        let line_for_tx = line.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .sales_review()
                        .create_procurement_confirmation_with_lines(
                            &confirmation_for_tx,
                            &[line_for_tx],
                            session,
                        )
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let pending = db
            .procurement_confirmations()
            .find_pending_by_submission(&confirmation.submission_id.clone(), &mut NoTransaction)
            .await
            .unwrap()
            .expect("待处理批次应命中");
        assert_eq!(pending.base.id, confirmation.base.id);

        let lines = db
            .procurement_confirmation_lines()
            .list_lines_by_confirmation(&confirmation.base.id.clone().into(), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0].confirmed_quantity.to_decimal(),
            Quantity::from_str("3.000000").unwrap().to_decimal(),
            "Decimal128 数量往返一致"
        );
        assert_eq!(
            lines[0].latest_cost_gross.to_decimal(),
            UnitPrice::from_str("8.5000").unwrap().to_decimal(),
            "Decimal128 单价往返一致"
        );
        assert_eq!(lines[0].supplier_id, SupplierAccountId::new("supplier-1"));
    })
}

#[tokio::test]
#[ignore]
async fn sales_change_order_update_optimistic_lock_and_soft_delete_restore() {
    require_mongo!(async {
        let test_db = TestDb::new("sr_change_opt").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut change_order = sample_change_order("o-1", "1");
        db.sales_change_orders()
            .create(&change_order, &mut NoTransaction)
            .await
            .unwrap();

        change_order
            .update(
                entities::sales_review::SalesChangeOrderUpdate {
                    change_type: Some(SalesChangeType::Amount),
                    reason: None,
                },
                "admin-2",
            )
            .unwrap();
        db.sales_change_orders()
            .update(&mut change_order, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(change_order.base.version, 2, "乐观锁成功后 version 递增");

        let mut stale = change_order.clone();
        let mut live = change_order.clone();
        live.update(
            entities::sales_review::SalesChangeOrderUpdate {
                change_type: Some(SalesChangeType::Goods),
                reason: None,
            },
            "admin-3",
        )
        .unwrap();
        db.sales_change_orders()
            .update(&mut live, &mut NoTransaction)
            .await
            .unwrap();

        stale
            .update(
                entities::sales_review::SalesChangeOrderUpdate {
                    change_type: Some(SalesChangeType::Other),
                    reason: None,
                },
                "admin-4",
            )
            .unwrap();
        let error = db
            .sales_change_orders()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        assert_eq!(stale.base.version, 2, "CAS 失败不得改动内存版本");

        let mut deletable = sample_change_order("o-2", "2");
        db.sales_change_orders()
            .create(&deletable, &mut NoTransaction)
            .await
            .unwrap();
        db.sales_change_orders()
            .soft_delete(&mut deletable, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .sales_change_orders()
            .find_by_id(&deletable.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后不可见");
        db.sales_change_orders()
            .restore(&mut deletable, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .sales_change_orders()
            .find_by_id(&deletable.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn unique_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("sr_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let review = sample_review("o-1", "1");
        db.sales_order_reviews()
            .create(&review, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_review = sample_review("o-1", "1");
        let error = db
            .sales_order_reviews()
            .create(&duplicate_review, &mut NoTransaction)
            .await
            .expect_err("重复 (submission_id, review_stage) 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let confirmation = sample_confirmation("o-1", "sub-1");
        let line = sample_confirmation_line(&confirmation, 1, "sl-1");
        db.procurement_confirmations()
            .create(&confirmation, &mut NoTransaction)
            .await
            .unwrap();
        db.procurement_confirmation_lines()
            .create(&line, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_line = sample_confirmation_line(&confirmation, 1, "sl-2");
        let error = db
            .procurement_confirmation_lines()
            .create(&duplicate_line, &mut NoTransaction)
            .await
            .expect_err("重复 (procurement_confirmation_id, line_no) 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let second_pending = sample_confirmation("o-1", "sub-1");
        let error = db
            .procurement_confirmations()
            .create(&second_pending, &mut NoTransaction)
            .await
            .expect_err("同一提交的第二个待处理确认批次必须被部分唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let change_order = sample_change_order("o-1", "1");
        db.sales_change_orders()
            .create(&change_order, &mut NoTransaction)
            .await
            .unwrap();
        let second_change = sample_change_order("o-1", "1");
        let error = db
            .sales_change_orders()
            .create(&second_change, &mut NoTransaction)
            .await
            .expect_err("同一销售单与基准版本的第二个进行中变更必须被拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_confirmation_and_lines() {
    require_mongo!(async {
        let test_db = TestDb::new("sr_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let confirmation = sample_confirmation("o-1", "sub-2");
        let line = sample_confirmation_line(&confirmation, 1, "sl-1");
        let db_clone = db.clone();
        let confirmation_for_tx = confirmation.clone();
        let line_for_tx = line.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .sales_review()
                        .create_procurement_confirmation_with_lines(
                            &confirmation_for_tx,
                            &[line_for_tx],
                            session,
                        )
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let found = db
            .procurement_confirmations()
            .find_by_id(&confirmation.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(found.is_none(), "回滚后确认头不得残留");
        let lines = db
            .procurement_confirmation_lines()
            .list_lines_by_confirmation(&confirmation.base.id.clone().into(), &mut NoTransaction)
            .await
            .unwrap();
        assert!(lines.is_empty(), "回滚后确认分行不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn submit_sales_change_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("sr_change_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut change_order = sample_change_order("o-1", "1");
        db.sales_change_orders()
            .create(&change_order, &mut NoTransaction)
            .await
            .unwrap();

        let submission = sample_change_submission(&change_order, 1);
        let submission_line = sample_change_submission_line(&submission, 1);
        change_order
            .submit_impact(submission.base.id.clone().into(), "target-hash-1", "admin-1")
            .unwrap();

        let db_clone = db.clone();
        let mut change_for_tx = change_order.clone();
        let submission_for_tx = submission.clone();
        let line_for_tx = submission_line.clone();
        let version_after = test_db
            .client()
            .with_transaction::<_, u64, database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .sales_review()
                        .submit_sales_change(&mut change_for_tx, &submission_for_tx, &[line_for_tx], session)
                        .await?;
                    Ok(change_for_tx.base.version)
                })
            })
            .await
            .expect("事务提交应成功");
        assert_eq!(version_after, 2, "变更单推进写入递增版本");

        let stored = db
            .sales_change_submissions()
            .find_by_id(&submission.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("变更提交应可见");
        assert_eq!(stored.submission_no, 1);
        assert_eq!(
            stored.gross_amount.to_decimal(),
            entities::money::Amount::from_str("29.97").unwrap().to_decimal()
        );

        let by_no = db
            .sales_change_submissions()
            .find_by_order_and_no(&change_order.base.id.clone().into(), 1, &mut NoTransaction)
            .await
            .unwrap()
            .expect("按 (变更单, 提交序号) 应命中");
        assert_eq!(by_no.base.id, submission.base.id);

        let lines = db
            .sales_change_submission_lines()
            .list_lines_by_submission(&submission.base.id.clone().into(), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].line_no, 1);

        let advanced = db
            .sales_change_orders()
            .find_by_id(&change_order.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("变更单应推进到待影响确认");
        assert_eq!(
            advanced.stable.status(),
            SalesChangeOrderStatus::PendingImpactConfirmation
        );
        assert_eq!(
            advanced.current_submission_id.as_ref().map(|id| id.to_string()),
            Some(submission.base.id.clone())
        );
    })
}

#[tokio::test]
#[ignore]
async fn submit_sales_change_conflict_rolls_back_whole_batch() {
    require_mongo!(async {
        let test_db = TestDb::new("sr_change_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut change_order = sample_change_order("o-1", "1");
        db.sales_change_orders()
            .create(&change_order, &mut NoTransaction)
            .await
            .unwrap();

        let submission = sample_change_submission(&change_order, 1);
        let submission_line = sample_change_submission_line(&submission, 1);

        let mut stale = change_order.clone();
        change_order
            .update(
                entities::sales_review::SalesChangeOrderUpdate {
                    change_type: Some(SalesChangeType::Amount),
                    reason: None,
                },
                "admin-9",
            )
            .unwrap();
        db.sales_change_orders()
            .update(&mut change_order, &mut NoTransaction)
            .await
            .unwrap();
        stale
            .submit_impact(submission.base.id.clone().into(), "target-hash-1", "admin-1")
            .unwrap();

        let db_clone = db.clone();
        let mut stale_for_tx = stale.clone();
        let submission_for_tx = submission.clone();
        let line_for_tx = submission_line.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .sales_review()
                        .submit_sales_change(&mut stale_for_tx, &submission_for_tx, &[line_for_tx], session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(result.is_err(), "陈旧变更单版本提交必须被 CAS 拒绝");

        let stored = db
            .sales_change_submissions()
            .find_by_id(&submission.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(stored.is_none(), "冲突回滚后变更提交不得残留");
        let lines = db
            .sales_change_submission_lines()
            .find_many(
                mongodb::bson::doc! { "sales_change_submission_id": submission.base.id },
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(lines.is_empty(), "冲突回滚后变更提交明细不得残留");
        let unchanged = db
            .sales_change_orders()
            .find_by_id(&change_order.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("变更单不受影响");
        assert_eq!(unchanged.stable.status(), SalesChangeOrderStatus::Draft);
        assert_eq!(unchanged.base.version, 2, "变更单保持并发更新后的版本");
    })
}

#[tokio::test]
#[ignore]
async fn projection_list_search_respects_filters_and_pagination() {
    require_mongo!(async {
        let test_db = TestDb::new("sr_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut reviews = Vec::new();
        for (i, tag) in ["1", "2", "3"].iter().enumerate() {
            let mut review = sample_review("o-1", tag);
            review.base.created_at = 1_700_000_000 + i as u64;
            db.sales_order_reviews()
                .create(&review, &mut NoTransaction)
                .await
                .unwrap();
            if *tag == "1" {
                review
                    .approve(
                        "leader-1",
                        Instant::from_unix_secs(1_800_000_000),
                        Some("同意".to_string()),
                    )
                    .unwrap();
                db.sales_order_reviews()
                    .update(&mut review, &mut NoTransaction)
                    .await
                    .unwrap();
            }
            reviews.push(review);
        }

        let filter = SalesOrderReviewFilter {
            submission_id: None,
            sales_order_id: Some(SalesOrderId::new("o-1")),
            review_stage: None,
            status: Some(SalesReviewStatus::Pending),
            page: 1,
            page_size: 2,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .sales_order_reviews()
            .search_sales_order_reviews(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "待处理审批只有两条");
        assert_eq!(page.items.len(), 2, "分页边界：首页两条");
        assert_eq!(page.items[0].review_stage, SalesReviewStage::SalesLeader);
        assert_eq!(page.items[0].status, SalesReviewStatus::Pending);
        assert!(page.items[0].reviewer_id.is_none());
        assert!(page.items[0].created_at > 0);

        let second = SalesOrderReviewFilter {
            submission_id: None,
            sales_order_id: None,
            review_stage: None,
            status: None,
            page: 2,
            page_size: 2,
            sort_by: Some("created_at".to_string()),
            sort_ascending: false,
        };
        let tail = db
            .sales_order_reviews()
            .search_sales_order_reviews(&second, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(tail.total, 3);
        assert_eq!(tail.items.len(), 1, "分页边界：第二页一条");
        assert_eq!(
            tail.items[0].reviewer_id.as_deref(),
            Some("leader-1"),
            "投影行含决策字段"
        );

        let confirmation = sample_confirmation("o-1", "sub-9");
        db.procurement_confirmations()
            .create(&confirmation, &mut NoTransaction)
            .await
            .unwrap();
        let pending_queue = ProcurementConfirmationFilter {
            submission_id: None,
            status: Some(ProcurementConfirmationStatus::Pending),
            page: 1,
            page_size: 10,
            sort_by: None,
            sort_ascending: false,
        };
        let queue = db
            .procurement_confirmations()
            .search_procurement_confirmations(&pending_queue, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(queue.total, 1, "待处理确认队列命中一条");
        assert_eq!(queue.items[0].submission_id, "sub-9");

        let change_order = sample_change_order("o-1", "1");
        db.sales_change_orders()
            .create(&change_order, &mut NoTransaction)
            .await
            .unwrap();
        let in_progress = db
            .sales_change_orders()
            .find_in_progress_by_order_and_base(
                &change_order.sales_order_id.clone(),
                &change_order.base_revision_id.clone(),
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .expect("进行中变更应命中");
        assert_eq!(in_progress.base.id, change_order.base.id);

        let list_filter = SalesChangeOrderFilter {
            sales_order_id: Some(SalesOrderId::new("o-1")),
            status: Some(SalesChangeOrderStatus::Draft),
            page: 1,
            page_size: 10,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let list = db
            .sales_change_orders()
            .search_sales_change_orders(&list_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(list.total, 1);
        assert_eq!(list.items[0].base_revision_id, "rev-1");
        assert_eq!(list.items[0].change_type, SalesChangeType::Quantity);
        assert!(list.items[0].version >= 1);
    })
}
