//! 询价转销售的服务端需求快照复制；客户端只提供销售商业字段。
use erp_core::common::time::Instant;
use erp_core::ids::{SkuId, SkuRevisionId};

use super::SalesOrderEditableDraftRequest;
use crate::entity::sales_inquiry::SalesInquiry;
use crate::entity::sales_order::inquiry_source::{InquirySalesSourceRequest, copied_delivery};
use crate::entity::sales_order::{LineType, SalesPricingMode};
use crate::{Error, Result};

impl SalesOrderEditableDraftRequest {
    /// 从已接受需求复制商品、数量和地址，保留销售明确填写的成交价与税率。
    /// # 参数
    /// `inquiry` 为已授权询价，`source` 为本次来源选择，`now` 为服务端时间。
    /// # 返回
    /// 返回以冻结需求为依据的完整可编辑销售草稿。
    /// # 错误
    /// 未明确成交价、来源不匹配、价格档位或适用范围失效时拒绝。
    pub fn with_inquiry_source(
        &self,
        inquiry: &SalesInquiry,
        source: &InquirySalesSourceRequest,
        now: Instant,
    ) -> Result<Self> {
        source.validate()?;
        if self.lines.len() != source.selections.len() {
            return Err(Error::ValidationError("销售商品行必须与询价来源选择逐一对应".into()));
        }
        let mut result = self.clone();
        for line in &mut result.lines {
            let selected = source
                .selections
                .iter()
                .find(|selection| selection.line_no == line.line_no)
                .ok_or_else(|| Error::ValidationError("销售商品行缺少明确询价来源".into()))?;
            let (demand, price) = selected.source_parts(inquiry, source.feedback_round, now)?;
            let goods = line
                .goods
                .as_mut()
                .ok_or_else(|| Error::ValidationError("询价转销售必须明确填写商品成交价".into()))?;
            if line.line_type != LineType::GoodsService || goods.pricing_mode == SalesPricingMode::Auto {
                return Err(Error::ValidationError(
                    "询价转销售须明确确认成交价或赠品，不能自动采用供给价".into(),
                ));
            }
            goods.pricing_mode.ensure_price(goods.unit_price_gross)?;
            line.item_name_snapshot.clone_from(&demand.product_name);
            line.spec_snapshot = Some(demand.specification.clone());
            line.unit_snapshot = Some(demand.unit_code.clone());
            goods.sku_id = SkuId::new(&demand.sku_id);
            goods.sku_revision_id = SkuRevisionId::new(&demand.sku_revision_id);
            goods.quantity = selected.quantity;
            goods.base_unit_code.clone_from(&demand.unit_code);
            goods.fulfillment_due_at = demand.delivery_due_at;
            goods.procurement_supply_price_gross = Some(
                price
                    .unit_price_gross
                    .ok_or_else(|| Error::ValidationError("询价供给单价未填写，不能转单".into()))?,
            );
            goods.delivery_allocations = selected
                .allocations
                .iter()
                .map(|allocation| {
                    let original = demand
                        .allocations
                        .iter()
                        .find(|row| row.allocation_id == allocation.allocation_id)
                        .ok_or_else(|| Error::ValidationError("询价需求地址不存在".into()))?;
                    Ok(copied_delivery(original, allocation.quantity))
                })
                .collect::<Result<Vec<_>>>()?;
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::entity::sales_inquiry::{
        InquiryAcceptance, InquiryCreateData, InquiryFeedbackSnapshot, InquiryRequirementSnapshot,
        InquiryStatus,
    };

    fn inquiry() -> SalesInquiry {
        let data: InquiryCreateData = serde_json::from_value(json!({
            "customer_id":"customer", "customer_name":"客户", "sales_owner_user_id":"sales", "business_org_unit_id":"org", "deadline_at":900,
            "draft":{"lines":[{"line_id":"line", "sku_id":"sku", "sku_revision_id":"sku-revision", "sku_code":"0001", "product_name":"来源商品", "specification":"规格", "unit_code":"件", "quantity_scale":0, "quantity":"10", "fulfillment_kind":"service", "delivery_due_at":2000}]}
        })).unwrap();
        let mut inquiry = SalesInquiry::create("inquiry".into(), "IQ-1".into(), data).unwrap();
        inquiry.requirement_version = 1;
        inquiry.feedback_round = 1;
        inquiry.status = InquiryStatus::Confirmed;
        inquiry.requirements.push(InquiryRequirementSnapshot {
            version: 1,
            submitted_by: "sales".into(),
            submitted_at: Instant::from_unix_secs(1),
            change_reason: "首次提交".into(),
            data: inquiry.draft.clone(),
        });
        inquiry.feedbacks.push(InquiryFeedbackSnapshot {round:1, requirement_version:1, published_by:"buyer".into(), published_at:Instant::from_unix_secs(2),
            data:serde_json::from_value(json!({"lines":[{"line_id":"line", "conclusion":"available", "available_quantity":"10", "prices":[{"price_id":"price", "unit_price_gross":"8", "tax_rate":"0.06", "valid_until":3000}]}]})).unwrap()});
        inquiry.acceptances.push(InquiryAcceptance {
            requirement_version: 1,
            feedback_round: 1,
            accepted_by: "sales".into(),
            accepted_at: Instant::from_unix_secs(3),
        });
        inquiry
    }
    fn draft(mode: &str, amount: &str) -> SalesOrderEditableDraftRequest {
        serde_json::from_value(json!({"editor_user_id":"sales", "lines":[{"line_no":1,"line_type":"GOODS_SERVICE","sales_tax_rate":"0.13","item_name_snapshot":"输入名称", "goods":{"sku_id":"client-sku","sku_revision_id":"client-revision","procurement_supply_price_gross":"1", "fulfillment_due_at":1111, "quantity":"99", "base_unit_code":"箱", "unit_price_gross":amount, "pricing_mode":mode}}]})).unwrap()
    }
    fn source() -> InquirySalesSourceRequest {
        serde_json::from_value(json!({"inquiry_id":"inquiry","feedback_round":1,"selections":[{"line_no":1,"inquiry_line_id":"line","price_id":"price","quantity":"2"}]})).unwrap()
    }
    #[test]
    fn server_copies_inquiry_demand_and_keeps_explicit_customer_price_separate_from_supply_reference() {
        let copied = draft("MANUAL", "100")
            .with_inquiry_source(&inquiry(), &source(), Instant::from_unix_secs(100))
            .unwrap();
        let line = &copied.lines[0];
        let goods = line.goods.as_ref().unwrap();
        assert_eq!(line.item_name_snapshot, "来源商品");
        assert_eq!(goods.sku_id.as_ref(), "sku");
        assert_eq!(goods.sku_revision_id.as_ref(), "sku-revision");
        assert_eq!(goods.quantity, "2".parse().unwrap());
        assert_eq!(goods.base_unit_code, "件");
        assert_eq!(goods.fulfillment_due_at, Instant::from_unix_secs(2000));
        assert_eq!(goods.unit_price_gross, "100".parse().unwrap());
        assert_eq!(goods.procurement_supply_price_gross, Some("8".parse().unwrap()));
        assert_eq!(line.sales_tax_rate, "0.13".parse().unwrap());
    }
    #[test]
    fn conversion_requires_explicit_price_and_current_accepted_applicable_round() {
        assert!(
            draft("AUTO", "100")
                .with_inquiry_source(&inquiry(), &source(), Instant::from_unix_secs(100))
                .is_err()
        );
        assert!(
            draft("MANUAL", "0")
                .with_inquiry_source(&inquiry(), &source(), Instant::from_unix_secs(100))
                .is_err()
        );
        assert!(
            draft("GIFT", "0")
                .with_inquiry_source(&inquiry(), &source(), Instant::from_unix_secs(100))
                .is_ok()
        );
        assert!(
            draft("MANUAL", "100")
                .with_inquiry_source(&inquiry(), &source(), Instant::from_unix_secs(3000))
                .is_err()
        );
        let mut closed = inquiry();
        closed.status = InquiryStatus::Closed;
        assert!(
            draft("MANUAL", "100")
                .with_inquiry_source(&closed, &source(), Instant::from_unix_secs(100))
                .is_err()
        );
        let mut wrong = source();
        wrong.feedback_round = 2;
        assert!(
            draft("MANUAL", "100")
                .with_inquiry_source(&inquiry(), &wrong, Instant::from_unix_secs(100))
                .is_err()
        );
    }
}
