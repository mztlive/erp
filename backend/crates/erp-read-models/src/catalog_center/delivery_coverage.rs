//! 销售录入收货地址时的配送提示：按 SKU 全部有效且可供供给的配送模板判定，不暴露供应商。

use std::collections::HashMap;

use erp_core::common::time::BusinessDate;
use erp_core::delivery_policy::{DeliveryMode, DeliveryPolicy, DeliveryTerm};
use erp_core::ids::SkuId;
use erp_core::region::RegionCode;
use erp_procurement::entity::facts::AvailabilityStatus;
use erp_procurement::entity::purchase_order::CreationBasisFacts;
use mongodb::Database;
use persistence_core::NoTransaction;
use serde::{Deserialize, Serialize};

use crate::purchase_center::repository::load_creation_basis_facts;
use crate::{Error, Result};

const MAX_LINES: usize = 100;
const MAX_REGIONS_PER_LINE: usize = 100;

/// 一条商品的收货地区。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryCoverageLineInput {
    /// 公司 SKU。
    pub sku_id: String,
    /// 收货地区代码。
    #[serde(default)]
    pub region_codes: Vec<String>,
}

/// 配送提示请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryCoverageRequest {
    /// 商品与收货地区。
    pub lines: Vec<DeliveryCoverageLineInput>,
}

/// 配送提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryCoverageHint {
    /// 无需提示。
    None,
    /// 所有有效供给的代发与集采都不配送。
    NoSupply,
    /// 代发均不配送，集采只能运费另计。
    BulkFreightExtraOnly,
}

/// 一个商品、地区的提示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeliveryCoverageItemView {
    /// 公司 SKU。
    pub sku_id: String,
    /// 收货地区代码。
    pub region_code: String,
    /// 提示。
    pub hint: DeliveryCoverageHint,
}

/// 配送提示结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeliveryCoverageView {
    /// 逐商品、地区的提示。
    pub items: Vec<DeliveryCoverageItemView>,
}

/// 按有效供给的配送政策判定一个地区的提示。
///
/// # 参数
/// * `policies` - 该 SKU 全部有效且可供供给的生效政策
/// * `region` - 收货地区
///
/// # 返回
/// 没有有效供给时不提示（由销售资格另行约束）；代发与集采都不配送时提示无可配送供给；
/// 代发都不配送且集采只能运费另计时提示需另付运费。
///
/// # 错误
/// 无。
pub fn coverage_hint(policies: &[&DeliveryPolicy], region: &RegionCode) -> DeliveryCoverageHint {
    if policies.is_empty()
        || policies.iter().any(|policy| policy.judge(DeliveryMode::Dropship, region).is_deliverable())
    {
        return DeliveryCoverageHint::None;
    }
    let bulk = policies.iter().map(|policy| policy.judge(DeliveryMode::Bulk, region)).collect::<Vec<_>>();
    if bulk.iter().all(|term| *term == DeliveryTerm::NotDeliverable) {
        DeliveryCoverageHint::NoSupply
    } else if bulk.contains(&DeliveryTerm::FreightIncluded) {
        DeliveryCoverageHint::None
    } else {
        DeliveryCoverageHint::BulkFreightExtraOnly
    }
}

/// 查询销售收货地区的配送提示。
///
/// # 参数
/// * `db` - 数据库
/// * `req` - 商品与收货地区
///
/// # 返回
/// 返回逐商品、地区的提示，地区去重且保持输入顺序。
///
/// # 错误
/// 行数或地区数超限、地区代码非法或供给事实读取失败时返回错误。
pub async fn sellable_delivery_coverage(
    db: &Database,
    req: DeliveryCoverageRequest,
) -> Result<DeliveryCoverageView> {
    if req.lines.len() > MAX_LINES
        || req.lines.iter().any(|line| line.region_codes.len() > MAX_REGIONS_PER_LINE)
    {
        return Err(Error::ValidationError("配送提示的商品或地址数量超出上限".into()));
    }
    let sku_ids = req.lines.iter().map(|line| SkuId::new(line.sku_id.trim())).collect::<Vec<_>>();
    let facts = load_creation_basis_facts(db, &sku_ids, &mut NoTransaction).await?;
    let policies = effective_policies(&facts, BusinessDate::today());
    let mut items = Vec::new();
    for line in &req.lines {
        let sku_id = line.sku_id.trim();
        let line_policies = policies.get(sku_id).map(Vec::as_slice).unwrap_or_default();
        let mut seen = Vec::new();
        for code in &line.region_codes {
            let region =
                RegionCode::parse(code).map_err(|error| Error::ValidationError(error.to_string()))?;
            if seen.contains(&region) {
                continue;
            }
            items.push(DeliveryCoverageItemView {
                sku_id: sku_id.to_string(),
                region_code: region.to_string(),
                hint: coverage_hint(line_policies, &region),
            });
            seen.push(region);
        }
    }
    Ok(DeliveryCoverageView { items })
}

