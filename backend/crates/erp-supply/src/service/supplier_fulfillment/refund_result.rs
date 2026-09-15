//! 退款事实与分配、原上限校验和调用方事务内写入。
use erp_core::common::time::Instant;
use erp_core::ids::{InboxMessageId, SupplierRefundAllocationId, SupplierRefundFactId};
use erp_core::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::SupplierFulfillmentService;
use crate::dto::supplier_fulfillment::*;
use crate::entity::supplier_fulfillment::*;
use crate::repository::SupplierFulfillmentExt;
use crate::{Error, Result};
impl SupplierFulfillmentService {
    /// 构建退款事实头与全部分配行，并校验 APPLY 合计恒等（§6.19）。
    ///
    /// # 参数
    /// * `order` - 供应商子订单
    /// * `req` - 退款成功结果请求
    /// * `connection_id` - 供应商连接
    /// * `message` - 同事务创建的 `inbox_message` 信封
    ///
    /// # 返回
    /// 返回 `(事实头, 分配行)`。
    ///
    /// # 错误
    /// 金额恒等或实体校验失败时返回 `LogicError`。
    pub fn build_refund_fact(
        &self,
        order: &SupplierFulfillmentOrder,
        req: &RecordRefundResultRequest,
        connection_id: &erp_core::ids::SupplierApiConnectionId,
        inbox_message_id: &InboxMessageId,
    ) -> Result<(SupplierRefundFact, Vec<SupplierRefundAllocation>)> {
        let fact = SupplierRefundFact::new(
            SupplierRefundFactId::new(next_id()),
            SupplierRefundFactData {
                supplier_id: order.supplier_id.clone(),
                connection_id: connection_id.clone(),
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                external_refund_no: req.external_refund_no.clone(),
                external_refund_version: req.external_refund_version.clone(),
                refund_amount: req.refund_amount,
                refunded_at: Instant::from_unix_secs(req.refunded_at),
                source_event_id: req.source_event_id.clone(),
                inbox_message_id: inbox_message_id.clone(),
            },
        )?;
        let allocations = req
            .allocations
            .iter()
            .enumerate()
            .map(|(index, allocation)| {
                SupplierRefundAllocation::new(
                    SupplierRefundAllocationId::new(next_id()),
                    SupplierRefundAllocationData {
                        supplier_refund_fact_id: SupplierRefundFactId::new(fact.base.id.as_str()),
                        allocation_no: (index + 1) as u32,
                        supplier_fulfillment_item_id: allocation.supplier_fulfillment_item_id.clone(),
                        original_cost_entry_id: allocation.original_cost_entry_id.clone(),
                        original_cost_allocation_id: allocation.original_cost_allocation_id.clone(),
                        original_payable_entry_id: allocation.original_payable_entry_id.clone(),
                        original_payment_allocation_id: allocation.original_payment_allocation_id.clone(),
                        refund_quantity: allocation.refund_quantity,
                        gross_amount: allocation.gross_amount,
                        net_amount: allocation.net_amount,
                        tax_amount: allocation.tax_amount,
                        payable_reduction_amount: allocation.payable_reduction_amount,
                        cash_refund_amount: allocation.cash_refund_amount,
                        cash_supplier_refund_id: None,
                        allocation_action: crate::entity::supplier_fulfillment::AllocationAction::Apply,
                        reverses_allocation_id: None,
                    },
                )
            })
            .collect::<std::result::Result<Vec<_>, erp_core::Error>>()?;
        fact.validate_allocations(&allocations)?;
        Ok((fact, allocations))
    }
    /// 读取原两项财务快照并在原位置推进退款状态。
    pub async fn advance_refund_result(
        &self,
        id: &str,
        order: &mut SupplierFulfillmentOrder,
        refund_amount: Amount,
    ) -> Result<()> {
        let financial = self
            .db
            .supplier_fulfillment()
            .refund_financial_snapshot(&SupplierFulfillmentOrderId::new(id), &mut NoTransaction)
            .await?;
        let order_total = financial.order_cost_gross;
        let refunded_total = financial.refunded_total;
        let total_after = refunded_total.checked_add(refund_amount);
        if total_after > order_total {
            return Err(Error::BusinessLogicError("累计净退款金额不得超过订单成本余额".to_string()));
        }
        order.advance_refund(if total_after == order_total {
            RefundStatus::Refunded
        } else {
            RefundStatus::Partial
        })?;
        Ok(())
    }
}
/// 先订单CAS，后退款事实与分配的原复合写。
pub async fn persist_refund_result(
    db: &Database,
    order: &mut SupplierFulfillmentOrder,
    fact: &SupplierRefundFact,
    allocations: &[SupplierRefundAllocation],
    executor: &mut dyn Executor,
) -> Result<()> {
    db.supplier_fulfillment_orders().update(order, executor).await?;
    db.supplier_fulfillment().create_refund_fact_with_allocations(fact, allocations, executor).await?;
    Ok(())
}
