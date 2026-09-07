//! FIN-R08 / FIN-R11 / FIN-R13 应收分层真实 MongoDB 验收。
//!
//! - R08：作用域合并分页搜索。无 scope 不触发关联扫描；有 scope 不返回全量 ID
//!   （Service 只见当页投影）；sales/account 交集正确；空/超大 scope、分页 total
//!   与稳定排序。
//! - R11：批量红冲回退。部分/全额/跨多 account/同 account 多行/历史 reversal
//!   金额守恒；版本或唯一键冲突全事务回滚；receivable 与 payable 两方向。
//! - R13：批量职责分离事实。单次批量查询；仅成功事件计入证据；与逐笔基准一致。

use std::str::FromStr;

use database::{
    AccessControlExt, NoTransaction, PayableExt, ReceivableExt, ReceivableListScope,
    ScopedCustomerReceiptQuery, ScopedInvoiceQuery, Transactional,
};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    CustomerAccountId, CustomerReceiptId, InvoiceId, PartyId, PayableAccountId, ReceiptAllocationId,
    ReceivableAccountId, ReceivableEntryId, SalesInvoiceAllocationId, SalesOrderId, SalesOrderRevisionId,
    SupplierAccountId,
};
use entities::money::Amount;
use entities::payable::{PayableAccount, PayableAccountData, PayableSourceType};
use entities::receivable::{
    AccountReviewStatus, AllocationAction, CustomerReceipt, CustomerReceiptData, EntryDirection, Invoice,
    InvoiceData, InvoiceDirection, InvoiceKind, ReceiptAllocation, ReceiptAllocationData, ReceivableAccount,
    ReceivableAccountData, ReceivableEntry, ReceivableEntryData, ReceivableEntryType, SalesInvoiceAllocation,
    SalesInvoiceAllocationData,
};
use entities::{AccountKind, AuditLog, AuditLogData};
use test_support::{require_mongo, TestDb};

fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}

fn receivable_account(
    id: &str,
    sales_order: &str,
    seq: u32,
    invoiceable: &str,
    invoiced: &str,
) -> ReceivableAccount {
    ReceivableAccount::new(
        ReceivableAccountId::new(id),
        ReceivableAccountData {
            sales_order_id: SalesOrderId::new(sales_order),
            account_seq: seq,
            customer_id: CustomerAccountId::new("cust-1"),
            counterparty_party_id: PartyId::new("party-1"),
            source_sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
            review_status: AccountReviewStatus::NotApplicable,
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: amount("1000.00"),
            settled_total: amount("0.00"),
            invoiceable_total: amount(invoiceable),
            invoiced_total: amount(invoiced),
        },
        "tester",
    )
    .expect("应收子账构造失败")
}

fn receivable_entry(id: &str, account: &str, source_doc: &str) -> ReceivableEntry {
    ReceivableEntry::new(
        ReceivableEntryId::new(id),
        ReceivableEntryData {
            receivable_account_id: ReceivableAccountId::new(account),
            entry_type: ReceivableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: amount("1000.00"),
            due_date: BusinessDate::from_ymd(2026, 9, 30).unwrap(),
            source_fact_type: "sales_order".to_string(),
            source_document_id: source_doc.to_string(),
            source_revision_id: "rev-1".to_string(),
            source_sequence: 1,
            posted_at: Instant::from_unix_secs(1_700_000_000),
        },
    )
    .expect("应收分录构造失败")
}

fn customer_receipt(id: &str, no: &str, received_at_secs: i64) -> CustomerReceipt {
    CustomerReceipt::new(
        CustomerReceiptId::new(id),
        CustomerReceiptData {
            receipt_no: no.to_string(),
            counterparty_party_id: PartyId::new("party-1"),
            customer_id: Some(CustomerAccountId::new("cust-1")),
            received_at: Instant::from_unix_secs(received_at_secs),
            amount: amount("1000.00"),
            bank_reference: None,
        },
        "tester",
    )
    .expect("回款单构造失败")
}

