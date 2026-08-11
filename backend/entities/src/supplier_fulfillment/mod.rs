//! 域 D32 `supplier_fulfillment`：supplier_fulfillment_order、supplier_fulfillment_item、
//! supplier_order_action(+_line)、supplier_order_status_history、supplier_refund_fact、
//! supplier_refund_allocation（页面：W26）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元（数据模型 §3.1）。
//! 字段字典见 §6.19；履约主线、取消与退款三条正交状态机按 §7.6 固化（§6.19 禁止折叠为
//! 单一状态枚举）；供应商退款是冲减供应商成本和应付的唯一事实，正式事实不设业务软删除
//! （§4.5.1、§6.19）。
//!
//! 本域在 §8.4 第 5 条中实现实体层可判定的部分：退款头金额与 APPLY 分配合计恒等、
//! 分配行含税/不含税/税额三元组恒等、应付冲减与现金退款拆分恒等、REVERSE 必须引用原
//! APPLY 分配；锁定原履约/成本/应付、追加成本与应付冲减、付款分配 REVERSE 及通用现金
//! 退款事实等跨聚合编排留给 P3。

pub mod fulfillment_item;
pub mod fulfillment_order;
pub mod order_action;
pub mod refund;
pub mod status;
pub mod status_history;

pub use crate::ids::{
    SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierOrderActionId, SupplierOrderActionLineId,
    SupplierOrderStatusHistoryId, SupplierRefundAllocationId, SupplierRefundFactId,
};
pub use fulfillment_item::{SupplierFulfillmentItem, SupplierFulfillmentItemData};
pub use fulfillment_order::{
    SupplierFulfillmentOrder, SupplierFulfillmentOrderData, SupplierFulfillmentOrderUpdate,
};
pub use order_action::{
    SupplierOrderAction, SupplierOrderActionData, SupplierOrderActionLine, SupplierOrderActionLineData,
    SupplierOrderActionStatus, SupplierOrderActionType, SupplierOrderActionUpdate,
};
pub use refund::{
    AllocationAction, SupplierRefundAllocation, SupplierRefundAllocationData, SupplierRefundFact,
    SupplierRefundFactData,
};
pub use status::{CancelStatus, FulfillmentStatus, RefundStatus};
pub use status_history::{SupplierOrderStatusHistory, SupplierOrderStatusHistoryData};
