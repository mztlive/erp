//! 采购二次确认的确定性选源规则。
//!
//! 本模块只计算单条销售提交行的最低可执行落地成本，不访问数据库。应用层负责先把
//! 供给状态、有效期、供应商能力等外部事实过滤成候选项，再调用本规则形成推荐分配。

use rust_decimal::Decimal;

use crate::errors::{Error, Result};
use crate::ids::{SupplierAccountId, SupplierCapabilityRevisionId, SupplierOfferingRevisionId};
use crate::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};

use super::FulfillmentMode;

/// 一条已经通过外部事实校验的供给候选。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcingCandidate {
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 精确供给修订。
    pub offering_revision_id: SupplierOfferingRevisionId,
    /// 精确供应商能力修订。
    pub capability_revision_id: SupplierCapabilityRevisionId,
    /// 本候选的履约方式。
    pub fulfillment_mode: FulfillmentMode,
    /// 未达到集采起订量时的一件代发含税单价。
    pub dropship_unit_cost_gross: UnitPrice,
    /// 达到集采起订量时的集采含税单价。
    pub bulk_unit_cost_gross: UnitPrice,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 启用集采价的最低采购数量。
    pub bulk_minimum_quantity: Quantity,
    /// 最大可分配数量；空值表示当前候选可覆盖本次需求。
    pub available_quantity: Option<Quantity>,
    /// 本候选被采用一次时计入的运费。
    pub freight_amount: Option<Amount>,
    /// 本候选被采用一次时计入的服务费。
    pub service_fee_amount: Option<Amount>,
}

/// 一条销售提交行及其全部可执行候选。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcingLine {
    /// 销售提交行。
    pub submission_line_id: String,
    /// 需要采购覆盖的数量。
    pub required_quantity: Quantity,
    /// 已通过应用层有效性过滤的候选。
    pub candidates: Vec<SourcingCandidate>,
}

/// 推荐方案中的一次供应商分配。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcingAllocation {
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 精确供给修订。
    pub offering_revision_id: SupplierOfferingRevisionId,
    /// 精确供应商能力修订。
    pub capability_revision_id: SupplierCapabilityRevisionId,
    /// 履约方式。
    pub fulfillment_mode: FulfillmentMode,
    /// 分配数量。
    pub quantity: Quantity,
    /// 含税供给单价。
    pub unit_cost_gross: UnitPrice,
    /// 当前分配是否达到起订量并采用集采价。
    pub uses_bulk_price: bool,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 本次分配计入的运费。
    pub freight_amount: Option<Amount>,
    /// 本次分配计入的服务费。
    pub service_fee_amount: Option<Amount>,
    /// 商品价与费用合计后的含税落地成本。
    pub landed_gross: Amount,
}

/// 一条销售提交行的最低成本推荐结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcingLinePlan {
    /// 销售提交行。
    pub submission_line_id: String,
    /// 精确覆盖需求的供应商分配。
    pub allocations: Vec<SourcingAllocation>,
    /// 本行落地成本合计。
    pub landed_gross: Amount,
}

/// 计算单条销售提交行的最低可执行落地成本方案。
///
/// 优先比较可独立覆盖全量需求的候选；同时构造按有效单位落地成本排序的拆分方案，
/// 两者取总成本更低者。每段分配数量达到集采起订量时使用集采价，否则使用一件代发价；
/// 交付方式不参与报价档位判断。
///
/// # 参数
/// * `line` - 需求数量与已通过外部事实过滤的候选
///
/// # 返回
/// 返回确定性最低成本分配；成本相同时按供应商、供给修订和履约方式稳定排序。
///
/// # 错误
/// 需求非正、没有候选或候选组合无法精确覆盖需求时返回错误。
pub fn recommend_sourcing_line(line: &SourcingLine) -> Result<SourcingLinePlan> {
    if line.required_quantity.to_decimal() <= Decimal::ZERO {
        return Err(Error::from("采购需求数量必须为正"));
    }
    if line.candidates.is_empty() {
        return Err(Error::from("没有满足当前供给条件的供应商"));
    }

    let mut plans = single_candidate_plans(line)?;
    plans.extend(split_candidate_plans(line)?);
    plans
        .into_iter()
        .min_by(compare_plans)
        .ok_or_else(|| Error::from("现有供应商可供数量或起订量无法覆盖采购需求"))
}

