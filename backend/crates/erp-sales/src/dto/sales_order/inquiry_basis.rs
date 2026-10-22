//! 销售询价开单依据，仅包含已接受的对销售需求与结果。
use erp_core::common::time::Instant;
use erp_core::money::Quantity;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::entity::sales_inquiry::{
    InquiryQuantitySelection, InquiryRequirementData, InquiryRequirementLine, InquiryReservationState,
    InquirySupplyPrice, SalesInquiry,
};
use crate::entity::sales_order::inquiry_source::{InquirySalesAllocation, InquirySalesLineSelection};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InquirySalesBasisLine {
    pub requirement: InquiryRequirementLine,
    pub price: InquirySupplyPrice,
    pub selection: InquirySalesLineSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InquirySalesBasis {
    pub inquiry_id: String,
    pub inquiry_no: String,
    pub requirement_version: u32,
    pub feedback_round: u32,
    pub customer_id: String,
    pub customer_name: String,
    pub lines: Vec<InquirySalesBasisLine>,
}

impl InquirySalesBasis {
    /// 按当前已接受反馈生成可转数量及精确价格段，不返回采购内部身份。
    /// # 参数
    /// `inquiry` 为当前已授权聚合；`round` 为明确轮次；`now` 为服务端时间。
    /// # 返回
    /// 返回尚有可转量的商品、地址、价格及对应来源选择。
    /// # 错误
    /// 未接受、需求损坏、数量溢出或全部失效时返回业务错误。
    pub fn from_inquiry(inquiry: &SalesInquiry, round: u32, now: Instant) -> Result<Self> {
        let feedback = inquiry.accepted_feedback(round, now)?;
        let requirement = requirement(inquiry, feedback.requirement_version)?;
        let mut lines = Vec::new();
        for result in &feedback.data.lines {
            if !transferable_line(inquiry, &result.line_id)? {
                continue;
            }
            let demand = requirement
                .lines
                .iter()
                .find(|line| line.line_id == result.line_id)
                .ok_or_else(|| Error::BusinessLogicError("询价反馈需求行不存在".into()))?;
            for price in &result.prices {
                if price.valid_until <= now {
                    continue;
                }
                let mut selection =
                    available_selection(inquiry, round, demand, price, result.available_quantity)?;
                selection.line_no = u32::try_from(lines.len() + 1)
                    .map_err(|_| Error::ValidationError("销售行数超出范围".into()))?;
                if selection.quantity.to_decimal() <= Decimal::ZERO
                    || (!price.allow_cumulative && selection.quantity < price.min_quantity)
                {
                    continue;
                }
                lines.push(InquirySalesBasisLine {
                    requirement: demand.clone(),
                    price: price.clone(),
                    selection,
                });
            }
        }
        if lines.is_empty() {
            return Err(Error::BusinessLogicError(
                "当前已接受结果没有可转数量，或报价已过期，请重新询价".into(),
            ));
        }
        Ok(Self {
            inquiry_id: inquiry.base.id.clone(),
            inquiry_no: inquiry.inquiry_no.clone(),
            requirement_version: feedback.requirement_version,
            feedback_round: round,
            customer_id: inquiry.customer_id.clone(),
            customer_name: inquiry.customer_name.clone(),
            lines,
        })
    }
}

fn requirement(inquiry: &SalesInquiry, version: u32) -> Result<&InquiryRequirementData> {
    inquiry
        .requirements
        .iter()
        .find(|entry| entry.version == version)
        .map(|entry| &entry.data)
        .ok_or_else(|| Error::BusinessLogicError("已接受询价缺少原需求".into()))
}

fn transferable_line(inquiry: &SalesInquiry, line_id: &str) -> Result<bool> {
    match inquiry.ensure_line_not_reinquiring(line_id) {
        Ok(()) => Ok(true),
        Err(Error::BusinessLogicError(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

fn used(
    inquiry: &SalesInquiry,
    line: &str,
    price: Option<&str>,
    allocation: Option<&str>,
) -> Result<Decimal> {
    inquiry
        .quantity_ledger
        .iter()
        .filter(|entry| entry.state != InquiryReservationState::Released)
        .flat_map(|entry| &entry.selections)
        .filter(|entry| matches_selection(entry, line, price, allocation))
        .try_fold(Decimal::ZERO, |sum, entry| {
            sum.checked_add(entry.quantity.to_decimal())
                .ok_or_else(|| Error::ValidationError("询价占用数量超出范围".into()))
        })
}
fn matches_selection(
    row: &InquiryQuantitySelection,
    line: &str,
    price: Option<&str>,
    allocation: Option<&str>,
) -> bool {
    row.line_id == line
        && price.is_none_or(|id| row.price_id.as_deref() == Some(id))
        && allocation.is_none_or(|id| row.allocation_id.as_deref() == Some(id))
}
fn remaining(total: Quantity, used: Decimal) -> Decimal {
    (total.to_decimal() - used).max(Decimal::ZERO)
}

fn available_selection(
    inquiry: &SalesInquiry,
    round: u32,
    demand: &InquiryRequirementLine,
    price: &InquirySupplyPrice,
    line_available: Quantity,
) -> Result<InquirySalesLineSelection> {
    let mut available = remaining(line_available, used(inquiry, &demand.line_id, None, None)?)
        .min(remaining(price.quantity, used(inquiry, &demand.line_id, Some(&price.price_id), None)?));
    if let Some(maximum) = price.max_quantity {
        available = available.min(maximum.to_decimal());
    }
    let mut allocations = Vec::new();
    for offered in &price.allocations {
        let all_offered = address_offered(inquiry, round, &demand.line_id, &offered.allocation_id)?;
        let all_remaining = (all_offered
            - used(inquiry, &demand.line_id, None, Some(&offered.allocation_id))?)
        .max(Decimal::ZERO);
        let amount = remaining(
            offered.quantity,
            used(inquiry, &demand.line_id, Some(&price.price_id), Some(&offered.allocation_id))?,
        )
        .min(all_remaining)
        .min(available);
        if amount > Decimal::ZERO {
            allocations.push(InquirySalesAllocation {
                allocation_id: offered.allocation_id.clone(),
                quantity: Quantity::try_from(amount)?,
            });
            available -= amount;
        }
    }
    let quantity = if price.allocations.is_empty() {
        available
    } else {
        allocations.iter().try_fold(Decimal::ZERO, |sum, row| {
            sum.checked_add(row.quantity.to_decimal())
                .ok_or_else(|| Error::ValidationError("本次配送数量超出范围".into()))
        })?
    };
    Ok(InquirySalesLineSelection {
        line_no: 0,
        inquiry_line_id: demand.line_id.clone(),
        price_id: price.price_id.clone(),
        quantity: Quantity::try_from(quantity)?,
        allocations,
    })
}

fn address_offered(
    inquiry: &SalesInquiry,
    round: u32,
    line_id: &str,
    allocation_id: &str,
) -> Result<Decimal> {
    inquiry
        .feedbacks
        .iter()
        .find(|feedback| feedback.round == round)
        .and_then(|round| round.data.lines.iter().find(|line| line.line_id == line_id))
        .map(|line| {
            line.prices
                .iter()
                .flat_map(|price| &price.allocations)
                .filter(|row| row.allocation_id == allocation_id)
                .map(|row| row.quantity.to_decimal())
                .try_fold(Decimal::ZERO, |sum, value| {
                    sum.checked_add(value)
                        .ok_or_else(|| Error::ValidationError("询价地址可供数量超出范围".into()))
                })
        })
        .transpose()
        .map(|total| total.unwrap_or(Decimal::ZERO))
}

#[cfg(test)]
mod tests {
    use erp_core::region::RegionCode;

    use super::*;
    use crate::entity::sales_inquiry::{
        InquiryCreateData, InquiryDeliveryAllocation, InquiryFeedbackData, InquiryFeedbackLine,
        InquiryFulfillmentKind, InquiryReinquiryData, InquiryRequirementData, InquirySupplyAllocation,
        InquirySupplyConclusion,
    };

    fn now() -> Instant {
        Instant::from_unix_secs(100)
    }

    fn demand(id: &str) -> InquiryRequirementLine {
        InquiryRequirementLine {
            line_id: id.into(),
            sku_id: format!("sku-{id}"),
            sku_revision_id: format!("revision-{id}"),
            sku_code: format!("000-{id}"),
            product_name: format!("商品 {id}"),
            specification: "盒".into(),
            unit_code: "盒".into(),
            quantity_scale: 0,
            quantity: "10".parse().unwrap(),
            fulfillment_kind: InquiryFulfillmentKind::Physical,
            delivery_due_at: Instant::from_unix_secs(1000),
            delivery_terms: String::new(),
            sales_reference_price_gross: None,
            allocations: vec![InquiryDeliveryAllocation {
                allocation_id: format!("address-{id}"),
                recipient: "收货人".into(),
                phone: "0013800000000".into(),
                region_code: RegionCode::parse_leaf("310115").unwrap(),
                region_name: String::new(),
                address: format!("测试路 {id} 号"),
                quantity: "10".parse().unwrap(),
            }],
        }
    }

    fn feedback_line(id: &str, quantity: &str, price_id: &str) -> InquiryFeedbackLine {
        InquiryFeedbackLine {
            line_id: id.into(),
            conclusion: if quantity == "10" {
                InquirySupplyConclusion::Available
            } else {
                InquirySupplyConclusion::PartiallyAvailable
            },
            available_quantity: quantity.parse().unwrap(),
            prices: vec![InquirySupplyPrice {
                price_id: price_id.into(),
                quantity: quantity.parse().unwrap(),
                unit_price_gross: Some("8".parse().unwrap()),
                tax_rate: Some("0.13".parse().unwrap()),
                delivery_due_at: Instant::from_unix_secs(1000),
                valid_until: Instant::from_unix_secs(900),
                min_quantity: "1".parse().unwrap(),
                max_quantity: None,
                allow_cumulative: false,
                allocations: vec![InquirySupplyAllocation {
                    allocation_id: format!("address-{id}"),
                    quantity: quantity.parse().unwrap(),
                }],
                included_cost_note: String::new(),
            }],
            public_note: "其余数量待确认".into(),
        }
    }

    fn feedback(b_quantity: &str, b_price_id: &str) -> InquiryFeedbackData {
        InquiryFeedbackData {
            lines: vec![feedback_line("a", "10", "price-a"), feedback_line("b", b_quantity, b_price_id)],
            fees: Vec::new(),
        }
    }

    fn accepted() -> SalesInquiry {
        let mut inquiry = SalesInquiry::create(
            "inquiry".into(),
            "IQ-1".into(),
            InquiryCreateData {
                customer_id: "customer".into(),
                customer_name: "客户".into(),
                sales_owner_user_id: "sales".into(),
                business_org_unit_id: "org".into(),
                deadline_at: Instant::from_unix_secs(500),
                copied_from_inquiry_id: None,
                draft: InquiryRequirementData { lines: vec![demand("a"), demand("b")], note: String::new() },
            },
        )
        .unwrap();
        inquiry.submit(1, "sales", String::new(), now()).unwrap();
        inquiry.publish(1, "buyer", feedback("10", "price-b"), now()).unwrap();
        inquiry.accept(1, "sales", 1, now()).unwrap();
        inquiry
            .reserve_quantities(
                "existing-sale",
                "existing-sale",
                1,
                vec![InquiryQuantitySelection {
                    line_id: "b".into(),
                    allocation_id: Some("address-b".into()),
                    price_id: Some("price-b".into()),
                    quantity: "4".parse().unwrap(),
                }],
                now(),
            )
            .unwrap();
        inquiry
    }

    fn reinquire_a(inquiry: &mut SalesInquiry) {
        inquiry
            .reinquire(
                1,
                "sales",
                InquiryReinquiryData {
                    reason: "重新确认 A 的供给价".into(),
                    line_ids: vec!["a".into()],
                    fee_ids: Vec::new(),
                    concerns: "价格".into(),
                    requirement: None,
                },
                now(),
            )
            .unwrap();
    }

    fn assert_only_original_b(inquiry: &SalesInquiry) {
        let basis = InquirySalesBasis::from_inquiry(inquiry, 1, now()).unwrap();
        assert_eq!(basis.feedback_round, 1);
        assert_eq!(basis.lines.len(), 1);
        assert_eq!(basis.lines[0].requirement.line_id, "b");
        assert_eq!(basis.lines[0].price.price_id, "price-b");
        assert_eq!(basis.lines[0].selection.quantity, "6".parse().unwrap());
        assert_eq!(basis.lines[0].selection.allocations[0].quantity, "6".parse().unwrap());
    }

    #[test]
    fn partial_reinquiry_basis_keeps_unaffected_old_acceptance_until_new_round_is_accepted() {
        let mut inquiry = accepted();
        reinquire_a(&mut inquiry);
        assert_only_original_b(&inquiry);
        inquiry.publish(1, "buyer", feedback("8", "price-b-new"), now()).unwrap();
        assert_only_original_b(&inquiry);
        assert!(InquirySalesBasis::from_inquiry(&inquiry, 2, now()).is_err());
        inquiry.accept(1, "sales", 2, now()).unwrap();
        assert!(InquirySalesBasis::from_inquiry(&inquiry, 1, now()).is_err());
        let basis = InquirySalesBasis::from_inquiry(&inquiry, 2, now()).unwrap();
        assert_eq!(basis.lines.len(), 2);
        let b = basis.lines.iter().find(|line| line.requirement.line_id == "b").unwrap();
        assert_eq!(b.price.price_id, "price-b-new");
        assert_eq!(b.selection.quantity, "4".parse().unwrap());
        assert_eq!(b.selection.allocations[0].quantity, "4".parse().unwrap());
    }

    #[test]
    fn requirement_revision_blocks_every_old_basis_even_for_unchanged_lines() {
        let mut inquiry = accepted();
        let mut requirement = inquiry.current_requirement().unwrap().data.clone();
        requirement.lines[0].delivery_terms = "新增交付要求".into();
        inquiry
            .reinquire(
                1,
                "sales",
                InquiryReinquiryData {
                    reason: "修改 A 的交付要求".into(),
                    line_ids: vec!["a".into()],
                    fee_ids: Vec::new(),
                    concerns: "交付要求".into(),
                    requirement: Some(requirement),
                },
                now(),
            )
            .unwrap();
        assert_eq!(inquiry.requirement_version, 2);
        assert!(InquirySalesBasis::from_inquiry(&inquiry, 1, now()).is_err());
    }
    #[test]
    fn feedback_draft_numbers_roundtrip_through_bson_without_zero_substitution() {
        let mut data = feedback("10", "price-b");
        data.lines[0].prices[0].unit_price_gross = None;
        data.lines[0].prices[0].tax_rate = None;
        data.lines[1].prices[0].unit_price_gross = Some("0".parse().unwrap());
        data.lines[1].prices[0].tax_rate = Some("0".parse().unwrap());
        let bytes = bson::serialize_to_vec(&data).unwrap();
        let restored: InquiryFeedbackData = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(restored, data);
        assert_eq!(restored.lines[0].prices[0].unit_price_gross, None);
        assert_eq!(restored.lines[1].prices[0].unit_price_gross, Some("0".parse().unwrap()));
    }
}
