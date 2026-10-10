//! 付款冲正的财务分段写入；付款任务位于回冲结算与反向分配之间。

use std::collections::{BTreeMap, HashSet};

use erp_core::common::time::Instant;
use erp_core::ids::{PayableAccountId, PaymentAllocationId, SupplierPaymentId};
use erp_core::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::offset_batch::load_payable_offset_facts;
use crate::entity::payable::{
    AllocationAction as PayableAllocationAction, PaymentAllocation, PaymentAllocationData,
    PaymentRequestAllocation, PaymentReversePlanRow, SupplierPayment, SupplierPaymentStatus,
};
use crate::repository::PayableExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 反向财务分配实际消费的金额与原发生时间。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentReversalFact {
    /// 本次反向金额。
    pub amount: Amount,
    /// 反向分配发生时点。
    pub occurred_at: Instant,
}

/// 结算回冲完成后待写入的反向分配和原付款状态。
pub struct PaymentReversalWrite {
    payment: SupplierPayment,
    reversal: PaymentReversalFact,
    reverse_rows: Vec<PaymentReversePlanRow>,
    seqs: Vec<u32>,
}

/// 在冲正累计上限检查之前读取并确认原付款已过账。
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
pub async fn load_posted_payment_for_reversal(
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
        return Err(Error::BusinessLogicError("只有已过账付款可以冲正".to_string()));
    }
    payment.require_payment_request_basis()?;
    Ok(payment)
}

