//! 域 D32 `supplier_fulfillment` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额/数量使用
//! `entities::money` 定点类型（serde_json 下自动字符串化）。

mod action;
mod list;
mod refund;

pub use action::{
    AfterSalesActionLineRequest, RecordSupplierRejectRequest, SubmitActionResultView,
    SubmitAfterSalesActionRequest, SupplierOrderActionBlockerView, SupplierOrderAllowedAction,
    SupplierOrderInvestigationAction, SupplierOrderInvestigationEvidenceView,
    SupplierOrderInvestigationOutcome, SupplierOrderInvestigationResultStatus,
    SupplierOrderObjectInvestigationCommand, SupplierOrderResolution, SupplierOrderTaskCompletionCommand,
    SupplierOrderTaskCompletionDecision, SupplierOrderTaskCompletionDecisionType,
    SupplierOrderTaskInvestigationAction, SupplierOrderTaskInvestigationCommand,
};
pub use list::{
    FulfillmentOrderListQuery, PageParams, PageView, PlaceFulfillmentItemRequest,
    PlaceFulfillmentOrderRequest, SupplierFulfillmentItemView, SupplierFulfillmentOrderDetailParams,
    SupplierFulfillmentOrderListParams, SupplierFulfillmentOrderView, SupplierOrderActionLineView,
    SupplierOrderActionView, SupplierOrderAddressView, SupplierOrderStatusHistoryView,
};
pub(crate) use list::{SortDir, normalize_sort};
pub use refund::{
    RecordRefundResultRequest, RefundAllocationRequest, SupplierRefundAllocationView, SupplierRefundFactView,
};

pub use super::supplier_fulfillment_scope::{
    FulfillmentHandoverCandidateView, HandoverFulfillmentOrderRequest, HandoverFulfillmentOrderView,
    SupplierFulfillmentOrderListView,
};
