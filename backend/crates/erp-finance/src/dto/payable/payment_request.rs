//! 采购付款申请写命令及查询合同。审核人与主体仅由服务端来源装配。

use serde::{Deserialize, Serialize};

use crate::entity::payable::{PurchasePaymentRequestData, PurchasePaymentRequestStatus};

/// 创建或修改尚未批准的申请草稿。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavePurchasePaymentRequest {
    pub expected_version: Option<u64>,
    pub data: PurchasePaymentRequestData,
}

/// 原子提交并占用逐采购余额。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitPurchasePaymentRequest {
    pub expected_version: u64,
    pub idempotency_key: String,
}

/// 撤回或关闭未付部分，保留原命令身份恢复未知结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelPurchasePaymentRequest {
    pub expected_version: u64,
    pub reason: String,
    pub idempotency_key: String,
}

/// 普通列表和任务读取的有界筛选条件。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurchasePaymentRequestQuery {
    pub scope_version: Option<String>,
    pub supplier_id: Option<String>,
    pub purchase_order_id: Option<String>,
    pub payable_account_id: Option<String>,
    pub work_item_id: Option<String>,
    pub created_by: Option<String>,
    pub status: Option<PurchasePaymentRequestStatus>,
    pub q: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u32>,
}
