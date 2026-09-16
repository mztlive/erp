//! 精确创建依据资格、拆单和业务日期规则。
use std::collections::HashSet;
use std::str::FromStr;

use chrono::{Datelike, FixedOffset};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::SupplierAccountId;
use erp_core::money::Quantity;

use crate::entity::facts::{AvailabilityStatus, OfferingFact, SalesOrderBasisFact, SalesRevisionFact};
use crate::entity::purchase_order::{
    BasisGroup, BasisLine, BasisScope, CreationBasisFacts, LineSupply, SalesProcurementCoverage,
    SalesProcurementCoverageLine, basis_scope_key, fulfillment_options, maximum_create_quantity,
    purchase_type_from_product_kind, stable_line_id,
};
use crate::{Error, Result};

/// 供应商当前商务资料中的付款条件与经营类目。
#[derive(Debug, Clone, PartialEq, Eq)]
struct SupplierSettlementTerms {
    /// 不含经营类目编码的付款条件代码。
    payment_term_code: String,
    /// 经营类目；未登记时为空。
    business_category: Option<String>,
}

impl SupplierSettlementTerms {
    /// 商务资料缺失时的缺省付款条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `NET-30` 且无经营类目。
    ///
    /// # 错误
    /// 无。
    fn net30() -> Self {
        Self { payment_term_code: "NET-30".to_string(), business_category: None }
    }
}

/// 由销售当前版本、当前覆盖和批量供给事实形成精确依据集合（纯规则）。
///
/// # 参数
/// * `order` - 已生效销售单
/// * `coverage` - 当前销售版本采购覆盖
/// * `responsibility_scope_ids` - 当前采购任务冻结的稳定销售行 ID
/// * `facts` - 任务涉及 SKU 的批量供给事实
///
/// # 返回
/// 返回任务责任范围内按精确拆分维度分组并稳定排序的依据。
///
/// # 错误
/// 商品类型映射或可供数量非法时返回错误。
///
/// # 关键业务约束
/// 同一依据内供应商、采购类型、付款条件和履约责任完全一致；非生效销售单返回
/// 空集合；`min(remaining, available)` 为零时丢弃该供应商。
pub fn basis_groups_from_facts(
    order: &SalesOrderBasisFact,
    coverage: &SalesProcurementCoverage,
    responsibility_scope_ids: &[String],
    facts: &CreationBasisFacts,
) -> Result<Vec<BasisGroup>> {
    if !order.is_effective {
        return Ok(Vec::new());
    }
    let responsibility_scope_ids =
        responsibility_scope_ids.iter().map(String::as_str).collect::<HashSet<_>>();
    let mut groups: Vec<BasisGroup> = Vec::new();
    for line in &coverage.lines {
        if !responsibility_scope_ids.contains(line.revision_line.sales_order_line_id.as_ref())
            || line.summary.remaining_quantity <= zero_quantity()
        {
            continue;
        }
        let supplies = qualified_supplies_for_line(facts, line)?;
        append_line_supplies(&coverage.revision, line.clone(), supplies, facts, &mut groups)?;
    }
    for group in &mut groups {
        group.lines.sort_by(|left, right| stable_line_id(left).cmp(stable_line_id(right)));
    }
    groups.sort_by_key(|group| basis_scope_key(&group.scope));
    Ok(groups)
}

/// 将一条销售目标行的合格供给加入精确依据分组。
///
/// # 参数
/// * `revision` - 销售当前版本
/// * `line` - 当前销售版本目标行
/// * `supplies` - 每供应商一条确定供给
/// * `facts` - 批量供给与供应商结算事实
/// * `groups` - 待追加依据集合
///
/// # 返回
/// 追加完成返回 `Ok(())`。
///
/// # 错误
/// 商品类型映射或可供数量计算失败时返回错误。
///
/// # 关键业务约束
/// 有限可供量使用 `min(remaining, available)`，不因不足全量而丢弃供应商；付款
/// 条件与经营类目只从批量事实解释，不再逐供应商读取。
fn append_line_supplies(
    revision: &SalesRevisionFact,
    line: SalesProcurementCoverageLine,
    supplies: Vec<LineSupply>,
    facts: &CreationBasisFacts,
    groups: &mut Vec<BasisGroup>,
) -> Result<()> {
    for supply in supplies {
        let supplier_id = supply.offering.supplier_id.clone();
        let terms = settlement_terms_for(facts, &supplier_id);
        let purchase_type = purchase_type_from_product_kind(line.product_kind)?;
        let max_create_quantity =
            maximum_create_quantity(line.summary.remaining_quantity, supply.availability.available_quantity)?;
        if max_create_quantity <= zero_quantity() {
            continue;
        }
        for &fulfillment_responsibility in fulfillment_options(line.product_kind)? {
            let scope = BasisScope {
                supplier_id: supplier_id.clone(),
                purchase_type,
                payment_term_code: terms.payment_term_code.clone(),
                fulfillment_responsibility,
            };
            let basis_line =
                BasisLine { coverage: line.clone(), supply: supply.clone(), max_create_quantity };
            if let Some(group) = groups.iter_mut().find(|group| group.scope == scope) {
                group.lines.push(basis_line);
            } else {
                groups.push(BasisGroup {
                    revision: revision.clone(),
                    scope,
                    business_category: terms.business_category.clone(),
                    lines: vec![basis_line],
                });
            }
        }
    }
    Ok(())
}