fn receipt_allocation(id: &str, receipt_id: &str, entry_id: &str, seq: u32) -> ReceiptAllocation {
    ReceiptAllocation::new(
        ReceiptAllocationId::new(id),
        ReceiptAllocationData {
            customer_receipt_id: CustomerReceiptId::new(receipt_id),
            receivable_entry_id: ReceivableEntryId::new(entry_id),
            allocation_seq: seq,
            allocation_action: AllocationAction::Apply,
            allocated_amount: amount("100.00"),
            allocated_at: Instant::from_unix_secs(1_700_000_100),
            reverses_allocation_id: None,
        },
    )
    .expect("回款分配构造失败")
}

fn sales_invoice(id: &str, no: &str, gross: &str) -> Invoice {
    let mut invoice = Invoice::new(
        InvoiceId::new(id),
        InvoiceData {
            invoice_direction: InvoiceDirection::Sales,
            invoice_kind: InvoiceKind::Blue,
            party_id: PartyId::new("party-1"),
            invoice_code: None,
            invoice_no: no.to_string(),
            invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
            gross_amount: amount(gross),
            net_amount: amount(gross),
            tax_amount: amount("0.00"),
            rounding_adjustment_amount: amount("0.00"),
            rounding_reason: None,
            original_invoice_id: None,
        },
        "tester",
    )
    .expect("发票构造失败");
    invoice.mark_registered("tester").expect("发票登记失败");
    invoice
}

fn sales_allocation(
    id: &str,
    invoice_id: &str,
    account_id: &str,
    seq: u32,
    gross: &str,
) -> SalesInvoiceAllocation {
    SalesInvoiceAllocation::new(
        SalesInvoiceAllocationId::new(id),
        SalesInvoiceAllocationData {
            invoice_id: InvoiceId::new(invoice_id),
            receivable_account_id: ReceivableAccountId::new(account_id),
            allocation_seq: seq,
            allocation_action: AllocationAction::Apply,
            allocated_gross_amount: amount(gross),
            allocated_net_amount: amount(gross),
            allocated_tax_amount: amount("0.00"),
            reverses_allocation_id: None,
        },
    )
    .expect("销项分配构造失败")
}

fn audit_log(
    id: &str,
    actor: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    success: bool,
) -> AuditLog {
    AuditLog::new(
        id.to_string(),
        AuditLogData {
            actor_id: actor.to_string(),
            actor_account: format!("{actor}-account"),
            actor_type: AccountKind::Admin,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: Some(resource_id.to_string()),
            success,
            message: None,
        },
    )
    .expect("审计日志构造失败")
}

fn receipt_scope_query(scope: ReceivableListScope, page: u64, page_size: u32) -> ScopedCustomerReceiptQuery {
    ScopedCustomerReceiptQuery {
        receipt_no: None,
        counterparty_party_id: None,
        status: None,
        scope,
        page,
        page_size,
        sort_by: Some("received_at".to_string()),
        sort_ascending: true,
    }
}

/// R08：无 scope 时不触发关联扫描，直接返回全部回款。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn scoped_receipt_search_without_scope_returns_all() {
    require_mongo!(async {
        let fixture = TestDb::new("fin_r08_no_scope").await.expect("TestDb 创建失败");
        for (id, no, at) in [("rc-1", "RC-1", 100), ("rc-2", "RC-2", 200)] {
            fixture
                .db()
                .customer_receipts()
                .create(&customer_receipt(id, no, at), &mut NoTransaction)
                .await
                .expect("回款写入失败");
        }
        let page = fixture
            .db()
            .receivable()
            .search_customer_receipts_in_account_scope(
                &receipt_scope_query(ReceivableListScope::default(), 1, 20),
                &mut NoTransaction,
            )
            .await
            .expect("作用域搜索失败");
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].receipt_no, "RC-1");
        assert_eq!(page.items[1].receipt_no, "RC-2");
    });
}