/// 构造能够独立覆盖全部需求的候选方案。
fn single_candidate_plans(line: &SourcingLine) -> Result<Vec<SourcingLinePlan>> {
    line.candidates
        .iter()
        .filter(|candidate| candidate_covers(candidate, line.required_quantity))
        .map(|candidate| plan_from_candidates(line, std::slice::from_ref(candidate)))
        .collect()
}

/// 构造拆分方案；每个候选分别作为首选项尝试一次，避免固定费用造成单一贪心顺序失真。
fn split_candidate_plans(line: &SourcingLine) -> Result<Vec<SourcingLinePlan>> {
    let mut ranked = line.candidates.clone();
    ranked.sort_by(|left, right| compare_candidate_cost(left, right, line.required_quantity));
    let mut plans = Vec::with_capacity(ranked.len() + 1);
    if let Some(plan) = split_in_order(line, &ranked)? {
        plans.push(plan);
    }
    for seed in 0..ranked.len() {
        let mut order = Vec::with_capacity(ranked.len());
        order.push(ranked[seed].clone());
        order.extend(
            ranked
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != seed)
                .map(|(_, candidate)| candidate.clone()),
        );
        if let Some(plan) = split_in_order(line, &order)? {
            plans.push(plan);
        }
    }
    Ok(plans)
}

/// 按给定候选顺序分配，只有精确覆盖时才返回方案。
fn split_in_order(line: &SourcingLine, candidates: &[SourcingCandidate]) -> Result<Option<SourcingLinePlan>> {
    let mut remaining = line.required_quantity.to_decimal();
    let mut selected = Vec::new();
    for candidate in candidates {
        if remaining <= Decimal::ZERO {
            break;
        }
        let capacity = candidate
            .available_quantity
            .map(Quantity::to_decimal)
            .unwrap_or(line.required_quantity.to_decimal())
            .min(remaining);
        if capacity <= Decimal::ZERO {
            continue;
        }
        selected.push((candidate, Quantity::try_from(capacity)?));
        remaining -= capacity;
    }
    if remaining > Decimal::ZERO {
        return Ok(None);
    }
    plan_from_allocations(line, &selected).map(Some)
}

/// 由一组全量候选构造计划。
fn plan_from_candidates(line: &SourcingLine, candidates: &[SourcingCandidate]) -> Result<SourcingLinePlan> {
    let allocations = candidates
        .iter()
        .map(|candidate| (candidate, line.required_quantity))
        .collect::<Vec<_>>();
    plan_from_allocations(line, &allocations)
}

/// 由确定数量的候选分配构造计划并汇总费用。
fn plan_from_allocations(
    line: &SourcingLine,
    allocations: &[(&SourcingCandidate, Quantity)],
) -> Result<SourcingLinePlan> {
    let mut landed_gross = Amount::try_from(Decimal::ZERO)?;
    let mut built = Vec::with_capacity(allocations.len());
    for (candidate, quantity) in allocations {
        let allocation = allocation(candidate, *quantity)?;
        landed_gross = landed_gross.checked_add(allocation.landed_gross);
        built.push(allocation);
    }
    built.sort_by(allocation_order);
    Ok(SourcingLinePlan {
        submission_line_id: line.submission_line_id.clone(),
        allocations: built,
        landed_gross,
    })
}

/// 将一个候选与数量转为正式分配。
fn allocation(candidate: &SourcingCandidate, quantity: Quantity) -> Result<SourcingAllocation> {
    let uses_bulk_price = quantity.to_decimal() >= candidate.bulk_minimum_quantity.to_decimal();
    let unit_cost_gross = price_for_quantity(candidate, quantity);
    let (product_gross, _, _) = line_amounts(unit_cost_gross, quantity, candidate.input_tax_rate);
    let landed_gross = optional_amounts(candidate)
        .into_iter()
        .fold(product_gross, Amount::checked_add);
    Ok(SourcingAllocation {
        supplier_id: candidate.supplier_id.clone(),
        offering_revision_id: candidate.offering_revision_id.clone(),
        capability_revision_id: candidate.capability_revision_id.clone(),
        fulfillment_mode: candidate.fulfillment_mode,
        quantity,
        unit_cost_gross,
        uses_bulk_price,
        input_tax_rate: candidate.input_tax_rate,
        freight_amount: candidate.freight_amount,
        service_fee_amount: candidate.service_fee_amount,
        landed_gross,
    })
}

