//! FIN-R10 / FIN-E08 销项发票并发与原子性真实 MongoDB 验收。
//!
//! 覆盖 review blockers：
//! - (a) 并发 `apply_invoicings_many` 同一应收子账恰好一方命中；
//! - (b) 超额拒绝整事务回滚，零分配、已开进度不变；
//! - (c) 分配 `insert_many` 唯一键冲突整事务回滚，已开进度与分配零残留；
//!   并断言 `invoice_total == allocation_total == account_delta` 金额守恒与无半写。

use std::str::FromStr;

use database::{ensure_indexes, NoTransaction, ReceivableExt, Transactional};
use entities::common::time::BusinessDate;
use entities::ids::{
    InvoiceId, ReceivableAccountId, SalesInvoiceAllocationId, SalesOrderId, SalesOrderRevisionId,
};
use entities::money::Amount;
use entities::receivable::{
    AccountReviewStatus, AllocationAction, Invoice, InvoiceData, InvoiceDirection, InvoiceKind,
    ReceivableAccount, ReceivableAccountData, SalesInvoiceAllocation, SalesInvoiceAllocationData,
};
use test_support::{require_mongo, TestDb};

fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}

fn receivable_account(id: &str, gross: &str, invoiceable: &str, invoiced: &str) -> ReceivableAccount {
    ReceivableAccount::new(
        ReceivableAccountId::new(id),
        ReceivableAccountData {
            sales_order_id: SalesOrderId::new("so-1"),
            account_seq: if id == "acct-1" { 1 } else { 2 },
            customer_id: entities::ids::CustomerAccountId::new("cust-1"),
            counterparty_party_id: entities::ids::PartyId::new("party-1"),
            source_sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
            review_status: AccountReviewStatus::NotApplicable,
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: amount(gross),
            settled_total: amount("0.00"),
            invoiceable_total: amount(invoiceable),
            invoiced_total: amount(invoiced),
        },
        "tester",
    )
    .expect("应收子账构造失败")
}

fn sales_invoice(id: &str, gross: &str, net: &str, tax: &str) -> Invoice {
    Invoice::new(
        InvoiceId::new(id),
        InvoiceData {
            invoice_direction: InvoiceDirection::Sales,
            invoice_kind: InvoiceKind::Blue,
            party_id: entities::ids::PartyId::new("party-1"),
            invoice_code: None,
            invoice_no: format!("INV-{id}"),
            invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
            gross_amount: amount(gross),
            net_amount: amount(net),
            tax_amount: amount(tax),
            rounding_adjustment_amount: amount("0.00"),
            rounding_reason: None,
            original_invoice_id: None,
        },
        "tester",
    )
    .expect("发票构造失败")
}

fn allocation(
    id: &str,
    invoice_id: &str,
    account_id: &str,
    seq: u32,
    gross: &str,
    net: &str,
    tax: &str,
) -> SalesInvoiceAllocation {
    SalesInvoiceAllocation::new(
        SalesInvoiceAllocationId::new(id),
        SalesInvoiceAllocationData {
            invoice_id: InvoiceId::new(invoice_id),
            receivable_account_id: ReceivableAccountId::new(account_id),
            allocation_seq: seq,
            allocation_action: AllocationAction::Apply,
            allocated_gross_amount: amount(gross),
            allocated_net_amount: amount(net),
            allocated_tax_amount: amount(tax),
            reverses_allocation_id: None,
        },
    )
    .expect("分配构造失败")
}