/// 从批量事实解释供应商当前付款条件与经营类目。
///
/// # 参数
/// * `facts` - 批量供应商结算事实
/// * `supplier_id` - 供应商身份
///
/// # 返回
/// 返回该供应商已拆开的付款条件与经营类目；供应商、商务版本缺失或付款条件为
/// 空时付款条件回退 `NET-30`。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 付款条件是精确拆单维度的一部分；经营类目不得写入付款条件代码。
fn settlement_terms_for(
    facts: &CreationBasisFacts,
    supplier_id: &SupplierAccountId,
) -> SupplierSettlementTerms {
    let Some(supplier) = facts.suppliers.get(&supplier_id.to_string()) else {
        return SupplierSettlementTerms::net30();
    };
    let Some(revision_id) = supplier.current_commercial_profile_revision_id.clone() else {
        return SupplierSettlementTerms::net30();
    };
    let Some(revision) = facts.commercial_profiles.get(&revision_id.to_string()) else {
        return SupplierSettlementTerms::net30();
    };
    let payment_term_code = revision.payment_term_code.clone();
    SupplierSettlementTerms {
        payment_term_code: if payment_term_code.is_empty() {
            "NET-30".to_string()
        } else {
            payment_term_code
        },
        business_category: revision.business_category.clone(),
    }
}

/// 查询一条销售当前版本行的合格供给，并为每个供应商确定一条稳定供给。
///
/// # 参数
/// * `facts` - 批量 ACTIVE 供给、当前修订与可供投影
/// * `line` - 销售当前版本目标行
///
/// # 返回
/// 返回按供应商和供给 ID 稳定排序、每供应商最多一条的供给。
///
/// # 错误
/// 可供数量为负时返回业务错误。
///
/// # 关键业务约束
/// 仅 ACTIVE、条款当前有效且 availability 为 AVAILABLE 的供给合格；同一 SKU
/// 的供给顺序由批量事实保证，与逐 SKU 查询完全一致。
fn qualified_supplies_for_line(
    facts: &CreationBasisFacts,
    line: &SalesProcurementCoverageLine,
) -> Result<Vec<LineSupply>> {
    let mut seen_suppliers = HashSet::new();
    let mut supplies = Vec::new();
    for offering in facts.offerings.iter().filter(|offering| offering.sku_id == line.goods_line.sku_id) {
        if seen_suppliers.contains(&offering.supplier_id.to_string()) {
            continue;
        }
        let Some(supply) = qualified_supply(facts, offering)? else {
            continue;
        };
        seen_suppliers.insert(supply.offering.supplier_id.to_string());
        supplies.push(supply);
    }
    Ok(supplies)
}

/// 复验单条供给当前修订与可供投影。
///
/// # 参数
/// * `facts` - 批量供给修订与可供投影
/// * `offering` - ACTIVE 供给稳定身份
///
/// # 返回
/// 当前合格时返回供给；缺少当前修订、条款失效或不可供时返回 `None`。
///
/// # 错误
/// 可供数量为负时返回业务错误。
///
/// # 关键业务约束
/// 可供数量为空表示供应商未给出上限，不等于不可供；条款有效期按当前业务日期
/// 判定，业务日期由 Service 注入。
fn qualified_supply(facts: &CreationBasisFacts, offering: &OfferingFact) -> Result<Option<LineSupply>> {
    let Some(revision_id) = offering.stable.current_revision_id.clone() else {
        return Ok(None);
    };
    let Some(revision) = facts.revisions.get(&revision_id) else {
        return Ok(None);
    };
    let today = BusinessDate::today();
    if revision.valid_from > today || revision.valid_to.is_some_and(|valid_to| valid_to < today) {
        return Ok(None);
    }
    let Some(availability) = facts.availabilities.get(&offering.base.id.to_string()) else {
        return Ok(None);
    };
    if availability.availability_status != AvailabilityStatus::Available {
        return Ok(None);
    }
    if availability.available_quantity.is_some_and(|quantity| quantity < zero_quantity()) {
        return Err(Error::BusinessLogicError("供应商可供数量不能为负".to_string()));
    }
    if availability.available_quantity.is_some_and(|quantity| quantity == zero_quantity()) {
        return Ok(None);
    }
    Ok(Some(LineSupply {
        offering: offering.clone(),
        revision: revision.clone(),
        availability: availability.clone(),
    }))
}

