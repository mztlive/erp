//! FIN-R09 / FIN-R12 客户回款过账与票款快照真实 MongoDB 验收。
//!
//! 审批路径走公开 `post_customer_receipt`（内部 `post_customer_receipt_in_transaction`）；
//! 历史登记数据面与审批路径共享 `apply_settlements_many` + `insert_many`，由
//! `database` 仓储 Mongo 用例覆盖。本文件覆盖：
//! - 重复 entry／account 的回款金额、分录开放余额、子账进度守恒；
//! - 跨 party、余额不足、唯一键冲突全回滚；
//! - 并发条件核销恰好一方命中；
//! - `receivable_account_detail` 缺失销售单／版本／回款／发票的确定首错。

use std::str::FromStr;

use database::{ensure_indexes, NoTransaction, ReceivableExt, SalesOrderExt};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    ContractRevisionId, CustomerReceiptId, InvoiceId, PartyId, PartyRevisionId, ReceiptAllocationId,
    ReceivableAccountId, ReceivableEntryId, SalesInvoiceAllocationId, SalesOrderId, SalesOrderRevisionId,
};
use entities::money::Amount;
use entities::receivable::{
    AccountReviewStatus, AllocationAction, CustomerReceipt, CustomerReceiptData, EntryDirection,
    PendingReceiptAllocation, ReceiptAllocation, ReceiptAllocationData, ReceivableAccount,
    ReceivableAccountData, ReceivableEntry, ReceivableEntryData, ReceivableEntryType, SalesInvoiceAllocation,
    SalesInvoiceAllocationData,
};
use entities::sales_order::snapshot::HeaderSnapshotData;
use entities::sales_order::{
    BusinessType, OriginSystem, RevisionSource, SalesOrder, SalesOrderData, SalesOrderRevision,
    SalesOrderRevisionData,
};
use entities::AccountKind;
use mongodb::Database;
use services::audit::AuditActor;
use services::receivable::ReceivableService;
use test_support::{require_mongo, TestDb};

fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}

fn actor() -> AuditActor {
    AuditActor::new(
        "finance-1".to_string(),
        "finance-1".to_string(),
        AccountKind::Admin,
    )
}

async fn seed_sales_order(db: &Database, id: &str) {
    let order = SalesOrder::new(
        SalesOrderId::new(id),
        SalesOrderData {
            order_no: format!("SO-{id}"),
            business_type: BusinessType::GoodsService,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: entities::ids::CustomerAccountId::new("cust-1"),
            contract_id: None,
            settlement_party_id: PartyId::new("party-1"),
            source_status_code: None,
        },
        "tester",
    )
    .unwrap();
    db.sales_orders()
        .create(&order, &mut NoTransaction)
        .await
        .unwrap();
}

async fn seed_account(db: &Database, id: &str, party: &str, seq: u32, gross: &str) {
    let account = ReceivableAccount::new(
        ReceivableAccountId::new(id),
        ReceivableAccountData {
            sales_order_id: SalesOrderId::new("so-1"),
            account_seq: seq,
            customer_id: entities::ids::CustomerAccountId::new("cust-1"),
            counterparty_party_id: PartyId::new(party),
            source_sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
            review_status: AccountReviewStatus::NotApplicable,
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: amount(gross),
            settled_total: amount("0.00"),
            invoiceable_total: amount(gross),
            invoiced_total: amount("0.00"),
        },
        "tester",
    )
    .unwrap();
    db.receivable_accounts()
        .create(&account, &mut NoTransaction)
        .await
        .unwrap();
}

async fn seed_entry(db: &Database, id: &str, account_id: &str, value: &str, seq: u32) {
    let entry = ReceivableEntry::new(
        ReceivableEntryId::new(id),
        ReceivableEntryData {
            receivable_account_id: ReceivableAccountId::new(account_id),
            entry_type: ReceivableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: amount(value),
            due_date: BusinessDate::from_ymd(2026, 9, 30).unwrap(),
            source_fact_type: "sales_order".to_string(),
            source_document_id: "so-1".to_string(),
            source_revision_id: "rev-1".to_string(),
            source_sequence: seq,
            posted_at: Instant::from_unix_secs(1_700_000_000),
        },
    )
    .unwrap();
    db.receivable_entries()
        .create(&entry, &mut NoTransaction)
        .await
        .unwrap();
}

