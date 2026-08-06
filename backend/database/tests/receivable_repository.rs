//! 域 D18 `receivable` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test receivable_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::ReceivableExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    CustomerAccountId, InvoiceId, PartyId, ReceiptAllocationId, ReceivableAccountId, ReceivableEntryId,
    ReceivableEntryOffsetId, ReceivableFundsReviewId, SalesInvoiceAllocationId, SalesOrderId,
    SalesOrderRevisionId, WorkItemId,
};
use entities::money::Amount;
use entities::receivable::{
    AccountReviewStatus, AllocationAction, CustomerReceipt, CustomerReceiptData, CustomerReceiptStatus,
    EntryDirection, FundsReviewType, Invoice, InvoiceData, InvoiceDirection, InvoiceKind, ReceiptAllocation,
    ReceiptAllocationData, ReceivableAccount, ReceivableAccountData, ReceivableAccountStatus,
    ReceivableEntry, ReceivableEntryData, ReceivableEntryOffset, ReceivableEntryOffsetData,
    ReceivableEntryType, ReceivableFundsReview, ReceivableFundsReviewData, ReviewResult,
    SalesInvoiceAllocation, SalesInvoiceAllocationData,
};
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 应收往来子账列表筛选条件类型（经 `ReceivableExt` 关联类型跨 crate 可达）。
type ReceivableAccountFilter = <Database as ReceivableExt>::ReceivableAccountFilter;
/// 客户回款单列表筛选条件类型。
type CustomerReceiptFilter = <Database as ReceivableExt>::CustomerReceiptFilter;

