//! 客户退款真实财务生产算法的纯仓储替身合同。
use super::*;
use crate::entity::receivable::{AccountReviewStatus, CustomerReceiptData, ReceivableAccountData};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{CustomerAccountId, PartyId, ReceivableAccountId, SalesOrderId, SalesOrderRevisionId};
use std::str::FromStr;
struct TestExecutor {
    _identity: u8,
}
impl Executor for TestExecutor {
    fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
        None
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Allocations,
    Facts,
    Settlement(String),
    Offset(u32),
    Decrease,
    Reverse(u32),
}
struct RecordingStore {
    executor: usize,
    calls: Vec<Call>,
    fail_at: Option<Call>,
    settlement_false: bool,
    allocations: Vec<ReceiptAllocation>,
    facts: OffsetFacts<ReceivableEntry, ReceivableAccount>,
    offsets: Vec<ReceivableEntryOffset>,
    decrease: Option<ReceivableEntry>,
    reverses: Vec<ReceiptAllocation>,
}
impl RecordingStore {
    fn new(ex: &mut TestExecutor) -> Self {
        let entries = vec![entry("entry-1", "account-1"), entry("entry-2", "account-2")];
        let accounts = vec![account("account-1"), account("account-2")];
        Self {
            executor: ex as *mut TestExecutor as usize,
            calls: vec![],
            fail_at: None,
            settlement_false: false,
            allocations: vec![
                allocation("allocation-2", 2, "entry-2", "30"),
                allocation("allocation-1", 1, "entry-1", "40"),
            ],
            facts: OffsetFacts {
                entries: entries.into_iter().map(|e| (e.base.id.clone(), e)).collect(),
                accounts: accounts.into_iter().map(|a| (a.base.id.clone(), a)).collect(),
            },
            offsets: vec![],
            decrease: None,
            reverses: vec![],
        }
    }
    fn visit(&mut self, call: Call, ex: &mut dyn Executor) -> Result<()> {
        assert_eq!(ex as *mut dyn Executor as *mut () as usize, self.executor);
        self.calls.push(call.clone());
        if self.fail_at.as_ref() == Some(&call) {
            return Err(Error::ConflictError("原退款仓储冲突".into()));
        }
        Ok(())
    }
}
#[async_trait]
impl RefundStore for RecordingStore {
    async fn allocations(
        &mut self,
        receipt: &CustomerReceipt,
        ex: &mut dyn Executor,
    ) -> Result<Vec<ReceiptAllocation>> {
        self.visit(Call::Allocations, ex)?;
        assert_eq!(receipt.base.id, "receipt-1");
        Ok(self.allocations.clone())
    }
    async fn offset_facts(
        &mut self,
        _chunks: &[ReceiptReverseChunk],
        ex: &mut dyn Executor,
    ) -> Result<OffsetFacts<ReceivableEntry, ReceivableAccount>> {
        self.visit(Call::Facts, ex)?;
        Ok(self.facts.clone())
    }
    async fn revert_settlement(
        &mut self,
        entry: &ReceivableEntry,
        chunk: &ReceiptReverseChunk,
        actor_id: &str,
        ex: &mut dyn Executor,
    ) -> Result<bool> {
        self.visit(Call::Settlement(entry.base.id.clone()), ex)?;
        assert_eq!(chunk.increase_entry_id.as_ref(), entry.base.id);
        assert_eq!(actor_id, "actor-1");
        Ok(!self.settlement_false)
    }
    async fn offset(&mut self, offset: &ReceivableEntryOffset, ex: &mut dyn Executor) -> Result<()> {
        self.visit(Call::Offset(offset.offset_sequence), ex)?;
        assert!(self.decrease.is_none(), "原序要求抵销先于减少分录入库");
        self.offsets.push(offset.clone());
        Ok(())
    }
    async fn decrease_entry(&mut self, entry: &ReceivableEntry, ex: &mut dyn Executor) -> Result<()> {
        self.visit(Call::Decrease, ex)?;
        assert!(!self.offsets.is_empty());
        for offset in &self.offsets {
            assert_eq!(offset.decrease_entry_id.as_ref(), entry.base.id);
        }
        self.decrease = Some(entry.clone());
        Ok(())
    }
    async fn reverse_allocation(
        &mut self,
        allocation: &ReceiptAllocation,
        ex: &mut dyn Executor,
    ) -> Result<()> {
        self.visit(Call::Reverse(allocation.allocation_seq), ex)?;
        assert!(self.decrease.is_some());
        self.reverses.push(allocation.clone());
        Ok(())
    }
}
fn a(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}
fn account(id: &str) -> ReceivableAccount {
    ReceivableAccount::new(
        ReceivableAccountId::new(id),
        ReceivableAccountData {
            sales_order_id: SalesOrderId::new("sales-1"),
            account_seq: 1,
            customer_id: CustomerAccountId::new("customer-1"),
            counterparty_party_id: PartyId::new("party-1"),
            source_sales_order_revision_id: SalesOrderRevisionId::new("revision-1"),
            review_status: AccountReviewStatus::NotApplicable,
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: a("100"),
            settled_total: a("70"),
            invoiceable_total: a("100"),
            invoiced_total: a("0"),
        },
        "actor-1",
    )
    .unwrap()
}
fn entry(id: &str, account_id: &str) -> ReceivableEntry {
    ReceivableEntry::new(
        ReceivableEntryId::new(id),
        ReceivableEntryData {
            receivable_account_id: ReceivableAccountId::new(account_id),
            entry_type: ReceivableEntryType::Original,
            direction: ReceivableEntryDirection::Increase,
            amount: a("100"),
            due_date: BusinessDate::today(),
            source_fact_type: "sales_order".into(),
            source_document_id: "sales-1".into(),
            source_revision_id: "revision-1".into(),
            source_sequence: 1,
            posted_at: Instant::from_unix_secs(10),
        },
    )
    .unwrap()
}
fn receipt() -> CustomerReceipt {
    let mut receipt = CustomerReceipt::new(
        CustomerReceiptId::new("receipt-1"),
        CustomerReceiptData {
            receipt_no: "RC-1".into(),
            counterparty_party_id: PartyId::new("party-1"),
            customer_id: Some(CustomerAccountId::new("customer-1")),
            received_at: Instant::from_unix_secs(20),
            amount: a("100"),
            bank_reference: None,
        },
        "actor-1",
    )
    .unwrap();
    receipt.status = CustomerReceiptStatus::Posted;
    receipt
}
fn allocation(id: &str, seq: u32, entry: &str, amount: &str) -> ReceiptAllocation {
    ReceiptAllocation::new(
        ReceiptAllocationId::new(id),
        ReceiptAllocationData {
            customer_receipt_id: CustomerReceiptId::new("receipt-1"),
            receivable_entry_id: ReceivableEntryId::new(entry),
            allocation_seq: seq,
            allocation_action: ReceivableAllocationAction::Apply,
            allocated_amount: a(amount),
            allocated_at: Instant::from_unix_secs(20),
            reverses_allocation_id: None,
        },
    )
    .unwrap()
}
fn refund() -> CustomerRefundPosting {
    CustomerRefundPosting {
        refund_id: "refund-1".into(),
        amount: a("50"),
        occurred_at: Instant::from_unix_secs(30),
    }
}
fn order() -> Vec<Call> {
    vec![
        Call::Allocations,
        Call::Facts,
        Call::Settlement("entry-1".into()),
        Call::Offset(1),
        Call::Settlement("entry-2".into()),
        Call::Offset(2),
        Call::Decrease,
        Call::Reverse(3),
        Call::Reverse(4),
    ]
}

