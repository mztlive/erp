//! 采购内部报价的不可变版本及准确需求作用域。
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::{SkuId, SupplierAccountId, SupplierOfferingId};
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

use super::{InquiryQuoteUse, QuoteTerms};
use crate::{Error, Result};

/// 内部报价适用的精确询价需求版本；需求变化必须建立新版本报价。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InquiryQuoteScope {
    pub inquiry_id: String,
    pub requirement_version: u32,
    pub line_id: String,
}

/// 内部报价输入；没有销售供给价字段。
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryQuoteData {
    pub quote_id: String,
    pub revision_no: u32,
    pub feedback_round: u32,
    #[serde(flatten)]
    pub scope: InquiryQuoteScope,
    pub supplier_id: SupplierAccountId,
    pub supplier_offering_id: SupplierOfferingId,
    pub sku_id: SkuId,
    pub terms: QuoteTerms,
    pub internal_note: String,
    pub attachment_ids: Vec<String>,
    pub recorded_by: String,
}

/// 供应领域独立保存的采购内部报价；只能经采购权限投影返回。
#[derive(Clone, Serialize, Deserialize, Entity)]
pub struct InquiryQuote {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub data: InquiryQuoteData,
}

impl InquiryQuote {
    /// 创建不可变内部报价版本。
    /// # 参数
    /// `id` 为版本主键，`data` 为独立内部报价资料。
    /// # 返回
    /// 返回尚未持久化的报价版本。
    /// # 错误
    /// 作用域、供应身份、条款或内部资料非法时拒绝。
    pub fn new(id: String, mut data: InquiryQuoteData) -> Result<Self> {
        if id.trim().is_empty()
            || data.revision_no == 0
            || data.feedback_round == 0
            || data.scope.requirement_version == 0
        {
            return Err(Error::ValidationError("询价报价身份或版本缺失".into()));
        }
        for value in [
            &data.quote_id,
            &data.scope.inquiry_id,
            &data.scope.line_id,
            data.supplier_id.as_ref(),
            data.supplier_offering_id.as_ref(),
            data.sku_id.as_ref(),
        ] {
            if value.trim().is_empty() || value.len() > 128 {
                return Err(Error::ValidationError("询价报价作用域或供应身份非法".into()));
            }
        }
        data.recorded_by =
            normalize_required_text(data.recorded_by, "报价登记人不能为空", 128, "报价登记人过长")?;
        if data.internal_note.chars().count() > 4000
            || data.attachment_ids.len() > 20
            || data.attachment_ids.iter().any(|id| id.trim().is_empty() || id.len() > 128)
        {
            return Err(Error::ValidationError("询价报价内部备注或附件非法".into()));
        }
        data.terms = data.terms.validated()?;
        Ok(Self { base: BaseModel::new(id), data })
    }

    pub(super) fn ensure_next_revision(&self, prior: Option<&InquiryQuote>) -> Result<()> {
        let next = prior.map_or(Some(1), |value| value.data.revision_no.checked_add(1));
        if next != Some(self.data.revision_no)
            || prior.is_some_and(|value| {
                value.data.scope != self.data.scope
                    || value.data.supplier_id != self.data.supplier_id
                    || value.data.supplier_offering_id != self.data.supplier_offering_id
                    || value.data.sku_id != self.data.sku_id
            })
        {
            return Err(Error::ConflictError("报价版本须连续且稳定报价身份不得换需求或供应来源".into()));
        }
        Ok(())
    }

    /// 校验精确需求和供应身份后核对条款，不修改报价历史。
    /// # 参数
    /// `usage` 为本次正式采用的服务器事实。
    /// # 返回
    /// 作用域和商业条件全部匹配时成功。
    /// # 错误
    /// 跨询价、跨需求版本、换SKU或供应商及条款失效时拒绝。
    pub fn ensure_applicable(&self, usage: &InquiryQuoteUse) -> Result<()> {
        if self.base.is_deleted()
            || self.data.scope.inquiry_id != usage.inquiry_id
            || self.data.scope.requirement_version != usage.requirement_version
            || self.data.scope.line_id != usage.line_id
            || self.data.sku_id != usage.sku_id
            || self.data.supplier_id != usage.supplier_id
        {
            return Err(Error::BusinessLogicError("专项报价不属于本次询价需求及供应身份".into()));
        }
        self.data.terms.ensure_applicable(usage)
    }
}