/// 并发同子账开票：总额 600+600 > 1000 时恰好一方命中，绝不超额。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_apply_invoicings_same_account_exactly_one_hits() {
    require_mongo!(async {
        let fixture = TestDb::new("receivable_concurrent_one")
            .await
            .expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let account = receivable_account("acct-1", "1000.00", "1000.00", "0.00");
        fixture
            .db()
            .receivable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .expect("子账写入失败");

        let deltas = vec![(ReceivableAccountId::new("acct-1"), amount("600.00"))];
        let db_a = fixture.db().clone();
        let deltas_a = deltas.clone();
        let task_a = tokio::spawn(async move {
            db_a.receivable_accounts()
                .apply_invoicings_many(&deltas_a, "tester-a", &mut NoTransaction)
                .await
                .expect("写入方 A 失败")
        });
        let db_b = fixture.db().clone();
        let task_b = tokio::spawn(async move {
            db_b.receivable_accounts()
                .apply_invoicings_many(&deltas, "tester-b", &mut NoTransaction)
                .await
                .expect("写入方 B 失败")
        });
        let result_a = task_a.await.expect("任务 A 失败");
        let result_b = task_b.await.expect("任务 B 失败");
        let applied = result_a.applied.len() + result_b.applied.len();
        let rejected = result_a.rejected.len() + result_b.rejected.len();
        assert_eq!(applied, 1, "600+600 超 1000 时恰好一方命中");
        assert_eq!(rejected, 1, "另一方必须被拒绝");

        let after = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(after.invoiced_total, amount("600.00"));
        assert_eq!(after.open_invoiceable_total, amount("400.00"));
        assert!(!after.open_invoiceable_total.to_decimal().is_sign_negative());
    });
}

