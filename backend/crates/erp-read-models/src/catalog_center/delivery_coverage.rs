//! SKU 配送覆盖读取：销售录入收货地址时的实时提示（不暴露供应商），以及公司商品池
//! 地区筛选预算所需的事实。两者共用同一有效供给口径与逐地区合并规则。

use std::collections::HashMap;

use erp_catalog::repository::prelude::*;
use erp_catalog::{CatalogExt, ProductKind};
use erp_core::common::time::BusinessDate;
use erp_core::delivery_coverage::{DeliveryReach, delivery_reach};
use erp_core::delivery_policy::{DeliveryPolicy, DeliveryRuleSet, DeliveryTerm};
use erp_core::ids::SkuId;
use erp_core::region::RegionCode;
use erp_procurement::entity::facts::{AvailabilityStatus, OfferingRevisionFact};
use erp_procurement::entity::purchase_order::CreationBasisFacts;
use erp_supply::entity::supplier_offering::SkuDeliveryInput;
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
    if policies.is_empty() {
        return DeliveryCoverageHint::None;
    }
    match delivery_reach(policies, region) {
        DeliveryReach::FreightIncluded => DeliveryCoverageHint::None,
        DeliveryReach::FreightExtra => DeliveryCoverageHint::BulkFreightExtraOnly,
        DeliveryReach::NotDeliverable => DeliveryCoverageHint::NoSupply,
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
    let supplies = effective_supply(&facts, BusinessDate::today());
    let mut items = Vec::new();
    for line in &req.lines {
        let sku_id = line.sku_id.trim();
        let line_policies = supplies.get(sku_id).map(|supply| supply.policies.as_slice()).unwrap_or_default();
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
                hint: coverage_hint(&line_policies.iter().collect::<Vec<_>>(), &region),
            });
            seen.push(region);
        }
    }
    Ok(DeliveryCoverageView { items })
}

/// 计算一批 SKU 配送覆盖所需的事实。
///
/// # 参数
/// * `db` - 数据库
/// * `sku_ids` - 公司 SKU，不重复
/// * `today` - 有效期判定日
///
/// # 返回
/// 逐 SKU 返回有效且可供供给的去重政策、供应商、下次按日期变化日与是否受配送范围限制；
/// 没有供给的 SKU 返回空政策。
///
/// # 错误
/// 供给、模板或商品事实读取失败时返回错误。
pub async fn sku_delivery_inputs(
    db: &Database,
    sku_ids: &[String],
    today: BusinessDate,
) -> Result<Vec<SkuDeliveryInput>> {
    let ids = sku_ids.iter().map(|id| SkuId::new(id.as_str())).collect::<Vec<_>>();
    let facts = load_creation_basis_facts(db, &ids, &mut NoTransaction).await?;
    let restricted = restricted_skus(db, &ids).await?;
    let mut supplies = effective_supply(&facts, today);
    Ok(sku_ids
        .iter()
        .map(|sku_id| {
            let supply = supplies.remove(sku_id).unwrap_or_default();
            let mut policies: Vec<DeliveryPolicy> = Vec::new();
            for policy in supply.policies {
                if !policies.contains(&policy) {
                    policies.push(policy);
                }
            }
            SkuDeliveryInput {
                sku_id: sku_id.clone(),
                restricted: restricted.get(sku_id).copied().unwrap_or(true),
                policies,
                supplier_ids: supply.supplier_ids,
                next_change_date: supply.next_change_date,
            }
        })
        .collect())
}

/// 读取 SKU 是否为实物商品；虚拟商品、线下服务与卡券不受配送范围限制。
async fn restricted_skus(db: &Database, sku_ids: &[SkuId]) -> Result<HashMap<String, bool>> {
    let skus = db.skus().find_by_ids(sku_ids, &mut NoTransaction).await?;
    let product_ids = skus.iter().map(|sku| sku.product_id.clone()).collect::<Vec<_>>();
    let products = db.products().find_by_ids(&product_ids, &mut NoTransaction).await?;
    let kinds = products
        .into_iter()
        .map(|product| (product.base.id.clone(), product.product_kind))
        .collect::<HashMap<_, _>>();
    Ok(skus
        .into_iter()
        .map(|sku| {
            let kind = kinds.get(&sku.product_id.to_string()).copied();
            (sku.base.id, kind.is_none_or(|kind| kind == ProductKind::Physical))
        })
        .collect())
}