/// 读取分配、规划反转并回冲全部 settlement，返回原 HashSet 供流程逐账户同步任务。
///
/// 调用方必须完成任务同步后才调用返回计划的 persist，不得重排反向分配写入。
///
/// # 参数
/// * `db` - 财务领域数据库。
/// * `payment` - 已过账的原付款。
/// * `reversal` - 本次冲正金额与发生时间。
/// * `actor_id` - 执行人。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 返回待持久化的冲正计划，以及本次回冲涉及的应付子账 ID。
///
/// # 错误
/// 分配计划、序号、分录或账户缺失、超额冲减或仓储失败时返回对应错误。
pub async fn prepare_payment_reversal(
    db: &Database,
    payment: SupplierPayment,
    reversal: PaymentReversalFact,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<(PaymentReversalWrite, HashSet<PayableAccountId>)> {
    let allocations = db
        .payment_allocations()
        .find_allocations_by_payments(&[payment.base.id.clone().into()], executor)
        .await?;
    let (reverse_rows, chunks) = PaymentAllocation::plan_reverse(&allocations, reversal.amount)?;
    let seqs = PaymentAllocation::next_allocation_seq_range(&allocations, reverse_rows.len())?;
    let affected_accounts = revert_payment_settlements(db, &chunks, actor_id, executor).await?;
    Ok((PaymentReversalWrite { payment, reversal, reverse_rows, seqs }, affected_accounts))
}

impl PaymentReversalWrite {
    /// 读取原付款身份及此次准确的逐应付冲正金额。
    /// # 参数
    /// * `db` - 财务数据库。
    /// * `executor` - 当前冲正事务。
    /// # 返回
    /// 返回原付款和由实际冲正计划汇总的逐应付金额。
    /// # 错误
    /// 原分录缺失或金额无法汇总时返回错误。
    pub async fn payment_request_reversal(
        &self,
        db: &Database,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierPayment, Vec<PaymentRequestAllocation>)> {
        let ids = self.reverse_rows.iter().map(|row| row.entry_id.clone()).collect::<Vec<_>>();
        let entries = db.payable_entries().find_entries_by_ids(&ids, executor).await?;
        let mut totals = BTreeMap::<String, Amount>::new();
        for row in &self.reverse_rows {
            let entry = entries
                .iter()
                .find(|entry| entry.base.id == row.entry_id.as_ref())
                .ok_or_else(|| Error::NotFound("冲正应付分录不存在".into()))?;
            let total = totals.entry(entry.payable_account_id.to_string()).or_insert(Amount::zero());
            *total = Amount::try_from(
                total
                    .to_decimal()
                    .checked_add(row.amount.to_decimal())
                    .ok_or_else(|| Error::BusinessLogicError("冲正金额汇总溢出".into()))?,
            )?;
        }
        Ok((
            self.payment.clone(),
            totals
                .into_iter()
                .map(|(payable_account_id, amount)| PaymentRequestAllocation { payable_account_id, amount })
                .collect(),
        ))
    }

    /// 付款任务同步成功后，用同一 Executor 写反向分配并累计原付款冲正金额。
    ///
    /// 本方法消耗 `self`。
    ///
    /// # 参数
    /// * `db` - 财务领域数据库。
    /// * `executor` - 与回冲结算相同的调用方执行器。
    ///
    /// # 返回
    /// 反向分配和原付款净金额写入成功后无返回值；全额冲正才变为 `Reversed`。
    ///
    /// # 错误
    /// 反向分配写入、状态迁移或付款更新失败时返回对应错误。
    pub async fn persist(self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        let (mut payment, allocations) = self.prepare_posting()?;
        for allocation in &allocations {
            db.payment_allocations().create(allocation, executor).await?;
        }
        db.supplier_payments().update(&mut payment, executor).await?;
        Ok(())
    }

    /// 将同一计划转为真实反向分配与原付款的净金额状态。
    fn prepare_posting(self) -> Result<(SupplierPayment, Vec<PaymentAllocation>)> {
        let mut payment = self.payment;
        payment.record_reversal(self.reversal.amount)?;
        let allocations =
            build_reverse_allocations(&self.reversal, &payment, &self.reverse_rows, &self.seqs)?;
        Ok((payment, allocations))
    }
}

/// 按冲减块回冲应付子账已核销进度。
///
/// # 错误
/// 分录缺失或超额冲减时返回错误。
async fn revert_payment_settlements(
    db: &Database,
    chunks: &[crate::entity::payable::PaymentReverseChunk],
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<HashSet<PayableAccountId>> {
    let facts =
        load_payable_offset_facts(db, chunks.iter().map(|chunk| chunk.increase_entry_id.clone()), session)
            .await?;
    let mut affected_accounts = HashSet::new();
    for chunk in chunks {
        let entry = facts
            .entries
            .get(chunk.increase_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
        if !facts.accounts.contains_key(entry.payable_account_id.as_ref()) {
            return Err(Error::NotFound("应付往来子账不存在".to_string()));
        }
        let reverted = db
            .payable_accounts()
            .revert_settlement(&entry.payable_account_id, &chunk.amount, actor_id, session)
            .await?;
        if !reverted {
            return Err(Error::BusinessLogicError("冲正冲减超过已核销金额".to_string()));
        }
        affected_accounts.insert(entry.payable_account_id.clone());
    }
    Ok(affected_accounts)
}

/// 写入反向核销分配。
///
/// # 错误
/// 仓储失败时返回错误。
fn build_reverse_allocations(
    reversal: &PaymentReversalFact,
    payment: &SupplierPayment,
    reverse_rows: &[crate::entity::payable::PaymentReversePlanRow],
    seqs: &[u32],
) -> Result<Vec<PaymentAllocation>> {
    if reverse_rows.len() != seqs.len() {
        return Err(Error::Internal("反向分配与序号数量不一致".into()));
    }
    let mut allocations = Vec::with_capacity(reverse_rows.len());
    for (reverse, seq) in reverse_rows.iter().zip(seqs.iter()) {
        let allocation = PaymentAllocation::new(
            PaymentAllocationId::new(next_id()),
            PaymentAllocationData {
                supplier_payment_id: payment.base.id.clone().into(),
                payable_entry_id: reverse.entry_id.clone(),
                allocation_seq: *seq,
                allocation_action: PayableAllocationAction::Reverse,
                allocated_amount: reverse.amount,
                allocated_at: reversal.occurred_at,
                reverses_allocation_id: Some(reverse.original_id.clone()),
            },
        )?;
        allocations.push(allocation);
    }
    Ok(allocations)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{FileAssetId, PartyBankAccountId, PayableEntryId, SupplierAccountId};

    use super::*;
    use crate::entity::payable::{PendingPaymentAllocation, SupplierPaymentData};

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn payment() -> SupplierPayment {
        let mut payment = SupplierPayment::new(
            SupplierPaymentId::new("payment-1"),
            SupplierPaymentData {
                payment_no: "FK-1".into(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                payee_bank_account_id: PartyBankAccountId::new("bank-1"),
                paid_at: Instant::from_unix_secs(1),
                amount: amount("100"),
                bank_reference: None,
                bank_receipt_asset_id: FileAssetId::new("receipt-1"),
            },
            "cashier",
        )
        .unwrap();
        payment.payment_request_id = Some("request-1".into());
        payment.payer_party_id = Some("company-1".into());
        payment.payer_party_name = Some("我方公司".into());
        payment.request_allocations =
            vec![PaymentRequestAllocation { payable_account_id: "account-1".into(), amount: amount("100") }];
        payment
            .post_from_execution(
                &[PendingPaymentAllocation::new(PayableEntryId::new("entry-1"), amount("100")).unwrap()],
                "cashier",
                Instant::from_unix_secs(1),
            )
            .unwrap();
        payment
    }

    fn initial_allocation() -> PaymentAllocation {
        PaymentAllocation::new(
            PaymentAllocationId::new("allocation-1"),
            PaymentAllocationData {
                supplier_payment_id: SupplierPaymentId::new("payment-1"),
                payable_entry_id: PayableEntryId::new("entry-1"),
                allocation_seq: 1,
                allocation_action: PayableAllocationAction::Apply,
                allocated_amount: amount("100"),
                allocated_at: Instant::from_unix_secs(1),
                reverses_allocation_id: None,
            },
        )
        .unwrap()
    }

    fn prepare(
        payment: SupplierPayment,
        allocations: &[PaymentAllocation],
        value: &str,
    ) -> PaymentReversalWrite {
        let (reverse_rows, _) = PaymentAllocation::plan_reverse(allocations, amount(value)).unwrap();
        let seqs = PaymentAllocation::next_allocation_seq_range(allocations, reverse_rows.len()).unwrap();
        PaymentReversalWrite {
            payment,
            reversal: PaymentReversalFact { amount: amount(value), occurred_at: Instant::from_unix_secs(2) },
            reverse_rows,
            seqs,
        }
    }

    #[test]
    fn partial_reversal_keeps_payment_posted_and_remaining_can_be_reversed() {
        let original = payment();
        let mut allocations = vec![initial_allocation()];
        let (partial, reverses) = prepare(original.clone(), &allocations, "30").prepare_posting().unwrap();
        assert_eq!(partial.amount, amount("100"));
        assert_eq!(partial.reversed_amount, amount("30"));
        assert_eq!(partial.effective_amount(), amount("70"));
        assert_eq!(partial.status, SupplierPaymentStatus::Posted);
        assert_eq!(reverses[0].reverses_allocation_id.as_ref().unwrap().as_ref(), "allocation-1");
        assert_eq!(reverses[0].allocated_amount, amount("30"));
        allocations.extend(reverses);
        assert!(PaymentAllocation::plan_reverse(&allocations, amount("70.01")).is_err());
        let (reversed, reverses) = prepare(partial, &allocations, "70").prepare_posting().unwrap();
        assert_eq!(reversed.status, SupplierPaymentStatus::Reversed);
        assert_eq!(reversed.reversed_amount, original.amount);
        assert_eq!(reversed.effective_amount(), Amount::zero());
        assert_eq!(reverses[0].allocated_amount, amount("70"));
        allocations.extend(reverses);
        assert!(PaymentAllocation::plan_reverse(&allocations, amount("0.01")).is_err());
    }

    #[test]
    fn invalid_reversal_does_not_change_original_payment_or_create_allocations() {
        let original = payment();
        for value in ["0", "-1", "100.01"] {
            let mut payment = original.clone();
            assert!(payment.record_reversal(amount(value)).is_err());
            assert_eq!(payment, original);
        }
        let mut draft = original.clone();
        draft.status = SupplierPaymentStatus::Draft;
        let expected = draft.clone();
        assert!(draft.record_reversal(amount("1")).is_err());
        assert_eq!(draft, expected);
    }
}
