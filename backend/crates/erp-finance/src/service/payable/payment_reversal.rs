//! 付款冲正的财务分段写入；付款任务位于回冲结算与反向分配之间。

use super::offset_batch::load_payable_offset_facts;
use crate::entity::payable::{
    AllocationAction as PayableAllocationAction, PaymentAllocation, PaymentAllocationData,
    PaymentReversePlanRow, SupplierPayment, SupplierPaymentStatus,
};
use crate::repository::PayableExt;
use crate::{Error, Result};
use erp_core::common::time::Instant;
use erp_core::ids::{PayableAccountId, PaymentAllocationId, SupplierPaymentId};
use erp_core::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;
use std::collections::HashSet;

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
    Ok(payment)
}

/// 读取分配、规划反转并回冲全部 settlement，返回原 HashSet 供流程逐账户同步任务。
///
/// 调用方必须完成任务同步后才调用返回计划的 persist，不得重排反向分配写入。
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
    Ok((
        PaymentReversalWrite {
            payment,
            reversal,
            reverse_rows,
            seqs,
        },
        affected_accounts,
    ))
}

impl PaymentReversalWrite {
    /// 付款任务同步成功后，用同一 Executor 写反向分配并将原付款迁到 Reversed。
    pub async fn persist(self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        persist_reverse_allocations(
            db,
            &self.reversal,
            &self.payment,
            &self.reverse_rows,
            &self.seqs,
            executor,
        )
        .await?;
        let mut payment = self.payment;
        payment.transition(SupplierPaymentStatus::Reversed)?;
        db.supplier_payments().update(&mut payment, executor).await?;
        Ok(())
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
    let facts = load_payable_offset_facts(
        db,
        chunks.iter().map(|chunk| chunk.increase_entry_id.clone()),
        session,
    )
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
async fn persist_reverse_allocations(
    db: &Database,
    reversal: &PaymentReversalFact,
    payment: &SupplierPayment,
    reverse_rows: &[crate::entity::payable::PaymentReversePlanRow],
    seqs: &[u32],
    session: &mut dyn Executor,
) -> Result<()> {
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
        db.payment_allocations().create(&allocation, session).await?;
    }
    Ok(())
}
