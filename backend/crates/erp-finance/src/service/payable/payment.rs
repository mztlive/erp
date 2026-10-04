//! 应付分配额度与付款过账的财务事务内接口。
use std::collections::{HashMap, HashSet};

use erp_core::common::time::Instant;
use erp_core::ids::{PayableAccountId, PayableEntryId, PaymentAllocationId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::payable::{
    PayableAccount, PayableEntry, PaymentAllocationLedger, PendingPaymentAllocation, SupplierPayment,
    SupplierPaymentStatus,
};
use crate::repository::PayableExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
/// 已在当前 Executor 更新应付余额的核销结果。
/// 组合层必须先同步这些账户的付款任务，然后用同一 Executor 完成付款写入。
pub struct PaymentSettlement {
    ledger: PaymentAllocationLedger,
    /// 本次已认证核销/过账执行人，不能由后续完成步骤替换为创建人。
    actor_id: String,
    /// 已变更应付余额的账户；保持仓储返回顺序。
    pub applied_account_ids: Vec<PayableAccountId>,
}

/// 当前付款阶段已在调用方事务执行器读取的财务事实。
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct PaymentSettlementFacts<'a> {
    entries: &'a [PayableEntry],
    accounts: &'a [PayableAccount],
}

impl<'a> PaymentSettlementFacts<'a> {
    /// 绑定同一 Executor、余额写入前读取的分录和子账。
    ///
    /// # 参数
    /// * `entries` - 已完成付款任务完整覆盖校验的分录
    /// * `accounts` - 已完成付款任务授权的子账
    ///
    /// # 返回
    /// 返回仅借用本次执行阶段事实的核销输入。
    pub fn new(entries: &'a [PayableEntry], accounts: &'a [PayableAccount]) -> Self {
        Self { entries, accounts }
    }
}
/// 校验分配与应付余额并条件更新财务账户；禁止自行开启事务。
/// 失败立即向调用者传播，后续任务与付款事实不得执行。
///
/// # 参数
/// 付款、分配、操作人和执行器均由当前财务用例提供。
///
/// # 返回
/// 返回已条件更新余额的核销结果。
///
/// # 错误
/// 付款状态、分配、供应商或开放余额不合法时返回错误。
pub async fn settle_supplier_payment(
    db: &Database,
    payment: &SupplierPayment,
    pending: &[PendingPaymentAllocation],
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<PaymentSettlement> {
    let ledger = load_payment_ledger(db, payment, pending, session).await?;
    let (entries, accounts) = load_settlement_facts(db, pending, session).await?;
    let facts = PaymentSettlementFacts::new(&entries, &accounts);
    apply_payment_settlement(db, payment, pending, ledger, facts, actor_id, session).await
}

/// 复用本次付款任务已经读取的应付事实并条件更新余额。
///
/// 该组合层入口只允许复用同一事务、同一 Executor、余额写入前读取的子账与
/// 分录。调用方必须先完成任务授权和完整分录覆盖校验；不得跨请求缓存事实。
/// 既有付款分配仍在当前 Executor 重读，余额仍由仓储条件更新仲裁。
///
/// # 参数
/// * `facts` - 当前任务授权与完整覆盖阶段读取的财务事实
/// * `session` - 读取以上事实时使用的调用方事务执行器
///
/// # 返回
/// 返回已条件更新余额的核销结果。
///
/// # 错误
/// 与 [`settle_supplier_payment`] 使用相同校验及错误顺序。
#[doc(hidden)]
pub async fn settle_supplier_payment_with_facts(
    db: &Database,
    payment: &SupplierPayment,
    pending: &[PendingPaymentAllocation],
    facts: PaymentSettlementFacts<'_>,
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<PaymentSettlement> {
    let ledger = load_payment_ledger(db, payment, pending, session).await?;
    apply_payment_settlement(db, payment, pending, ledger, facts, actor_id, session).await
}

/// 按原付款状态与既有分配顺序建立本次核销账本。
async fn load_payment_ledger(
    db: &Database,
    payment: &SupplierPayment,
    pending: &[PendingPaymentAllocation],
    session: &mut dyn Executor,
) -> Result<PaymentAllocationLedger> {
    if payment.status == SupplierPaymentStatus::Reversed {
        return Err(Error::BusinessLogicError("已冲正付款不能再核销".to_string()));
    }
    let existing = db
        .payment_allocations()
        .find_allocations_by_payments(&[payment.base.id.clone().into()], session)
        .await?;
    PaymentAllocationLedger::new(payment.base.id.clone().into(), payment.amount, &existing, pending)
        .map_err(Into::into)
}

/// 为没有上游任务事实的既有入口读取去重分录和关联子账。
async fn load_settlement_facts(
    db: &Database,
    pending: &[PendingPaymentAllocation],
    session: &mut dyn Executor,
) -> Result<(Vec<PayableEntry>, Vec<PayableAccount>)> {
    let mut entry_ids: Vec<PayableEntryId> =
        pending.iter().map(|line| line.payable_entry_id.clone()).collect();
    entry_ids.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    entry_ids.dedup();
    let entries = db.payable_entries().find_entries_by_ids(&entry_ids, session).await?;
    let mut account_ids: Vec<PayableAccountId> =
        entries.iter().map(|entry| entry.payable_account_id.clone()).collect();
    account_ids.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    account_ids.dedup();
    let accounts = db.payable_accounts().find_accounts_by_ids(&account_ids, session).await?;
    Ok((entries, accounts))
}

/// 对已读取事实执行原核销规则，然后按仓储条件更新余额。
async fn apply_payment_settlement(
    db: &Database,
    payment: &SupplierPayment,
    pending: &[PendingPaymentAllocation],
    mut ledger: PaymentAllocationLedger,
    facts: PaymentSettlementFacts<'_>,
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<PaymentSettlement> {
    apply_payment_ledger(payment, pending, &mut ledger, facts.entries, facts.accounts)?;
    let settlement = db
        .payable_accounts()
        .apply_settlements_many(ledger.account_settlement_deltas(), actor_id, session)
        .await?;
    if !settlement.rejected.is_empty() {
        return Err(Error::BusinessLogicError("子账剩余开放余额不足，核销被拒绝".to_string()));
    }
    Ok(PaymentSettlement { ledger, actor_id: actor_id.to_string(), applied_account_ids: settlement.applied })
}

/// 使用财务领域账本按请求顺序校验分录、子账与供应商并构造分配。
fn apply_payment_ledger(
    payment: &SupplierPayment,
    pending: &[PendingPaymentAllocation],
    ledger: &mut PaymentAllocationLedger,
    entries: &[PayableEntry],
    accounts: &[PayableAccount],
) -> Result<()> {
    let entries_by_id: HashMap<&str, &PayableEntry> =
        entries.iter().map(|entry| (entry.base.id.as_str(), entry)).collect();
    let accounts_by_id: HashMap<&str, &PayableAccount> =
        accounts.iter().map(|account| (account.base.id.as_str(), account)).collect();
    let mut checked_accounts = HashSet::new();
    let allocation_ids: Vec<PaymentAllocationId> =
        (0..pending.len()).map(|_| PaymentAllocationId::new(next_id())).collect();
    for (line, allocation_id) in pending.iter().zip(allocation_ids) {
        let entry = entries_by_id
            .get(line.payable_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
        if checked_accounts.insert(&entry.payable_account_id) {
            let account = accounts_by_id
                .get(entry.payable_account_id.as_ref())
                .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
            if account.supplier_id != payment.supplier_id {
                return Err(Error::BusinessLogicError("禁止跨供应商核销".to_string()));
            }
        }
        ledger.apply(line, entry, allocation_id, Instant::now())?;
    }

    Ok(())
}
/// 在任务同步成功后落付款状态、不可变执行身份与分配事实。
///
/// # 参数
/// * `db` - 财务领域数据库
/// * `payment` - 同一次执行的付款单
/// * `pending` - 本次核销分配
/// * `settlement` - 已写余额并冻结本次执行人的核销结果
/// * `session` - 余额更新时使用的调用方执行器
///
/// # 返回
/// 返回已经持久化过账身份、时间与核销分配的付款单。
///
/// # 错误
/// 付款状态、分配、执行身份或持久化失败时返回错误。
pub async fn finish_supplier_payment(
    db: &Database,
    payment: &mut SupplierPayment,
    pending: &[PendingPaymentAllocation],
    settlement: &PaymentSettlement,
    session: &mut dyn Executor,
) -> Result<()> {
    payment.post_from_execution(pending, &settlement.actor_id, Instant::now())?;
    db.supplier_payments().update(payment, session).await?;
    db.payable().create_payment_allocations_many(settlement.ledger.new_allocations(), session).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{FileAssetId, PartyBankAccountId, SupplierAccountId, SupplierPaymentId};
    use erp_core::money::Amount;

    use super::*;
    use crate::entity::payable::{
        EntryDirection, PayableAccountData, PayableEntryData, PayableEntryType, PayableSourceType,
        SupplierPaymentData,
    };

    /// 复用完整分录集合时，核销仍按请求顺序生成序号并聚合同一子账。
    #[test]
    fn reused_payment_facts_keep_allocation_order_and_aggregate_amounts() {
        let payment = payment();
        let entries = [entry("entry-1", "account-1"), entry("entry-2", "account-1")];
        let accounts = [account("account-1", "supplier-1")];
        let pending =
            [allocation("entry-2", "20.00"), allocation("entry-1", "10.00"), allocation("entry-2", "5.00")];
        let mut ledger = ledger(&payment, &pending);

        apply_payment_ledger(&payment, &pending, &mut ledger, &entries, &accounts).unwrap();

        let order = ledger
            .new_allocations()
            .iter()
            .map(|line| (line.payable_entry_id.as_ref(), line.allocation_seq))
            .collect::<Vec<_>>();
        assert_eq!(order, [("entry-2", 1), ("entry-1", 2), ("entry-2", 3)]);
        assert_eq!(
            ledger.account_settlement_deltas(),
            [(PayableAccountId::new("account-1"), amount("35.00"))]
        );
    }

    /// 缺失分录与跨供应商并存时，保持逐请求行原首错顺序。
    #[test]
    fn reused_payment_facts_keep_missing_entry_and_supplier_error_order() {
        let payment = payment();
        let entries = [entry("entry-1", "account-1")];
        let accounts = [account("account-1", "other-supplier")];
        let pending = [allocation("missing-entry", "10.00"), allocation("entry-1", "10.00")];
        let error =
            apply_payment_ledger(&payment, &pending, &mut ledger(&payment, &pending), &entries, &accounts)
                .unwrap_err();
        assert!(matches!(error, Error::NotFound(message) if message == "应付分录不存在"));

        let pending = [allocation("entry-1", "10.00"), allocation("missing-entry", "10.00")];
        let error =
            apply_payment_ledger(&payment, &pending, &mut ledger(&payment, &pending), &entries, &accounts)
                .unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(message) if message == "禁止跨供应商核销"));
    }

    /// 复用事实也必须检查子账存在及重复分录累计开放余额。
    #[test]
    fn reused_payment_facts_reject_missing_account_and_entry_over_allocation() {
        let payment = payment();
        let entries = [entry("entry-1", "account-1")];
        let pending = [allocation("entry-1", "10.00")];
        let error = apply_payment_ledger(&payment, &pending, &mut ledger(&payment, &pending), &entries, &[])
            .unwrap_err();
        assert!(matches!(error, Error::NotFound(message) if message == "应付往来子账不存在"));

        let pending = [allocation("entry-1", "60.00"), allocation("entry-1", "50.00")];
        let mut ledger = ledger(&payment, &pending);
        let error = apply_payment_ledger(
            &payment,
            &pending,
            &mut ledger,
            &entries,
            &[account("account-1", "supplier-1")],
        )
        .unwrap_err();
        assert!(error.to_string().contains("核销金额超过应付分录开放余额"));
        assert_eq!(ledger.new_allocations().len(), 1);
    }

    /// 构造测试核销账本，执行与复用入口相同的财务领域构造规则。
    fn ledger(payment: &SupplierPayment, pending: &[PendingPaymentAllocation]) -> PaymentAllocationLedger {
        PaymentAllocationLedger::new(payment.base.id.clone().into(), payment.amount, &[], pending).unwrap()
    }

    /// 构造一条请求核销行。
    fn allocation(entry_id: &str, value: &str) -> PendingPaymentAllocation {
        PendingPaymentAllocation::new(PayableEntryId::new(entry_id), amount(value)).unwrap()
    }

    /// 构造固定测试付款，金额足以覆盖分录超额边界。
    fn payment() -> SupplierPayment {
        SupplierPayment::new(
            SupplierPaymentId::new("payment-1"),
            SupplierPaymentData {
                payment_no: "FK-1".to_string(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                payee_bank_account_id: PartyBankAccountId::new("bank-1"),
                paid_at: Instant::from_unix_secs(1),
                amount: amount("200.00"),
                bank_reference: None,
                bank_receipt_asset_id: FileAssetId::new("asset-1"),
            },
            "creator",
        )
        .unwrap()
    }

    /// 构造供应商身份可调整的应付子账。
    fn account(id: &str, supplier_id: &str) -> PayableAccount {
        PayableAccount::new(
            PayableAccountId::new(id),
            PayableAccountData {
                source_document_id: "purchase-1".to_string(),
                supplier_id: SupplierAccountId::new(supplier_id),
                source_type: PayableSourceType::PurchaseOrder,
                gross_total: amount("200.00"),
                settled_total: amount("0.00"),
                invoiceable_total: amount("200.00"),
                invoiced_total: amount("0.00"),
            },
            "cashier",
        )
        .unwrap()
    }

    /// 构造金额为一百元的正式应付分录。
    fn entry(id: &str, account_id: &str) -> PayableEntry {
        PayableEntry::new(
            PayableEntryId::new(id),
            PayableEntryData {
                payable_account_id: PayableAccountId::new(account_id),
                entry_type: PayableEntryType::Original,
                direction: EntryDirection::Increase,
                amount: amount("100.00"),
                due_date: BusinessDate::from_ymd(2026, 10, 2).unwrap(),
                source_fact_type: "PURCHASE_ORDER".to_string(),
                source_document_id: "purchase-1".to_string(),
                source_revision_id: "revision-1".to_string(),
                source_sequence: 1,
                posted_at: Instant::from_unix_secs(1),
            },
        )
        .unwrap()
    }

    /// 解析测试定点金额。
    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }
}
