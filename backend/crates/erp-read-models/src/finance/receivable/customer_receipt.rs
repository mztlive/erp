//! Customer receipt read projections with immutable approval bindings.

use super::approval_view::document_approval_view;
use super::ReceivableReadService;
use crate::finance::dto::CustomerReceiptView;
use crate::{Error, Result};
use erp_core::ids::CustomerReceiptId;
use erp_core::money::Amount;
use erp_finance::dto::receivable::{CustomerReceiptListParams, PageView, SortDir};
use erp_finance::entity::receivable::{AllocationAction, ReceiptAllocation};
use erp_finance::repository::ReceivableExt;
use erp_finance::service::receivable::mapping::zero_amount;
use erp_workflow::service::document_registry::find_approval_binding;
use erp_workflow::DocumentRegistryExt;
use persistence_core::NoTransaction;
use std::collections::HashMap;
use validator::Validate;

impl ReceivableReadService {
    /// 分页查询客户回款单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`receipt_no`/`counterparty_party_id`/`status`）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn customer_receipt_list(
        &self,
        params: &CustomerReceiptListParams,
    ) -> Result<PageView<CustomerReceiptView>> {
        params.validate()?;
        let query = params.normalized()?;
        let scope_query = erp_finance::repository::ScopedCustomerReceiptQuery {
            receipt_no: query.receipt_no,
            counterparty_party_id: query.counterparty_party_id,
            status: query.status,
            scope: erp_finance::repository::ReceivableListScope {
                sales_order_id: query.sales_order_id,
                receivable_account_id: query.receivable_account_id,
            },
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .receivable()
            .search_customer_receipts_in_account_scope(&scope_query, &mut NoTransaction)
            .await?;
        let receipt_ids = page
            .items
            .iter()
            .map(|row| CustomerReceiptId::new(row.id.clone()))
            .collect::<Vec<_>>();
        let document_ids = page.items.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let mut allocations_by_receipt = HashMap::<String, Vec<ReceiptAllocation>>::new();
        for allocation in self
            .db
            .receipt_allocations()
            .find_allocations_by_receipts(&receipt_ids, &mut NoTransaction)
            .await?
        {
            allocations_by_receipt
                .entry(allocation.customer_receipt_id.to_string())
                .or_default()
                .push(allocation);
        }
        for allocations in allocations_by_receipt.values_mut() {
            allocations.sort_unstable_by_key(|allocation| allocation.allocation_seq);
        }
        let bindings_by_document = self
            .db
            .business_documents()
            .find_documents_by_ids(&document_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|document| (document.base.id.clone(), document.approval_binding))
            .collect::<HashMap<_, _>>();
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            let allocations = allocations_by_receipt.remove(&row.id).unwrap_or_default();
            let (allocated_total, allocations) = allocation_view(&allocations);
            let approval_binding = bindings_by_document.get(&row.id).and_then(Option::as_ref);
            views.push(CustomerReceiptView {
                id: row.id,
                receipt_no: row.receipt_no,
                status: row.status,
                counterparty_party_id: row.counterparty_party_id,
                customer_id: row.customer_id,
                received_at: row.received_at,
                amount: row.amount,
                bank_reference: row.bank_reference,
                version: row.version,
                created_at: row.created_at,
                allocated_total,
                unallocated_amount: row.amount.checked_sub(allocated_total),
                allocations,
                pending_allocations: row.pending_allocations,
                approval: document_approval_view(approval_binding, None, row.status),
            });
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: scope_query.page,
            page_size: scope_query.page_size,
        })
    }
    /// 查询客户回款单详情（含核销分配行）。
    ///
    /// # 参数
    /// * `id` - 回款单 ID
    ///
    /// # 返回
    /// 返回回款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 回款单不存在
    pub async fn customer_receipt_detail(&self, id: &str) -> Result<CustomerReceiptView> {
        self.customer_receipt_view(id.to_string()).await
    }
    /// 装配客户回款单视图。
    ///
    /// # 参数
    /// * `id` - 回款单 ID
    ///
    /// # 返回
    /// 返回回款单视图（含分配行与未分配余额）。
    ///
    /// # 错误
    /// * `NotFound` - 回款单不存在
    pub async fn customer_receipt_view(&self, id: String) -> Result<CustomerReceiptView> {
        let receipt = self
            .db
            .customer_receipts()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
        let allocations = self
            .db
            .receipt_allocations()
            .find_allocations_by_receipts(&[receipt.base.id.clone().into()], &mut NoTransaction)
            .await?;
        let (allocated_total, views) = allocation_view(&allocations);
        let binding = match find_approval_binding(&self.db, &id, &mut NoTransaction)
            .await
            .map_err(crate::Error::from)
        {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        Ok(CustomerReceiptView {
            id: receipt.base.id.clone(),
            receipt_no: receipt.receipt_no,
            status: receipt.status,
            counterparty_party_id: receipt.counterparty_party_id.to_string(),
            customer_id: receipt.customer_id.map(|id| id.to_string()),
            received_at: receipt.received_at,
            amount: receipt.amount,
            bank_reference: receipt.bank_reference,
            version: receipt.base.version,
            created_at: receipt.base.created_at,
            unallocated_amount: receipt.amount.checked_sub(allocated_total),
            allocated_total,
            allocations: views,
            pending_allocations: receipt.pending_allocations,
            approval: document_approval_view(binding.as_ref(), None, receipt.status),
        })
    }
}
/// 汇总回款核销分配并装配视图（`APPLY` 加、`REVERSE` 减）。
///
/// # 参数
/// * `allocations` - 回款核销分配集合
///
/// # 返回
/// 返回 `(净已核销合计, 分配视图列表)`。
fn allocation_view(
    allocations: &[ReceiptAllocation],
) -> (Amount, Vec<erp_finance::dto::receivable::ReceiptAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .iter()
        .map(|allocation| {
            match allocation.allocation_action {
                AllocationAction::Apply => net = net.checked_add(allocation.allocated_amount),
                AllocationAction::Reverse => net = net.checked_sub(allocation.allocated_amount),
            }
            erp_finance::dto::receivable::ReceiptAllocationView {
                id: allocation.base.id.clone(),
                allocation_seq: allocation.allocation_seq,
                allocation_action: allocation.allocation_action,
                receivable_entry_id: allocation.receivable_entry_id.to_string(),
                allocated_amount: allocation.allocated_amount,
                allocated_at: allocation.allocated_at,
                reverses_allocation_id: allocation
                    .reverses_allocation_id
                    .as_ref()
                    .map(|id| id.to_string()),
            }
        })
        .collect();
    (net, views)
}