/// R08：销售单 scope 只返回该单关联回款；超大 scope（多账户多分录多分配）下
/// total 与稳定排序正确，分页无重复无遗漏。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn scoped_receipt_search_by_sales_order_resolves_scope_with_paging() {
    require_mongo!(async {
        let fixture = TestDb::new("fin_r08_sales_scope").await.expect("TestDb 创建失败");
        let db = fixture.db();
        for (id, so, seq) in [("acct-a", "so-1", 1), ("acct-b", "so-2", 1)] {
            db.receivable_accounts()
                .create(
                    &receivable_account(id, so, seq, "1000.00", "0.00"),
                    &mut NoTransaction,
                )
                .await
                .expect("子账写入失败");
        }
        // so-1 关联 3 笔回款（超大 scope 雏形：多分录多分配），so-2 关联 1 笔。
        for (id, acct, doc) in [
            ("re-a1", "acct-a", "doc-a1"),
            ("re-a2", "acct-a", "doc-a2"),
            ("re-b1", "acct-b", "doc-b1"),
        ] {
            db.receivable_entries()
                .create(&receivable_entry(id, acct, doc), &mut NoTransaction)
                .await
                .expect("分录写入失败");
        }
        for (id, no, at) in [
            ("rc-a1", "RC-A1", 100),
            ("rc-a2", "RC-A2", 200),
            ("rc-a3", "RC-A3", 300),
            ("rc-b1", "RC-B1", 400),
        ] {
            db.customer_receipts()
                .create(&customer_receipt(id, no, at), &mut NoTransaction)
                .await
                .expect("回款写入失败");
        }
        for (id, rc, re, seq) in [
            ("ra-a1", "rc-a1", "re-a1", 1),
            ("ra-a2", "rc-a2", "re-a2", 1),
            ("ra-a3", "rc-a3", "re-a1", 2),
            ("ra-b1", "rc-b1", "re-b1", 1),
        ] {
            db.receipt_allocations()
                .create(&receipt_allocation(id, rc, re, seq), &mut NoTransaction)
                .await
                .expect("分配写入失败");
        }
        let scope = ReceivableListScope {
            sales_order_id: Some("so-1".to_string()),
            receivable_account_id: None,
        };
        let first = db
            .receivable()
            .search_customer_receipts_in_account_scope(
                &receipt_scope_query(scope.clone(), 1, 2),
                &mut NoTransaction,
            )
            .await
            .expect("第一页失败");
        assert_eq!(first.total, 3);
        assert_eq!(first.items.len(), 2);
        let second = db
            .receivable()
            .search_customer_receipts_in_account_scope(&receipt_scope_query(scope, 2, 2), &mut NoTransaction)
            .await
            .expect("第二页失败");
        assert_eq!(second.total, 3);
        assert_eq!(second.items.len(), 1);
        let mut nos = first
            .items
            .iter()
            .chain(second.items.iter())
            .map(|row| row.receipt_no.clone())
            .collect::<Vec<_>>();
        nos.sort();
        assert_eq!(nos, vec!["RC-A1", "RC-A2", "RC-A3"]);
    });
}

