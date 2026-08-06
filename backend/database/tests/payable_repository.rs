//! 域 D19 `payable` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test payable_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::PayableExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    InvoiceId, PayableAccountId, PayableEntryId, PayableEntryOffsetId, PaymentAllocationId,
    PurchaseInvoiceAllocationId, SupplierAccountId, SupplierPaymentId,
};
use entities::money::Amount;
use entities::payable::{
    AllocationAction, EntryDirection, PayableAccount, PayableAccountData, PayableAccountStatus, PayableEntry,
    PayableEntryData, PayableEntryOffset, PayableEntryOffsetData, PayableEntryType, PayableSourceType,
    PaymentAllocation, PaymentAllocationData, PurchaseInvoiceAllocation, PurchaseInvoiceAllocationData,
    SupplierPayment, SupplierPaymentData, SupplierPaymentStatus,
};
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 应付往来子账列表筛选条件类型（经 `PayableExt` 关联类型跨 crate 可达）。
type PayableAccountFilter = <Database as PayableExt>::PayableAccountFilter;
/// 供应商付款单列表筛选条件类型。
type SupplierPaymentFilter = <Database as PayableExt>::SupplierPaymentFilter;

/// 构造可复用的应付往来子账。
fn sample_account(seq: u32, gross: &str) -> PayableAccount {
    PayableAccount::new(
        PayableAccountId::new(format!("pa-{seq}")),
        PayableAccountData {
            source_document_id: format!("PO-{seq}"),
            supplier_id: SupplierAccountId::new("sup-1"),
            source_type: PayableSourceType::PurchaseOrder,
            gross_total: Amount::from_str(gross).unwrap(),
            settled_total: Amount::from_str("0.00").unwrap(),
            invoiceable_total: Amount::from_str(gross).unwrap(),
            invoiced_total: Amount::from_str("0.00").unwrap(),
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的原始应付分录。
fn sample_entry(account_id: &PayableAccountId, source_no: &str, amount: &str) -> PayableEntry {
    PayableEntry::new(
        PayableEntryId::new(format!("pe-{source_no}")),
        PayableEntryData {
            payable_account_id: account_id.clone(),
            entry_type: PayableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: Amount::from_str(amount).unwrap(),
            due_date: BusinessDate::from_ymd(2026, 9, 30).unwrap(),
            source_fact_type: "PURCHASE_ORDER".to_string(),
            source_document_id: source_no.to_string(),
            source_revision_id: format!("{source_no}-r1"),
            source_sequence: 1,
            posted_at: Instant::from_unix_secs(1_700_000_000),
        },
    )
    .unwrap()
}

/// 构造可复用的供应商付款单。
fn sample_payment(no: &str, amount: &str) -> SupplierPayment {
    SupplierPayment::new(
        SupplierPaymentId::new(format!("sp-{no}")),
        SupplierPaymentData {
            payment_no: no.to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            paid_at: Instant::from_unix_secs(1_700_000_000),
            amount: Amount::from_str(amount).unwrap(),
            bank_reference: Some("BANK-1".to_string()),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as PayableExt>::PAYABLE_ACCOUNTS,
        &["uk_payable_accounts_source", "idx_payable_accounts_aging"],
    )
    .await
    .expect("payable_accounts 索引缺失");
    assert_indexes(
        db,
        <Database as PayableExt>::PAYABLE_ENTRIES,
        &[
            "uk_payable_entries_identity",
            "idx_payable_entries_account_due",
            "idx_payable_entries_source",
        ],
    )
    .await
    .expect("payable_entries 索引缺失");
    assert_indexes(
        db,
        <Database as PayableExt>::PAYABLE_ENTRY_OFFSETS,
        &[
            "uk_payable_entry_offsets_decrease",
            "idx_payable_entry_offsets_increase",
        ],
    )
    .await
    .expect("payable_entry_offsets 索引缺失");
    assert_indexes(
        db,
        <Database as PayableExt>::SUPPLIER_PAYMENTS,
        &["uk_supplier_payments_no", "idx_supplier_payments_supplier_status"],
    )
    .await
    .expect("supplier_payments 索引缺失");
    assert_indexes(
        db,
        <Database as PayableExt>::PAYMENT_ALLOCATIONS,
        &[
            "uk_payment_allocations_payment_seq",
            "idx_payment_allocations_entry_time",
            "idx_payment_allocations_reverse",
        ],
    )
    .await
    .expect("payment_allocations 索引缺失");
    assert_indexes(
        db,
        <Database as PayableExt>::PURCHASE_INVOICE_ALLOCATIONS,
        &[
            "uk_purchase_invoice_allocations_invoice_seq",
            "idx_purchase_invoice_allocations_account",
            "idx_purchase_invoice_allocations_reverse",
        ],
    )
    .await
    .expect("purchase_invoice_allocations 索引缺失");
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
async fn account_create_read_roundtrip_preserves_decimal128_and_time() {
    require_mongo!(async {
        let test_db = TestDb::new("pay_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let account = sample_account(1, "1234.56");
        db.payable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(account.base.version, 1);

        let found = db
            .payable_accounts()
            .find_by_id(&account.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_amount_fidelity(found.gross_total, "1234.56");
        assert_amount_fidelity(found.settled_total, "0.00");
        assert_amount_fidelity(found.open_total, "1234.56");
        assert_amount_fidelity(found.invoiceable_total, "1234.56");
        assert_amount_fidelity(found.invoiced_total, "0.00");
        assert_amount_fidelity(found.open_invoiceable_total, "1234.56");
        assert_eq!(found.stable.status(), PayableAccountStatus::Open);
        assert_eq!(found.supplier_id, SupplierAccountId::new("sup-1"));
        assert_eq!(found.source_type, PayableSourceType::PurchaseOrder);

        let account_id: PayableAccountId = account.base.id.clone().into();
        let entry = sample_entry(&account_id, "PO-1", "1234.56");
        db.payable_entries()
            .create(&entry, &mut NoTransaction)
            .await
            .unwrap();
        let found_entry = db
            .payable_entries()
            .find_by_id(&entry.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("分录应可读回");
        assert_amount_fidelity(found_entry.amount, "1234.56");
        assert_eq!(
            found_entry.due_date,
            BusinessDate::from_ymd(2026, 9, 30).unwrap(),
            "业务日期字段必须往返一致"
        );
        assert_eq!(
            found_entry.posted_at,
            Instant::from_unix_secs(1_700_000_000),
            "入账时间字段必须往返一致"
        );
    })
}

#[tokio::test]
#[ignore]
async fn optimistic_lock_success_increments_version_and_stale_fails() {
    require_mongo!(async {
        let test_db = TestDb::new("pay_optlock").await.unwrap();
        let db = test_db.db();

        let mut account = sample_account(1, "1000.00");
        db.payable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();

        let mut stale = account.clone();
        account
            .update(entities::payable::PayableAccountUpdate::default(), "admin-2")
            .unwrap();
        db.payable_accounts()
            .update(&mut account, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(account.base.version, 2, "乐观锁成功后 version 递增");

        stale
            .update(entities::payable::PayableAccountUpdate::default(), "admin-3")
            .unwrap();
        let error = db
            .payable_accounts()
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
        let test_db = TestDb::new("pay_sd").await.unwrap();
        let db = test_db.db();

        let mut account = sample_account(1, "1000.00");
        db.payable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();

        db.payable_accounts()
            .soft_delete(&mut account, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .payable_accounts()
            .find_by_id(&account.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.payable_accounts()
            .restore(&mut account, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .payable_accounts()
            .find_by_id(&account.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
        assert_eq!(account.base.version, 3);
    })
}

#[tokio::test]
#[ignore]
async fn account_and_payment_unique_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("pay_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let account = sample_account(1, "1000.00");
        db.payable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_account = sample_account(1, "2000.00");
        let error = db
            .payable_accounts()
            .create(&duplicate_account, &mut NoTransaction)
            .await
            .expect_err("同一 (source_type, source_document_id) 重复写入必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        let payment = sample_payment("PAY-1", "1000.00");
        db.supplier_payments()
            .create(&payment, &mut NoTransaction)
            .await
            .unwrap();
        let duplicate_payment = sample_payment("PAY-1", "1.00");
        let error = db
            .supplier_payments()
            .create(&duplicate_payment, &mut NoTransaction)
            .await
            .expect_err("重复付款单号必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));
    })
}

#[tokio::test]
#[ignore]
async fn conditional_settlement_never_over_writeoffs() {
    require_mongo!(async {
        let test_db = TestDb::new("pay_writeoff").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let account = sample_account(1, "1000.00");
        db.payable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        let account_id = account.base.id.clone();

        let applied = db
            .payable_accounts()
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
            .payable_accounts()
            .find_by_id(&account_id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_amount_fidelity(found.settled_total, "400.00");
        assert_amount_fidelity(found.open_total, "600.00");
        assert_eq!(found.stable.status(), PayableAccountStatus::PartiallySettled);

        let applied = db
            .payable_accounts()
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
            .payable_accounts()
            .find_by_id(&account_id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_amount_fidelity(found.open_total, "0.00");
        assert_eq!(found.stable.status(), PayableAccountStatus::Settled);

        let rejected = db
            .payable_accounts()
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
            .payable_accounts()
            .find_by_id(&account_id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_amount_fidelity(found.settled_total, "1000.00");
        assert_amount_fidelity(found.open_total, "0.00");
        assert_eq!(
            found.stable.status(),
            PayableAccountStatus::Settled,
            "拒绝后状态不得变化"
        );

        let over_revert = db
            .payable_accounts()
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
            .payable_accounts()
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
            .payable_accounts()
            .find_by_id(&account_id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_amount_fidelity(found.settled_total, "0.00");
        assert_eq!(found.stable.status(), PayableAccountStatus::Open);
    })
}

#[tokio::test]
#[ignore]
async fn conditional_invoicing_never_exceeds_invoiceable_total() {
    require_mongo!(async {
        let test_db = TestDb::new("pay_invoice_prog").await.unwrap();
        let db = test_db.db();

        let account = sample_account(1, "1000.00");
        db.payable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        let account_id = account.base.id.clone();

        let applied = db
            .payable_accounts()
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
            .payable_accounts()
            .find_by_id(&account_id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_amount_fidelity(found.invoiced_total, "700.00");
        assert_amount_fidelity(found.open_invoiceable_total, "300.00");

        let rejected = db
            .payable_accounts()
            .apply_invoicing(
                &account_id,
                &Amount::from_str("300.01").unwrap(),
                "system",
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(!rejected, "超过可收票额度必须整体拒绝");

        let over_revert = db
            .payable_accounts()
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
            .payable_accounts()
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
        let test_db = TestDb::new("pay_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let db_clone = db.clone();
        let account = sample_account(1, "1000.00");
        let entry = sample_entry(&account.base.id.clone().into(), "PO-1", "1000.00");
        let account_for_tx = account.clone();
        let entry_for_tx = entry.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .payable()
                        .create_payable_with_entry(&account_for_tx, &entry_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");
        assert!(db
            .payable_accounts()
            .find_by_id(&account.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
        assert!(db
            .payable_entries()
            .find_by_id(&entry.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());

        let db_clone = db.clone();
        let account2 = sample_account(2, "500.00");
        let entry2 = sample_entry(&account2.base.id.clone().into(), "PO-2", "500.00");
        let account_for_tx = account2.clone();
        let entry_for_tx = entry2.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .payable()
                        .create_payable_with_entry(&account_for_tx, &entry_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");
        assert!(
            db.payable_accounts()
                .find_by_id(&account2.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_none(),
            "回滚后子账不得残留"
        );
        assert!(
            db.payable_entries()
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
        let test_db = TestDb::new("pay_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let account = sample_account(1, "1000.00");
        db.payable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        let entry = sample_entry(&account.base.id.clone().into(), "PO-1", "1000.00");
        db.payable_entries()
            .create(&entry, &mut NoTransaction)
            .await
            .unwrap();

        let account2 = sample_account(2, "500.00");
        let duplicate_entry = sample_entry(&account.base.id.clone().into(), "PO-1", "500.00");
        let error = db
            .payable()
            .create_payable_with_entry(&account2, &duplicate_entry, &mut NoTransaction)
            .await
            .expect_err("第二笔写入违反业务幂等唯一索引");
        assert!(matches!(error, database::Error::DuplicateKey(_)));
        assert!(
            db.payable_accounts()
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
async fn entry_offsets_and_allocations_batch_queries_avoid_n_plus_one() {
    require_mongo!(async {
        let test_db = TestDb::new("pay_batch").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let account = sample_account(1, "1000.00");
        let entry = sample_entry(&account.base.id.clone().into(), "PO-1", "1000.00");
        db.payable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();
        db.payable_entries()
            .create(&entry, &mut NoTransaction)
            .await
            .unwrap();
        let account_id: PayableAccountId = account.base.id.clone().into();
        let entry_id: PayableEntryId = entry.base.id.clone().into();

        let decrease = PayableEntry::new(
            PayableEntryId::new("pe-dec"),
            PayableEntryData {
                payable_account_id: account_id.clone(),
                entry_type: PayableEntryType::SupplierRefund,
                direction: EntryDirection::Decrease,
                amount: Amount::from_str("100.00").unwrap(),
                due_date: BusinessDate::from_ymd(2026, 10, 31).unwrap(),
                source_fact_type: "SUPPLIER_REFUND".to_string(),
                source_document_id: "SRF-1".to_string(),
                source_revision_id: "SRF-1-r1".to_string(),
                source_sequence: 1,
                posted_at: Instant::from_unix_secs(1_700_000_000),
            },
        )
        .unwrap();
        let decrease_id: PayableEntryId = decrease.base.id.clone().into();
        db.payable_entries()
            .create(&decrease, &mut NoTransaction)
            .await
            .unwrap();
        let offset = PayableEntryOffset::new(
            PayableEntryOffsetId::new("oe-1"),
            PayableEntryOffsetData {
                decrease_entry_id: decrease_id.clone(),
                increase_entry_id: entry_id.clone(),
                offset_sequence: 1,
                offset_amount: Amount::from_str("100.00").unwrap(),
            },
        )
        .unwrap();
        db.payable_entry_offsets()
            .create(&offset, &mut NoTransaction)
            .await
            .unwrap();

        let payment = sample_payment("PAY-1", "1000.00");
        let payment_id: SupplierPaymentId = payment.base.id.clone().into();
        db.supplier_payments()
            .create(&payment, &mut NoTransaction)
            .await
            .unwrap();
        let allocation = PaymentAllocation::new(
            PaymentAllocationId::new("pa-all-1"),
            PaymentAllocationData {
                supplier_payment_id: payment_id.clone(),
                payable_entry_id: entry_id.clone(),
                allocation_seq: 1,
                allocation_action: AllocationAction::Apply,
                allocated_amount: Amount::from_str("1000.00").unwrap(),
                allocated_at: Instant::from_unix_secs(1_700_000_000),
                reverses_allocation_id: None,
            },
        )
        .unwrap();
        db.payment_allocations()
            .create(&allocation, &mut NoTransaction)
            .await
            .unwrap();

        let invoice_allocation = PurchaseInvoiceAllocation::new(
            PurchaseInvoiceAllocationId::new("pi-all-1"),
            PurchaseInvoiceAllocationData {
                invoice_id: InvoiceId::new("inv-9"),
                payable_account_id: account_id.clone(),
                allocation_seq: 1,
                allocation_action: AllocationAction::Apply,
                allocated_gross_amount: Amount::from_str("250.00").unwrap(),
                allocated_net_amount: Amount::from_str("235.85").unwrap(),
                allocated_tax_amount: Amount::from_str("14.15").unwrap(),
                reverses_allocation_id: None,
            },
        )
        .unwrap();
        db.purchase_invoice_allocations()
            .create(&invoice_allocation, &mut NoTransaction)
            .await
            .unwrap();

        let entries = db
            .payable_entries()
            .find_entries_by_accounts(std::slice::from_ref(&account_id), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(entries.len(), 2, "$in 一次取回，不得 N+1");
        let ordered = db
            .payable_entries()
            .find_entries_by_account(&account_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(ordered.len(), 2);
        assert_eq!(ordered[0].source_sequence, 1, "按来源序号升序");

        let offsets = db
            .payable_entry_offsets()
            .find_offsets_by_decrease(&decrease_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(offsets.len(), 1);
        assert_amount_fidelity(offsets[0].offset_amount, "100.00");

        let reverse_sources = db
            .payable_entry_offsets()
            .find_offsets_by_increase(&entry_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(reverse_sources.len(), 1);

        let allocations = db
            .payment_allocations()
            .find_allocations_by_payments(&[payment_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(allocations.len(), 1);
        assert_amount_fidelity(allocations[0].allocated_amount, "1000.00");

        let entry_allocations = db
            .payment_allocations()
            .find_allocations_by_entries(&[entry_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(entry_allocations.len(), 1);

        let invoice_allocations = db
            .purchase_invoice_allocations()
            .find_allocations_by_invoices(&[InvoiceId::new("inv-9")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(invoice_allocations.len(), 1);

        let account_allocations = db
            .purchase_invoice_allocations()
            .find_allocations_by_accounts(&[account_id], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(account_allocations.len(), 1);
    })
}

#[tokio::test]
#[ignore]
async fn account_list_respects_pagination_sort_whitelist_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("pay_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let gross = ["1000.00", "800.00", "600.00"];
        for (seq, gross) in gross.iter().enumerate() {
            let seq = seq as u32 + 1;
            let mut account = sample_account(seq, gross);
            if seq == 3 {
                account.stable.status = PayableAccountStatus::Settled;
                account.open_total = Amount::from_str("0.00").unwrap();
            }
            db.payable_accounts()
                .create(&account, &mut NoTransaction)
                .await
                .unwrap();
        }

        let filter = PayableAccountFilter {
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            source_type: Some(PayableSourceType::PurchaseOrder),
            status: None,
            page: 2,
            page_size: 2,
            sort_by: Some("open_total".to_string()),
            sort_ascending: true,
        };
        let page = db
            .payable_accounts()
            .search_payable_accounts(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 1, "第二页只应剩一条");
        assert_eq!(
            page.items[0].id, "pa-1",
            "按 open_total 升序第二页为开放余额最大的子账"
        );

        let whitelist_fallback = PayableAccountFilter {
            supplier_id: None,
            source_type: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: Some("$where".to_string()),
            sort_ascending: false,
        };
        let page = db
            .payable_accounts()
            .search_payable_accounts(&whitelist_fallback, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 3, "非白名单排序回退 created_at 降序，仍返回全部");

        let settled = PayableAccountFilter {
            supplier_id: None,
            source_type: None,
            status: Some(PayableAccountStatus::Settled),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let rows = db
            .payable_accounts()
            .search_payable_accounts(&settled, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(rows.total, 1);
        let row = &rows.items[0];
        assert_eq!(row.id, "pa-3");
        assert_eq!(row.supplier_id, "sup-1");
        assert_eq!(row.source_type, PayableSourceType::PurchaseOrder);
        assert_eq!(row.stable.status(), PayableAccountStatus::Settled);
        assert_amount_fidelity(row.gross_total, "600.00");
        assert_amount_fidelity(row.open_total, "0.00");
        assert!(row.version >= 1);
        assert!(row.created_at > 0);
    })
}

#[tokio::test]
#[ignore]
async fn payment_list_filters_by_regex_and_status() {
    require_mongo!(async {
        let test_db = TestDb::new("pay_payment_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut posted = sample_payment("PAY-2026-001", "1000.00");
        posted.transition(SupplierPaymentStatus::Posted).unwrap();
        db.supplier_payments()
            .create(&sample_payment("PAY-2026-002", "500.00"), &mut NoTransaction)
            .await
            .unwrap();
        db.supplier_payments()
            .create(&posted, &mut NoTransaction)
            .await
            .unwrap();

        let filter = SupplierPaymentFilter {
            payment_no: Some("PAY-2026".to_string()),
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            status: Some(SupplierPaymentStatus::Posted),
            page: 1,
            page_size: 20,
            sort_by: Some("paid_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .supplier_payments()
            .search_supplier_payments(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        let row = &page.items[0];
        assert_eq!(row.payment_no, "PAY-2026-001");
        assert_eq!(row.status, SupplierPaymentStatus::Posted);
        assert_eq!(row.supplier_id, "sup-1");
        assert_amount_fidelity(row.amount, "1000.00");

        let found = db
            .supplier_payments()
            .find_by_payment_no("PAY-2026-002", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按单号应可读回");
        assert_amount_fidelity(found.amount, "500.00");
    })
}
