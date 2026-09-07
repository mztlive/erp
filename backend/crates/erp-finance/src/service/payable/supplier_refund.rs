//! 供应商退款的财务反向分配、已结算回冲和减少分录写入。

use super::offset_batch::{load_payable_offset_facts, OffsetFacts};
use crate::entity::payable::PayableAccount;
use crate::entity::payable::{
    AllocationAction as PayableAllocationAction, EntryDirection as PayableEntryDirection, PayableEntry,
    PayableEntryData, PayableEntryOffset, PayableEntryOffsetData, PayableEntryType, PaymentAllocation,
    PaymentAllocationData, SupplierPayment, SupplierPaymentStatus,
};
use crate::repository::PayableExt;
use crate::{Error, Result};
use erp_core::common::time::BusinessDate;
use erp_core::common::time::Instant;
use erp_core::ids::PayableAccountId;
use erp_core::ids::{PayableEntryId, PayableEntryOffsetId, PaymentAllocationId, SupplierPaymentId};
use erp_core::money::Amount;
use mongodb::Database;
use persistence_core::Executor;

/// 财务执行只消费退款稳定身份、金额和原发生时点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierRefundPostingFact {
    /// 退款身份，作为原减少分录 source document/revision。
    pub refund_id: String,
    /// 本次退款金额。
    pub amount: Amount,
    /// 原退款发生时点。
    pub occurred_at: Instant,
}

