//! 采购选源的配送判定：按收货地址或目标仓库所在地区判定供给能否配送、是否运费另计。
//!
//! 配送政策由供给领域解析后作为事实传入；本模块只判定、校验集采直发起订量，并
//! 定义采购单冻结的判定结果与"供应商不收运费"标记。

use std::collections::BTreeMap;

use erp_core::common::time::Instant;
use erp_core::delivery_policy::{DeliveryPolicy, DeliveryTerm};
use erp_core::ids::SalesOrderLineId;
use erp_core::money::Quantity;
use erp_core::region::RegionCode;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{FulfillmentResponsibility, PurchaseDeliveryAllocation, SupplyPriceBasis};

const WAIVER_REASON_MAX_LEN: usize = 200;

/// 供给当前生效的配送模板及政策。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OfferingDeliveryFact {
    /// 生效模板名称；未设置模板时为空。
    pub template_name: Option<String>,
    /// 生效模板版本号；未设置模板时为空。
    pub template_revision_no: Option<u64>,
    /// 生效政策；未设置模板时为全国配送、包邮。
    pub policy: DeliveryPolicy,
}

/// 一条采购行冻结的配送判定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseDeliveryJudgement {
    /// 稳定销售行。
    pub sales_order_line_id: SalesOrderLineId,
    /// 判定时的模板名称；未设置模板时为空。
    #[serde(default)]
    pub template_name: Option<String>,
    /// 判定时的模板版本号。
    #[serde(default)]
    pub template_revision_no: Option<u64>,
    /// 是否有地址或目标仓库为运费另计。
    pub freight_extra: bool,
}

/// 采购单冻结的价格依据、逐行配送判定与"供应商不收运费"标记。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseSourcingDelivery {
    /// 采购显式确认改选的逐销售行原因，独立于原询价历史。
    #[serde(default)]
    pub reselection_reasons: Vec<PurchaseSourcingReason>,
    /// 价格依据；费用采购单为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supply_price_basis: Option<SupplyPriceBasis>,
    /// 选源时冻结的逐行配送判定。
    #[serde(default)]
    pub delivery_judgements: Vec<PurchaseDeliveryJudgement>,
    /// "供应商不收运费"标记。
    #[serde(default)]
    pub freight_waivers: Vec<PurchaseFreightWaiver>,
}

/// 正式采购创建时冻结的改选解释。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseSourcingReason {
    pub sales_order_line_id: SalesOrderLineId,
    pub reason: String,
}

/// 采购标记"供应商不收运费"的记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseFreightWaiver {
    /// 稳定销售行。
    pub sales_order_line_id: SalesOrderLineId,
    /// 原因。
    pub reason: String,
    /// 操作人。
    pub waived_by: String,
    /// 标记时间。
    pub waived_at: Instant,
}

impl PurchaseFreightWaiver {
    /// 校验并构造标记。
    ///
    /// # 参数
    /// * `sales_order_line_id` - 稳定销售行
    /// * `reason` - 原因
    /// * `waived_by` - 操作人
    /// * `waived_at` - 标记时间
    ///
    /// # 返回
    /// 返回已规范化原因的标记。
    ///
    /// # 错误
    /// 原因为空或超过 200 字时返回错误。
    pub fn new(
        sales_order_line_id: SalesOrderLineId,
        reason: String,
        waived_by: &str,
        waived_at: Instant,
    ) -> Result<Self> {
        let reason = normalize_required_text(
            reason,
            "请填写供应商不收运费的原因",
            WAIVER_REASON_MAX_LEN,
            "原因不超过 200 字",
        )?;
        Ok(Self { sales_order_line_id, reason, waived_by: waived_by.to_string(), waived_at })
    }
}

