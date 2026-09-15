use crate::dto::supplier_fulfillment::SupplierOrderResolution;
use crate::entity::supplier_fulfillment::{
    SupplierOrderActionLine, SupplierRefundAllocation, SupplierRefundFact, VerifiedSupplierOrderResolution,
};

impl From<VerifiedSupplierOrderResolution> for SupplierOrderResolution {
    /// 将实体层已验证业务终态映射为服务契约枚举。
    fn from(value: VerifiedSupplierOrderResolution) -> Self {
        match value {
            VerifiedSupplierOrderResolution::OrderAccepted => Self::OrderAccepted,
            VerifiedSupplierOrderResolution::OrderRejected => Self::OrderRejected,
            VerifiedSupplierOrderResolution::OrderCompleted => Self::OrderCompleted,
            VerifiedSupplierOrderResolution::Canceled => Self::Canceled,
            VerifiedSupplierOrderResolution::Refunded => Self::Refunded,
        }
    }
}

/// 从动作行实体构造响应视图。
///
/// # 参数
/// * `line` - 动作行实体
///
/// # 返回
/// 返回响应视图。
pub fn action_line_view(
    line: SupplierOrderActionLine,
) -> crate::dto::supplier_fulfillment::SupplierOrderActionLineView {
    crate::dto::supplier_fulfillment::SupplierOrderActionLineView {
        id: line.base.id,
        line_no: line.line_no,
        supplier_fulfillment_item_id: line.supplier_fulfillment_item_id.to_string(),
        quantity: line.quantity,
        amount: line.amount,
    }
}

/// 从退款事实头与分配行构造响应视图。
///
/// # 参数
/// * `fact` - 退款事实头
/// * `allocations` - 分配行集合
///
/// # 返回
/// 返回响应视图。
pub fn refund_fact_view(
    fact: &SupplierRefundFact,
    allocations: &[SupplierRefundAllocation],
) -> crate::dto::supplier_fulfillment::SupplierRefundFactView {
    crate::dto::supplier_fulfillment::SupplierRefundFactView {
        id: fact.base.id.clone(),
        supplier_fulfillment_order_id: fact.supplier_fulfillment_order_id.to_string(),
        external_refund_no: fact.external_refund_no.clone(),
        external_refund_version: fact.external_refund_version.clone(),
        refund_amount: fact.refund_amount,
        refunded_at: fact.refunded_at.unix_secs(),
        source_event_id: fact.source_event_id.clone(),
        allocations: allocations.iter().map(refund_allocation_view).collect(),
        created_at: fact.base.created_at,
    }
}

/// 从退款分配行实体构造响应视图。
///
/// # 参数
/// * `allocation` - 退款分配行实体
///
/// # 返回
/// 返回响应视图。
fn refund_allocation_view(
    allocation: &SupplierRefundAllocation,
) -> crate::dto::supplier_fulfillment::SupplierRefundAllocationView {
    crate::dto::supplier_fulfillment::SupplierRefundAllocationView {
        id: allocation.base.id.clone(),
        allocation_no: allocation.allocation_no,
        supplier_fulfillment_item_id: allocation.supplier_fulfillment_item_id.to_string(),
        refund_quantity: allocation.refund_quantity,
        gross_amount: allocation.gross_amount,
        net_amount: allocation.net_amount,
        tax_amount: allocation.tax_amount,
        payable_reduction_amount: allocation.payable_reduction_amount,
        cash_refund_amount: allocation.cash_refund_amount,
        allocation_action: allocation.allocation_action,
    }
}