/// 将精确时间转换为上海业务自然日。
///
/// # 参数
/// * `instant` - 销售履约期限
///
/// # 返回
/// 返回 Asia/Shanghai 自然日。
///
/// # 错误
/// 时区或日期构造失败时返回内部错误。
///
/// # 关键业务约束
/// 不按 UTC 日期截断。
pub fn business_date_of(instant: Instant) -> Result<BusinessDate> {
    let business_tz = FixedOffset::east_opt(8 * 60 * 60)
        .ok_or_else(|| Error::Internal("无法形成 Asia/Shanghai 时区".to_string()))?;
    let naive = instant.as_utc().with_timezone(&business_tz).date_naive();
    BusinessDate::from_ymd(naive.year(), naive.month(), naive.day())
        .ok_or_else(|| Error::Internal("履约期限日期非法".to_string()))
}

/// 返回合法采购数量零值。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回六位精度数量零值。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 只用于边界比较，不代表缺失业务数量。
pub fn zero_quantity() -> Quantity {
    Quantity::from_str("0").expect("零数量合法")
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{SkuId, SupplierCommercialProfileRevisionId, SupplierOfferingId};
    use erp_core::money::{Rate, UnitPrice};

    use super::*;
    use crate::entity::facts::{
        AvailabilityFact, CurrentRevisionFact, FactIdentity, OfferingRevisionFact, SupplierCommercialFact,
        SupplierRoleFact, VersionedFactIdentity,
    };

    fn offering() -> OfferingFact {
        OfferingFact {
            base: FactIdentity { id: "offering-1".to_string() },
            stable: CurrentRevisionFact { current_revision_id: Some("revision-1".to_string()) },
            sku_id: SkuId::new("sku-1"),
            supplier_id: SupplierAccountId::new("supplier-1"),
        }
    }
    fn facts(quantity: Option<&str>, status: AvailabilityStatus) -> CreationBasisFacts {
        let mut facts = CreationBasisFacts::default();
        facts.revisions.insert(
            "revision-1".to_string(),
            OfferingRevisionFact {
                base: FactIdentity { id: "revision-1".to_string() },
                valid_from: BusinessDate::today(),
                valid_to: None,
                bulk_supply_price_gross: UnitPrice::from_str("1").unwrap(),
                dropship_supply_price_gross: UnitPrice::from_str("2").unwrap(),
                input_tax_rate: Rate::from_str("0").unwrap(),
            },
        );
        facts.availabilities.insert(
            "offering-1".to_string(),
            AvailabilityFact {
                base: VersionedFactIdentity { id: "availability-1".to_string(), version: 3 },
                supplier_offering_id: SupplierOfferingId::new("offering-1"),
                availability_status: status,
                available_quantity: quantity.map(|value| Quantity::from_str(value).unwrap()),
                source_revision_token: None,
            },
        );
        facts
    }
    /// 缺少当前条款修订先跳过；不得让随后负可供量覆盖原资格顺序。
    #[test]
    fn missing_revision_precedes_negative_availability() {
        let mut facts = facts(Some("-1"), AvailabilityStatus::Available);
        facts.revisions.clear();
        assert!(qualified_supply(&facts, &offering()).unwrap().is_none());
    }
    /// 明确不可供先跳过；只有可供投影才进入数量合法性判断。
    #[test]
    fn unavailable_supply_precedes_negative_quantity_validation() {
        assert!(
            qualified_supply(&facts(Some("-1"), AvailabilityStatus::Unavailable), &offering())
                .unwrap()
                .is_none()
        );
        assert!(
            matches!(qualified_supply(&facts(Some("-1"),AvailabilityStatus::Available),&offering()),Err(Error::BusinessLogicError(message)) if message=="供应商可供数量不能为负")
        );
    }
    /// 数量为空表示无限制，零数量仍不构成合格依据。
    #[test]
    fn unlimited_and_zero_availability_remain_distinct() {
        let unlimited =
            qualified_supply(&facts(None, AvailabilityStatus::Available), &offering()).unwrap().unwrap();
        assert_eq!(unlimited.availability.available_quantity, None);
        assert!(
            qualified_supply(&facts(Some("0"), AvailabilityStatus::Available), &offering())
                .unwrap()
                .is_none()
        );
    }
    /// 缺少供应商或当前商务资料沿用 NET-30；空付款代码不抹去已解析的经营类目。
    #[test]
    fn settlement_missing_facts_and_blank_payment_keep_original_fallback() {
        let id = SupplierAccountId::new("supplier-1");
        let mut facts = CreationBasisFacts::default();
        assert_eq!(settlement_terms_for(&facts, &id), SupplierSettlementTerms::net30());
        facts.suppliers.insert(
            id.to_string(),
            SupplierRoleFact {
                current_commercial_profile_revision_id: Some(SupplierCommercialProfileRevisionId::new(
                    "profile-1",
                )),
            },
        );
        assert_eq!(settlement_terms_for(&facts, &id), SupplierSettlementTerms::net30());
        facts.commercial_profiles.insert(
            "profile-1".to_string(),
            SupplierCommercialFact::new(String::new()).with_business_category("茶叶"),
        );
        let terms = settlement_terms_for(&facts, &id);
        assert_eq!(terms.payment_term_code, "NET-30");
        assert_eq!(terms.business_category.as_deref(), Some("茶叶"));
    }
}