/// 构造可复用的应收往来子账。
fn sample_account(seq: u32, gross: &str) -> ReceivableAccount {
    ReceivableAccount::new(
        ReceivableAccountId::new(format!("ra-{seq}")),
        ReceivableAccountData {
            sales_order_id: SalesOrderId::new("so-1"),
            account_seq: seq,
            customer_id: CustomerAccountId::new("cust-1"),
            counterparty_party_id: PartyId::new("party-1"),
            source_sales_order_revision_id: SalesOrderRevisionId::new("so-1-r1"),
            review_status: AccountReviewStatus::NotApplicable,
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: Amount::from_str(gross).unwrap(),
            settled_total: Amount::from_str("0.00").unwrap(),
            invoiceable_total: Amount::from_str(gross).unwrap(),
            invoiced_total: Amount::from_str("0.00").unwrap(),
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的原始应收分录。
fn sample_entry(account_id: &ReceivableAccountId, source_no: &str, amount: &str) -> ReceivableEntry {
    ReceivableEntry::new(
        ReceivableEntryId::new(format!("re-{source_no}")),
        ReceivableEntryData {
            receivable_account_id: account_id.clone(),
            entry_type: ReceivableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: Amount::from_str(amount).unwrap(),
            due_date: BusinessDate::from_ymd(2026, 9, 30).unwrap(),
            source_fact_type: "SALES_ORDER".to_string(),
            source_document_id: source_no.to_string(),
            source_revision_id: format!("{source_no}-r1"),
            source_sequence: 1,
            posted_at: Instant::from_unix_secs(1_700_000_000),
        },
    )
    .unwrap()
}

/// 构造可复用的客户回款单。
fn sample_receipt(no: &str, amount: &str) -> CustomerReceipt {
    CustomerReceipt::new(
        entities::ids::CustomerReceiptId::new(format!("cr-{no}")),
        CustomerReceiptData {
            receipt_no: no.to_string(),
            counterparty_party_id: PartyId::new("party-1"),
            customer_id: Some(CustomerAccountId::new("cust-1")),
            received_at: Instant::from_unix_secs(1_700_000_000),
            amount: Amount::from_str(amount).unwrap(),
            bank_reference: Some("BANK-1".to_string()),
        },
    )
    .unwrap()
}

/// 构造可复用的销项蓝票。
fn sample_invoice(no: &str, code: Option<&str>, gross: &str) -> Invoice {
    Invoice::new(
        InvoiceId::new(format!("inv-{no}")),
        InvoiceData {
            invoice_direction: InvoiceDirection::Sales,
            invoice_kind: InvoiceKind::Blue,
            party_id: PartyId::new("party-1"),
            invoice_code: code.map(str::to_string),
            invoice_no: no.to_string(),
            invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
            gross_amount: Amount::from_str(gross).unwrap(),
            net_amount: Amount::from_str("884.96").unwrap(),
            tax_amount: Amount::from_str("115.04").unwrap(),
            rounding_adjustment_amount: Amount::from_str("0.00").unwrap(),
            rounding_reason: None,
            original_invoice_id: None,
        },
        "admin-1",
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as ReceivableExt>::RECEIVABLE_ACCOUNTS,
        &[
            "uk_receivable_accounts_sales_order",
            "idx_receivable_accounts_aging",
            "idx_receivable_accounts_customer",
        ],
    )
    .await
    .expect("receivable_accounts 索引缺失");
    assert_indexes(
        db,
        <Database as ReceivableExt>::RECEIVABLE_ENTRIES,
        &[
            "uk_receivable_entries_identity",
            "idx_receivable_entries_account_due",
            "idx_receivable_entries_source",
        ],
    )
    .await
    .expect("receivable_entries 索引缺失");
    assert_indexes(
        db,
        <Database as ReceivableExt>::RECEIVABLE_FUNDS_REVIEWS,
        &[
            "uk_receivable_funds_reviews_account_no",
            "uk_receivable_funds_reviews_work_item",
            "uk_receivable_funds_reviews_supersedes",
        ],
    )
    .await
    .expect("receivable_funds_reviews 索引缺失");
    assert_indexes(
        db,
        <Database as ReceivableExt>::RECEIVABLE_ENTRY_OFFSETS,
        &[
            "uk_receivable_entry_offsets_decrease",
            "idx_receivable_entry_offsets_increase",
        ],
    )
    .await
    .expect("receivable_entry_offsets 索引缺失");
    assert_indexes(
        db,
        <Database as ReceivableExt>::CUSTOMER_RECEIPTS,
        &["uk_customer_receipts_no", "idx_customer_receipts_party_status"],
    )
    .await
    .expect("customer_receipts 索引缺失");
    assert_indexes(
        db,
        <Database as ReceivableExt>::RECEIPT_ALLOCATIONS,
        &[
            "uk_receipt_allocations_receipt_seq",
            "idx_receipt_allocations_entry_time",
            "idx_receipt_allocations_reverse",
        ],
    )
    .await
    .expect("receipt_allocations 索引缺失");
    assert_indexes(
        db,
        <Database as ReceivableExt>::INVOICES,
        &[
            "uk_invoices_coded",
            "uk_invoices_uncoded",
            "idx_invoices_party_status",
            "idx_invoices_original",
        ],
    )
    .await
    .expect("invoices 索引缺失");
    assert_indexes(
        db,
        <Database as ReceivableExt>::SALES_INVOICE_ALLOCATIONS,
        &[
            "uk_sales_invoice_allocations_invoice_seq",
            "idx_sales_invoice_allocations_account",
            "idx_sales_invoice_allocations_reverse",
        ],
    )
    .await
    .expect("sales_invoice_allocations 索引缺失");
}

/// 断言金额 Decimal128 往返保真（原值、小数位逐字一致）。
fn assert_amount_fidelity(actual: Amount, expected: &str) {
    assert_eq!(
        actual.to_decimal(),
        Amount::from_str(expected).unwrap().to_decimal()
    );
    assert_eq!(actual.to_decimal().to_string(), expected);
}

#[tokio::test]
#[ignore]
async fn account_create_read_roundtrip_preserves_decimal128() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let account = sample_account(1, "1234.56");
        db.receivable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(account.base.version, 1);

        let found = db
            .receivable_accounts()
            .find_by_id(&account.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_amount_fidelity(found.gross_total, "1234.56");
        assert_amount_fidelity(found.invoiceable_total, "1234.56");
        assert_amount_fidelity(found.open_total, "1234.56");
        assert_amount_fidelity(found.open_invoiceable_total, "1234.56");
        assert_eq!(found.stable.status(), ReceivableAccountStatus::Open);
        assert_eq!(found.sales_order_id, SalesOrderId::new("so-1"));
        assert_eq!(found.account_seq, 1);
    })
}

#[tokio::test]
#[ignore]
async fn optimistic_lock_success_increments_version_and_stale_fails() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_optlock").await.unwrap();
        let db = test_db.db();

        let mut account = sample_account(1, "1000.00");
        db.receivable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();

        let mut stale = account.clone();
        account
            .update(
                entities::receivable::ReceivableAccountUpdate {
                    review_status: Some(AccountReviewStatus::Reviewed),
                    reviewed_by: Some("reviewer-1".to_string()),
                    reviewed_at: Some(Instant::from_unix_secs(1_700_000_100)),
                    review_evidence_reference: Some("evid-1".to_string()),
                },
                "admin-2",
            )
            .unwrap();
        db.receivable_accounts()
            .update(&mut account, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(account.base.version, 2, "乐观锁成功后 version 递增");

        stale
            .update(
                entities::receivable::ReceivableAccountUpdate {
                    review_status: Some(AccountReviewStatus::SyncDeltaPending),
                    reviewed_by: None,
                    reviewed_at: None,
                    review_evidence_reference: None,
                },
                "admin-3",
            )
            .unwrap();
        let error = db
            .receivable_accounts()
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
async fn stable_account_soft_delete_and_restore() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_sd").await.unwrap();
        let db = test_db.db();

        let mut account = sample_account(1, "1000.00");
        db.receivable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();

        db.receivable_accounts()
            .soft_delete(&mut account, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .receivable_accounts()
            .find_by_id(&account.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.receivable_accounts()
            .restore(&mut account, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .receivable_accounts()
            .find_by_id(&account.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
        assert_eq!(account.base.version, 3);
    })
}

#[tokio::test]
#[ignore]
async fn account_and_receipt_unique_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let account = sample_account(1, "1000.00");
        db.receivable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_account = sample_account(1, "2000.00");
        let error = db
            .receivable_accounts()
            .create(&duplicate_account, &mut NoTransaction)
            .await
            .expect_err("同一 (sales_order_id, account_seq) 重复写入必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let receipt = sample_receipt("RC-1", "1000.00");
        db.customer_receipts()
            .create(&receipt, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_receipt = sample_receipt("RC-1", "1.00");
        let error = db
            .customer_receipts()
            .create(&duplicate_receipt, &mut NoTransaction)
            .await
            .expect_err("重复回款单号必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));
    })
}

#[tokio::test]
#[ignore]
async fn invoice_coded_and_uncoded_uniqueness_rules() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_inv_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let coded = sample_invoice("01234567", Some("1100"), "1000.00");
        db.invoices().create(&coded, &mut NoTransaction).await.unwrap();

        let same_coded = sample_invoice("01234567", Some("1100"), "1000.00");
        let error = db
            .invoices()
            .create(&same_coded, &mut NoTransaction)
            .await
            .expect_err("同一 (方向, 代码, 号码) 有代码发票必须被 uk_invoices_coded 拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let diff_code = sample_invoice("01234567", Some("2200"), "1000.00");
        db.invoices()
            .create(&diff_code, &mut NoTransaction)
            .await
            .expect("不同代码同号码的有代码发票不应被误判重复");

        let uncoded = sample_invoice("99999999", None, "1000.00");
        db.invoices().create(&uncoded, &mut NoTransaction).await.unwrap();
        let duplicate_uncoded = sample_invoice("99999999", None, "1000.00");
        let error = db
            .invoices()
            .create(&duplicate_uncoded, &mut NoTransaction)
            .await
            .expect_err("无代码数电票重复号码必须被 uk_invoices_uncoded 拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));
    })
}

#[tokio::test]
#[ignore]
async fn conditional_settlement_never_over_writeoffs() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_writeoff").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let account = sample_account(1, "1000.00");
        db.receivable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        let account_id = account.base.id.clone();

        let applied = db
            .receivable_accounts()
            .apply_settlement(
                &account_id,
                &Amount::from_str("400.00").unwrap(),
                "system",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(applied, "额度内核销应生效");
        let mut found = db
            .receivable_accounts()
            .find_by_id(&account_id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_amount_fidelity(found.settled_total, "400.00");
        assert_amount_fidelity(found.open_total, "600.00");
        assert_eq!(found.stable.status(), ReceivableAccountStatus::PartiallySettled);

        let applied = db
            .receivable_accounts()
            .apply_settlement(
                &account_id,
                &Amount::from_str("600.00").unwrap(),
                "system",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(applied);
        found = db
            .receivable_accounts()
            .find_by_id(&account_id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_amount_fidelity(found.open_total, "0.00");
        assert_eq!(found.stable.status(), ReceivableAccountStatus::Settled);

        let rejected = db
            .receivable_accounts()
            .apply_settlement(
                &account_id,
                &Amount::from_str("0.01").unwrap(),
                "system",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(!rejected, "超额核销必须整体拒绝");
        found = db
            .receivable_accounts()
            .find_by_id(&account_id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_amount_fidelity(found.settled_total, "1000.00");
        assert_amount_fidelity(found.open_total, "0.00");
        assert_eq!(
            found.stable.status(),
            ReceivableAccountStatus::Settled,
            "拒绝后状态不得变化"
        );

        let over_revert = db
            .receivable_accounts()
            .revert_settlement(
                &account_id,
                &Amount::from_str("1000.01").unwrap(),
                "system",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(!over_revert, "冲减超过已核销必须整体拒绝");
        let reverted = db
            .receivable_accounts()
            .revert_settlement(
                &account_id,
                &Amount::from_str("1000.00").unwrap(),
                "system",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(reverted);
        found = db
            .receivable_accounts()
            .find_by_id(&account_id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_amount_fidelity(found.settled_total, "0.00");
        assert_eq!(found.stable.status(), ReceivableAccountStatus::Open);
    })
}

#[tokio::test]
#[ignore]
async fn conditional_invoicing_never_exceeds_invoiceable_total() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_invoice_prog").await.unwrap();
        let db = test_db.db();

        let account = sample_account(1, "1000.00");
        db.receivable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        let account_id = account.base.id.clone();

        let applied = db
            .receivable_accounts()
            .apply_invoicing(
                &account_id,
                &Amount::from_str("700.00").unwrap(),
                "system",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(applied);
        let found = db
            .receivable_accounts()
            .find_by_id(&account_id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_amount_fidelity(found.invoiced_total, "700.00");
        assert_amount_fidelity(found.open_invoiceable_total, "300.00");

        let rejected = db
            .receivable_accounts()
            .apply_invoicing(
                &account_id,
                &Amount::from_str("300.01").unwrap(),
                "system",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(!rejected, "超过可开票额度必须整体拒绝");

        let over_revert = db
            .receivable_accounts()
            .revert_invoicing(
                &account_id,
                &Amount::from_str("700.01").unwrap(),
                "system",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(!over_revert);
        let reverted = db
            .receivable_accounts()
            .revert_invoicing(
                &account_id,
                &Amount::from_str("700.00").unwrap(),
                "system",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(reverted);
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_create_commits_and_rolls_back_atomically() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let db_clone = db.clone();
        let account = sample_account(1, "1000.00");
        let entry = sample_entry(&account.base.id.clone().into(), "SO-1", "1000.00");
        let account_for_tx = account.clone();
        let entry_for_tx = entry.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .receivable()
                        .create_receivable_with_entry(&account_for_tx, &entry_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");
        assert!(db
            .receivable_accounts()
            .find_by_id(&account.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
        assert!(db
            .receivable_entries()
            .find_by_id(&entry.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());

        let db_clone = db.clone();
        let account2 = sample_account(2, "500.00");
        let entry2 = sample_entry(&account2.base.id.clone().into(), "SO-2", "500.00");
        let account_for_tx = account2.clone();
        let entry_for_tx = entry2.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .receivable()
                        .create_receivable_with_entry(&account_for_tx, &entry_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");
        assert!(
            db.receivable_accounts()
                .find_by_id(&account2.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后子账不得残留"
        );
        assert!(
            db.receivable_entries()
                .find_by_id(&entry2.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后分录不得残留"
        );
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_without_transaction_leaves_predictable_partial_state() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let account = sample_account(1, "1000.00");
        db.receivable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        let entry = sample_entry(&account.base.id.clone().into(), "SO-1", "1000.00");
        db.receivable_entries()
            .create(&entry, &mut NoTransaction)
            .await
            .unwrap();

        let account2 = sample_account(2, "500.00");
        let duplicate_entry = sample_entry(&account.base.id.clone().into(), "SO-1", "500.00");
        let error = db
            .receivable()
            .create_receivable_with_entry(&account2, &duplicate_entry, &mut NoTransaction)
            .await
            .expect_err("第二笔写入违反业务幂等唯一索引");
        assert!(matches!(error, database::Error::DuplicateKey(_)));
        assert!(
            db.receivable_accounts()
                .find_by_id(&account2.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "NoTransaction 下第一笔自动提交，残留半成品（可预期行为，Service 必须传事务）"
        );
    })
}

#[tokio::test]
#[ignore]
async fn funds_review_chain_append_locks_tail_and_rejects_forks() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_review_chain").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let account = sample_account(1, "1000.00");
        db.receivable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        let account_id: ReceivableAccountId = account.base.id.clone().into();

        let head = ReceivableFundsReview::new(
            ReceivableFundsReviewId::new("fr-1"),
            ReceivableFundsReviewData {
                receivable_account_id: account_id.clone(),
                review_no: 1,
                review_type: FundsReviewType::Opening,
                work_item_id: WorkItemId::new("wi-1"),
                evidence_document_id: None,
                evidence_reference: Some("bank-evidence-1".to_string()),
                review_result: ReviewResult::Passed,
                reviewed_by: "reviewer-1".to_string(),
                reviewed_at: Instant::from_unix_secs(1_700_000_000),
                supersedes_review_id: None,
            },
        )
        .unwrap();
        let head_id = head.base.id.clone();

        let db_clone = db.clone();
        let head_for_tx = head.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .receivable()
                        .append_funds_review(&head_for_tx, session)
                        .await
                })
            })
            .await
            .expect("链头追加应成功");
        assert_eq!(
            db.receivable_funds_reviews()
                .find_reviews_by_account(&account_id, &mut NoTransaction)
                .await
                .unwrap()
                .len(),
            1
        );

        let db_clone = db.clone();
        let account_id_for_tx = account_id.clone();
        let head_id_for_tx = head_id.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    let tail = db_clone
                        .receivable_funds_reviews()
                        .find_reviews_by_account(&account_id_for_tx, session)
                        .await?
                        .pop()
                        .expect("链头应存在");
                    let fork = ReceivableFundsReview::new(
                        ReceivableFundsReviewId::new("fr-fork"),
                        ReceivableFundsReviewData {
                            receivable_account_id: account_id_for_tx.clone(),
                            review_no: 2,
                            review_type: FundsReviewType::SyncDelta,
                            work_item_id: WorkItemId::new("wi-fork"),
                            evidence_document_id: None,
                            evidence_reference: Some("delta-evidence".to_string()),
                            review_result: ReviewResult::Passed,
                            reviewed_by: "reviewer-2".to_string(),
                            reviewed_at: Instant::from_unix_secs(1_700_000_100),
                            supersedes_review_id: Some(ReceivableFundsReviewId::new(&tail.base.id)),
                        },
                    )
                    .unwrap();
                    db_clone.receivable().append_funds_review(&fork, session).await?;
                    let fork2 = ReceivableFundsReview::new(
                        ReceivableFundsReviewId::new("fr-fork2"),
                        ReceivableFundsReviewData {
                            receivable_account_id: account_id_for_tx.clone(),
                            review_no: 2,
                            review_type: FundsReviewType::SyncDelta,
                            work_item_id: WorkItemId::new("wi-fork2"),
                            evidence_document_id: None,
                            evidence_reference: Some("delta-evidence-2".to_string()),
                            review_result: ReviewResult::Passed,
                            reviewed_by: "reviewer-3".to_string(),
                            reviewed_at: Instant::from_unix_secs(1_700_000_200),
                            supersedes_review_id: Some(ReceivableFundsReviewId::new(&head_id_for_tx)),
                        },
                    )
                    .unwrap();
                    db_clone.receivable().append_funds_review(&fork2, session).await
                })
            })
            .await
            .expect_err("同号复核重复追加（链尾已被占用）必须失败并整体回滚");

        let reviews = db
            .receivable_funds_reviews()
            .find_reviews_by_account(&account_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(reviews.len(), 1, "回滚后只保留链头，分叉不得残留");
        assert_eq!(reviews[0].review_no, 1);

        let db_clone = db.clone();
        let account_id_for_tx = account_id.clone();
        let second = ReceivableFundsReview::new(
            ReceivableFundsReviewId::new("fr-2"),
            ReceivableFundsReviewData {
                receivable_account_id: account_id_for_tx.clone(),
                review_no: 2,
                review_type: FundsReviewType::SyncDelta,
                work_item_id: WorkItemId::new("wi-2"),
                evidence_document_id: None,
                evidence_reference: Some("delta-evidence".to_string()),
                review_result: ReviewResult::Passed,
                reviewed_by: "reviewer-2".to_string(),
                reviewed_at: Instant::from_unix_secs(1_700_000_100),
                supersedes_review_id: Some(ReceivableFundsReviewId::new(&head_id)),
            },
        )
        .unwrap();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move { db_clone.receivable().append_funds_review(&second, session).await })
            })
            .await
            .expect("正确锁定链尾的追加应成功");
        let reviews = db
            .receivable_funds_reviews()
            .find_reviews_by_account(&account_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(reviews.len(), 2);
        assert_eq!(reviews[1].review_no, 2);
        assert_eq!(
            reviews[1].supersedes_review_id,
            Some(ReceivableFundsReviewId::new(&head_id))
        );
    })
}

#[tokio::test]
#[ignore]
async fn account_list_respects_pagination_sort_whitelist_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        for seq in 1..=3 {
            let mut account = sample_account(seq, "1000.00");
            if seq == 3 {
                account.stable.status = ReceivableAccountStatus::Settled;
                account.open_total = Amount::from_str("0.00").unwrap();
            }
            db.receivable_accounts()
                .create(&account, &mut NoTransaction)
                .await
                .unwrap();
        }

        let filter = ReceivableAccountFilter {
            customer_id: Some(CustomerAccountId::new("cust-1")),
            counterparty_party_id: None,
            status: None,
            sales_order_id: None,
            page: 2,
            page_size: 2,
            sort_by: Some("account_seq".to_string()),
            sort_ascending: true,
        };
        let page = db
            .receivable_accounts()
            .search_receivable_accounts(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 1, "第二页只应剩一条");
        assert_eq!(page.items[0].account_seq, 3);

        let no_match = ReceivableAccountFilter {
            customer_id: Some(CustomerAccountId::new("cust-不存在")),
            counterparty_party_id: None,
            status: None,
            sales_order_id: None,
            page: 1,
            page_size: 20,
            sort_by: Some("$where".to_string()),
            sort_ascending: false,
        };
        let empty = db
            .receivable_accounts()
            .search_receivable_accounts(&no_match, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(empty.total, 0);

        let settled = ReceivableAccountFilter {
            customer_id: None,
            counterparty_party_id: None,
            status: Some(ReceivableAccountStatus::Settled),
            sales_order_id: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let rows = db
            .receivable_accounts()
            .search_receivable_accounts(&settled, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(rows.total, 1);
        let row = &rows.items[0];
        assert_eq!(row.id, "ra-3");
        assert_eq!(row.customer_id, "cust-1");
        assert_eq!(row.counterparty_party_id, "party-1");
        assert_eq!(row.stable.status(), ReceivableAccountStatus::Settled);
        assert_amount_fidelity(row.gross_total, "1000.00");
        assert!(row.version >= 1);
        assert!(row.created_at > 0);
    })
}

#[tokio::test]
#[ignore]
async fn receipt_list_filters_by_regex_and_status() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_receipt_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut posted = sample_receipt("RC-2026-001", "1000.00");
        posted.transition(CustomerReceiptStatus::Posted).unwrap();
        db.customer_receipts()
            .create(&sample_receipt("RC-2026-002", "500.00"), &mut NoTransaction)
            .await
            .unwrap();
        db.customer_receipts()
            .create(&posted, &mut NoTransaction)
            .await
            .unwrap();

        let filter = CustomerReceiptFilter {
            receipt_no: Some("RC-2026".to_string()),
            counterparty_party_id: Some(PartyId::new("party-1")),
            status: Some(CustomerReceiptStatus::Posted),
            page: 1,
            page_size: 20,
            sort_by: Some("received_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .customer_receipts()
            .search_customer_receipts(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        let row = &page.items[0];
        assert_eq!(row.receipt_no, "RC-2026-001");
        assert_eq!(row.status, CustomerReceiptStatus::Posted);
        assert_amount_fidelity(row.amount, "1000.00");

        let found = db
            .customer_receipts()
            .find_by_receipt_no("RC-2026-002", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按单号应可读回");
        assert_amount_fidelity(found.amount, "500.00");
    })
}

#[tokio::test]
#[ignore]
async fn entry_offsets_and_allocations_batch_queries_avoid_n_plus_one() {
    require_mongo!(async {
        let test_db = TestDb::new("rcv_batch").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let account = sample_account(1, "1000.00");
        let entry = sample_entry(&account.base.id.clone().into(), "SO-1", "1000.00");
        db.receivable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        db.receivable_entries()
            .create(&entry, &mut NoTransaction)
            .await
            .unwrap();
        let entry_id: ReceivableEntryId = entry.base.id.clone().into();

        let decrease = ReceivableEntry::new(
            ReceivableEntryId::new("re-dec"),
            ReceivableEntryData {
                receivable_account_id: account.base.id.clone().into(),
                entry_type: ReceivableEntryType::Refund,
                direction: EntryDirection::Decrease,
                amount: Amount::from_str("100.00").unwrap(),
                due_date: BusinessDate::from_ymd(2026, 10, 31).unwrap(),
                source_fact_type: "REFUND".to_string(),
                source_document_id: "RF-1".to_string(),
                source_revision_id: "RF-1-r1".to_string(),
                source_sequence: 1,
                posted_at: Instant::from_unix_secs(1_700_000_000),
            },
        )
        .unwrap();
        let decrease_id: ReceivableEntryId = decrease.base.id.clone().into();
        db.receivable_entries()
            .create(&decrease, &mut NoTransaction)
            .await
            .unwrap();
        let offset = ReceivableEntryOffset::new(
            ReceivableEntryOffsetId::new("oe-1"),
            ReceivableEntryOffsetData {
                decrease_entry_id: decrease_id.clone(),
                increase_entry_id: entry_id.clone(),
                offset_sequence: 1,
                offset_amount: Amount::from_str("100.00").unwrap(),
            },
        )
        .unwrap();
        db.receivable_entry_offsets()
            .create(&offset, &mut NoTransaction)
            .await
            .unwrap();

        let receipt = sample_receipt("RC-1", "1000.00");
        let receipt_id: entities::ids::CustomerReceiptId = receipt.base.id.clone().into();
        db.customer_receipts()
            .create(&receipt, &mut NoTransaction)
            .await
            .unwrap();
        let allocation = ReceiptAllocation::new(
            ReceiptAllocationId::new("ra-all-1"),
            ReceiptAllocationData {
                customer_receipt_id: receipt_id.clone(),
                receivable_entry_id: entry_id.clone(),
                allocation_seq: 1,
                allocation_action: AllocationAction::Apply,
                allocated_amount: Amount::from_str("1000.00").unwrap(),
                allocated_at: Instant::from_unix_secs(1_700_000_000),
                reverses_allocation_id: None,
            },
        )
        .unwrap();
        db.receipt_allocations()
            .create(&allocation, &mut NoTransaction)
            .await
            .unwrap();

        let invoice = sample_invoice("INV-1", Some("1100"), "1000.00");
        let invoice_id: InvoiceId = invoice.base.id.clone().into();
        db.invoices().create(&invoice, &mut NoTransaction).await.unwrap();
        let sales_allocation = SalesInvoiceAllocation::new(
            SalesInvoiceAllocationId::new("si-all-1"),
            SalesInvoiceAllocationData {
                invoice_id: invoice_id.clone(),
                receivable_account_id: account.base.id.clone().into(),
                allocation_seq: 1,
                allocation_action: AllocationAction::Apply,
                allocated_gross_amount: Amount::from_str("1000.00").unwrap(),
                allocated_net_amount: Amount::from_str("884.96").unwrap(),
                allocated_tax_amount: Amount::from_str("115.04").unwrap(),
                reverses_allocation_id: None,
            },
        )
        .unwrap();
        db.sales_invoice_allocations()
            .create(&sales_allocation, &mut NoTransaction)
            .await
            .unwrap();

        let offsets = db
            .receivable_entry_offsets()
            .find_offsets_by_decrease(&decrease_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(offsets.len(), 1);
        assert_amount_fidelity(offsets[0].offset_amount, "100.00");

        let reverse_sources = db
            .receivable_entry_offsets()
            .find_offsets_by_increase(&entry_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(reverse_sources.len(), 1);

        let allocations = db
            .receipt_allocations()
            .find_allocations_by_receipts(std::slice::from_ref(&receipt_id), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(allocations.len(), 1);
        assert_amount_fidelity(allocations[0].allocated_amount, "1000.00");

        let invoice_allocations = db
            .sales_invoice_allocations()
            .find_allocations_by_invoices(std::slice::from_ref(&invoice_id), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(invoice_allocations.len(), 1);

        let red_invoices = db
            .invoices()
            .find_red_invoices_by_original(&invoice_id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(red_invoices.is_empty());
    })
}
