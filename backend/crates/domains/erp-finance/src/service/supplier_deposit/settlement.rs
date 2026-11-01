//! 已核实扣款转正式核销，以及到账返款和原核销冲回的共同事务。

use std::slice::from_ref;

use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_core::ids::{PayableEntryId, PaymentAllocationId};
use erp_core::money::Amount;
use id_generator::next_id;
use persistence_core::Executor;

use super::{SupplierDepositService, fingerprint, identity, replay, require_transaction};
use crate::entity::payable::{
    AllocationAction, PayableAccount, PayableEntry, PayableEntryOffset, PaymentAllocation,
    PaymentAllocationData, PaymentAllocationLedger, PaymentRequestAllocation, PendingPaymentAllocation,
    PurchasePaymentRequest, PurchasePaymentRequestStatus,
};
use crate::entity::supplier_deposit::{
    DepositBatchAllocation, DepositLedgerKind, DepositSettlementScope, DepositTransactionEvidence,
    SupplierDepositAccount, SupplierDepositLedgerEntry, SupplierDepositReservation,
    SupplierDepositSettlement, SupplierDepositUsage, ensure_deposit_entry_balances, validate_deposit_actor,
};
use crate::repository::prelude::*;
use crate::repository::{PayableExt, PurchasePaymentRequestExt, SupplierDepositExt};
use crate::{Error, Result};

struct SettlementContext {
    reservation: SupplierDepositReservation,
    account: SupplierDepositAccount,
    request: PurchasePaymentRequest,
    payable: PayableAccount,
    entries: Vec<PayableEntry>,
    allocations: Vec<PaymentAllocation>,
    entry_allocations: Vec<PaymentAllocation>,
    offsets: Vec<PayableEntryOffset>,
}

impl SettlementContext {
    fn scope<'a>(&'a self, usage: &'a SupplierDepositUsage) -> DepositSettlementScope<'a> {
        DepositSettlementScope {
            usage,
            reservation: &self.reservation,
            account: &self.account,
            request: &self.request,
            payable: &self.payable,
            entries: &self.entries,
        }
    }
}

struct ReturnReversal {
    request: PurchasePaymentRequest,
    payable_account_id: String,
    allocations: Vec<PaymentAllocation>,
    amount: Amount,
    restore_request: bool,
}