/// 判定一条采购行的配送范围。
///
/// # 参数
/// * `delivery` - 供给生效配送政策
/// * `price_basis` - 采购价格依据
/// * `responsibility` - 履约责任
/// * `allocations` - 直发本次承担的收货地址
/// * `warehouse_region` - 集采入仓目标仓库所在地区
///
/// # 返回
/// 返回是否运费另计；电子交付、服务履约和无地址的直发不做判定。
///
/// # 错误
/// 任一收货地址或目标仓库不配送，或入仓目标仓库未设置所在地区时返回错误。
pub fn judge_line_delivery(
    delivery: &OfferingDeliveryFact,
    price_basis: SupplyPriceBasis,
    responsibility: FulfillmentResponsibility,
    allocations: &[PurchaseDeliveryAllocation],
    warehouse_region: Option<&RegionCode>,
) -> Result<bool> {
    let mode = price_basis.delivery_mode();
    match responsibility {
        FulfillmentResponsibility::SupplierDirect => {
            let mut freight_extra = false;
            for allocation in allocations {
                match delivery.policy.judge(mode, &allocation.region_code) {
                    DeliveryTerm::NotDeliverable => {
                        return Err(Error::from(format!(
                            "收货地址 {} 不在该供给{}配送范围内",
                            allocation.region_code.full_name(),
                            mode.label()
                        )));
                    },
                    DeliveryTerm::FreightExtra => freight_extra = true,
                    DeliveryTerm::FreightIncluded => {},
                }
            }
            Ok(freight_extra)
        },
        FulfillmentResponsibility::Warehouse => {
            let region =
                warehouse_region.ok_or_else(|| Error::from("目标仓库未设置所在地区，请先维护仓库资料"))?;
            match delivery.policy.judge(mode, region) {
                DeliveryTerm::NotDeliverable => Err(Error::from(format!(
                    "目标仓库所在地区 {} 不在该供给{}配送范围内",
                    region.full_name(),
                    mode.label()
                ))),
                term => Ok(term == DeliveryTerm::FreightExtra),
            }
        },
        FulfillmentResponsibility::Electronic
        | FulfillmentResponsibility::Service
        | FulfillmentResponsibility::NotApplicable => Ok(false),
    }
}

/// 一条集采直发采购行的起订量事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkOrderQuantity<'a> {
    /// 供给主键。
    pub offering_id: &'a str,
    /// 本行采购数量。
    pub quantity: Quantity,
    /// 该供给集采起订量。
    pub minimum: Quantity,
}

