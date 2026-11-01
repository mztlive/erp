//! 询价费用的采购内部收款及凭证依据。
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::SupplierAccountId;
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 费用公开金额由销售结果持有，本结构仅保存采购内部依据。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryFeeBasisData {
    pub inquiry_id: String,
    pub requirement_version: u32,
    pub feedback_round: u32,
    pub fee_id: String,
    pub payee_supplier_id: SupplierAccountId,
    pub payment_term_code: String,
    pub internal_note: String,
    pub attachment_ids: Vec<String>,
    pub recorded_by: String,
}

/// 按询价、需求版本、反馈轮次及稳定费用身份冻结内部收款依据。
#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct InquiryFeeBasis {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub data: InquiryFeeBasisData,
}
impl InquiryFeeBasis {
    /// 建立不可变费用收款依据，不产生应付或付款授权。
    /// # 参数
    /// `id` 为依据主键，`data` 为已授权采购输入。
    /// # 返回
    /// 返回内部费用依据。
    /// # 错误
    /// 作用域、收款身份、付款条件或内部资料非法时拒绝。
    pub fn new(id: String, mut data: InquiryFeeBasisData) -> Result<Self> {
        if id.trim().is_empty()
            || data.requirement_version == 0
            || data.feedback_round == 0
            || [&data.inquiry_id, &data.fee_id, data.payee_supplier_id.as_ref(), &data.recorded_by]
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 128)
            || data.internal_note.chars().count() > 4000
            || data.attachment_ids.len() > 20
            || data.attachment_ids.iter().any(|id| id.trim().is_empty() || id.len() > 128)
        {
            return Err(Error::ValidationError("询价费用内部依据身份或内容非法".into()));
        }
        data.payment_term_code = normalize_required_text(
            data.payment_term_code,
            "询价费用付款条件不能为空",
            128,
            "询价费用付款条件过长",
        )?;
        Ok(Self { base: BaseModel::new(id), data })
    }
}
