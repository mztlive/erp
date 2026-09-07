//! Finance validation and ledger writes for historical card funds registration.

use crate::dto::receivable::CardFundsRegistrationAllocation;
use crate::entity::receivable::{
    CardFundsRegistrationAllocationInput, CardFundsRegistrationAllocations,
    CardFundsRegistrationAllocationsError, CustomerReceipt, ReceivableAccount, ReceivableEntry,
    ReceivableFundsLedger,
};
use crate::ports::receivable::CardFundsSnapshot;
use crate::repository::ReceivableExt;
use crate::service::receivable::mapping::map_ledger_error;
use crate::{Error, Result};
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerReceiptId, ReceiptAllocationId, ReceivableAccountId, ReceivableEntryId};
use erp_core::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use std::collections::HashMap;

/// 将 W13 服务 DTO 转换为领域输入并构造已验证分配集合。
///
/// # 参数
/// * `allocations` - HTTP 契约复用的分配 DTO 行
/// * `account_id` - 事务内重新加载的当前任务应收子账 ID
/// * `expected_total` - 本次登记的含税总额
///
/// # 返回
/// 返回保持请求顺序的领域值对象，供后续编排复用其已验证合计。
///
/// # 错误
/// 领域账户错误和合计错误映射为既有 `BusinessLogicError`，非正金额映射为
/// 既有 `ValidationError`，文案与对外错误语义保持不变。
///
/// # 约束
/// 本函数只做 DTO 到领域输入的适配，不重复实现账户、金额或守恒规则。
pub fn card_funds_registration_allocations(
    allocations: &[CardFundsRegistrationAllocation],
    account_id: &str,
    expected_total: Amount,
) -> Result<CardFundsRegistrationAllocations> {
    let lines = allocations
        .iter()
        .map(|allocation| CardFundsRegistrationAllocationInput {
            target_account_id: allocation.target_account_id.clone(),
            amount: allocation.amount,
        })
        .collect();
    CardFundsRegistrationAllocations::new(ReceivableAccountId::new(account_id), expected_total, lines)
        .map_err(|error| match error {
            CardFundsRegistrationAllocationsError::NonPositiveAmount => {
                Error::ValidationError(error.to_string())
            }
            CardFundsRegistrationAllocationsError::TargetAccountMismatch
            | CardFundsRegistrationAllocationsError::TotalMismatch => {
                Error::BusinessLogicError(error.to_string())
            }
        })
}

/// 历史回款计划持久化入参。
pub struct PersistCardFundsReceiptPlanInput<'a> {
    /// 当前任务账户。
    pub account: &'a ReceivableAccount,
    /// 事务内票款快照。
    pub snapshot: &'a CardFundsSnapshot,
    /// 已插入的历史回款。
    pub receipt: &'a CustomerReceipt,
    /// 领域核销计划。
    pub plan: &'a [(ReceivableEntryId, Amount)],
    /// 更新人。
    pub actor_id: &'a str,
    /// 子账余额不足时的既有文案。
    pub insufficient_message: &'a str,
}

/// 将历史回款计划经账本批量核销并写入分配。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 账户、快照、回款与核销计划
/// * `session` - 调用方事务
///
/// # 返回
/// 进度与分配全部写入时返回 `Ok(())`。
///
/// # 错误
/// 账本、条件核销或批量插入失败时返回错误，由事务回滚。
///
/// # 约束
/// 与审批过账共享 `ReceivableFundsLedger` 与仓储批量数据面。
pub async fn persist_card_funds_receipt_plan(
    db: &Database,
    input: PersistCardFundsReceiptPlanInput<'_>,
    session: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let mut ledger = ReceivableFundsLedger::from_historical_plan(
        CustomerReceiptId::new(input.receipt.base.id.clone()),
        input.receipt.amount,
        input.plan,
    )
    .map_err(map_ledger_error)?;
    let pending = ledger.pending().to_vec();
    let allocation_ids: Vec<ReceiptAllocationId> = (0..pending.len())
        .map(|_| ReceiptAllocationId::new(next_id()))
        .collect();
    let allocated_at = Instant::now();
    let entries_by_id: HashMap<&str, &ReceivableEntry> = input
        .snapshot
        .entries
        .iter()
        .map(|entry| (entry.base.id.as_str(), entry))
        .collect();
    for (index, line) in pending.iter().enumerate() {
        let entry = entries_by_id
            .get(line.receivable_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
        if entry.receivable_account_id.as_ref() != input.account.base.id.as_str() {
            return Err(Error::BusinessLogicError("禁止跨往来主体核销".to_string()));
        }
        ledger
            .apply(line, entry, allocation_ids[index].clone(), allocated_at)
            .map_err(map_ledger_error)?;
    }
    let settlement_deltas = ledger.account_settlement_deltas();
    let settlement = db
        .receivable_accounts()
        .apply_settlements_many(&settlement_deltas, input.actor_id, session)
        .await?;
    if !settlement.rejected.is_empty() {
        return Err(Error::BusinessLogicError(input.insufficient_message.to_string()));
    }
    db.receivable()
        .create_receipt_allocations_many(ledger.new_allocations(), session)
        .await?;
    Ok(())
}

/// 将历史分配计划错误映射为既有校验／业务错误。
///
/// # 参数
/// * `error` - 领域错误
///
/// # 返回
/// 非正金额为 ValidationError，其余为 BusinessLogicError。
///
/// # 错误
/// 本函数即错误转换。
///
/// # 约束
/// 文案与原 `plan_card_funds_receipt_allocations` 一致。
pub fn map_plan_error(error: erp_core::Error) -> Error {
    let message = error.to_string();
    if message == "回款金额必须大于零" {
        Error::ValidationError(message)
    } else {
        Error::BusinessLogicError(message)
    }
}

/// Persist historical sales invoice amounts and allocation facts in the current transaction.
///
/// Account progress is written before the invoice and allocation, matching the original
/// registration sequence. Insufficient allowance and repository errors stop the caller.
pub async fn persist_historical_invoice(
    db: &Database,
    account: &ReceivableAccount,
    invoice: &crate::entity::receivable::Invoice,
    actor_id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    use crate::entity::receivable::{AllocationAction, SalesInvoiceAllocation, SalesInvoiceAllocationData};
    use erp_core::ids::{InvoiceId, SalesInvoiceAllocationId};
    let applied = db
        .receivable_accounts()
        .apply_invoicing(
            &ReceivableAccountId::new(account.base.id.clone()),
            &invoice.gross_amount,
            actor_id,
            executor,
        )
        .await?;
    if !applied {
        return Err(Error::BusinessLogicError(
            "子账剩余可开票额度不足，历史发票登记被拒绝".to_string(),
        ));
    }
    db.invoices().create(invoice, executor).await?;
    let allocation = SalesInvoiceAllocation::new(
        SalesInvoiceAllocationId::new(next_id()),
        SalesInvoiceAllocationData {
            invoice_id: InvoiceId::new(invoice.base.id.clone()),
            receivable_account_id: ReceivableAccountId::new(account.base.id.clone()),
            allocation_seq: 1,
            allocation_action: AllocationAction::Apply,
            allocated_gross_amount: invoice.gross_amount,
            allocated_net_amount: invoice.net_amount,
            allocated_tax_amount: invoice.tax_amount,
            reverses_allocation_id: None,
        },
    )?;
    db.sales_invoice_allocations()
        .create(&allocation, executor)
        .await?;
    Ok(())
}