/// 判断候选能否独立满足数量。
fn candidate_covers(candidate: &SourcingCandidate, quantity: Quantity) -> bool {
    candidate
        .available_quantity
        .is_none_or(|available| available.to_decimal() >= quantity.to_decimal())
}

/// 根据分配给当前供应商的实际采购数量选择报价档位。
fn price_for_quantity(candidate: &SourcingCandidate, quantity: Quantity) -> UnitPrice {
    if quantity.to_decimal() >= candidate.bulk_minimum_quantity.to_decimal() {
        candidate.bulk_unit_cost_gross
    } else {
        candidate.dropship_unit_cost_gross
    }
}

/// 返回候选的一次性费用。
fn optional_amounts(candidate: &SourcingCandidate) -> impl Iterator<Item = Amount> + '_ {
    candidate
        .freight_amount
        .into_iter()
        .chain(candidate.service_fee_amount)
}

/// 按最大可用量折算候选有效单位成本并提供稳定次序。
fn compare_candidate_cost(
    left: &SourcingCandidate,
    right: &SourcingCandidate,
    required: Quantity,
) -> std::cmp::Ordering {
    effective_unit_cost(left, required)
        .cmp(&effective_unit_cost(right, required))
        .then_with(|| candidate_key(left).cmp(&candidate_key(right)))
}

/// 计算排序用有效单位落地成本。
fn effective_unit_cost(candidate: &SourcingCandidate, required: Quantity) -> Decimal {
    let quantity = candidate
        .available_quantity
        .map(Quantity::to_decimal)
        .unwrap_or(required.to_decimal())
        .min(required.to_decimal());
    if quantity <= Decimal::ZERO {
        return Decimal::MAX;
    }
    let quantity = Quantity::try_from(quantity).expect("候选排序数量已经校验为正数");
    let product = price_for_quantity(candidate, quantity).to_decimal() * quantity.to_decimal();
    let fees = optional_amounts(candidate).fold(Decimal::ZERO, |sum, fee| sum + fee.to_decimal());
    (product + fees) / quantity.to_decimal()
}

/// 按总成本与稳定分配次序比较计划。
fn compare_plans(left: &SourcingLinePlan, right: &SourcingLinePlan) -> std::cmp::Ordering {
    left.landed_gross
        .cmp(&right.landed_gross)
        .then_with(|| plan_key(left).cmp(&plan_key(right)))
}

/// 候选稳定键。
fn candidate_key(candidate: &SourcingCandidate) -> (String, String, &'static str) {
    (
        candidate.supplier_id.to_string(),
        candidate.offering_revision_id.to_string(),
        candidate.fulfillment_mode.as_str(),
    )
}

/// 分配稳定排序。
fn allocation_order(left: &SourcingAllocation, right: &SourcingAllocation) -> std::cmp::Ordering {
    (
        left.supplier_id.to_string(),
        left.offering_revision_id.to_string(),
        left.fulfillment_mode.as_str(),
    )
        .cmp(&(
            right.supplier_id.to_string(),
            right.offering_revision_id.to_string(),
            right.fulfillment_mode.as_str(),
        ))
}