/// 一个 SKU 的有效供给汇总。
#[derive(Default)]
struct EffectiveSupply {
    policies: Vec<DeliveryPolicy>,
    supplier_ids: Vec<String>,
    next_change_date: Option<BusinessDate>,
}

/// 按 SKU 归集有效且当前可供供给的生效配送政策与供应商，并记录各供给条款最近一次按日期变化。
fn effective_supply(facts: &CreationBasisFacts, today: BusinessDate) -> HashMap<String, EffectiveSupply> {
    let mut supplies: HashMap<String, EffectiveSupply> = HashMap::new();
    for offering in &facts.offerings {
        let Some(revision) =
            offering.stable.current_revision_id.as_ref().and_then(|id| facts.revisions.get(id))
        else {
            continue;
        };
        let supply = supplies.entry(offering.sku_id.to_string()).or_default();
        supply.next_change_date = earliest(supply.next_change_date, next_validity_change(revision, today));
        let available = facts.availabilities.get(&offering.base.id).is_some_and(|availability| {
            availability.availability_status == AvailabilityStatus::Available
                && availability
                    .available_quantity
                    .is_none_or(|quantity| quantity.to_decimal() > rust_decimal::Decimal::ZERO)
        });
        let Some(delivery) = facts.deliveries.get(&offering.base.id) else { continue };
        if !today.is_within(revision.valid_from, revision.valid_to) || !available {
            continue;
        }
        let mut policy = delivery.policy.clone();
        if revision.bulk_supply_price_gross.is_none() || revision.bulk_minimum_order_quantity.is_none() {
            policy.bulk = DeliveryRuleSet::uniform(DeliveryTerm::NotDeliverable);
        }
        supply.policies.push(policy);
        let supplier_id = offering.supplier_id.to_string();
        if !supply.supplier_ids.contains(&supplier_id) {
            supply.supplier_ids.push(supplier_id);
        }
    }
    supplies
}

/// 条款下一次按日期改变有效性的日期：未来的生效日或失效日。
fn next_validity_change(revision: &OfferingRevisionFact, today: BusinessDate) -> Option<BusinessDate> {
    let start = (revision.valid_from > today).then_some(revision.valid_from);
    let end = revision.valid_to.filter(|until| *until > today);
    earliest(start, end)
}

