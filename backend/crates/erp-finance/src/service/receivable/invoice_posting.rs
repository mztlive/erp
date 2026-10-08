//! 在调用方既有事务内过账销项发票。

use std::collections::HashMap;

use erp_core::ids::SalesInvoiceAllocationId;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::receivable::sales_invoice_allocation_plan::SalesInvoiceAllocationLine;
use crate::entity::receivable::{Invoice, ReceivableAccount, SalesInvoiceAllocationPlan};
use crate::repository::ReceivableExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 按分配金额更新可开票进度，登记发票并写入分配事实。
///
/// 调用方必须传入既有事务执行器。返回受影响账户及其在计划中的原顺序，供后续销售与流程组合。
/// 账户缺失、跨主体与额度不足保持原错误语义。
///
/// # 参数
/// * `db` - 财务领域数据库。
/// * `invoice` - 待登记发票；成功时就地标为已登记。
/// * `plan_lines` - 销项分配行。
/// * `actor_id` - 登记人。
/// * `executor` - 调用方事务执行器；本函数不另开事务。
///
/// # 返回
/// 返回已读取的应收子账，以及计划中的账户 ID 原序。
///
/// # 错误
/// 子账不存在时返回 `NotFound`；跨往来主体或剩余可开票额度不足时返回 `BusinessLogicError`；
/// 分配计划、状态迁移或仓储失败时返回对应错误。
pub async fn persist_sales_invoice_allocations(
    db: &Database,
    invoice: &mut Invoice,
    plan_lines: &[SalesInvoiceAllocationLine],
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<(Vec<ReceivableAccount>, Vec<String>)> {
    let allocation_ids: Vec<SalesInvoiceAllocationId> =
        (0..plan_lines.len()).map(|_| SalesInvoiceAllocationId::new(next_id())).collect();
    let plan = SalesInvoiceAllocationPlan::new(
        invoice.base.id.clone().into(),
        invoice.gross_amount,
        invoice.net_amount,
        invoice.tax_amount,
        plan_lines,
        &allocation_ids,
    )?;
    let account_id_strs: Vec<String> =
        plan.account_invoicing_deltas().iter().map(|(id, _)| id.to_string()).collect();
    let accounts = db.receivable_accounts().find_accounts_by_ids(&account_id_strs, executor).await?;
    let accounts_by_id: HashMap<&str, &ReceivableAccount> =
        accounts.iter().map(|account| (account.base.id.as_str(), account)).collect();
    for (account_id, _) in plan.account_invoicing_deltas() {
        let account = accounts_by_id
            .get(account_id.as_ref())
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
        if account.counterparty_party_id != invoice.party_id {
            return Err(Error::BusinessLogicError("禁止跨往来主体开票".to_string()));
        }
    }
    let invoicing = db
        .receivable_accounts()
        .apply_invoicings_many(plan.account_invoicing_deltas(), actor_id, executor)
        .await?;
    if !invoicing.rejected.is_empty() {
        return Err(Error::BusinessLogicError("子账剩余可开票额度不足，开票被拒绝".to_string()));
    }
    invoice.mark_registered(actor_id)?;
    db.invoices().update(invoice, executor).await?;
    db.receivable().create_sales_invoice_allocations_many(plan.new_allocations(), executor).await?;
    Ok((accounts, account_id_strs))
}
