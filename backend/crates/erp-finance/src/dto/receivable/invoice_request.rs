//! 开票申请协议。定义和审批人由服务端绑定，客户端不能指定。
use crate::entity::receivable::{InvoiceRequestData, InvoiceRequestStatus};
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};

/// 原子创建并提交，或修改撤回后的草稿再提交。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitInvoiceRequest {
    pub receivable_account_id: String,
    pub request_id: Option<String>,
    pub expected_version: Option<u64>,
    pub data: InvoiceRequestData,
    pub idempotency_key: String,
}
/// 撤回命令；保留同一载荷重试以恢复未知结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelInvoiceRequest {
    pub expected_version: u64,
    pub reason: String,
    pub idempotency_key: String,
}
/// 申请列表筛选，账户和销售单条件与状态取交集。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InvoiceRequestQuery {
    pub sales_order_id: Option<String>,
    pub customer_id: Option<String>,
    pub receivable_account_id: Option<String>,
    pub work_item_id: Option<String>,
    pub status: Option<InvoiceRequestStatus>,
    pub q: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u32>,
}
/// 销售应收的开票申请额度摘要。各字段含税，已登记不重复占用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceRequestAmounts {
    pub receivable_account_id: String,
    pub available_amount: Amount,
    pub pending_amount: Amount,
    pub approved_remaining_amount: Amount,
    pub invoiced_amount: Amount,
}
