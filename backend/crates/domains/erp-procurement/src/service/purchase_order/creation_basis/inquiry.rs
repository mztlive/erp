//! 专项报价候选沿同一采购依据入口分组，成本与通用条款分离。
use erp_core::ids::SupplierOfferingId;
use erp_core::money::Quantity;
use rust_decimal::Decimal;

use super::mapping::settlement_terms_for;
use crate::entity::facts::{AvailabilityStatus, SalesOrderBasisFact};
use crate::entity::purchase_order::{
    BasisGroup, BasisLine, BasisScope, CreationBasisFacts, LineSupply, PurchaseInquiryAddress,
    PurchaseInquiryQuoteSource, SalesProcurementCoverage, SalesProcurementCoverageLine, SupplyPriceBasis,
    business_date_of, maximum_create_quantity, purchase_options, purchase_type_from_product_kind,
};
use crate::{Error, Result};

/// 由跨域读模型核验销售需求来源与接受记录后提供的专项候选。
#[derive(Debug, Clone)]
pub struct InquiryBasisCandidate {
    pub sales_order_line_id: String,
    pub supplier_offering_id: SupplierOfferingId,
    pub source: PurchaseInquiryQuoteSource,
    pub remaining_quote_quantity: Quantity,
    pub remaining_address_limits: Vec<PurchaseInquiryAddress>,
}

/// 把只属于当前销售询价来源的专项候选加入同一正式创建依据。
/// # 参数
/// 销售生效事实、最新覆盖、当前任务行、批量供给及服务端来源候选。
/// # 返回
/// 返回按专项报价版本及既有拆单维度隔离的依据。
/// # 错误
/// 条款身份、商品类型或数量非法时拒绝。
pub fn inquiry_basis_groups(
    order: &SalesOrderBasisFact,
    coverage: &SalesProcurementCoverage,
    responsibility: &[String],
    facts: &CreationBasisFacts,
    candidates: &[InquiryBasisCandidate],
) -> Result<Vec<BasisGroup>> {
    if !order.is_effective {
        return Ok(vec![]);
    }
    let mut groups = Vec::new();
    for candidate in candidates {
        candidate.source.validate()?;
        let Some(line) = coverage.lines.iter().find(|line| {
            line.revision_line.sales_order_line_id.as_ref() == candidate.sales_order_line_id
                && responsibility.iter().any(|id| id == &candidate.sales_order_line_id)
        }) else {
            continue;
        };
        if candidate.source.expected_delivery_date > business_date_of(line.goods_line.fulfillment_due_at)? {
            continue;
        }
        let Some(supply) = special_supply(facts, &candidate.supplier_offering_id) else { continue };
        if supply.offering.sku_id != line.goods_line.sku_id {
            continue;
        }
        let max = maximum_create_quantity(
            line.summary.remaining_quantity,
            Some(candidate.remaining_quote_quantity),
        )?
        .min(supply.availability.available_quantity.unwrap_or(candidate.remaining_quote_quantity));
        let max = address_maximum(line, candidate, max)?;
        if max.to_decimal() <= Decimal::ZERO {
            continue;
        }
        append_groups(coverage, line, &supply, candidate, max, facts, &mut groups)?;
    }
    Ok(groups)
}
fn special_supply(facts: &CreationBasisFacts, id: &SupplierOfferingId) -> Option<LineSupply> {
    let offering = facts.offerings.iter().find(|offering| offering.base.id == id.as_ref())?;
    let revision = facts.revisions.get(offering.stable.current_revision_id.as_ref()?)?;
    let availability = facts.availabilities.get(&offering.base.id)?;
    if availability.availability_status != AvailabilityStatus::Available {
        return None;
    }
    Some(LineSupply {
        offering: offering.clone(),
        revision: revision.clone(),
        availability: availability.clone(),
        delivery: facts.deliveries.get(&offering.base.id).cloned().unwrap_or_default(),
    })
}

fn append_groups(
    coverage: &SalesProcurementCoverage,
    line: &SalesProcurementCoverageLine,
    supply: &LineSupply,
    candidate: &InquiryBasisCandidate,
    max: Quantity,
    facts: &CreationBasisFacts,
    groups: &mut Vec<BasisGroup>,
) -> Result<()> {
    let settlement = settlement_terms_for(facts, &supply.offering.supplier_id);
    for &(fulfillment_responsibility, price_basis) in purchase_options(line.product_kind)? {
        if price_basis == SupplyPriceBasis::Bulk
            && (supply.revision.bulk_supply_price_gross.is_none()
                || supply.revision.bulk_minimum_order_quantity.is_none())
        {
            continue;
        }
        let scope = BasisScope {
            inquiry_quote_revision_id: Some(candidate.source.quote_revision_id.clone()),
            supplier_id: supply.offering.supplier_id.clone(),
            purchase_type: purchase_type_from_product_kind(line.product_kind)?,
            payment_term_code: settlement.payment_term_code.clone(),
            fulfillment_responsibility,
            price_basis,
        };
        let basis = BasisLine {
            coverage: line.clone(),
            supply: supply.clone(),
            max_create_quantity: max,
            inquiry_quote: Some(candidate.source.clone()),
        };
        groups.push(BasisGroup {
            revision: coverage.revision.clone(),
            scope,
            business_category: settlement.business_category.clone(),
            lines: vec![basis],
        });
    }
    Ok(())
}

fn address_maximum(
    line: &SalesProcurementCoverageLine,
    candidate: &InquiryBasisCandidate,
    max: Quantity,
) -> Result<Quantity> {
    if candidate.source.address_limits.is_empty() {
        return Ok(max);
    }
    let mut total = Decimal::ZERO;
    for row in &line.goods_line.delivery_allocations {
        let fingerprint = PurchaseInquiryAddress::fingerprint(
            &row.recipient,
            &row.phone,
            row.region_code.as_str(),
            &row.address,
        );
        let Some(limit) =
            candidate.remaining_address_limits.iter().find(|limit| limit.address_fingerprint == fingerprint)
        else {
            continue;
        };
        let assigned = line
            .goods_line
            .assigned_delivery_allocations
            .iter()
            .filter(|assigned| {
                PurchaseInquiryAddress::fingerprint(
                    &assigned.recipient,
                    &assigned.phone,
                    assigned.region_code.as_str(),
                    &assigned.address,
                ) == fingerprint
            })
            .try_fold(Decimal::ZERO, |sum, assigned| {
                sum.checked_add(assigned.quantity.to_decimal())
                    .ok_or_else(|| Error::ValidationError("已分配地址数量超出范围".into()))
            })?;
        let remaining = row
            .quantity
            .to_decimal()
            .checked_sub(assigned)
            .ok_or_else(|| Error::ValidationError("地址剩余数量超出范围".into()))?
            .max(Decimal::ZERO);
        total = total
            .checked_add(remaining.min(limit.quantity.to_decimal()))
            .ok_or_else(|| Error::ValidationError("专项地址剩余数量超出范围".into()))?;
    }
    Ok(max.min(Quantity::try_from(total)?))
}
