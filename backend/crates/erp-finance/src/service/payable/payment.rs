//! 应付分配额度与付款过账的财务事务内接口。
use crate::entity::payable::{
    PayableAccount, PayableEntry, PaymentAllocationLedger, PendingPaymentAllocation, SupplierPayment,
    SupplierPaymentStatus,
};
use crate::repository::PayableExt;
use crate::{Error, Result};
use erp_core::common::time::Instant;
use erp_core::ids::{PayableAccountId, PayableEntryId, PaymentAllocationId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;
use std::collections::{HashMap, HashSet};
/// 已在当前 Executor 更新应付余额的核销结果。
/// 组合层必须先同步这些账户的付款任务，然后用同一 Executor 完成付款写入。
pub struct PaymentSettlement {
    ledger: PaymentAllocationLedger,
    /// 已变更应付余额的账户；保持仓储返回顺序。
    pub applied_account_ids: Vec<PayableAccountId>,
}
/// 校验分配与应付余额并条件更新财务账户；禁止自行开启事务。
/// 失败立即向调用者传播，后续任务与付款事实不得执行。
pub async fn settle_supplier_payment_in_transaction(
    db: &Database,
    payment: &SupplierPayment,
    pending: &[PendingPaymentAllocation],
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<PaymentSettlement> {
    if payment.status == SupplierPaymentStatus::Reversed {
        return Err(Error::BusinessLogicError("已冲正付款不能再核销".to_string()));
    }
    let existing = db
        .payment_allocations()
        .find_allocations_by_payments(&[payment.base.id.clone().into()], session)
        .await?;
    let mut ledger =
        PaymentAllocationLedger::new(payment.base.id.clone().into(), payment.amount, &existing, pending)?;

    let mut entry_ids: Vec<PayableEntryId> =
        pending.iter().map(|line| line.payable_entry_id.clone()).collect();
    entry_ids.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    entry_ids.dedup();
    let entries = db
        .payable_entries()
        .find_entries_by_ids(&entry_ids, session)
        .await?;
    let entries_by_id: HashMap<&str, &PayableEntry> = entries
        .iter()
        .map(|entry| (entry.base.id.as_str(), entry))
        .collect();
    let mut account_ids: Vec<PayableAccountId> = entries
        .iter()
        .map(|entry| entry.payable_account_id.clone())
        .collect();
    account_ids.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    account_ids.dedup();
    let accounts = db
        .payable_accounts()
        .find_accounts_by_ids(&account_ids, session)
        .await?;
    let accounts_by_id: HashMap<&str, &PayableAccount> = accounts
        .iter()
        .map(|account| (account.base.id.as_str(), account))
        .collect();
    let mut checked_accounts: HashSet<PayableAccountId> = HashSet::new();
    let allocation_ids: Vec<PaymentAllocationId> = (0..pending.len())
        .map(|_| PaymentAllocationId::new(next_id()))
        .collect();
    for (index, line) in pending.iter().enumerate() {
        let entry = entries_by_id
            .get(line.payable_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
        if checked_accounts.insert(entry.payable_account_id.clone()) {
            let account = accounts_by_id
                .get(entry.payable_account_id.as_ref())
                .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
            if account.supplier_id != payment.supplier_id {
                return Err(Error::BusinessLogicError("禁止跨供应商核销".to_string()));
            }
        }
        ledger.apply(line, entry, allocation_ids[index].clone(), Instant::now())?;
    }

    let settlement = db
        .payable_accounts()
        .apply_settlements_many(ledger.account_settlement_deltas(), actor_id, session)
        .await?;
    if !settlement.rejected.is_empty() {
        return Err(Error::BusinessLogicError(
            "子账剩余开放余额不足，核销被拒绝".to_string(),
        ));
    }
    Ok(PaymentSettlement {
        ledger,
        applied_account_ids: settlement.applied,
    })
}
/// 在任务同步成功后落付款状态与分配事实；必须复用余额更新时的 Executor。
pub async fn finish_supplier_payment_in_transaction(
    db: &Database,
    payment: &mut SupplierPayment,
    pending: &[PendingPaymentAllocation],
    settlement: &PaymentSettlement,
    session: &mut dyn Executor,
) -> Result<()> {
    payment.post_from_execution(pending)?;
    db.supplier_payments().update(payment, session).await?;
    db.payable()
        .create_payment_allocations_many(settlement.ledger.new_allocations(), session)
        .await?;
    Ok(())
}
