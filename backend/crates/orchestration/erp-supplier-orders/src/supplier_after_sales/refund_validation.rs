use erp_core::money::Amount;
use erp_finance::entity::cost::CostEntry;
use erp_finance::entity::payable::AllocationAction;
use erp_finance::repository::{CostExt, PayableExt};
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::after_sales::RefundExecution;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, ApiOrderSourceLine};
use erp_supply::dto::supplier_fulfillment::{RecordRefundResultRequest, RefundAllocationRequest};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

pub(super) async fn validate_finance(
    db: &Database,
    row: &RefundExecution,
    request: &RecordRefundResultRequest,
    executor: &mut dyn Executor,
) -> Result<()> {
    let execution = db
        .supplier_api_order_executions()
        .find_by_id(&row.request.order.merchant_order_no, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原采购执行不存在".into()))?;

    for allocation in &request.allocations {
        validate_line(db, &execution, allocation, executor).await?;
    }
    Ok(())
}

async fn validate_line(
    db: &Database,
    execution: &ApiOrderExecution,
    line: &RefundAllocationRequest,
    executor: &mut dyn Executor,
) -> Result<()> {
    let source = execution
        .source_lines
        .iter()
        .find(|source| source.fulfillment_item_id == line.supplier_fulfillment_item_id.as_ref())
        .ok_or_else(|| Error::ConflictError("退款分配缺少原采购来源行".into()))?;
    let cost = db
        .cost_entries()
        .find_by_id(line.original_cost_entry_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("原成本事实不存在".into()))?;
    validate_cost_line(db, source, &cost, executor).await?;

    let allocation = db
        .cost_allocations()
        .find_by_id(line.original_cost_allocation_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("原成本归属不存在".into()))?;
    let payable = db
        .payable_entries()
        .find_by_id(line.original_payable_entry_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("原应付事实不存在".into()))?;
    let account = db
        .payable_accounts()
        .find_by_id(payable.payable_account_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("原应付账户不存在".into()))?;

    if cost.supplier_id.as_ref().map(|id| id.as_ref()) != Some(execution.supplier_id.as_str())
        || ![execution.base.id.as_str(), execution.purchase_order_id.as_str()]
            .contains(&cost.source_document_id.as_str())
        || allocation.cost_entry_id != line.original_cost_entry_id
        || allocation.sales_order_line_id.as_ref().map(|id| id.as_ref())
            != source.sales_order_line_id.as_deref()
        || account.supplier_id.as_ref() != execution.supplier_id
        || account.source_document_id != execution.purchase_order_id
        || line.gross_amount > allocation.allocated_gross_amount
        || line.net_amount > allocation.allocated_net_amount
        || line.gross_amount > payable.amount
    {
        return Err(Error::ConflictError("退款财务分配不属于原采购成本、归属或应付范围".into()));
    }

    validate_payment(db, line, executor).await
}

async fn validate_payment(
    db: &Database,
    line: &RefundAllocationRequest,
    executor: &mut dyn Executor,
) -> Result<()> {
    if let Some(payment_id) = &line.original_payment_allocation_id {
        let payment = db
            .payment_allocations()
            .find_by_id(payment_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("原付款分配不存在".into()))?;
        if payment.allocation_action != AllocationAction::Apply
            || payment.source.supplier_payment_id().is_none()
        {
            return Err(Error::ConflictError(
                "现金退款必须引用原普通付款的正向分配；预存款返还须由资金调查独立核实".into(),
            ));
        }
        if payment.payable_entry_id != line.original_payable_entry_id
            || line.cash_refund_amount > payment.allocated_amount
        {
            return Err(Error::ConflictError("现金退款分配不属于原应付付款".into()));
        }
    } else if line.cash_refund_amount != Amount::zero() {
        return Err(Error::ConflictError("现金退款必须关联原付款分配".into()));
    }
    Ok(())
}

async fn validate_cost_line(
    db: &Database,
    source: &ApiOrderSourceLine,
    cost: &CostEntry,
    executor: &mut dyn Executor,
) -> Result<()> {
    if [source.fulfillment_item_id.as_str(), source.purchase_order_revision_line_id.as_str()]
        .contains(&cost.source_line_id.as_str())
    {
        return Ok(());
    }
    if cost.source_fact_type == "purchase_fulfillment" {
        let allocation = db
            .purchase_line_sales_allocations()
            .find_by_id(&cost.source_line_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原实际履约成本的采购分配不存在".into()))?;
        if allocation.purchase_order_revision_line_id.as_ref() == source.purchase_order_revision_line_id {
            return Ok(());
        }
    }
    Err(Error::ConflictError("原成本尚未证明属于退款采购明细，不按名称、行序或金额猜测归属".into()))
}