/// 计划稳定键。
fn plan_key(plan: &SourcingLinePlan) -> Vec<(String, String, &'static str)> {
    plan.allocations
        .iter()
        .map(|allocation| {
            (
                allocation.supplier_id.to_string(),
                allocation.offering_revision_id.to_string(),
                allocation.fulfillment_mode.as_str(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn candidate(
        supplier: &str,
        mode: FulfillmentMode,
        dropship_price: &str,
        bulk_price: &str,
        bulk_minimum: &str,
        available: Option<&str>,
        freight: Option<&str>,
    ) -> SourcingCandidate {
        SourcingCandidate {
            supplier_id: SupplierAccountId::new(supplier),
            offering_revision_id: SupplierOfferingRevisionId::new(format!("offering-{supplier}")),
            capability_revision_id: SupplierCapabilityRevisionId::new(format!("capability-{supplier}")),
            fulfillment_mode: mode,
            dropship_unit_cost_gross: UnitPrice::from_str(dropship_price).unwrap(),
            bulk_unit_cost_gross: UnitPrice::from_str(bulk_price).unwrap(),
            input_tax_rate: Rate::from_str("0.130000").unwrap(),
            bulk_minimum_quantity: Quantity::from_str(bulk_minimum).unwrap(),
            available_quantity: available.map(|value| Quantity::from_str(value).unwrap()),
            freight_amount: freight.map(|value| Amount::from_str(value).unwrap()),
            service_fee_amount: None,
        }
    }

    #[test]
    fn recommendation_uses_landed_cost_instead_of_unit_price_only() {
        let line = SourcingLine {
            submission_line_id: "line-1".to_string(),
            required_quantity: Quantity::from_str("10").unwrap(),
            candidates: vec![
                candidate(
                    "cheap-unit-high-freight",
                    FulfillmentMode::CompanyWarehouse,
                    "8.0000",
                    "8.0000",
                    "10",
                    None,
                    Some("50.00"),
                ),
                candidate(
                    "best-landed",
                    FulfillmentMode::SupplierDirect,
                    "10.0000",
                    "10.0000",
                    "10",
                    None,
                    None,
                ),
            ],
        };

        let plan = recommend_sourcing_line(&line).unwrap();
        assert_eq!(plan.allocations[0].supplier_id.as_ref(), "best-landed");
        assert_eq!(plan.landed_gross.to_string(), "100.00");
    }

    #[test]
    fn recommendation_uses_dropship_price_below_bulk_minimum_regardless_of_mode() {
        let line = SourcingLine {
            submission_line_id: "line-1".to_string(),
            required_quantity: Quantity::from_str("3").unwrap(),
            candidates: vec![
                candidate(
                    "warehouse",
                    FulfillmentMode::CompanyWarehouse,
                    "5.0000",
                    "1.0000",
                    "10",
                    None,
                    None,
                ),
                candidate(
                    "direct",
                    FulfillmentMode::SupplierDirect,
                    "6.0000",
                    "2.0000",
                    "10",
                    None,
                    None,
                ),
            ],
        };

        let plan = recommend_sourcing_line(&line).unwrap();
        assert_eq!(plan.allocations[0].supplier_id.as_ref(), "warehouse");
        assert_eq!(plan.allocations[0].unit_cost_gross.to_string(), "5.0000");
    }

    #[test]
    fn recommendation_uses_bulk_price_at_minimum_regardless_of_mode() {
        let line = SourcingLine {
            submission_line_id: "line-1".to_string(),
            required_quantity: Quantity::from_str("10").unwrap(),
            candidates: vec![candidate(
                "direct",
                FulfillmentMode::SupplierDirect,
                "6.0000",
                "2.0000",
                "10",
                None,
                None,
            )],
        };

        let plan = recommend_sourcing_line(&line).unwrap();
        assert_eq!(plan.allocations[0].unit_cost_gross.to_string(), "2.0000");
        assert_eq!(plan.landed_gross.to_string(), "20.00");
    }

    #[test]
    fn recommendation_splits_when_no_supplier_covers_the_full_quantity() {
        let line = SourcingLine {
            submission_line_id: "line-1".to_string(),
            required_quantity: Quantity::from_str("10").unwrap(),
            candidates: vec![
                candidate(
                    "first",
                    FulfillmentMode::SupplierDirect,
                    "4.0000",
                    "4.0000",
                    "10",
                    Some("6"),
                    None,
                ),
                candidate(
                    "second",
                    FulfillmentMode::SupplierDirect,
                    "5.0000",
                    "5.0000",
                    "10",
                    Some("4"),
                    None,
                ),
            ],
        };

        let plan = recommend_sourcing_line(&line).unwrap();
        assert_eq!(plan.allocations.len(), 2);
        assert_eq!(plan.landed_gross.to_string(), "44.00");
        assert!(plan
            .allocations
            .iter()
            .all(|allocation| allocation.unit_cost_gross.to_decimal() >= Decimal::from(4)));
    }

    #[test]
    fn recommendation_fails_when_capacity_cannot_cover_demand() {
        let line = SourcingLine {
            submission_line_id: "line-1".to_string(),
            required_quantity: Quantity::from_str("10").unwrap(),
            candidates: vec![candidate(
                "short",
                FulfillmentMode::SupplierDirect,
                "4.0000",
                "3.0000",
                "1",
                Some("3"),
                None,
            )],
        };

        assert!(recommend_sourcing_line(&line).is_err());
    }
}
