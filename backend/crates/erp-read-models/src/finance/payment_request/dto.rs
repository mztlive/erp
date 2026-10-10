//! 付款申请的脱敏整单与当前事实投影。

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_finance::entity::payable::{PaymentRequestAllocation, PurchasePaymentRequest};
use erp_party::PartyBankAccountView;
use serde::Serialize;
use serde_json::Value;

use crate::finance::dto::DocumentApprovalView;

/// 当前余额与履约事实独立于提交时快照返回。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentRequestCurrentLine {
    pub purchase_order_id: String,
    pub payable_account_id: String,
    pub purchase_revision_id: Option<String>,
    pub status: String,
    pub receiving_progress: String,
    pub current_gross_total: Amount,
    pub current_settled_total: Amount,
    pub current_open_total: Amount,
    pub current_reserved_total: Amount,
    pub current_available_total: Amount,
    pub source_changed: bool,
    pub payable_entries: Vec<PaymentRequestPayableEntryView>,
}

/// 当前来源账户的增加分录及有效可核销余额。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentRequestPayableEntryView {
    pub id: String,
    pub amount: Amount,
    pub settled_total: Amount,
    pub offset_total: Amount,
    pub open_total: Amount,
}

/// 全部采购来源获准或正式任务获准后交付的整单视图。
#[derive(Debug, Clone, Serialize)]
pub struct PurchasePaymentRequestView {
    #[serde(flatten)]
    pub request: PurchasePaymentRequest,
    pub supplier_name: String,
    pub created_by_name: Option<String>,
    pub purchase_previews: Vec<Value>,
    pub sales_previews: Vec<Value>,
    pub current_lines: Vec<PaymentRequestCurrentLine>,
    pub approval: DocumentApprovalView,
    pub allowed_actions: Vec<String>,
    pub attachments: Vec<PaymentRequestAttachmentView>,
    pub payments: Vec<PaymentRequestPaymentView>,
}

/// 本申请形成的实际付款与有效冲正金额，回单沿本申请资格读取。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentRequestPaymentView {
    pub payment_id: String,
    pub payment_no: String,
    pub paid_at: Instant,
    pub amount: Amount,
    pub reversed_amount: Amount,
    pub effective_amount: Amount,
    pub allocations: Vec<PaymentRequestAllocation>,
    pub receipt: Option<PaymentRequestAttachmentView>,
}

/// 精确历史提交的原审核材料，不包含当前操作资格或后续付款进度。
#[derive(Debug, Clone, Serialize)]
pub struct PurchasePaymentRequestSubmissionView {
    #[serde(flatten)]
    pub request: PurchasePaymentRequest,
    pub purchase_previews: Vec<Value>,
    pub sales_previews: Vec<Value>,
    pub attachments: Vec<PaymentRequestAttachmentView>,
}

/// 本申请允许目录的安全文件元数据。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentRequestAttachmentView {
    pub file_asset_id: String,
    pub file_name: String,
    pub content_type: String,
    pub byte_size: u64,
}

/// 已证明申请动作和采购正式来源的候选；收款账户仅为候选且全部脱敏。
#[derive(Debug, Clone, Serialize)]
pub struct PurchasePaymentRequestCandidate {
    pub purchase_order_id: String,
    pub purchase_no: String,
    pub purchase_revision_id: String,
    pub supplier_id: String,
    pub supplier_name: String,
    pub payer_party_id: String,
    pub payer_party_name: String,
    pub payable_account_id: String,
    pub gross_total: Amount,
    pub settled_total: Amount,
    pub open_total: Amount,
    pub reserved_total: Amount,
    pub available_total: Amount,
    pub payment_terms: Value,
    pub bank_accounts: Vec<PartyBankAccountView>,
}
