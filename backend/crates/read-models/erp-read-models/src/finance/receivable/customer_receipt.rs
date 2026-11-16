//! 原命令授权事务内的客户回款投影；对外读取入口归 funds_scope。
use erp_core::money::Amount;
use erp_finance::dto::receivable::ReceiptAllocationView;
use erp_finance::entity::receivable::{AllocationAction, ReceiptAllocation};
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_finance::service::receivable::mapping::zero_amount;
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::Executor;

use super::ReceivableReadService;
use crate::finance::command_snapshot;
use crate::finance::dto::CustomerReceiptView;
use crate::{Error, Result};

impl ReceivableReadService {
    /// 使用调用方执行器读取完整回款、核销安排与冻结审批绑定。
    ///
    /// # 参数
    /// * `id` - 回款单主键
    /// * `executor` - 已完成完整来源授权的事务执行器
    ///
    /// # 返回
    /// 返回实际金额、当前版本及原拟核销分配，不进行资金范围裁剪。
    ///
    /// # 错误
    /// 回款不存在、绑定事实或仓储读取失败时返回对应错误。
    pub async fn customer_receipt_view_with_executor(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CustomerReceiptView> {
        command_snapshot(executor)?;
        let receipt = self
            .db
            .customer_receipts()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
        let allocations = self
            .db
            .receipt_allocations()
            .find_allocations_by_receipts(&[receipt.base.id.clone().into()], executor)
            .await?;
        let (allocated_total, views) = allocation_view(&allocations);
        let binding = match find_approval_binding(&self.db, id, executor).await.map_err(crate::Error::from) {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let approval = super::approval_query::load_receipt_document_approval(
            &self.db,
            &receipt,
            binding.as_ref(),
            executor,
        )
        .await?;
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
            approval,
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
fn allocation_view(allocations: &[ReceiptAllocation]) -> (Amount, Vec<ReceiptAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .iter()
        .map(|allocation| {
            match allocation.allocation_action {
                AllocationAction::Apply => net = net.checked_add(allocation.allocated_amount),
                AllocationAction::Reverse => net = net.checked_sub(allocation.allocated_amount),
            }
            ReceiptAllocationView {
                id: allocation.base.id.clone(),
                allocation_seq: allocation.allocation_seq,
                allocation_action: allocation.allocation_action,
                receivable_entry_id: allocation.receivable_entry_id.to_string(),
                allocated_amount: allocation.allocated_amount,
                allocated_at: allocation.allocated_at,
                reverses_allocation_id: allocation.reverses_allocation_id.as_ref().map(|id| id.to_string()),
            }
        })
        .collect();
    (net, views)
}
