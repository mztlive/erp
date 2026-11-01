//! 采购专用的实际专项采用投影；不复制内部议价或附件资料。
use erp_core::money::Quantity;
use serde::Serialize;

use crate::entity::purchase_order::PurchaseInquiryQuoteSource;

/// 从已冻结采购行读取的需求与实际报价两条来源链。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InquiryQuoteSourceView {
    pub inquiry_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inquiry_no: Option<String>,
    pub requirement_version: u32,
    pub feedback_round: u32,
    pub inquiry_line_id: String,
    pub quote_id: String,
    pub quote_revision_id: String,
    pub quote_revision_no: u32,
    pub adopted_quantity: Option<String>,
}
impl InquiryQuoteSourceView {
    /// 只投影采购行已经冻结的真实采用身份和数量。
    /// # 参数
    /// `source`为正式采用快照，`quantity`为当前展示的采购提交或版本行数量。
    /// # 返回
    /// 返回采购追溯白名单；单号由授权读模型沿稳定销售来源补齐。
    /// # 错误
    /// 无；不读取最新商品或报价主数据覆盖历史。
    pub fn from_source(source: &PurchaseInquiryQuoteSource, quantity: Option<Quantity>) -> Self {
        Self {
            inquiry_id: source.inquiry_id.clone(),
            inquiry_no: None,
            requirement_version: source.requirement_version,
            feedback_round: source.feedback_round,
            inquiry_line_id: source.inquiry_line_id.clone(),
            quote_id: source.quote_id.clone(),
            quote_revision_id: source.quote_revision_id.clone(),
            quote_revision_no: source.quote_revision_no,
            adopted_quantity: quantity.map(|quantity| quantity.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    #[test]
    fn purchase_trace_uses_frozen_adoption_version_and_actual_line_quantity() {
        let source = PurchaseInquiryQuoteSource {
            inquiry_id: "inquiry".into(),
            requirement_version: 2,
            feedback_round: 3,
            inquiry_line_id: "demand-line".into(),
            quote_id: "quote".into(),
            quote_revision_id: "quote-revision".into(),
            quote_revision_no: 4,
            quote_version: 1,
            unit_cost_gross: "8".parse().unwrap(),
            input_tax_rate: "0.13".parse().unwrap(),
            available_quantity: "100".parse().unwrap(),
            address_limits: vec![],
            expected_delivery_date: "2026-10-22".parse().unwrap(),
            valid_to: Some("2026-11-01".parse().unwrap()),
        };
        let view = InquiryQuoteSourceView::from_source(&source, Some("3".parse().unwrap()));
        assert_eq!(
            serde_json::to_value(view).unwrap(),
            json!({"inquiry_id":"inquiry","requirement_version":2,
            "feedback_round":3,"inquiry_line_id":"demand-line","quote_id":"quote","quote_revision_id":"quote-revision",
            "quote_revision_no":4,"adopted_quantity":"3"})
        );
    }
}