/// 校验集采直发向同一供给采购的数量合计不少于集采起订量。
///
/// # 参数
/// * `lines` - 本次确认中价格依据为集采且供应商直发的采购行
///
/// # 返回
/// 每条供给的合计均达到起订量时返回 `Ok(())`。
///
/// # 错误
/// 任一供给合计不足起订量时返回错误。
pub fn ensure_bulk_direct_minimum(lines: &[BulkOrderQuantity<'_>]) -> Result<()> {
    let mut totals: BTreeMap<&str, (rust_decimal::Decimal, Quantity)> = BTreeMap::new();
    for line in lines {
        let entry = totals.entry(line.offering_id).or_insert((rust_decimal::Decimal::ZERO, line.minimum));
        entry.0 += line.quantity.to_decimal();
    }
    for (total, minimum) in totals.values() {
        if *total < minimum.to_decimal() {
            return Err(Error::from(format!("集采直发数量合计须不少于集采起订量 {minimum}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::delivery_policy::{DeliveryPolicy, DeliveryRegionRule, DeliveryRuleSet, DeliveryTerm};
    use erp_core::ids::SalesOrderLineId;
    use erp_core::money::Quantity;
    use erp_core::region::RegionCode;

    use super::*;

    fn code(value: &str) -> RegionCode {
        RegionCode::parse(value).unwrap()
    }

    fn delivery() -> OfferingDeliveryFact {
        let rule = |region: &str, term| DeliveryRegionRule { region_code: code(region), term };
        OfferingDeliveryFact {
            template_name: Some("常温".into()),
            template_revision_no: Some(3),
            policy: DeliveryPolicy::new(
                DeliveryRuleSet {
                    default_term: DeliveryTerm::FreightIncluded,
                    rules: vec![rule("650000", DeliveryTerm::NotDeliverable)],
                },
                DeliveryRuleSet {
                    default_term: DeliveryTerm::FreightExtra,
                    rules: vec![
                        rule("440000", DeliveryTerm::FreightIncluded),
                        rule("650000", DeliveryTerm::NotDeliverable),
                    ],
                },
            )
            .unwrap(),
        }
    }

    fn allocation(region: &str) -> PurchaseDeliveryAllocation {
        PurchaseDeliveryAllocation {
            recipient: "张三".into(),
            phone: "13800000000".into(),
            region_code: code(region),
            region_name: String::new(),
            address: "测试路 1 号".into(),
            quantity: Quantity::from_str("1").unwrap(),
        }
    }

    #[test]
    fn direct_bulk_marks_freight_extra_outside_free_regions() {
        let free = judge_line_delivery(
            &delivery(),
            SupplyPriceBasis::Bulk,
            FulfillmentResponsibility::SupplierDirect,
            &[allocation("440106")],
            None,
        )
        .unwrap();
        assert!(!free);
        let extra = judge_line_delivery(
            &delivery(),
            SupplyPriceBasis::Bulk,
            FulfillmentResponsibility::SupplierDirect,
            &[allocation("440106"), allocation("310115")],
            None,
        )
        .unwrap();
        assert!(extra);
    }

    #[test]
    fn direct_rejects_not_deliverable_address_for_both_modes() {
        for basis in [SupplyPriceBasis::Dropship, SupplyPriceBasis::Bulk] {
            let error = judge_line_delivery(
                &delivery(),
                basis,
                FulfillmentResponsibility::SupplierDirect,
                &[allocation("650102")],
                None,
            )
            .unwrap_err();
            assert!(error.to_string().contains("不在该供给"));
        }
    }

    #[test]
    fn dropship_never_marks_freight_extra() {
        let extra = judge_line_delivery(
            &delivery(),
            SupplyPriceBasis::Dropship,
            FulfillmentResponsibility::SupplierDirect,
            &[allocation("310115")],
            None,
        )
        .unwrap();
        assert!(!extra);
    }

    #[test]
    fn warehouse_judges_target_region_and_requires_it() {
        let judge = |region: Option<&RegionCode>| {
            judge_line_delivery(
                &delivery(),
                SupplyPriceBasis::Bulk,
                FulfillmentResponsibility::Warehouse,
                &[],
                region,
            )
        };
        assert!(!judge(Some(&code("440305"))).unwrap());
        assert!(judge(Some(&code("310115"))).unwrap());
        assert!(judge(Some(&code("650102"))).is_err());
        assert!(judge(None).unwrap_err().to_string().contains("未设置所在地区"));
    }

    #[test]
    fn electronic_and_service_are_not_judged() {
        for responsibility in [FulfillmentResponsibility::Electronic, FulfillmentResponsibility::Service] {
            assert!(
                !judge_line_delivery(&delivery(), SupplyPriceBasis::Dropship, responsibility, &[], None)
                    .unwrap()
            );
        }
    }

    #[test]
    fn bulk_direct_minimum_sums_lines_of_the_same_offering() {
        let quantity = |value: &str| Quantity::from_str(value).unwrap();
        let line = |offering_id, value| BulkOrderQuantity {
            offering_id,
            quantity: quantity(value),
            minimum: quantity("50"),
        };
        assert!(ensure_bulk_direct_minimum(&[line("a", "30"), line("a", "20")]).is_ok());
        let error = ensure_bulk_direct_minimum(&[line("a", "30"), line("b", "50")]).unwrap_err();
        assert!(error.to_string().contains("集采起订量 50"));
        assert!(ensure_bulk_direct_minimum(&[]).is_ok());
    }

    #[test]
    fn waiver_requires_reason_within_limit() {
        let line = || SalesOrderLineId::new("sol-1");
        let at = Instant::now();
        assert!(PurchaseFreightWaiver::new(line(), "  ".into(), "buyer", at).is_err());
        assert!(PurchaseFreightWaiver::new(line(), "长".repeat(201), "buyer", at).is_err());
        let waiver = PurchaseFreightWaiver::new(line(), " 供应商承担 ".into(), "buyer", at).unwrap();
        assert_eq!(waiver.reason, "供应商承担");
    }
}
