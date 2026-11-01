//! 在调用方事务中写入红字发票及应收或应付冲减。

use erp_core::ids::{
    InvoiceId, PayableAccountId, PurchaseInvoiceAllocationId, ReceivableAccountId, SalesInvoiceAllocationId,
};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::red_invoice_plan::aggregate_reversal_deltas;
use crate::entity::payable::{PurchaseInvoiceAllocation, PurchaseInvoiceAllocationData};
use crate::entity::receivable::{
    AllocationAction, Invoice, InvoiceDirection, RedInvoiceAllocationPlan, SalesInvoiceAllocation,
    SalesInvoiceAllocationData,
};
use crate::repository::prelude::*;
use crate::repository::{PayableExt, ReceivableExt};
use crate::{Error, Result};

/// 写入已登记的红字发票，并按已校验的冲减计划更新进度与分配。
///
/// 保持发票创建、必要时更新原票状态、账户差额与分配的原顺序。
/// 返回受影响的应收账户身份，供流程与销售后续步骤使用。
/// 全部操作使用传入的执行器，本函数不开启事务。
///
/// # 参数
/// * `db` - 财务领域数据库。
/// * `red_invoice` - 待创建的红字发票。
/// * `original_invoice` - 原票；全额冲减时就地标为已红冲。
/// * `allocation_plan` - 已校验的红冲分配计划。
/// * `actor_id` - 执行人。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 销项时返回受影响的应收账户 ID；进项时返回空列表。
///
/// # 错误
/// 红冲金额超过已开票或已收票进度时返回 `BusinessLogicError`；
/// 分配实体构造失败或仓储写入失败时返回对应错误。
pub async fn persist_red_invoice(
    db: &Database,
    red_invoice: &Invoice,
    original_invoice: &mut Invoice,
    allocation_plan: &RedInvoiceAllocationPlan,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    db.invoices().create(red_invoice, executor).await?;
    if allocation_plan.is_full_reversal() {
        original_invoice.mark_red_invoiced(actor_id)?;
        db.invoices().update(original_invoice, executor).await?;
    }

    let mut sales_order_account_ids = Vec::new();
    // FIN-R11：按 direction 与 account 聚合 reversal delta 后批量
    // 条件更新与批量插入；同一 account 多行只更新一次。
    let reversal_deltas = aggregate_reversal_deltas(allocation_plan.lines());
    match original_invoice.invoice_direction {
        InvoiceDirection::Sales => {
            let deltas = reversal_deltas
                .iter()
                .map(|(account_id, gross)| (ReceivableAccountId::new(account_id.clone()), *gross))
                .collect::<Vec<_>>();
            let reverted =
                db.receivable_accounts().revert_invoicings_many(&deltas, actor_id, executor).await?;
            if !reverted.rejected.is_empty() {
                return Err(Error::BusinessLogicError("红冲金额超过已开票进度".to_string()));
            }
            let mut new_allocations = Vec::with_capacity(allocation_plan.lines().len());
            for (index, line) in allocation_plan.lines().iter().enumerate() {
                new_allocations.push(SalesInvoiceAllocation::new(
                    SalesInvoiceAllocationId::new(next_id()),
                    SalesInvoiceAllocationData {
                        invoice_id: InvoiceId::new(red_invoice.base.id.clone()),
                        receivable_account_id: ReceivableAccountId::new(line.account_id.clone()),
                        allocation_seq: (index as u32) + 1,
                        allocation_action: AllocationAction::Reverse,
                        allocated_gross_amount: line.gross,
                        allocated_net_amount: line.net,
                        allocated_tax_amount: line.tax,
                        reverses_allocation_id: Some(SalesInvoiceAllocationId::new(
                            line.original_allocation_id.clone(),
                        )),
                    },
                )?);
            }
            db.receivable().create_sales_invoice_allocations_many(&new_allocations, executor).await?;
            sales_order_account_ids.extend(reversal_deltas.iter().map(|(account_id, _)| account_id.clone()));
        },
        InvoiceDirection::Purchase => {
            let deltas = reversal_deltas
                .iter()
                .map(|(account_id, gross)| (PayableAccountId::new(account_id.clone()), *gross))
                .collect::<Vec<_>>();
            let reverted = db.payable_accounts().revert_invoicings_many(&deltas, actor_id, executor).await?;
            if !reverted.rejected.is_empty() {
                return Err(Error::BusinessLogicError("红冲金额超过已收票进度".to_string()));
            }
            let mut new_allocations = Vec::with_capacity(allocation_plan.lines().len());
            for (index, line) in allocation_plan.lines().iter().enumerate() {
                new_allocations.push(PurchaseInvoiceAllocation::new(
                    PurchaseInvoiceAllocationId::new(next_id()),
                    PurchaseInvoiceAllocationData {
                        invoice_id: InvoiceId::new(red_invoice.base.id.clone()),
                        payable_account_id: PayableAccountId::new(line.account_id.clone()),
                        allocation_seq: (index as u32) + 1,
                        allocation_action: crate::entity::payable::AllocationAction::Reverse,
                        allocated_gross_amount: line.gross,
                        allocated_net_amount: line.net,
                        allocated_tax_amount: line.tax,
                        reverses_allocation_id: Some(PurchaseInvoiceAllocationId::new(
                            line.original_allocation_id.clone(),
                        )),
                    },
                )?);
            }
            db.payable().create_purchase_invoice_allocations_many(&new_allocations, executor).await?;
        },
    }
    Ok(sales_order_account_ids)
}
