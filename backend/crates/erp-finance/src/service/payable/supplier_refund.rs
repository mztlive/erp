//! 原付款退款只追加反向核销并回冲已结算金额。应付差额由采购变更独立追加，禁止重复冲减。

use erp_core::common::time::Instant;
use erp_core::ids::{PayableAccountId, PayableEntryId, PaymentAllocationId, SupplierPaymentId};
use erp_core::money::Amount;
use mongodb::Database;
use persistence_core::Executor;

use super::offset_batch::{OffsetFacts, load_payable_offset_facts};
use crate::entity::payable::{
    AllocationAction as PayableAllocationAction, PayableAccount, PayableEntry, PaymentAllocation,
    PaymentAllocationData, SupplierPayment, SupplierPaymentStatus,
};
use crate::repository::PayableExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 财务执行只消费退款稳定身份、金额和原发生时点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierRefundPostingFact {
    /// 退款身份，供调用方追溯退款记录。
    pub refund_id: String,
    /// 本次退款金额。
    pub amount: Amount,
    /// 原退款发生时点。
    pub occurred_at: Instant,
}

/// 在原退款累计限额校验之前读取并确认财务付款已过账。
///
/// # 参数
/// * `db` - 财务领域数据库。
/// * `payment_id` - 原付款单主键。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回状态为 `Posted` 的付款单。
///
/// # 错误
/// 付款不存在时返回 `NotFound`；状态不是已过账时返回 `BusinessLogicError`；仓储读取失败时返回对应错误。
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

/// 写入反向核销分配并回冲已付进度，不修改应付总额。
///
/// # 参数
/// * `db` - 财务领域数据库。
/// * `refund` - 退款稳定身份、金额与原发生时点。
/// * `payment` - 已过账的原付款。
/// * `actor_id` - 执行人。
/// * `session` - 调用方事务执行器。
///
/// # 返回
/// 反向分配和进度写入完成时无返回值；采购应付差额由原采购改单负责。
///
/// # 错误
/// 分录或账户缺失、跨供应商、超额冲减、实体构造失败或仓储失败时返回对应错误。
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
    revert_settlements(port, payment, actor_id, &chunks, session).await?;
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

/// 按实际反向核销块回冲已付进度，校验原供应商和来源账户。
///
/// # 错误
/// 分录缺失、跨供应商或超额冲减时返回错误。
async fn revert_settlements<P: RefundPostingPort>(
    port: &P,
    payment: &crate::entity::payable::SupplierPayment,
    actor_id: &str,
    chunks: &[crate::entity::payable::PaymentReverseChunk],
    session: &mut dyn Executor,
) -> Result<()> {
    let facts =
        port.facts(chunks.iter().map(|chunk| chunk.increase_entry_id.clone()).collect(), session).await?;
    for chunk in chunks {
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
    }
    Ok(())
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
    let reverted =
        port.revert_settlement(&entry.payable_account_id, &chunk.amount, actor_id, session).await?;
    if !reverted {
        return Err(Error::BusinessLogicError("退款冲减超过已核销金额".to_string()));
    }
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
    async fn allocation(&self, allocation: &PaymentAllocation, executor: &mut dyn Executor) -> Result<()>;
    fn next_id(&self) -> String;
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
        Ok(self.db.payable_accounts().revert_settlement(id, amount, actor, e).await?)
    }
    async fn allocation(&self, allocation: &PaymentAllocation, e: &mut dyn Executor) -> Result<()> {
        self.db.payment_allocations().create(allocation, e).await?;
        Ok(())
    }
    fn next_id(&self) -> String {
        id_generator::next_id()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::Mutex;

    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{FileAssetId, PartyBankAccountId, SupplierAccountId};

    use super::*;
    use crate::entity::payable::{
        EntryDirection as PayableEntryDirection, PayableAccountData, PayableEntryData, PayableEntryOffset,
        PayableEntryType, PayableSourceType, PendingPaymentAllocation, SupplierPaymentData,
    };

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
            "creator",
        )
        .unwrap();
        payment.payment_request_id = Some("request-1".into());
        payment.payer_party_id = Some("company-1".into());
        payment.payer_party_name = Some("我方公司".into());
        payment.request_allocations = vec![crate::entity::payable::PaymentRequestAllocation {
            payable_account_id: "account-1".into(),
            amount: amount("100"),
        }];
        payment
            .post_from_execution(
                &[PendingPaymentAllocation::new(PayableEntryId::new("entry"), amount("100")).unwrap()],
                "executor",
                Instant::from_unix_secs(2),
            )
            .unwrap();
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
        OffsetFacts { entries, accounts: HashMap::from([("account".into(), account)]) }
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
            Ok(vec![allocation("apply1", "entry1", 1, "60"), allocation("apply2", "entry2", 2, "40")])
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
    async fn refund_reverses_settlement_without_reducing_purchase_liability_twice() {
        let mut e = TestExecutor { visits: 0 };
        let port = port(&mut e);
        let payment = payment();
        persist_refund(&port, &input(), &payment, "actor", &mut e).await.unwrap();
        let state = port.recorded.lock().unwrap();
        assert_eq!(
            state.calls,
            ["allocations", "facts", "settlement", "settlement", "id", "reverse", "id", "reverse"]
        );
        assert!(state.entries.is_empty());
        assert!(state.offsets.is_empty());
        let total = state.allocations.iter().fold(amount("0"), |a, row| a.checked_add(row.allocated_amount));
        assert_eq!(total, amount("80"));
        let offset_total = state.offsets.iter().fold(amount("0"), |a, row| a.checked_add(row.offset_amount));
        assert_eq!(offset_total, amount("0"));
        assert_eq!(state.allocations.iter().map(|row| row.allocation_seq).collect::<Vec<_>>(), [3, 4]);
        assert!(
            state.allocations.iter().all(|row| row.allocation_action == PayableAllocationAction::Reverse
                && row.reverses_allocation_id.is_some())
        );
        assert_eq!(payment.status, SupplierPaymentStatus::Posted);
        assert_eq!(e.visits, state.io);
    }

    #[tokio::test]
    async fn supplier_refund_finance_stops_at_each_io_failure() {
        for fail in 0..6 {
            let mut e = TestExecutor { visits: 0 };
            let mut port = port(&mut e);
            port.fail = Some(fail);
            let error = persist_refund(&port, &input(), &payment(), "actor", &mut e).await.unwrap_err();
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
            let error = persist_refund(&port, &input(), &payment(), "actor", &mut e).await.unwrap_err();
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