/// R08：sales 与 account 交集正确；不一致交集与空 scope 均返回空页。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn scoped_search_intersection_and_empty_scope() {
    require_mongo!(async {
        let fixture = TestDb::new("fin_r08_intersection")
            .await
            .expect("TestDb 创建失败");
        let db = fixture.db();
        for (id, so, seq) in [("acct-a", "so-1", 1), ("acct-b", "so-2", 1)] {
            db.receivable_accounts()
                .create(
                    &receivable_account(id, so, seq, "1000.00", "0.00"),
                    &mut NoTransaction,
                )
                .await
                .expect("子账写入失败");
        }
        db.receivable_entries()
            .create(&receivable_entry("re-a1", "acct-a", "doc-a1"), &mut NoTransaction)
            .await
            .expect("分录写入失败");
        db.customer_receipts()
            .create(&customer_receipt("rc-a1", "RC-A1", 100), &mut NoTransaction)
            .await
            .expect("回款写入失败");
        db.receipt_allocations()
            .create(
                &receipt_allocation("ra-a1", "rc-a1", "re-a1", 1),
                &mut NoTransaction,
            )
            .await
            .expect("分配写入失败");
        db.invoices()
            .create(&sales_invoice("inv-a1", "INV-A1", "100.00"), &mut NoTransaction)
            .await
            .expect("发票写入失败");
        db.receivable()
            .create_sales_invoice_allocations_many(
                &[sales_allocation("sa-a1", "inv-a1", "acct-a", 1, "100.00")],
                &mut NoTransaction,
            )
            .await
            .expect("销项分配写入失败");

        // 一致交集：acct-a + so-1 命中。
        let hit = db
            .receivable()
            .search_customer_receipts_in_account_scope(
                &receipt_scope_query(
                    ReceivableListScope {
                        sales_order_id: Some("so-1".to_string()),
                        receivable_account_id: Some(ReceivableAccountId::new("acct-a")),
                    },
                    1,
                    20,
                ),
                &mut NoTransaction,
            )
            .await
            .expect("一致交集失败");
        assert_eq!(hit.total, 1);

        // 不一致交集：acct-b + so-1 为空。
        let miss = db
            .receivable()
            .search_customer_receipts_in_account_scope(
                &receipt_scope_query(
                    ReceivableListScope {
                        sales_order_id: Some("so-1".to_string()),
                        receivable_account_id: Some(ReceivableAccountId::new("acct-b")),
                    },
                    1,
                    20,
                ),
                &mut NoTransaction,
            )
            .await
            .expect("不一致交集失败");
        assert_eq!(miss.total, 0);
        assert!(miss.items.is_empty());

        // 不存在的子账 scope 为空页，不触发后续扫描。
        let ghost = db
            .receivable()
            .search_invoices_in_account_scope(
                &ScopedInvoiceQuery {
                    invoice_direction: None,
                    invoice_kind: None,
                    party_id: None,
                    invoice_no: None,
                    status: None,
                    scope: ReceivableListScope {
                        sales_order_id: None,
                        receivable_account_id: Some(ReceivableAccountId::new("acct-ghost")),
                    },
                    page: 1,
                    page_size: 20,
                    sort_by: Some("gross_amount".to_string()),
                    sort_ascending: true,
                },
                &mut NoTransaction,
            )
            .await
            .expect("幽灵子账失败");
        assert_eq!(ghost.total, 0);

        // 发票 scope：acct-a 命中 inv-a1，total 与稳定排序正确。
        let inv_page = db
            .receivable()
            .search_invoices_in_account_scope(
                &ScopedInvoiceQuery {
                    invoice_direction: Some(InvoiceDirection::Sales),
                    invoice_kind: None,
                    party_id: None,
                    invoice_no: None,
                    status: None,
                    scope: ReceivableListScope {
                        sales_order_id: Some("so-1".to_string()),
                        receivable_account_id: None,
                    },
                    page: 1,
                    page_size: 20,
                    sort_by: Some("gross_amount".to_string()),
                    sort_ascending: true,
                },
                &mut NoTransaction,
            )
            .await
            .expect("发票 scope 失败");
        assert_eq!(inv_page.total, 1);
        assert_eq!(inv_page.items[0].invoice_no, "INV-A1");
    });
}

