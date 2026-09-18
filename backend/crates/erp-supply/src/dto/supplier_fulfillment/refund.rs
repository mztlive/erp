//! 供应商退款事实视图与登记请求。

use application_core::non_blank;
use erp_core::ids::{
    CostAllocationId, CostEntryId, PayableEntryId, PaymentAllocationId, SupplierFulfillmentItemId,
};
use erp_core::money::{Amount, Quantity};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::supplier_fulfillment::AllocationAction;

/// 供应商退款事实响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierRefundFactView {
    /// 实体主键。
    pub id: String,
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: String,
    /// 外部退款号。
    pub external_refund_no: String,
    /// 外部退款版本。
    pub external_refund_version: String,
    /// 实际退款金额。
    pub refund_amount: Amount,
    /// 实际退款时间（秒级时间戳）。
    pub refunded_at: i64,
    /// 来源事件 ID。
    pub source_event_id: String,
    /// 分配行。
    pub allocations: Vec<SupplierRefundAllocationView>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商退款分配行响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierRefundAllocationView {
    /// 实体主键。
    pub id: String,
    /// 退款头内稳定分配序号。
    pub allocation_no: u32,
    /// 原供应商履约明细。
    pub supplier_fulfillment_item_id: String,
    /// 实际供应商退款数量。
    pub refund_quantity: Quantity,
    /// 含税成本冲减金额。
    pub gross_amount: Amount,
    /// 不含税成本冲减金额。
    pub net_amount: Amount,
    /// 税额冲减金额。
    pub tax_amount: Amount,
    /// 未付应付冲减金额。
    pub payable_reduction_amount: Amount,
    /// 已付现金退回拆分金额。
    pub cash_refund_amount: Amount,
    /// 分配动作。
    pub allocation_action: AllocationAction,
}

/// 供应商退款成功结果登记请求（幂等键 `(connection_id, external_refund_no,
/// external_refund_version)`，§6.19；分配行冻结冲减范围）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RecordRefundResultRequest {
    /// 外部退款号。
    #[validate(custom(function = "non_blank", message = "外部退款号不能为空"))]
    pub external_refund_no: String,
    /// 外部退款版本。
    #[validate(custom(function = "non_blank", message = "外部退款版本不能为空"))]
    pub external_refund_version: String,
    /// 实际退款金额。
    pub refund_amount: Amount,
    /// 实际退款时间（秒级时间戳）。
    #[validate(range(min = 1, message = "退款时间必须大于 0"))]
    pub refunded_at: i64,
    /// 来源事件 ID。
    #[validate(custom(function = "non_blank", message = "来源事件ID不能为空"))]
    pub source_event_id: String,
    /// 退款分配行（`APPLY`）。
    #[validate(length(min = 1, message = "退款分配至少一行"))]
    pub allocations: Vec<RefundAllocationRequest>,
}

/// 供应商退款分配行请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RefundAllocationRequest {
    /// 原供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 被冲减的原成本。
    pub original_cost_entry_id: CostEntryId,
    /// 被冲减的原成本归属。
    pub original_cost_allocation_id: CostAllocationId,
    /// 被冲减的原应付分录。
    pub original_payable_entry_id: PayableEntryId,
    /// 原应付已付款部分的付款分配，可空。
    pub original_payment_allocation_id: Option<PaymentAllocationId>,
    /// 实际供应商退款数量。
    pub refund_quantity: Quantity,
    /// 含税成本冲减金额。
    pub gross_amount: Amount,
    /// 不含税成本冲减金额。
    pub net_amount: Amount,
    /// 税额冲减金额。
    pub tax_amount: Amount,
    /// 未付应付冲减金额。
    pub payable_reduction_amount: Amount,
    /// 已付现金退回拆分金额。
    pub cash_refund_amount: Amount,
}
