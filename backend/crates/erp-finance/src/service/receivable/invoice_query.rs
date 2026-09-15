//! Invoice lists and details assembled exclusively from finance facts.

use std::collections::HashMap;

use erp_core::ids::InvoiceId;
use erp_core::money::Amount;
use persistence_core::NoTransaction;
use validator::Validate;

use super::ReceivableService;
use crate::dto::receivable::{InvoiceListParams, InvoiceView, PageView, SortDir};
use crate::entity::payable::PurchaseInvoiceAllocation;
use crate::entity::receivable::{AllocationAction, InvoiceDirection, InvoiceKind, SalesInvoiceAllocation};
use crate::repository::{PayableExt, ReceivableExt};
use crate::service::receivable::mapping::zero_amount;
use crate::{Error, Result};

impl ReceivableService {
    /// 分页查询发票列表（销项/进项共用，`invoice_direction` 筛选）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// * `keyword_ids` - 读取模型提供的完整关键词命中身份；None 只适用于无关键词
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn invoice_list(
        &self,
        params: &InvoiceListParams,
        keyword_ids: Option<Vec<String>>,
    ) -> Result<PageView<InvoiceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let scope_query = crate::repository::ScopedInvoiceQuery {
            keyword_ids,
            invoice_direction: query.invoice_direction,
            invoice_kind: query.invoice_kind,
            party_id: query.party_id,
            invoice_no: query.invoice_no,
            status: query.status,
            scope: crate::repository::ReceivableListScope {
                sales_order_id: query.sales_order_id,
                receivable_account_id: query.receivable_account_id,
            },
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page =
            self.db.receivable().search_invoices_in_account_scope(&scope_query, &mut NoTransaction).await?;
        let invoice_ids = page.items.iter().map(|row| InvoiceId::new(row.id.clone())).collect::<Vec<_>>();
        let mut sales_allocations_by_invoice = HashMap::<String, Vec<SalesInvoiceAllocation>>::new();
        for allocation in self
            .db
            .sales_invoice_allocations()
            .find_allocations_by_invoices(&invoice_ids, &mut NoTransaction)
            .await?
        {
            sales_allocations_by_invoice
                .entry(allocation.invoice_id.to_string())
                .or_default()
                .push(allocation);
        }
        let mut purchase_allocations_by_invoice = HashMap::<String, Vec<PurchaseInvoiceAllocation>>::new();
        for allocation in self
            .db
            .purchase_invoice_allocations()
            .find_allocations_by_invoices(&invoice_ids, &mut NoTransaction)
            .await?
        {
            purchase_allocations_by_invoice
                .entry(allocation.invoice_id.to_string())
                .or_default()
                .push(allocation);
        }
        for allocations in sales_allocations_by_invoice.values_mut() {
            allocations.sort_unstable_by_key(|allocation| allocation.allocation_seq);
        }
        for allocations in purchase_allocations_by_invoice.values_mut() {
            allocations.sort_unstable_by_key(|allocation| allocation.allocation_seq);
        }
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            let (allocated_total, allocations) = match row.invoice_direction {
                InvoiceDirection::Sales => {
                    sales_allocation_view(&sales_allocations_by_invoice.remove(&row.id).unwrap_or_default())
                },
                InvoiceDirection::Purchase => purchase_allocation_view(
                    &purchase_allocations_by_invoice.remove(&row.id).unwrap_or_default(),
                ),
            };
            views.push(InvoiceView {
                sales_invoice_request_id: row.sales_invoice_request_id,
                id: row.id,
                invoice_direction: row.invoice_direction,
                invoice_kind: row.invoice_kind,
                party_id: row.party_id,
                invoice_code: row.invoice_code,
                invoice_no: row.invoice_no,
                invoice_date: row.invoice_date,
                gross_amount: row.gross_amount,
                net_amount: row.net_amount,
                tax_amount: row.tax_amount,
                rounding_adjustment_amount: row.rounding_adjustment_amount,
                rounding_reason: row.rounding_reason,
                original_invoice_id: row.original_invoice_id,
                status: row.stable.status(),
                version: row.version,
                created_at: row.created_at,
                allocated_total,
                unallocated_amount: unallocated_amount(row.invoice_kind, row.gross_amount, allocated_total),
                allocations,
            });
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: scope_query.page,
            page_size: scope_query.page_size,
        })
    }
    /// 查询发票详情（含分配行）。
    ///
    /// # 参数
    /// * `id` - 发票 ID
    ///
    /// # 返回
    /// 返回发票视图。
    ///
    /// # 错误
    /// * `NotFound` - 发票不存在
    pub async fn invoice_detail(&self, id: &str) -> Result<InvoiceView> {
        self.invoice_view(id.to_string()).await
    }
    /// 装配发票视图。
    ///
    /// # 参数
    /// * `id` - 发票 ID
    ///
    /// # 返回
    /// 返回发票视图（含分配行与未分配余额）。
    ///
    /// # 错误
    /// * `NotFound` - 发票不存在
    async fn invoice_view(&self, id: String) -> Result<InvoiceView> {
        let invoice = self
            .db
            .invoices()
            .find_by_id(&id, &mut NoTransaction)
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
                    .find_allocations_by_invoices(&[invoice.base.id.clone().into()], &mut NoTransaction)
                    .await?;
                purchase_allocation_view(&rows)
            },
            InvoiceDirection::Sales => {
                let allocations = self
                    .db
                    .sales_invoice_allocations()
                    .find_allocations_by_invoices(&[invoice.base.id.clone().into()], &mut NoTransaction)
                    .await?;
                sales_allocation_view(&allocations)
            },
        };
        Ok(InvoiceView {
            sales_invoice_request_id: invoice.sales_invoice_request_id.clone(),
            id: invoice.base.id.clone(),
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
    allocations: &[SalesInvoiceAllocation],
) -> (Amount, Vec<crate::dto::receivable::SalesInvoiceAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .iter()
        .map(|allocation| {
            match allocation.allocation_action {
                AllocationAction::Apply => net = net.checked_add(allocation.allocated_gross_amount),
                AllocationAction::Reverse => net = net.checked_sub(allocation.allocated_gross_amount),
            }
            crate::dto::receivable::SalesInvoiceAllocationView {
                id: allocation.base.id.clone(),
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
    allocations: &[PurchaseInvoiceAllocation],
) -> (Amount, Vec<crate::dto::receivable::SalesInvoiceAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .iter()
        .map(|allocation| {
            // 进项/销项分配动作枚举跨域不共享（见 A-G7），此处显式转换。
            let action = match allocation.allocation_action {
                crate::entity::payable::AllocationAction::Apply => AllocationAction::Apply,
                crate::entity::payable::AllocationAction::Reverse => AllocationAction::Reverse,
            };
            match action {
                AllocationAction::Apply => net = net.checked_add(allocation.allocated_gross_amount),
                AllocationAction::Reverse => net = net.checked_sub(allocation.allocated_gross_amount),
            }
            crate::dto::receivable::SalesInvoiceAllocationView {
                id: allocation.base.id.clone(),
                allocation_seq: allocation.allocation_seq,
                allocation_action: action,
                receivable_account_id: allocation.payable_account_id.to_string(),
                allocated_gross_amount: allocation.allocated_gross_amount,
                allocated_net_amount: allocation.allocated_net_amount,
                allocated_tax_amount: allocation.allocated_tax_amount,
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