fn earliest(left: Option<BusinessDate>, right: Option<BusinessDate>) -> Option<BusinessDate> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (left, right) => left.or(right),
    }
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

    mod supply {
        use erp_core::common::time::BusinessDate;
        use erp_core::delivery_policy::{DeliveryPolicy, DeliveryRuleSet, DeliveryTerm};
        use erp_core::ids::{SkuId, SupplierOfferingId};
        use erp_core::region::RegionCode;
        use erp_procurement::entity::facts::{
            AvailabilityFact, AvailabilityStatus, CurrentRevisionFact, FactIdentity, OfferingFact,
            OfferingRevisionFact, VersionedFactIdentity,
        };
        use erp_procurement::entity::purchase_order::{CreationBasisFacts, OfferingDeliveryFact};

        use super::super::{effective_supply, next_validity_change};

        fn date(value: &str) -> BusinessDate {
            value.parse().unwrap()
        }

        struct Offer {
            id: &'static str,
            supplier: &'static str,
            from: &'static str,
            to: Option<&'static str>,
            status: AvailabilityStatus,
            quantity: Option<&'static str>,
        }

        fn revision(offer: &Offer) -> OfferingRevisionFact {
            OfferingRevisionFact {
                base: FactIdentity { id: format!("{}-terms", offer.id) },
                version: 1,
                supplier_offering_id: SupplierOfferingId::new(offer.id),
                valid_from: date(offer.from),
                valid_to: offer.to.map(date),
                bulk_supply_price_gross: Some("10".parse().unwrap()),
                dropship_supply_price_gross: "11".parse().unwrap(),
                input_tax_rate: "0.13".parse().unwrap(),
                bulk_minimum_order_quantity: Some("1".parse().unwrap()),
            }
        }

        fn facts(offers: &[Offer], policy: &DeliveryPolicy) -> CreationBasisFacts {
            let mut facts = CreationBasisFacts::default();
            for offer in offers {
                facts.offerings.push(OfferingFact {
                    base: FactIdentity { id: offer.id.into() },
                    version: 1,
                    stable: CurrentRevisionFact { current_revision_id: Some(format!("{}-terms", offer.id)) },
                    sku_id: SkuId::new("sku-1"),
                    supplier_id: offer.supplier.to_string().into(),
                });
                facts.revisions.insert(format!("{}-terms", offer.id), revision(offer));
                facts.availabilities.insert(
                    offer.id.into(),
                    AvailabilityFact {
                        base: VersionedFactIdentity { id: format!("{}-availability", offer.id), version: 1 },
                        supplier_offering_id: SupplierOfferingId::new(offer.id),
                        availability_status: offer.status,
                        available_quantity: offer.quantity.map(|value| value.parse().unwrap()),
                        source_revision_token: None,
                    },
                );
                facts.deliveries.insert(
                    offer.id.into(),
                    OfferingDeliveryFact { policy: policy.clone(), ..Default::default() },
                );
            }
            facts
        }

        fn offer(
            id: &'static str,
            supplier: &'static str,
            from: &'static str,
            to: Option<&'static str>,
        ) -> Offer {
            Offer { id, supplier, from, to, status: AvailabilityStatus::Available, quantity: None }
        }

        #[test]
        fn only_valid_available_offerings_contribute_and_next_change_is_earliest() {
            let policy = DeliveryPolicy::new(
                DeliveryRuleSet::uniform(DeliveryTerm::NotDeliverable),
                DeliveryRuleSet::uniform(DeliveryTerm::FreightExtra),
            )
            .unwrap();
            let offers = [
                offer("open", "supplier-a", "2026-10-01", None),
                offer("future", "supplier-b", "2026-10-20", None),
                Offer {
                    quantity: Some("0"),
                    ..offer("sold-out", "supplier-c", "2026-10-01", Some("2026-10-16"))
                },
                Offer {
                    status: AvailabilityStatus::Unavailable,
                    ..offer("paused", "supplier-d", "2026-10-01", None)
                },
                offer("open-twin", "supplier-a", "2026-09-01", None),
                offer("expired-today", "supplier-e", "2026-10-01", Some("2026-10-14")),
            ];
            let facts = facts(&offers, &policy);
            let supplies = effective_supply(&facts, date("2026-10-14"));
            let supply = supplies.get("sku-1").unwrap();
            assert_eq!(supply.policies.len(), 2);
            assert_eq!(supply.supplier_ids, vec!["supplier-a".to_string()]);
            assert_eq!(supply.next_change_date, Some(date("2026-10-16")));
            assert!(effective_supply(&CreationBasisFacts::default(), date("2026-10-14")).is_empty());
        }

        #[test]
        fn missing_bulk_terms_do_not_claim_bulk_delivery_coverage() {
            let policy = DeliveryPolicy::new(
                DeliveryRuleSet::uniform(DeliveryTerm::NotDeliverable),
                DeliveryRuleSet::uniform(DeliveryTerm::FreightIncluded),
            )
            .unwrap();
            let mut facts = facts(&[offer("api", "supplier", "2026-10-01", None)], &policy);
            let revision = facts.revisions.get_mut("api-terms").unwrap();
            revision.bulk_supply_price_gross = None;
            revision.bulk_minimum_order_quantity = None;
            let supplies = effective_supply(&facts, date("2026-10-14"));
            let policies = supplies.get("sku-1").unwrap().policies.iter().collect::<Vec<_>>();
            assert_eq!(
                super::coverage_hint(&policies, &RegionCode::parse("440305").unwrap()),
                super::DeliveryCoverageHint::NoSupply
            );
        }

        #[test]
        fn expiry_date_is_the_change_date() {
            let ending = revision(&offer("ending", "supplier-a", "2026-10-01", Some("2026-10-15")));
            assert_eq!(next_validity_change(&ending, date("2026-10-14")), Some(date("2026-10-15")));
            assert_eq!(next_validity_change(&ending, date("2026-10-15")), None);
            let starting = revision(&offer("starting", "supplier-a", "2026-10-20", Some("2026-11-01")));
            assert_eq!(next_validity_change(&starting, date("2026-10-14")), Some(date("2026-10-20")));
        }
    }
}