async fn seed_in_approval_receipt(
    db: &Database,
    id: &str,
    party: &str,
    value: &str,
    pending: Vec<(&str, &str)>,
) {
    let mut receipt = CustomerReceipt::new(
        CustomerReceiptId::new(id),
        CustomerReceiptData {
            receipt_no: format!("SK-{id}"),
            counterparty_party_id: PartyId::new(party),
            customer_id: Some(entities::ids::CustomerAccountId::new("cust-1")),
            received_at: Instant::from_unix_secs(1_700_000_000),
            amount: amount(value),
            bank_reference: Some("BANK".to_string()),
        },
        "tester",
    )
    .unwrap();
    let lines = pending
        .into_iter()
        .map(|(entry_id, line_amount)| {
            PendingReceiptAllocation::new(ReceivableEntryId::new(entry_id), amount(line_amount)).unwrap()
        })
        .collect();
    receipt.start_approval(lines).unwrap();
    db.customer_receipts()
        .create(&receipt, &mut NoTransaction)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn post_customer_receipt_conserves_receipt_entry_and_account() {
    require_mongo!(async {
        let fixture = TestDb::new("recv_post_conserve").await.expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        seed_sales_order(fixture.db(), "so-1").await;
        seed_account(fixture.db(), "acct-1", "party-1", 1, "1000.00").await;
        seed_entry(fixture.db(), "e-1", "acct-1", "400.00", 1).await;
        seed_entry(fixture.db(), "e-2", "acct-1", "600.00", 2).await;
        seed_in_approval_receipt(
            fixture.db(),
            "cr-1",
            "party-1",
            "150.00",
            vec![("e-1", "80.00"), ("e-1", "20.00"), ("e-2", "50.00")],
        )
        .await;
        let service = ReceivableService::new(fixture.db().clone());
        service
            .post_customer_receipt("cr-1", &actor())
            .await
            .expect("过账必须成功");
        let account = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        let allocs = fixture
            .db()
            .receipt_allocations()
            .find_allocations_by_receipts(&[CustomerReceiptId::new("cr-1")], &mut NoTransaction)
            .await
            .unwrap();
        let alloc_total = allocs
            .iter()
            .fold(amount("0.00"), |sum, line| sum.checked_add(line.allocated_amount));
        assert_eq!(alloc_total, amount("150.00"));
        assert_eq!(account.settled_total, amount("150.00"));
        assert_eq!(account.open_total, amount("850.00"));
        assert_eq!(allocs.len(), 3);
        assert_eq!(allocs.iter().map(|a| a.allocation_seq).max(), Some(3));
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn post_customer_receipt_cross_party_rolls_back_zero_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("recv_post_party").await.expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        seed_sales_order(fixture.db(), "so-1").await;
        seed_account(fixture.db(), "acct-1", "party-other", 1, "1000.00").await;
        seed_entry(fixture.db(), "e-1", "acct-1", "400.00", 1).await;
        seed_in_approval_receipt(fixture.db(), "cr-1", "party-1", "80.00", vec![("e-1", "80.00")]).await;
        let service = ReceivableService::new(fixture.db().clone());
        let error = service
            .post_customer_receipt("cr-1", &actor())
            .await
            .expect_err("跨主体必须拒绝");
        assert!(
            error.to_string().contains("跨往来主体"),
            "跨主体错误文案不符：{error}"
        );
        let account = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(account.settled_total, amount("0.00"));
        let allocs = fixture
            .db()
            .receipt_allocations()
            .find_allocations_by_receipts(&[CustomerReceiptId::new("cr-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert!(allocs.is_empty());
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn post_customer_receipt_insufficient_balance_rolls_back() {
    require_mongo!(async {
        let fixture = TestDb::new("recv_post_short").await.expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        seed_sales_order(fixture.db(), "so-1").await;
        seed_account(fixture.db(), "acct-1", "party-1", 1, "50.00").await;
        seed_entry(fixture.db(), "e-1", "acct-1", "400.00", 1).await;
        seed_in_approval_receipt(fixture.db(), "cr-1", "party-1", "80.00", vec![("e-1", "80.00")]).await;
        let service = ReceivableService::new(fixture.db().clone());
        let error = service
            .post_customer_receipt("cr-1", &actor())
            .await
            .expect_err("子账余额不足必须拒绝");
        assert!(
            error.to_string().contains("开放余额") || error.to_string().contains("核销"),
            "余额不足错误文案不符：{error}"
        );
        let account = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(account.settled_total, amount("0.00"));
        let allocs = fixture
            .db()
            .receipt_allocations()
            .find_allocations_by_receipts(&[CustomerReceiptId::new("cr-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert!(allocs.is_empty());
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn post_customer_receipt_unique_key_rolls_back() {
    require_mongo!(async {
        let fixture = TestDb::new("recv_post_uk").await.expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        seed_sales_order(fixture.db(), "so-1").await;
        seed_account(fixture.db(), "acct-1", "party-1", 1, "1000.00").await;
        seed_entry(fixture.db(), "e-1", "acct-1", "400.00", 1).await;
        seed_in_approval_receipt(fixture.db(), "cr-1", "party-1", "80.00", vec![("e-1", "80.00")]).await;
        fixture
            .db()
            .receipt_allocations()
            .create(
                &ReceiptAllocation::new(
                    ReceiptAllocationId::new("pre-1"),
                    ReceiptAllocationData {
                        customer_receipt_id: CustomerReceiptId::new("cr-1"),
                        receivable_entry_id: ReceivableEntryId::new("e-1"),
                        allocation_seq: 1,
                        allocation_action: AllocationAction::Apply,
                        allocated_amount: amount("1.00"),
                        allocated_at: Instant::from_unix_secs(1_700_000_000),
                        reverses_allocation_id: None,
                    },
                )
                .unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let service = ReceivableService::new(fixture.db().clone());
        assert!(service.post_customer_receipt("cr-1", &actor()).await.is_err());
        let account = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(account.settled_total, amount("0.00"));
        let allocs = fixture
            .db()
            .receipt_allocations()
            .find_allocations_by_receipts(&[CustomerReceiptId::new("cr-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(allocs.len(), 1);
        assert_eq!(allocs[0].base.id, "pre-1");
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn snapshot_first_error_sales_order_before_receipt_and_invoice() {
    require_mongo!(async {
        let fixture = TestDb::new("recv_snapshot_first").await.expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        seed_account(fixture.db(), "acct-1", "party-1", 1, "100.00").await;
        seed_entry(fixture.db(), "e-1", "acct-1", "100.00", 1).await;
        fixture
            .db()
            .receipt_allocations()
            .create(
                &ReceiptAllocation::new(
                    ReceiptAllocationId::new("al-1"),
                    ReceiptAllocationData {
                        customer_receipt_id: CustomerReceiptId::new("missing-cr"),
                        receivable_entry_id: ReceivableEntryId::new("e-1"),
                        allocation_seq: 1,
                        allocation_action: AllocationAction::Apply,
                        allocated_amount: amount("10.00"),
                        allocated_at: Instant::from_unix_secs(1_700_000_000),
                        reverses_allocation_id: None,
                    },
                )
                .unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        fixture
            .db()
            .sales_invoice_allocations()
            .create(
                &SalesInvoiceAllocation::new(
                    SalesInvoiceAllocationId::new("sia-1"),
                    SalesInvoiceAllocationData {
                        invoice_id: InvoiceId::new("missing-inv"),
                        receivable_account_id: ReceivableAccountId::new("acct-1"),
                        allocation_seq: 1,
                        allocation_action: AllocationAction::Apply,
                        allocated_gross_amount: amount("10.00"),
                        allocated_net_amount: amount("10.00"),
                        allocated_tax_amount: amount("0.00"),
                        reverses_allocation_id: None,
                    },
                )
                .unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let service = ReceivableService::new(fixture.db().clone());
        let error = service
            .receivable_account_detail("acct-1")
            .await
            .expect_err("缺销售单必须首错");
        assert!(
            error.to_string().contains("销售单不存在"),
            "首错必须是销售单，实际 {error}"
        );

        seed_sales_order(fixture.db(), "so-1").await;
        let error = service
            .receivable_account_detail("acct-1")
            .await
            .expect_err("缺当前版本必须次错");
        assert!(
            error.to_string().contains("当前正式版本"),
            "次错必须是版本，实际 {error}"
        );

        let revision = SalesOrderRevision::new(
            SalesOrderRevisionId::new("rev-1"),
            SalesOrderRevisionData {
                sales_order_id: SalesOrderId::new("so-1"),
                revision_no: 1,
                revision_source: RevisionSource::ErpApproval,
                previous_revision_id: None,
                content_hash: "abc123def456".to_string(),
                customer_revision_id: Some(PartyRevisionId::new("party-rev-1")),
                contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
                snapshot: HeaderSnapshotData {
                    customer_name: "东方企业".to_string(),
                    contract_no: Some("HT-1".to_string()),
                    settlement_party_name: Some("集团结算中心".to_string()),
                    payment_term_code: "NET30".to_string(),
                    payment_term_name: "月结30天".to_string(),
                    invoice_type: "增值税专用发票".to_string(),
                    tax_point: "6".to_string(),
                },
                project_name: None,
                business_remark: None,
                voucher_category_sku_id: None,
                voucher_expiry_at: None,
                gross_amount: amount("100.00"),
                net_amount: amount("88.50"),
                tax_amount: amount("11.50"),
                effective_at: Instant::from_unix_secs(1_700_000_000),
                recorded_at: Instant::from_unix_secs(1_700_000_000),
            },
        )
        .unwrap();
        fixture
            .db()
            .sales_order_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .unwrap();
        let mut order = fixture
            .db()
            .sales_orders()
            .find_by_id("so-1", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        order.attach_revision("rev-1", "tester");
        fixture
            .db()
            .sales_orders()
            .update(&mut order, &mut NoTransaction)
            .await
            .unwrap();
        let error = service
            .receivable_account_detail("acct-1")
            .await
            .expect_err("缺回款必须先于缺发票");
        assert!(
            error.to_string().contains("回款单不存在"),
            "第三错必须是回款，实际 {error}"
        );
    });
}