/// 超额拒绝整事务回滚：零分配、已开进度不变，无半写。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn over_limit_rejected_tx_rollback_leaves_zero_allocations_and_unchange() {
    require_mongo!(async {
        let fixture = TestDb::new("receivable_over_limit")
            .await
            .expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        // acct-1 可开 500，acct-2 可开 500；事务内尝试 acct-2 超额 600
        for acct in [
            receivable_account("acct-1", "1000.00", "500.00", "0.00"),
            receivable_account("acct-2", "1000.00", "500.00", "0.00"),
        ] {
            fixture
                .db()
                .receivable_accounts()
                .create(&acct, &mut NoTransaction)
                .await
                .expect("子账写入失败");
        }
        let invoice = sales_invoice("inv-over", "1100.00", "968.00", "132.00");
        fixture
            .db()
            .invoices()
            .create(&invoice, &mut NoTransaction)
            .await
            .expect("发票写入失败");

        let deltas = vec![
            (ReceivableAccountId::new("acct-1"), amount("500.00")),
            (ReceivableAccountId::new("acct-2"), amount("600.00")),
        ];
        let allocations = vec![
            allocation("alloc-1", "inv-over", "acct-1", 1, "500.00", "440.00", "60.00"),
            allocation("alloc-2", "inv-over", "acct-2", 2, "600.00", "528.00", "72.00"),
        ];
        let db_handle = fixture.db().clone();
        let outcome = fixture
            .client()
            .with_transaction::<_, _, database::Error>(move |session| {
                let deltas = deltas.clone();
                let allocations = allocations.clone();
                let db = db_handle.clone();
                Box::pin(async move {
                    let result = db
                        .receivable_accounts()
                        .apply_invoicings_many(&deltas, "tester", session)
                        .await?;
                    if !result.rejected.is_empty() {
                        return Err(database::Error::DatabaseError(mongodb::error::Error::custom(
                            "over-limit rejected",
                        )));
                    }
                    db.receivable()
                        .create_sales_invoice_allocations_many(&allocations, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(outcome.is_err(), "超额必须使事务失败");

        for id in ["acct-1", "acct-2"] {
            let after = fixture
                .db()
                .receivable_accounts()
                .find_by_id(id, &mut NoTransaction)
                .await
                .expect("读取失败")
                .expect("子账必须存在");
            assert_eq!(after.invoiced_total, amount("0.00"), "{id} 回滚后不得推进");
        }
        let allocs = fixture
            .db()
            .sales_invoice_allocations()
            .find_allocations_by_invoices(&[InvoiceId::new("inv-over")], &mut NoTransaction)
            .await
            .expect("分配查询失败");
        assert!(allocs.is_empty(), "回滚后不得留下分配");
    });
}

/// 分配唯一键冲突整事务回滚：已开进度与分配零残留，且 invoice_total==allocation_total==delta 守恒在成功路径可验证。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn duplicate_allocation_unique_conflict_rolls_back_deltas() {
    require_mongo!(async {
        let fixture = TestDb::new("receivable_dup_alloc")
            .await
            .expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let account = receivable_account("acct-1", "1000.00", "1000.00", "0.00");
        fixture
            .db()
            .receivable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .expect("子账写入失败");
        let invoice = sales_invoice("inv-dup", "100.00", "88.00", "12.00");
        fixture
            .db()
            .invoices()
            .create(&invoice, &mut NoTransaction)
            .await
            .expect("发票写入失败");

        // 两条分配共享同一 (invoice_id, allocation_seq) 唯一键，insert_many 必冲突
        let dup_allocations = vec![
            allocation("alloc-dup-1", "inv-dup", "acct-1", 1, "60.00", "52.80", "7.20"),
            allocation("alloc-dup-2", "inv-dup", "acct-1", 1, "40.00", "35.20", "4.80"),
        ];
        let deltas = vec![(ReceivableAccountId::new("acct-1"), amount("100.00"))];
        let db_handle = fixture.db().clone();
        let dup = dup_allocations.clone();
        let deltas_clone = deltas.clone();
        let outcome = fixture
            .client()
            .with_transaction::<_, _, database::Error>(move |session| {
                let deltas = deltas_clone.clone();
                let dup = dup.clone();
                let db = db_handle.clone();
                Box::pin(async move {
                    let result = db
                        .receivable_accounts()
                        .apply_invoicings_many(&deltas, "tester", session)
                        .await?;
                    assert!(result.rejected.is_empty(), "额度内必须命中");
                    db.receivable()
                        .create_sales_invoice_allocations_many(&dup, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(outcome.is_err(), "唯一键冲突必须使事务失败");

        let after = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(after.invoiced_total, amount("0.00"), "回滚后已开进度必须为 0");

        let allocs = fixture
            .db()
            .sales_invoice_allocations()
            .find_allocations_by_invoices(&[InvoiceId::new("inv-dup")], &mut NoTransaction)
            .await
            .expect("分配查询失败");
        assert!(allocs.is_empty(), "唯一键冲突回滚后不得留下分配");

        // 成功路径守恒对比：重新以合法分配写入，断言三方相等
        let good_allocations = vec![
            allocation("alloc-good-1", "inv-dup", "acct-1", 1, "60.00", "52.80", "7.20"),
            allocation("alloc-good-2", "inv-dup", "acct-1", 2, "40.00", "35.20", "4.80"),
        ];
        let db_handle2 = fixture.db().clone();
        fixture
            .client()
            .with_transaction::<_, _, database::Error>(move |session| {
                let deltas = vec![(ReceivableAccountId::new("acct-1"), amount("100.00"))];
                let good = good_allocations.clone();
                let db = db_handle2.clone();
                Box::pin(async move {
                    let result = db
                        .receivable_accounts()
                        .apply_invoicings_many(&deltas, "tester", session)
                        .await?;
                    assert!(result.rejected.is_empty());
                    db.receivable()
                        .create_sales_invoice_allocations_many(&good, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("合法写入必须成功");
        let after_ok = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(
            after_ok.invoiced_total,
            amount("100.00"),
            "守恒：account delta 必须等于 invoice total"
        );
        let allocs_ok = fixture
            .db()
            .sales_invoice_allocations()
            .find_allocations_by_invoices(&[InvoiceId::new("inv-dup")], &mut NoTransaction)
            .await
            .expect("分配查询失败");
        assert_eq!(allocs_ok.len(), 2);
        let allocation_total: Amount = allocs_ok
            .iter()
            .fold(amount("0.00"), |sum, a| sum.checked_add(a.allocated_gross_amount));
        assert_eq!(
            allocation_total,
            amount("100.00"),
            "守恒：allocation total 必须等于 invoice total"
        );
        assert_eq!(
            allocation_total, after_ok.invoiced_total,
            "守恒：allocation total 必须等于 account delta"
        );
    });
}