/// 真实算法保持逐chunk抵销先写、唯一减少分录后写、反向核销最后写，原回款保持Posted。
#[tokio::test]
async fn refund_writes_offsets_before_decrease_then_reverse_allocations_with_frozen_facts() {
    let mut ex = TestExecutor { _identity: 1 };
    let mut store = RecordingStore::new(&mut ex);
    let receipt = receipt();
    let input = refund();
    post_with_store(&mut store, &input, &receipt, "actor-1", &mut ex)
        .await
        .unwrap();
    assert_eq!(store.calls, order());
    assert_eq!(receipt.status, CustomerReceiptStatus::Posted);
    assert_eq!(store.offsets[0].offset_amount, a("40"));
    assert_eq!(store.offsets[1].offset_amount, a("10"));
    assert_eq!(store.offsets[0].increase_entry_id.as_ref(), "entry-1");
    assert_eq!(store.offsets[1].increase_entry_id.as_ref(), "entry-2");
    let decrease = store.decrease.unwrap();
    assert_eq!(decrease.amount, input.amount);
    assert_eq!(decrease.entry_type, ReceivableEntryType::Refund);
    assert_eq!(decrease.direction, ReceivableEntryDirection::Decrease);
    assert_eq!(decrease.receivable_account_id.as_ref(), "account-1");
    assert_eq!(decrease.source_fact_type, "customer_refund");
    assert_eq!(decrease.source_document_id, input.refund_id);
    assert_eq!(decrease.source_revision_id, input.refund_id);
    assert_eq!(decrease.source_sequence, 1);
    assert_eq!(decrease.posted_at, input.occurred_at);
    for (index, allocation) in store.reverses.iter().enumerate() {
        assert_eq!(allocation.allocation_action, ReceivableAllocationAction::Reverse);
        assert_eq!(allocation.allocated_at, input.occurred_at);
        assert_eq!(allocation.customer_receipt_id.as_ref(), "receipt-1");
        assert_eq!(
            allocation.reverses_allocation_id.as_ref().unwrap().as_ref(),
            format!("allocation-{}", index + 1)
        );
        assert_ne!(allocation.base.id, decrease.base.id);
    }
}
/// 每个实际仓储失败保持原错误并停止其后全部步骤，包含第二chunk失败不写减少分录。
#[tokio::test]
async fn refund_stops_at_each_repository_failure_with_same_executor() {
    let expected = order();
    for (index, call) in expected.iter().enumerate() {
        let mut ex = TestExecutor { _identity: 1 };
        let mut store = RecordingStore::new(&mut ex);
        store.fail_at = Some(call.clone());
        assert!(
            matches!(post_with_store(&mut store,&refund(),&receipt(),"actor-1",&mut ex).await,Err(Error::ConflictError(message)) if message=="原退款仓储冲突")
        );
        assert_eq!(store.calls, expected[..=index]);
    }
}
/// 分录/账户缺项及跨主体在回冲之前失败关闭；条件回冲false保持原文案。
#[tokio::test]
async fn refund_keeps_missing_cross_party_and_settlement_failure_order() {
    for case in 0..4 {
        let mut ex = TestExecutor { _identity: 1 };
        let mut store = RecordingStore::new(&mut ex);
        match case {
            0 => {
                store.facts.entries.remove("entry-1");
            }
            1 => {
                store.facts.accounts.remove("account-1");
            }
            2 => {
                store
                    .facts
                    .accounts
                    .get_mut("account-1")
                    .unwrap()
                    .counterparty_party_id = PartyId::new("other-party")
            }
            _ => store.settlement_false = true,
        }
        let error = post_with_store(&mut store, &refund(), &receipt(), "actor-1", &mut ex)
            .await
            .unwrap_err();
        match case {
            0 => assert!(matches!(error,Error::NotFound(message) if message=="应收分录不存在")),
            1 => assert!(matches!(error,Error::NotFound(message) if message=="应收往来子账不存在")),
            2 => assert!(matches!(error,Error::BusinessLogicError(message) if message=="禁止跨往来主体退款")),
            _ => assert!(
                matches!(error,Error::BusinessLogicError(message) if message=="退款冲减超过已核销金额")
            ),
        };
        assert_eq!(store.calls.len(), if case == 3 { 3 } else { 2 });
        assert!(store.offsets.is_empty());
        assert!(store.decrease.is_none());
        assert!(store.reverses.is_empty());
    }
}
/// 第二次部分退款消费第一次反向分配；累计可逆分配不足在批读事实和任何写入之前拒绝。
#[tokio::test]
async fn refund_reloads_interleaved_reverse_facts_and_rejects_over_reversal() {
    let mut ex = TestExecutor { _identity: 1 };
    let mut store = RecordingStore::new(&mut ex);
    post_with_store(&mut store, &refund(), &receipt(), "actor-1", &mut ex)
        .await
        .unwrap();
    store.allocations.extend(store.reverses.clone());
    store.calls.clear();
    store.offsets.clear();
    store.decrease = None;
    store.reverses.clear();
    let second = CustomerRefundPosting {
        refund_id: "refund-2".into(),
        amount: a("10"),
        occurred_at: Instant::from_unix_secs(40),
    };
    post_with_store(&mut store, &second, &receipt(), "actor-1", &mut ex)
        .await
        .unwrap();
    assert_eq!(store.reverses.len(), 1);
    assert_eq!(store.reverses[0].receivable_entry_id.as_ref(), "entry-2");
    assert_eq!(store.reverses[0].allocation_seq, 5);
    assert_eq!(store.reverses[0].allocated_amount, a("10"));
    store.allocations.extend(store.reverses.clone());
    store.calls.clear();
    let excess = CustomerRefundPosting {
        refund_id: "refund-3".into(),
        amount: a("11"),
        occurred_at: Instant::from_unix_secs(50),
    };
    assert!(matches!(
        post_with_store(&mut store, &excess, &receipt(), "actor-1", &mut ex).await,
        Err(Error::Logic(_))
    ));
    assert_eq!(store.calls, [Call::Allocations]);
}