impl SupplierDepositService {
    /// 将原真实扣款在逐采购批准范围内核销，重放不重复形成付款或核销。
    /// # 参数
    /// * `usage_id` / `operation_key` - 原使用及本次独立核销身份。
    /// * `pending` - 当前已批准且可归属原采购的应付分配。
    /// * `actor` / `executor` - 核验人及账户、申请和核销共同事务。
    /// # 返回
    /// 返回保存本次正式核销后的使用；充值不再次计作采购付款。
    /// # 错误
    /// 来源、批准版本、归属、余额或幂等载荷不合法及仓储失败时拒绝。
    pub async fn settle_usage(
        &self,
        usage_id: &str,
        operation_key: &str,
        pending: &[PendingPaymentAllocation],
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositUsage> {
        require_transaction(executor)?;
        validate_deposit_actor(actor)?;
        let mut usage = self.usage(usage_id, executor).await?;
        let id = identity(&usage.account_id, operation_key)?;
        let payload_fingerprint = fingerprint(&(usage_id, pending))?;
        if let Some(receipt) = self.db.supplier_deposit_settlements().find_by_id(&id, executor).await? {
            replay(&payload_fingerprint, &receipt.payload_fingerprint)?;
            return Ok(usage);
        }
        let entry_ids = pending.iter().map(|line| line.payable_entry_id.clone()).collect::<Vec<_>>();
        let mut context = self.settlement_context(&usage, &entry_ids, executor).await?;
        usage.settlement_total(&context.allocations)?;
        let mut ledger = context.scope(&usage).ledger(&context.allocations, pending)?;
        ensure_deposit_entry_balances(
            &context.entries,
            pending,
            &context.entry_allocations,
            &context.offsets,
        )?;
        self.fill_usage_ledger(&mut ledger, pending, &context.entries)?;
        let amount = SupplierDepositSettlement::allocation_total(ledger.new_allocations())?;
        context.request.settle_deposit(&usage.reservation_id, amount)?;
        let allocation_ids =
            ledger.new_allocations().iter().map(|row| row.base.id.clone()).collect::<Vec<_>>();
        usage.record_settlement(amount, allocation_ids.clone())?;
        let receipt = SupplierDepositSettlement {
            base: BaseModel::new(id),
            account_id: usage.account_id.clone(),
            operation_key: operation_key.into(),
            payload_fingerprint,
            usage_id: usage_id.into(),
            amount,
            allocation_ids,
            actor_id: actor.into(),
            occurred_at: Instant::now(),
        };
        self.persist_usage_settlement(&mut usage, &mut context, &ledger, &receipt, executor).await?;
        Ok(usage)
    }

    /// 保存原扣款的真实到账返款，并只冲回可证明的原核销。
    /// # 参数
    /// * `usage_id` / `operation_key` - 原使用及独立返款操作身份。
    /// * `amount` / `evidence` - 已核实到账金额和稳定外部证据。
    /// * `actor` / `executor` - 核验人和当前共同事务。
    /// # 返回
    /// 返回真实返款及正式冲回分别保存的使用；未核销部分保留批准额度锁。
    /// # 错误
    /// 交易无原单归属、超返、异载荷、资金不守恒或数据库失败时拒绝。
    pub async fn return_usage(
        &self,
        usage_id: &str,
        operation_key: &str,
        amount: Amount,
        evidence: DepositTransactionEvidence,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositUsage> {
        require_transaction(executor)?;
        validate_deposit_actor(actor)?;
        evidence.validate()?;
        let mut usage = self.usage(usage_id, executor).await?;
        let id = identity(&usage.account_id, operation_key)?;
        let payload_fingerprint = fingerprint(&(usage_id, amount, &evidence))?;
        if let Some(entry) = self.db.supplier_deposit_ledger().find_by_id(&id, executor).await? {
            replay(&payload_fingerprint, &entry.payload_fingerprint)?;
            return Ok(usage);
        }
        let debit = self.usage_debit(&usage, executor).await?;
        if debit.evidence.external_order_id != evidence.external_order_id {
            return Err(Error::BusinessLogicError("返款未关联原扣款供应商订单".into()));
        }
        let batches = usage.return_batches(amount)?;
        let reversal = self.return_reversal(&usage, amount, actor, executor).await?;
        let reversed = reversal.as_ref().map_or(Amount::zero(), |plan| plan.amount);
        usage.record_return(amount, reversed)?;
        let mut account = self.account(&usage.account_id, executor).await?;
        account.credit(amount)?;
        if reversed < amount || SupplierDepositSettlement::batch_total(&batches)? < amount {
            account.flag_reconciliation_risk(Instant::now())?;
        }
        if let Some(plan) = reversal {
            self.persist_return_reversal(plan, actor, executor).await?;
        }
        self.persist_return_batches(&usage.account_id, &batches, executor).await?;
        let entry = SupplierDepositLedgerEntry {
            base: BaseModel::new(id),
            account_id: usage.account_id.clone(),
            operation_key: operation_key.into(),
            payload_fingerprint,
            kind: DepositLedgerKind::Return,
            amount,
            reservation_id: Some(usage.reservation_id.clone()),
            original_usage_id: Some(usage_id.into()),
            batch_allocations: batches,
            evidence,
            actor_id: actor.into(),
        };
        self.db.supplier_deposit_accounts().update(&mut account, executor).await?;
        self.db.supplier_deposit_usages().update(&mut usage, executor).await?;
        self.db.supplier_deposit_ledger().create(&entry, executor).await?;
        Ok(usage)
    }

    async fn usage(&self, id: &str, executor: &mut dyn Executor) -> Result<SupplierDepositUsage> {
        self.db
            .supplier_deposit_usages()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原预存款使用不存在".into()))
    }

    async fn usage_debit(
        &self,
        usage: &SupplierDepositUsage,
        executor: &mut dyn Executor,
    ) -> Result<SupplierDepositLedgerEntry> {
        let debit = self
            .db
            .supplier_deposit_ledger()
            .find_by_id(&usage.ledger_entry_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原预存款扣款流水不存在".into()))?;
        if debit.kind != DepositLedgerKind::Debit
            || debit.account_id != usage.account_id
            || debit.amount != usage.amount
            || debit.reservation_id.as_deref() != Some(usage.reservation_id.as_str())
        {
            return Err(Error::ConflictError("原预存款扣款、账户和使用不一致".into()));
        }
        Ok(debit)
    }

    async fn settlement_context(
        &self,
        usage: &SupplierDepositUsage,
        entry_ids: &[PayableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<SettlementContext> {
        self.usage_debit(usage, executor).await?;
        let reservation = self.reservation(&usage.reservation_id, executor).await?;
        let account = self.account(&usage.account_id, executor).await?;
        let request = self
            .request_for_reservation(&account.base.id, &account.identity, &reservation.basis, executor)
            .await?;
        let payable = self
            .db
            .payable_accounts()
            .find_by_id(&reservation.basis.payable_account_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原采购应付账户不存在".into()))?;
        let entries = self.db.payable_entries().find_entries_by_ids(entry_ids, executor).await?;
        let allocations = self
            .db
            .payment_allocations()
            .find_allocations_by_deposit_usages(from_ref(&usage.base.id), executor)
            .await?;
        let entry_allocations =
            self.db.payment_allocations().find_allocations_by_entries(entry_ids, executor).await?;
        let offsets = self.db.payable_entry_offsets().find_offsets_by_increases(entry_ids, executor).await?;
        Ok(SettlementContext {
            reservation,
            account,
            request,
            payable,
            entries,
            allocations,
            entry_allocations,
            offsets,
        })
    }

    fn fill_usage_ledger(
        &self,
        ledger: &mut PaymentAllocationLedger,
        pending: &[PendingPaymentAllocation],
        entries: &[PayableEntry],
    ) -> Result<()> {
        for line in pending {
            let entry = entries
                .iter()
                .find(|entry| entry.base.id == line.payable_entry_id.as_ref())
                .ok_or_else(|| Error::NotFound("原核销应付分录不存在".into()))?;
            ledger.apply(line, entry, PaymentAllocationId::new(next_id()), Instant::now())?;
        }
        Ok(())
    }

    async fn persist_usage_settlement(
        &self,
        usage: &mut SupplierDepositUsage,
        context: &mut SettlementContext,
        ledger: &PaymentAllocationLedger,
        receipt: &SupplierDepositSettlement,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let accounts = self.db.payable_accounts();
        if !accounts
            .consume_payment_request(&context.payable.base.id, &receipt.amount, &receipt.actor_id, executor)
            .await?
            || !accounts
                .apply_settlement(&context.payable.base.id, &receipt.amount, &receipt.actor_id, executor)
                .await?
        {
            return Err(Error::ConflictError("原付款申请占用或采购应付余额不足".into()));
        }
        self.db.purchase_payment_requests().update(&mut context.request, executor).await?;
        self.db.payable().create_payment_allocations_many(ledger.new_allocations(), executor).await?;
        self.db.supplier_deposit_usages().update(usage, executor).await?;
        self.db.supplier_deposit_settlements().create(receipt, executor).await?;
        Ok(())
    }

    async fn return_reversal(
        &self,
        usage: &SupplierDepositUsage,
        amount: Amount,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReturnReversal>> {
        let reversed = usage.return_settlement_amount(amount)?;
        if reversed == Amount::zero() {
            return Ok(None);
        }
        let existing = self
            .db
            .payment_allocations()
            .find_allocations_by_deposit_usages(from_ref(&usage.base.id), executor)
            .await?;
        usage.settlement_total(&existing)?;
        let Some(mut context) = self.return_context(usage, &existing, executor).await? else {
            return Ok(None);
        };
        if context.scope(usage).validate().is_err() {
            return Ok(None);
        }
        if existing.iter().any(|allocation| {
            !context.entries.iter().any(|entry| entry.base.id == allocation.payable_entry_id.as_ref())
        }) {
            return Ok(None);
        }
        let allocations = build_return_allocations(&existing, reversed)?;
        let restore_request = context.request.status != PurchasePaymentRequestStatus::Closed;
        if context
            .request
            .reverse_payment(&[PaymentRequestAllocation {
                payable_account_id: context.payable.base.id.clone(),
                amount: reversed,
            }])
            .is_err()
        {
            return Ok(None);
        }
        validate_deposit_actor(actor)?;
        Ok(Some(ReturnReversal {
            request: context.request,
            payable_account_id: context.payable.base.id,
            allocations,
            amount: reversed,
            restore_request,
        }))
    }

    async fn return_context(
        &self,
        usage: &SupplierDepositUsage,
        existing: &[PaymentAllocation],
        executor: &mut dyn Executor,
    ) -> Result<Option<SettlementContext>> {
        let reservation = self.reservation(&usage.reservation_id, executor).await?;
        let account = self.account(&usage.account_id, executor).await?;
        let Some(request) = self
            .db
            .purchase_payment_requests()
            .find_by_id(&reservation.basis.payment_request_id, executor)
            .await?
        else {
            return Ok(None);
        };
        let Some(payable) =
            self.db.payable_accounts().find_by_id(&reservation.basis.payable_account_id, executor).await?
        else {
            return Ok(None);
        };
        let entry_ids = existing.iter().map(|row| row.payable_entry_id.clone()).collect::<Vec<_>>();
        let entries = self.db.payable_entries().find_entries_by_ids(&entry_ids, executor).await?;
        Ok(Some(SettlementContext {
            reservation,
            account,
            request,
            payable,
            entries,
            allocations: existing.to_vec(),
            entry_allocations: vec![],
            offsets: vec![],
        }))
    }

    async fn persist_return_reversal(
        &self,
        mut plan: ReturnReversal,
        actor: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let accounts = self.db.payable_accounts();
        if !accounts.revert_settlement(&plan.payable_account_id, &plan.amount, actor, executor).await? {
            return Err(Error::ConflictError("返款冲回超过原采购净已付".into()));
        }
        if plan.restore_request
            && !accounts
                .restore_payment_request(&plan.payable_account_id, &plan.amount, actor, executor)
                .await?
        {
            return Err(Error::ConflictError("无法恢复原付款申请批准额度".into()));
        }
        self.db.purchase_payment_requests().update(&mut plan.request, executor).await?;
        self.db.payable().create_payment_allocations_many(&plan.allocations, executor).await?;
        Ok(())
    }

    async fn persist_return_batches(
        &self,
        account_id: &str,
        allocations: &[DepositBatchAllocation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for allocation in allocations {
            let mut batch = self
                .db
                .supplier_deposit_batches()
                .find_by_id(&allocation.batch_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("原扣款充值批次不存在".into()))?;
            if batch.account_id != account_id {
                return Err(Error::ConflictError("返款批次与原资金账户不一致".into()));
            }
            batch.record_return(allocation.amount)?;
            self.db.supplier_deposit_batches().update(&mut batch, executor).await?;
        }
        Ok(())
    }
}

fn build_return_allocations(
    existing: &[PaymentAllocation],
    amount: Amount,
) -> Result<Vec<PaymentAllocation>> {
    let (rows, _) = PaymentAllocation::plan_reverse(existing, amount)?;
    let seqs = PaymentAllocation::next_allocation_seq_range(existing, rows.len())?;
    rows.into_iter()
        .zip(seqs)
        .map(|(row, allocation_seq)| {
            PaymentAllocation::new(
                PaymentAllocationId::new(next_id()),
                PaymentAllocationData {
                    source: row.source,
                    payable_entry_id: row.entry_id,
                    allocation_seq,
                    allocation_action: AllocationAction::Reverse,
                    allocated_amount: row.amount,
                    allocated_at: Instant::now(),
                    reverses_allocation_id: Some(row.original_id),
                },
            )
            .map_err(Into::into)
        })
        .collect()
}