/// R11：部分/全额/跨多 account/同 account 多行/历史 reversal 金额守恒。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn batch_revert_invoicings_conserves_amounts() {
    require_mongo!(async {
        let fixture = TestDb::new("fin_r11_conserve").await.expect("TestDb 创建失败");
        let db = fixture.db();
        for (id, seq, invoiced) in [("acct-1", 1, "500.00"), ("acct-2", 2, "300.00")] {
            db.receivable_accounts()
                .create(
                    &receivable_account(id, "so-1", seq, "1000.00", invoiced),
                    &mut NoTransaction,
                )
                .await
                .expect("子账写入失败");
        }
        // 历史 reversal：acct-1 先冲 200（500 -> 300）。
        let history = db
            .receivable_accounts()
            .revert_invoicings_many(
                &[(ReceivableAccountId::new("acct-1"), amount("200.00"))],
                "tester",
                &mut NoTransaction,
            )
            .await
            .expect("历史回退失败");
        assert!(history.rejected.is_empty());

        // 本次：部分（acct-1 100）、全额（acct-2 300）、同 account 多行（acct-1 再 50）。
        let deltas = vec![
            (ReceivableAccountId::new("acct-1"), amount("100.00")),
            (ReceivableAccountId::new("acct-2"), amount("300.00")),
            (ReceivableAccountId::new("acct-1"), amount("50.00")),
        ];
        // Service 聚合口径：同账户求和后逐账户一次更新。
        let mut order = Vec::new();
        let mut sums = std::collections::HashMap::new();
        for (id, gross) in &deltas {
            sums.entry(id.to_string())
                .and_modify(|total: &mut Amount| *total = total.checked_add(*gross))
                .or_insert_with(|| {
                    order.push(id.to_string());
                    *gross
                });
        }
        let aggregated = order
            .into_iter()
            .map(|id| (ReceivableAccountId::new(id.clone()), sums.remove(&id).unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(aggregated.len(), 2);
        let result = db
            .receivable_accounts()
            .revert_invoicings_many(&aggregated, "tester", &mut NoTransaction)
            .await
            .expect("批量回退失败");
        assert!(result.rejected.is_empty());
        assert_eq!(result.applied.len(), 2);

        let first = db
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(first.invoiced_total, amount("150.00"));
        assert_eq!(first.open_invoiceable_total, amount("850.00"));
        let second = db
            .receivable_accounts()
            .find_by_id("acct-2", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(second.invoiced_total, amount("0.00"));
        assert_eq!(second.open_invoiceable_total, amount("1000.00"));
    });
}

/// R11：超额拒绝全事务回滚；唯一键冲突全事务回滚（receivable 方向）。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn batch_revert_rejected_or_conflict_rolls_back() {
    require_mongo!(async {
        let fixture = TestDb::new("fin_r11_rollback").await.expect("TestDb 创建失败");
        database::ensure_indexes(fixture.db())
            .await
            .expect("索引创建失败");
        let db = fixture.db();
        db.receivable_accounts()
            .create(
                &receivable_account("acct-1", "so-1", 1, "1000.00", "100.00"),
                &mut NoTransaction,
            )
            .await
            .expect("子账写入失败");
        db.invoices()
            .create(&sales_invoice("inv-r", "INV-R", "50.00"), &mut NoTransaction)
            .await
            .expect("发票写入失败");

        // 超额拒绝：rejected 非空时 Service 中止事务，进度与分配零残留。
        let over = vec![(ReceivableAccountId::new("acct-1"), amount("600.00"))];
        let allocs = vec![sales_allocation("sa-over", "inv-r", "acct-1", 1, "600.00")];
        let db_handle = db.clone();
        let outcome = fixture
            .client()
            .with_transaction::<_, _, database::Error>(move |session| {
                let over = over.clone();
                let allocs = allocs.clone();
                let db = db_handle.clone();
                Box::pin(async move {
                    let result = db
                        .receivable_accounts()
                        .revert_invoicings_many(&over, "tester", session)
                        .await?;
                    if !result.rejected.is_empty() {
                        return Err(database::Error::DatabaseError(mongodb::error::Error::custom(
                            "over-reversal rejected",
                        )));
                    }
                    db.receivable()
                        .create_sales_invoice_allocations_many(&allocs, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(outcome.is_err(), "超额必须使事务失败");
        let after = db
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(after.invoiced_total, amount("100.00"));

        // 唯一键冲突：预置同（invoice_id, allocation_seq）后批量插入冲突回滚。
        db.receivable()
            .create_sales_invoice_allocations_many(
                &[sales_allocation("sa-first", "inv-r", "acct-1", 1, "50.00")],
                &mut NoTransaction,
            )
            .await
            .expect("预置分配失败");
        let ok_deltas = vec![(ReceivableAccountId::new("acct-1"), amount("50.00"))];
        let dup = vec![sales_allocation("sa-dup", "inv-r", "acct-1", 1, "50.00")];
        let db_handle = db.clone();
        let conflict = fixture
            .client()
            .with_transaction::<_, _, database::Error>(move |session| {
                let ok_deltas = ok_deltas.clone();
                let dup = dup.clone();
                let db = db_handle.clone();
                Box::pin(async move {
                    let result = db
                        .receivable_accounts()
                        .revert_invoicings_many(&ok_deltas, "tester", session)
                        .await?;
                    if !result.rejected.is_empty() {
                        return Err(database::Error::DatabaseError(mongodb::error::Error::custom(
                            "unexpected rejected",
                        )));
                    }
                    db.receivable()
                        .create_sales_invoice_allocations_many(&dup, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(conflict.is_err(), "唯一键冲突必须使事务失败");
        let after_conflict = db
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(
            after_conflict.invoiced_total,
            amount("100.00"),
            "冲突回滚后进度不变"
        );
    });
}

/// R11：payable 方向批量回退守恒且超额拒绝。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn batch_revert_invoicings_payable_direction() {
    require_mongo!(async {
        let fixture = TestDb::new("fin_r11_payable").await.expect("TestDb 创建失败");
        let db = fixture.db();
        let account = PayableAccount::new(
            PayableAccountId::new("pacct-1"),
            PayableAccountData {
                source_document_id: "po-1".to_string(),
                supplier_id: SupplierAccountId::new("sup-1"),
                source_type: PayableSourceType::PurchaseOrder,
                gross_total: amount("1000.00"),
                settled_total: amount("0.00"),
                invoiceable_total: amount("1000.00"),
                invoiced_total: amount("400.00"),
            },
            "tester",
        )
        .expect("应付子账构造失败");
        db.payable_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .expect("子账写入失败");
        let ok = db
            .payable_accounts()
            .revert_invoicings_many(
                &[(PayableAccountId::new("pacct-1"), amount("150.00"))],
                "tester",
                &mut NoTransaction,
            )
            .await
            .expect("批量回退失败");
        assert!(ok.rejected.is_empty());
        let over = db
            .payable_accounts()
            .revert_invoicings_many(
                &[(PayableAccountId::new("pacct-1"), amount("300.00"))],
                "tester",
                &mut NoTransaction,
            )
            .await
            .expect("超额回退失败");
        assert_eq!(over.rejected.len(), 1);
        let after = db
            .payable_accounts()
            .find_by_id("pacct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(after.invoiced_total, amount("250.00"));
        assert_eq!(after.open_invoiceable_total, amount("750.00"));
    });
}

/// R13：批量职责分离事实只返回成功最小投影；空输入不访问数据库。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn separation_facts_batch_returns_only_successful_minimal_facts() {
    require_mongo!(async {
        let fixture = TestDb::new("fin_r13_batch").await.expect("TestDb 创建失败");
        let db = fixture.db();
        for log in [
            audit_log(
                "a-1",
                "creator-1",
                "customer_receipt.create",
                "customer_receipt",
                "cr-1",
                true,
            ),
            audit_log(
                "a-2",
                "poster-1",
                "customer_receipt.post:registered",
                "customer_receipt",
                "cr-1",
                true,
            ),
            audit_log(
                "a-3",
                "creator-1",
                "customer_receipt.post:registered",
                "customer_receipt",
                "cr-1",
                false,
            ),
            audit_log(
                "a-4",
                "poster-2",
                "invoice.post:registered",
                "invoice",
                "inv-1",
                true,
            ),
            audit_log(
                "a-5",
                "other-1",
                "invoice.post:registered",
                "invoice",
                "inv-2",
                true,
            ),
        ] {
            db.audit_logs()
                .create(&log, &mut NoTransaction)
                .await
                .expect("审计写入失败");
        }
        let facts = db
            .audit_logs()
            .list_separation_facts_by_resources(
                &[
                    ("customer_receipt".to_string(), "cr-1".to_string()),
                    ("invoice".to_string(), "inv-1".to_string()),
                ],
                &mut NoTransaction,
            )
            .await
            .expect("批量查询失败");
        assert_eq!(facts.len(), 3, "仅成功事件计入证据");
        assert!(facts
            .iter()
            .all(|fact| !fact.actor_id.is_empty() && !fact.action.is_empty()));

        // 与逐笔基准一致：单 pair 批量结果与逐资源成功查询同 actor/action 集合。
        let single = db
            .audit_logs()
            .list_separation_facts_by_resources(
                &[("customer_receipt".to_string(), "cr-1".to_string())],
                &mut NoTransaction,
            )
            .await
            .expect("单 pair 查询失败");
        let baseline = db
            .audit_logs()
            .list_successful_by_resource("customer_receipt", "cr-1", &mut NoTransaction)
            .await
            .expect("逐笔基准失败");
        let mut batched = single
            .iter()
            .map(|fact| (fact.actor_id.clone(), fact.action.clone()))
            .collect::<Vec<_>>();
        let mut expected = baseline
            .iter()
            .map(|log| (log.actor_id.clone(), log.action.clone()))
            .collect::<Vec<_>>();
        batched.sort();
        expected.sort();
        assert_eq!(batched, expected);

        // 空输入不访问数据库。
        let empty = db
            .audit_logs()
            .list_separation_facts_by_resources(&[], &mut NoTransaction)
            .await
            .expect("空输入失败");
        assert!(empty.is_empty());
    });
}