/// 按 SKU 归集有效且当前可供供给的生效配送政策。
fn effective_policies(
    facts: &CreationBasisFacts,
    today: BusinessDate,
) -> HashMap<String, Vec<&DeliveryPolicy>> {
    let mut policies: HashMap<String, Vec<&DeliveryPolicy>> = HashMap::new();
    for offering in &facts.offerings {
        let Some(revision) =
            offering.stable.current_revision_id.as_ref().and_then(|id| facts.revisions.get(id))
        else {
            continue;
        };
        let valid = revision.valid_from <= today && revision.valid_to.is_none_or(|until| until >= today);
        let available = facts.availabilities.get(&offering.base.id).is_some_and(|availability| {
            availability.availability_status == AvailabilityStatus::Available
                && availability
                    .available_quantity
                    .is_none_or(|quantity| quantity.to_decimal() > rust_decimal::Decimal::ZERO)
        });
        if !valid || !available {
            continue;
        }
        if let Some(delivery) = facts.deliveries.get(&offering.base.id) {
            policies.entry(offering.sku_id.to_string()).or_default().push(&delivery.policy);
        }
    }
    policies
}

#[cfg(test)]
mod tests {
    use erp_core::delivery_policy::{DeliveryPolicy, DeliveryRegionRule, DeliveryRuleSet, DeliveryTerm};
    use erp_core::region::RegionCode;

    use super::{DeliveryCoverageHint, coverage_hint};

    fn code(value: &str) -> RegionCode {
        RegionCode::parse(value).unwrap()
    }

    fn policy(dropship_xinjiang: DeliveryTerm, bulk_default: DeliveryTerm) -> DeliveryPolicy {
        DeliveryPolicy::new(
            DeliveryRuleSet {
                default_term: DeliveryTerm::FreightIncluded,
                rules: vec![DeliveryRegionRule { region_code: code("650000"), term: dropship_xinjiang }],
            },
            DeliveryRuleSet::uniform(bulk_default),
        )
        .unwrap()
    }

    #[test]
    fn no_supply_when_every_offering_refuses_both_modes() {
        let refuse = policy(DeliveryTerm::NotDeliverable, DeliveryTerm::NotDeliverable);
        assert_eq!(coverage_hint(&[&refuse], &code("650102")), DeliveryCoverageHint::NoSupply);
        assert_eq!(coverage_hint(&[&refuse], &code("440305")), DeliveryCoverageHint::None);
    }

    #[test]
    fn freight_extra_only_when_dropship_refuses_and_bulk_charges() {
        let charge = policy(DeliveryTerm::NotDeliverable, DeliveryTerm::FreightExtra);
        assert_eq!(coverage_hint(&[&charge], &code("650102")), DeliveryCoverageHint::BulkFreightExtraOnly);
        let free = policy(DeliveryTerm::NotDeliverable, DeliveryTerm::FreightIncluded);
        assert_eq!(coverage_hint(&[&charge, &free], &code("650102")), DeliveryCoverageHint::None);
    }

    #[test]
    fn any_dropship_or_missing_supply_needs_no_hint() {
        let refuse = policy(DeliveryTerm::NotDeliverable, DeliveryTerm::NotDeliverable);
        let open = policy(DeliveryTerm::FreightIncluded, DeliveryTerm::NotDeliverable);
        assert_eq!(coverage_hint(&[&refuse, &open], &code("650102")), DeliveryCoverageHint::None);
        assert_eq!(coverage_hint(&[], &code("650102")), DeliveryCoverageHint::None);
    }
}
