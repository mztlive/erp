//! 在已授权命令的原执行器中装配发票结果。

use erp_core::money::Amount;
use persistence_core::Executor;

use super::ReceivableService;
use crate::dto::receivable::InvoiceView;
use crate::entity::payable::PurchaseInvoiceAllocation;
use crate::entity::receivable::{AllocationAction, InvoiceDirection, InvoiceKind, SalesInvoiceAllocation};
use crate::repository::prelude::*;
use crate::repository::{PayableExt, ReceivableExt};
use crate::service::receivable::mapping::zero_amount;
use crate::{Error, Result};

impl ReceivableService {
    /// 在调用方执行器装配财务事实；访问资格由调用方先行统一授权。
    ///
    /// # 参数
    /// * `id` - 发票 ID
    /// * `executor` - 授权与当前财务事实读取使用的原执行器。
    ///
    /// # 返回
    /// 返回发票视图（含分配行与未分配余额）。
    ///
    /// # 错误
    /// * `NotFound` - 发票不存在
    pub async fn invoice_detail_with_executor(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoiceView> {
        let invoice = self
            .db
            .invoices()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("发票不存在".to_string()))?;
        let (allocated_total, views) = match invoice.invoice_direction {
            InvoiceDirection::Purchase => {
                // 进项票分配独立存储（purchase_invoice_allocations）；视图共用销项形状，
                // 应付子账 ID 落 receivable_account_id 字段（前端以
                // payable_account_id ?? receivable_account_id 兜底读取）。
                let rows = self
                    .db
                    .purchase_invoice_allocations()
                    .find_allocations_by_invoices(&[invoice.base.id.clone().into()], executor)
                    .await?;
                purchase_allocation_view(rows)
            },
            InvoiceDirection::Sales => {
                let allocations = self
                    .db
                    .sales_invoice_allocations()
                    .find_allocations_by_invoices(&[invoice.base.id.clone().into()], executor)
                    .await?;
                sales_allocation_view(allocations)
            },
        };
        Ok(InvoiceView {
            sales_invoice_request_id: invoice.sales_invoice_request_id,
            id: invoice.base.id,
            invoice_direction: invoice.invoice_direction,
            invoice_kind: invoice.invoice_kind,
            party_id: invoice.party_id.to_string(),
            invoice_code: invoice.invoice_code,
            invoice_no: invoice.invoice_no,
            invoice_date: invoice.invoice_date,
            gross_amount: invoice.gross_amount,
            net_amount: invoice.net_amount,
            tax_amount: invoice.tax_amount,
            rounding_adjustment_amount: invoice.rounding_adjustment_amount,
            rounding_reason: invoice.rounding_reason,
            original_invoice_id: invoice.original_invoice_id.map(|id| id.to_string()),
            status: invoice.stable.status(),
            version: invoice.base.version,
            created_at: invoice.base.created_at,
            unallocated_amount: unallocated_amount(
                invoice.invoice_kind,
                invoice.gross_amount,
                allocated_total,
            ),
            allocated_total,
            allocations: views,
        })
    }
}
/// 汇总销项发票分配并装配视图（`APPLY` 加、`REVERSE` 减）。
///
/// # 参数
/// * `allocations` - 销项发票分配集合
///
/// # 返回
/// 返回 `(净已分配含税合计, 分配视图列表)`。
fn sales_allocation_view(
    allocations: Vec<SalesInvoiceAllocation>,
) -> (Amount, Vec<crate::dto::receivable::SalesInvoiceAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .into_iter()
        .map(|allocation| {
            net = allocation.allocation_action.apply_to_net(net, allocation.allocated_gross_amount);
            crate::dto::receivable::SalesInvoiceAllocationView {
                id: allocation.base.id,
                allocation_seq: allocation.allocation_seq,
                allocation_action: allocation.allocation_action,
                receivable_account_id: allocation.receivable_account_id.to_string(),
                allocated_gross_amount: allocation.allocated_gross_amount,
                allocated_net_amount: allocation.allocated_net_amount,
                allocated_tax_amount: allocation.allocated_tax_amount,
                reverses_allocation_id: allocation.reverses_allocation_id.as_ref().map(|id| id.to_string()),
            }
        })
        .collect();
    (net, views)
}
/// 汇总进项发票分配并转换为跨方向复用的发票分配视图。
///
/// # 参数
/// * `allocations` - 进项发票分配集合
///
/// # 返回
/// 返回 `(净已分配含税合计, 分配视图列表)`。
fn purchase_allocation_view(
    allocations: Vec<PurchaseInvoiceAllocation>,
) -> (Amount, Vec<crate::dto::receivable::SalesInvoiceAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .into_iter()
        .map(|allocation| {
            let action = AllocationAction::from(allocation.allocation_action);
            net = action.apply_to_net(net, allocation.allocated_gross_amount);
            crate::dto::receivable::SalesInvoiceAllocationView {
                id: allocation.base.id,
                allocation_seq: allocation.allocation_seq,
                allocation_action: action,
                receivable_account_id: allocation.payable_account_id.to_string(),
                allocated_gross_amount: allocation.allocated_gross_amount,
                allocated_net_amount: Some(allocation.allocated_net_amount),
                allocated_tax_amount: Some(allocation.allocated_tax_amount),
                reverses_allocation_id: allocation.reverses_allocation_id.as_ref().map(|id| id.to_string()),
            }
        })
        .collect();
    (net, views)
}

/// 按票据方向计算剩余分配额；红票分配冲减以负数记账，不能再次从正票面扣减。
fn unallocated_amount(kind: InvoiceKind, gross: Amount, allocated: Amount) -> Amount {
    match kind {
        InvoiceKind::Blue => gross.checked_sub(allocated),
        InvoiceKind::Red => gross.checked_add(allocated),
    }
}

#[cfg(test)]
mod tests {
    use erp_core::money::Amount;

    use super::{InvoiceKind, unallocated_amount};

    /// 正票面与负冲减分配必须守恒，完整红冲不得产生两倍未分配额。
    #[test]
    fn invoice_remainder_respects_red_allocation_direction() {
        let gross: Amount = "2576.00".parse().unwrap();
        let reversed: Amount = "-2576.00".parse().unwrap();
        assert_eq!(unallocated_amount(InvoiceKind::Red, gross, reversed), "0.00".parse().unwrap());
        assert_eq!(unallocated_amount(InvoiceKind::Blue, gross, gross), "0.00".parse().unwrap());
        assert_eq!(
            unallocated_amount(InvoiceKind::Red, gross, "-1000.00".parse().unwrap()),
            "1576.00".parse().unwrap()
        );
        assert_eq!(
            unallocated_amount(InvoiceKind::Blue, gross, "1000.00".parse().unwrap()),
            "1576.00".parse().unwrap()
        );
    }
}
