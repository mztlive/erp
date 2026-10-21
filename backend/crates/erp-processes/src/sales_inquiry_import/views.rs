//! 客户工作簿预览与确认的 HTTP 共用合同。

use erp_sales::entity::sales_inquiry::InquiryRequirementData;
use erp_sales::entity::sales_inquiry_import::{
    INQUIRY_TEMPLATE_VERSION, InquiryImportIssue, InquiryImportMerge, InquiryImportPreview,
};
use serde::{Deserialize, Serialize};

/// 预览只返回客户需求、商品快照与行错误。
#[derive(Clone, Serialize)]
pub struct InquiryImportPreviewView {
    pub preview_id: String,
    pub template_version: String,
    pub expected_version: u64,
    pub input_fingerprint: String,
    pub errors: Vec<InquiryImportIssue>,
    pub merges: Vec<InquiryImportMerge>,
    pub data: InquiryRequirementData,
    pub expires_at: i64,
}

impl From<InquiryImportPreview> for InquiryImportPreviewView {
    fn from(preview: InquiryImportPreview) -> Self {
        Self {
            preview_id: preview.base.id,
            template_version: INQUIRY_TEMPLATE_VERSION.into(),
            expected_version: preview.expected_version,
            input_fingerprint: preview.input_fingerprint,
            errors: preview.errors,
            merges: preview.merges,
            data: preview.data,
            expires_at: preview.expires_at,
        }
    }
}

/// 确认只接收已授权预览引用，不接收可替换预览需求的自由载荷。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmInquiryImportCommand {
    pub command_key: String,
    pub expected_version: u64,
    pub preview_id: String,
    pub input_fingerprint: String,
    pub confirm_merge_duplicates: bool,
}