/// 在原退款累计限额校验之前读取并确认财务付款已过账。
pub async fn load_posted_payment_for_refund(
    db: &Database,
    payment_id: &SupplierPaymentId,
    executor: &mut dyn Executor,
) -> Result<SupplierPayment> {
    let payment = db
        .supplier_payments()
        .find_by_id(payment_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原付款不存在".to_string()))?;
    if payment.status != SupplierPaymentStatus::Posted {
        return Err(Error::BusinessLogicError("只有已过账付款可以退款".to_string()));
    }
    Ok(payment)
}

/// 写入反向核销分配、冲减进度与减少分录。
///
/// # 错误
/// 跨供应商、超额冲减或仓储失败时返回错误。
pub async fn persist_refund_offsets_and_reversals(
    db: &Database,
    refund: &SupplierRefundPostingFact,
    payment: &crate::entity::payable::SupplierPayment,
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<()> {
    persist_refund(&MongoRefundPosting { db }, refund, payment, actor_id, session).await
}

async fn persist_refund<P: RefundPostingPort>(
    port: &P,
    refund: &SupplierRefundPostingFact,
    payment: &SupplierPayment,
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<()> {
    let allocations = port.allocations(&payment.base.id.clone().into(), session).await?;
    let (reverse_rows, chunks) = PaymentAllocation::plan_reverse(&allocations, refund.amount)?;
    let seqs = PaymentAllocation::next_allocation_seq_range(&allocations, reverse_rows.len())?;
    let decrease_entry = create_decrease_offsets(port, refund, payment, actor_id, &chunks, session).await?;
    if let Some(entry) = decrease_entry {
        port.entry(&entry, session).await?;
    }
    for (reverse, seq) in reverse_rows.iter().zip(seqs.iter()) {
        let allocation = PaymentAllocation::new(
            PaymentAllocationId::new(port.next_id()),
            PaymentAllocationData {
                supplier_payment_id: payment.base.id.clone().into(),
                payable_entry_id: reverse.entry_id.clone(),
                allocation_seq: *seq,
                allocation_action: PayableAllocationAction::Reverse,
                allocated_amount: reverse.amount,
                allocated_at: refund.occurred_at,
                reverses_allocation_id: Some(reverse.original_id.clone()),
            },
        )?;
        port.allocation(&allocation, session).await?;
    }
    Ok(())
}

/// 按冲减块写减少分录抵销并回冲已核销进度。
///
/// # 错误
/// 分录缺失、跨供应商或超额冲减时返回错误。
async fn create_decrease_offsets<P: RefundPostingPort>(
    port: &P,
    refund: &SupplierRefundPostingFact,
    payment: &crate::entity::payable::SupplierPayment,
    actor_id: &str,
    chunks: &[crate::entity::payable::PaymentReverseChunk],
    session: &mut dyn Executor,
) -> Result<Option<PayableEntry>> {
    let facts = port
        .facts(
            chunks
                .iter()
                .map(|chunk| chunk.increase_entry_id.clone())
                .collect(),
            session,
        )
        .await?;
    let mut decrease_entry: Option<PayableEntry> = None;
    for (offset_index, chunk) in chunks.iter().enumerate() {
        let entry = facts
            .entries
            .get(chunk.increase_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
        let account = facts
            .accounts
            .get(entry.payable_account_id.as_ref())
            .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
        if account.supplier_id != payment.supplier_id {
            return Err(Error::BusinessLogicError("禁止跨供应商退款".to_string()));
        }
        revert_supplier_refund_settlement(port, entry, chunk, actor_id, session).await?;
        if decrease_entry.is_none() {
            decrease_entry = Some(build_decrease_entry(port, refund, &entry.payable_account_id)?);
        }
        persist_decrease_offset(port, decrease_entry.as_ref(), chunk, offset_index, session).await?;
    }
    Ok(decrease_entry)
}

/// 原子回冲应付子账已核销进度。
///
/// # 错误
/// 超额冲减或仓储失败时返回错误。
async fn revert_supplier_refund_settlement<P: RefundPostingPort>(
    port: &P,
    entry: &PayableEntry,
    chunk: &crate::entity::payable::PaymentReverseChunk,
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<()> {
    let reverted = port
        .revert_settlement(&entry.payable_account_id, &chunk.amount, actor_id, session)
        .await?;
    if !reverted {
        return Err(Error::BusinessLogicError("退款冲减超过已核销金额".to_string()));
    }
    Ok(())
}

/// 构造供应商退款减少应付分录。
///
/// # 错误
/// 分录字段校验失败时返回错误。
fn build_decrease_entry<P: RefundPostingPort>(
    port: &P,
    refund: &SupplierRefundPostingFact,
    payable_account_id: &erp_core::ids::PayableAccountId,
) -> Result<PayableEntry> {
    Ok(PayableEntry::new(
        PayableEntryId::new(port.next_id()),
        PayableEntryData {
            payable_account_id: payable_account_id.clone(),
            entry_type: PayableEntryType::SupplierRefund,
            direction: PayableEntryDirection::Decrease,
            amount: refund.amount,
            due_date: port.today(),
            source_fact_type: "supplier_refund".to_string(),
            source_document_id: refund.refund_id.clone(),
            source_revision_id: refund.refund_id.clone(),
            source_sequence: 1,
            posted_at: refund.occurred_at,
        },
    )?)
}

/// 写入一条减少分录抵销。
///
/// # 错误
/// 减少分录缺失或仓储失败时返回错误。
async fn persist_decrease_offset<P: RefundPostingPort>(
    port: &P,
    decrease_entry: Option<&PayableEntry>,
    chunk: &crate::entity::payable::PaymentReverseChunk,
    offset_index: usize,
    session: &mut dyn Executor,
) -> Result<()> {
    let decrease_id = decrease_entry
        .ok_or_else(|| Error::Internal("退款减少分录未创建".to_string()))?
        .base
        .id
        .clone()
        .into();
    port.offset(
        &PayableEntryOffset::new(
            PayableEntryOffsetId::new(port.next_id()),
            PayableEntryOffsetData {
                decrease_entry_id: decrease_id,
                increase_entry_id: chunk.increase_entry_id.clone(),
                offset_sequence: offset_index as u32 + 1,
                offset_amount: chunk.amount,
            },
        )?,
        session,
    )
    .await?;
    Ok(())
}

/// 退款真实财务边界；业务构造和 plan_reverse 仍由同一生产函数执行。
#[async_trait::async_trait]
trait RefundPostingPort: Send + Sync {
    async fn allocations(
        &self,
        payment_id: &SupplierPaymentId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentAllocation>>;
    async fn facts(
        &self,
        entry_ids: Vec<PayableEntryId>,
        executor: &mut dyn Executor,
    ) -> Result<OffsetFacts<PayableEntry, PayableAccount>>;
    async fn revert_settlement(
        &self,
        account_id: &PayableAccountId,
        amount: &Amount,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
    async fn offset(&self, offset: &PayableEntryOffset, executor: &mut dyn Executor) -> Result<()>;
    async fn entry(&self, entry: &PayableEntry, executor: &mut dyn Executor) -> Result<()>;
    async fn allocation(&self, allocation: &PaymentAllocation, executor: &mut dyn Executor) -> Result<()>;
    fn next_id(&self) -> String;
    fn today(&self) -> BusinessDate;
}
struct MongoRefundPosting<'a> {
    db: &'a Database,
}
#[async_trait::async_trait]
impl RefundPostingPort for MongoRefundPosting<'_> {
    async fn allocations(
        &self,
        payment_id: &SupplierPaymentId,
        e: &mut dyn Executor,
    ) -> Result<Vec<PaymentAllocation>> {
        Ok(self
            .db
            .payment_allocations()
            .find_allocations_by_payments(std::slice::from_ref(payment_id), e)
            .await?)
    }
    async fn facts(
        &self,
        ids: Vec<PayableEntryId>,
        e: &mut dyn Executor,
    ) -> Result<OffsetFacts<PayableEntry, PayableAccount>> {
        load_payable_offset_facts(self.db, ids, e).await
    }
    async fn revert_settlement(
        &self,
        id: &PayableAccountId,
        amount: &Amount,
        actor: &str,
        e: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self
            .db
            .payable_accounts()
            .revert_settlement(id, amount, actor, e)
            .await?)
    }
    async fn offset(&self, offset: &PayableEntryOffset, e: &mut dyn Executor) -> Result<()> {
        self.db.payable_entry_offsets().create(offset, e).await?;
        Ok(())
    }
    async fn entry(&self, entry: &PayableEntry, e: &mut dyn Executor) -> Result<()> {
        self.db.payable_entries().create(entry, e).await?;
        Ok(())
    }
    async fn allocation(&self, allocation: &PaymentAllocation, e: &mut dyn Executor) -> Result<()> {
        self.db.payment_allocations().create(allocation, e).await?;
        Ok(())
    }
    fn next_id(&self) -> String {
        id_generator::next_id()
    }
    fn today(&self) -> BusinessDate {
        BusinessDate::today()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::payable::{PayableAccountData, PayableSourceType, SupplierPaymentData};
    use erp_core::ids::{FileAssetId, PartyBankAccountId, SupplierAccountId};
    use std::{collections::HashMap, str::FromStr, sync::Mutex};

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }
    fn payment() -> SupplierPayment {
        let mut payment = SupplierPayment::new(
            SupplierPaymentId::new("pay"),
            SupplierPaymentData {
                payment_no: "PAY-1".into(),
                supplier_id: SupplierAccountId::new("supplier"),
                payee_bank_account_id: PartyBankAccountId::new("bank"),
                paid_at: Instant::from_unix_secs(1),
                amount: amount("100"),
                bank_reference: None,
                bank_receipt_asset_id: FileAssetId::new("file"),
            },
        )
        .unwrap();
        payment.transition(SupplierPaymentStatus::Posted).unwrap();
        payment
    }
    fn allocation(id: &str, entry: &str, seq: u32, value: &str) -> PaymentAllocation {
        PaymentAllocation::new(
            PaymentAllocationId::new(id),
            PaymentAllocationData {
                supplier_payment_id: SupplierPaymentId::new("pay"),
                payable_entry_id: PayableEntryId::new(entry),
                allocation_seq: seq,
                allocation_action: PayableAllocationAction::Apply,
                allocated_amount: amount(value),
                allocated_at: Instant::from_unix_secs(1),
                reverses_allocation_id: None,
            },
        )
        .unwrap()
    }
    fn facts(supplier: &str) -> OffsetFacts<PayableEntry, PayableAccount> {
        let account = PayableAccount::new(
            PayableAccountId::new("account"),
            PayableAccountData {
                source_document_id: "po".into(),
                supplier_id: SupplierAccountId::new(supplier),
                source_type: PayableSourceType::PurchaseOrder,
                gross_total: amount("100"),
                settled_total: amount("100"),
                invoiceable_total: amount("100"),
                invoiced_total: amount("0"),
            },
            "actor",
        )
        .unwrap();
        let entries = ["entry1", "entry2"]
            .into_iter()
            .map(|id| {
                let entry = PayableEntry::new(
                    PayableEntryId::new(id),
                    PayableEntryData {
                        payable_account_id: PayableAccountId::new("account"),
                        entry_type: PayableEntryType::Original,
                        direction: PayableEntryDirection::Increase,
                        amount: amount("100"),
                        due_date: BusinessDate::from_str("2026-09-01").unwrap(),
                        source_fact_type: "purchase_order".into(),
                        source_document_id: "po".into(),
                        source_revision_id: "rev".into(),
                        source_sequence: 1,
                        posted_at: Instant::from_unix_secs(1),
                    },
                )
                .unwrap();
                (id.to_string(), entry)
            })
            .collect();
        OffsetFacts {
            entries,
            accounts: HashMap::from([("account".into(), account)]),
        }
    }
    struct TestExecutor {
        visits: usize,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            self.visits += 1;
            None
        }
    }
    #[derive(Default)]
    struct Recorded {
        calls: Vec<&'static str>,
        io: usize,
        ids: usize,
        offsets: Vec<PayableEntryOffset>,
        entries: Vec<PayableEntry>,
        allocations: Vec<PaymentAllocation>,
    }
    struct RecordingPort {
        identity: usize,
        fail: Option<usize>,
        supplier: &'static str,
        revert: bool,
        recorded: Mutex<Recorded>,
    }
    impl RecordingPort {
        fn io(&self, name: &'static str, e: &mut dyn Executor) -> Result<()> {
            assert_eq!(e as *mut dyn Executor as *mut () as usize, self.identity);
            e.session();
            let mut state = self.recorded.lock().unwrap();
            let index = state.io;
            state.io += 1;
            state.calls.push(name);
            if self.fail == Some(index) {
                return Err(Error::ConflictError(format!("failure {index}")));
            }
            Ok(())
        }
    }
    #[async_trait::async_trait]
    impl RefundPostingPort for RecordingPort {
        async fn allocations(
            &self,
            _: &SupplierPaymentId,
            e: &mut dyn Executor,
        ) -> Result<Vec<PaymentAllocation>> {
            self.io("allocations", e)?;
            Ok(vec![
                allocation("apply1", "entry1", 1, "60"),
                allocation("apply2", "entry2", 2, "40"),
            ])
        }
        async fn facts(
            &self,
            _: Vec<PayableEntryId>,
            e: &mut dyn Executor,
        ) -> Result<OffsetFacts<PayableEntry, PayableAccount>> {
            self.io("facts", e)?;
            Ok(facts(self.supplier))
        }
        async fn revert_settlement(
            &self,
            _: &PayableAccountId,
            _: &Amount,
            _: &str,
            e: &mut dyn Executor,
        ) -> Result<bool> {
            self.io("settlement", e)?;
            Ok(self.revert)
        }
        async fn offset(&self, offset: &PayableEntryOffset, e: &mut dyn Executor) -> Result<()> {
            self.io("offset", e)?;
            self.recorded.lock().unwrap().offsets.push(offset.clone());
            Ok(())
        }
        async fn entry(&self, entry: &PayableEntry, e: &mut dyn Executor) -> Result<()> {
            self.io("entry", e)?;
            self.recorded.lock().unwrap().entries.push(entry.clone());
            Ok(())
        }
        async fn allocation(&self, allocation: &PaymentAllocation, e: &mut dyn Executor) -> Result<()> {
            self.io("reverse", e)?;
            self.recorded.lock().unwrap().allocations.push(allocation.clone());
            Ok(())
        }
        fn next_id(&self) -> String {
            let mut state = self.recorded.lock().unwrap();
            state.calls.push("id");
            state.ids += 1;
            format!("id{}", state.ids)
        }
        fn today(&self) -> BusinessDate {
            self.recorded.lock().unwrap().calls.push("today");
            BusinessDate::from_str("2026-09-01").unwrap()
        }
    }
    fn input() -> SupplierRefundPostingFact {
        SupplierRefundPostingFact {
            refund_id: "refund".into(),
            amount: amount("80"),
            occurred_at: Instant::from_unix_secs(10),
        }
    }
    fn port(executor: &mut TestExecutor) -> RecordingPort {
        RecordingPort {
            identity: (executor as *mut TestExecutor) as usize,
            fail: None,
            supplier: "supplier",
            revert: true,
            recorded: Mutex::new(Recorded::default()),
        }
    }

    #[tokio::test]
    async fn refund_offsets_precede_decrease_entry_and_reverse_amount_is_conserved() {
        let mut e = TestExecutor { visits: 0 };
        let port = port(&mut e);
        let payment = payment();
        persist_refund(&port, &input(), &payment, "actor", &mut e)
            .await
            .unwrap();
        let state = port.recorded.lock().unwrap();
        assert_eq!(
            state.calls,
            [
                "allocations",
                "facts",
                "settlement",
                "id",
                "today",
                "id",
                "offset",
                "settlement",
                "id",
                "offset",
                "entry",
                "id",
                "reverse",
                "id",
                "reverse"
            ]
        );
        assert_eq!(state.entries.len(), 1);
        let entry = &state.entries[0];
        assert_eq!(entry.base.id, "id1");
        assert_eq!(entry.source_document_id, "refund");
        assert_eq!(entry.source_revision_id, "refund");
        assert_eq!(entry.amount, amount("80"));
        assert_eq!(entry.posted_at, Instant::from_unix_secs(10));
        let total = state
            .allocations
            .iter()
            .fold(amount("0"), |a, row| a.checked_add(row.allocated_amount));
        assert_eq!(total, amount("80"));
        let offset_total = state
            .offsets
            .iter()
            .fold(amount("0"), |a, row| a.checked_add(row.offset_amount));
        assert_eq!(offset_total, total);
        assert_eq!(
            state
                .allocations
                .iter()
                .map(|row| row.allocation_seq)
                .collect::<Vec<_>>(),
            [3, 4]
        );
        assert!(state
            .allocations
            .iter()
            .all(|row| row.allocation_action == PayableAllocationAction::Reverse
                && row.reverses_allocation_id.is_some()));
        assert_eq!(payment.status, SupplierPaymentStatus::Posted);
        assert_eq!(e.visits, state.io);
    }

    #[tokio::test]
    async fn supplier_refund_finance_stops_at_each_io_failure() {
        for fail in 0..9 {
            let mut e = TestExecutor { visits: 0 };
            let mut port = port(&mut e);
            port.fail = Some(fail);
            let error = persist_refund(&port, &input(), &payment(), "actor", &mut e)
                .await
                .unwrap_err();
            assert!(matches!(error,Error::ConflictError(message) if message==format!("failure {fail}")));
            assert_eq!(port.recorded.lock().unwrap().io, fail + 1);
        }
    }

    #[tokio::test]
    async fn supplier_and_conditional_settlement_guards_precede_ids_and_offsets() {
        for foreign in [true, false] {
            let mut e = TestExecutor { visits: 0 };
            let mut port = port(&mut e);
            port.supplier = if foreign { "other" } else { "supplier" };
            port.revert = false;
            let error = persist_refund(&port, &input(), &payment(), "actor", &mut e)
                .await
                .unwrap_err();
            assert!(
                matches!(error,Error::BusinessLogicError(message) if message==if foreign{"禁止跨供应商退款"}else{"退款冲减超过已核销金额"})
            );
            let state = port.recorded.lock().unwrap();
            assert_eq!(state.ids, 0);
            assert!(state.offsets.is_empty());
            assert!(state.entries.is_empty());
            assert!(state.allocations.is_empty());
        }
    }
}
