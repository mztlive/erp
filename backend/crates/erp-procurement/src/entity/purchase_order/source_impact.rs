//! 销售变更对采购的影响判定。
//!
//! 第一期 §6.3“销售变更对采购的影响”：按稳定销售行比较采购的基准销售行与当前
//! 销售行，只有与采购事实冲突的变化才使采购受影响。基准行取维持登记时的销售
//! 版本行；没有登记时取采购行自身引用的销售版本行。数量增加、价格、税率和条款
//! 变化不影响采购。本模块只承载纯规则，不依赖 I/O。

use std::collections::{HashMap, HashSet};

use erp_core::common::time::BusinessDate;
use erp_core::ids::{SalesOrderLineId, SalesOrderRevisionLineId};
use erp_core::money::Quantity;
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::business_date::business_date_of;
use super::coverage::{
    HistoricalSalesLineFact, ProcurementCoverageFacts, SalesProcurementCoverage, build_procurement_coverage,
};
use super::delivery_allocation::PurchaseDeliveryAllocation;
use super::order::{PurchaseOrder, PurchaseOrderStatus};
use super::types::{FulfillmentResponsibility, PurchaseLineType};
use crate::entity::facts::{SalesGoodsLineFact, SalesRevisionLineFact};

/// 采购受影响的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceImpactKind {
    /// 销售行数量归零或被移除，含换品。
    Zeroed {
        /// 基准销售数量。
        baseline_quantity: Quantity,
    },
    /// 销售行减量，且该行全部参与覆盖的供给超过新数量。
    QuantityReduced {
        /// 基准销售数量。
        baseline_quantity: Quantity,
        /// 当前销售数量。
        current_quantity: Quantity,
        /// 该行当前超出数量。
        excess_quantity: Quantity,
    },
    /// 履约期限提前，早于采购行预计交付日。
    DueEarlier {
        /// 当前销售履约期限。
        current_due: BusinessDate,
        /// 采购行预计交付日。
        expected_delivery_date: BusinessDate,
    },
    /// 直发或服务采购承担的收件资料被移除，或减量到低于各采购累计承担量。
    DeliveryChanged,
    /// 线下服务采购的服务地区变化。
    ServiceRegionChanged {
        /// 基准服务地区。
        baseline_region: Option<String>,
        /// 当前服务地区。
        current_region: Option<String>,
    },
}

impl SourceImpactKind {
    /// 判断该原因是否暂停付款。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 数量归零或减少时返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn blocks_payment(&self) -> bool {
        matches!(self, Self::Zeroed { .. } | Self::QuantityReduced { .. })
    }
}

/// 一行销售对一张采购的影响。
///
/// 持久化与接口使用扁平字段（见 [`SourceImpactReasonWire`]），避免标签枚举缓冲后
/// 数量字段丢失数据库定点编码；持久化往返由仓储测试覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "SourceImpactReasonWire", try_from = "SourceImpactReasonWire")]
pub struct SourceImpactReason {
    /// 稳定销售行。
    pub sales_order_line_id: String,
    /// 当前销售行号；行已移除时取基准行号。
    pub line_no: u32,
    /// 商品名称快照。
    pub item_name: String,
    /// 影响原因。
    pub kind: SourceImpactKind,
}

/// 影响原因代码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceImpactKindCode {
    /// 数量归零或被移除。
    Zeroed,
    /// 减量且覆盖超出。
    QuantityReduced,
    /// 履约期限提前。
    DueEarlier,
    /// 收件资料变化。
    DeliveryChanged,
    /// 服务地区变化。
    ServiceRegionChanged,
}

/// 影响原因的扁平传输形状。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceImpactReasonWire {
    /// 稳定销售行。
    pub sales_order_line_id: String,
    /// 销售行号。
    pub line_no: u32,
    /// 商品名称快照。
    pub item_name: String,
    /// 原因代码。
    pub kind: SourceImpactKindCode,
    /// 基准销售数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_quantity: Option<Quantity>,
    /// 当前销售数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_quantity: Option<Quantity>,
    /// 当前超出数量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excess_quantity: Option<Quantity>,
    /// 当前履约期限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_due: Option<BusinessDate>,
    /// 采购预计交付日。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_delivery_date: Option<BusinessDate>,
    /// 基准服务地区。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_region: Option<String>,
    /// 当前服务地区。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_region: Option<String>,
}

impl From<SourceImpactReason> for SourceImpactReasonWire {
    fn from(reason: SourceImpactReason) -> Self {
        let mut wire = Self {
            sales_order_line_id: reason.sales_order_line_id,
            line_no: reason.line_no,
            item_name: reason.item_name,
            kind: SourceImpactKindCode::DeliveryChanged,
            baseline_quantity: None,
            current_quantity: None,
            excess_quantity: None,
            current_due: None,
            expected_delivery_date: None,
            baseline_region: None,
            current_region: None,
        };
        match reason.kind {
            SourceImpactKind::Zeroed { baseline_quantity } => {
                wire.kind = SourceImpactKindCode::Zeroed;
                wire.baseline_quantity = Some(baseline_quantity);
            },
            SourceImpactKind::QuantityReduced { baseline_quantity, current_quantity, excess_quantity } => {
                wire.kind = SourceImpactKindCode::QuantityReduced;
                (wire.baseline_quantity, wire.current_quantity, wire.excess_quantity) =
                    (Some(baseline_quantity), Some(current_quantity), Some(excess_quantity));
            },
            SourceImpactKind::DueEarlier { current_due, expected_delivery_date } => {
                wire.kind = SourceImpactKindCode::DueEarlier;
                (wire.current_due, wire.expected_delivery_date) =
                    (Some(current_due), Some(expected_delivery_date));
            },
            SourceImpactKind::DeliveryChanged => {},
            SourceImpactKind::ServiceRegionChanged { baseline_region, current_region } => {
                wire.kind = SourceImpactKindCode::ServiceRegionChanged;
                (wire.baseline_region, wire.current_region) = (baseline_region, current_region);
            },
        }
        wire
    }
}

impl TryFrom<SourceImpactReasonWire> for SourceImpactReason {
    type Error = Error;

    fn try_from(wire: SourceImpactReasonWire) -> Result<Self> {
        let missing = || Error::from("采购依据影响原因缺少必要字段");
        let kind = match wire.kind {
            SourceImpactKindCode::Zeroed => {
                SourceImpactKind::Zeroed { baseline_quantity: wire.baseline_quantity.ok_or_else(missing)? }
            },
            SourceImpactKindCode::QuantityReduced => SourceImpactKind::QuantityReduced {
                baseline_quantity: wire.baseline_quantity.ok_or_else(missing)?,
                current_quantity: wire.current_quantity.ok_or_else(missing)?,
                excess_quantity: wire.excess_quantity.ok_or_else(missing)?,
            },
            SourceImpactKindCode::DueEarlier => SourceImpactKind::DueEarlier {
                current_due: wire.current_due.ok_or_else(missing)?,
                expected_delivery_date: wire.expected_delivery_date.ok_or_else(missing)?,
            },
            SourceImpactKindCode::DeliveryChanged => SourceImpactKind::DeliveryChanged,
            SourceImpactKindCode::ServiceRegionChanged => SourceImpactKind::ServiceRegionChanged {
                baseline_region: wire.baseline_region,
                current_region: wire.current_region,
            },
        };
        Ok(Self {
            sales_order_line_id: wire.sales_order_line_id,
            line_no: wire.line_no,
            item_name: wire.item_name,
            kind,
        })
    }
}

/// 一张受影响的采购及其逐行原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseSourceImpact {
    /// 采购单 ID。
    pub purchase_order_id: String,
    /// 按销售行号排序的影响原因，至少一条。
    pub reasons: Vec<SourceImpactReason>,
}

impl PurchaseSourceImpact {
    /// 判断该采购是否因数量归零或减少暂停付款。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 任一原因暂停付款时返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn blocks_payment(&self) -> bool {
        self.reasons.iter().any(|reason| reason.kind.blocks_payment())
    }
}

/// 判定同一销售单下全部参与覆盖的采购受影响情况。
///
/// # 参数
/// * `facts` - 构造 `coverage` 所用的同一批覆盖事实，含历史销售版本行与维持基准
/// * `coverage` - 由 `facts` 构造的当前逐行覆盖
///
/// # 返回
/// 返回受影响采购，顺序与 `facts.purchase_orders` 一致；未受影响的采购不出现。
///
/// # 错误
/// 采购行缺少稳定销售行、履约期限无法换算为业务日期时返回领域错误。
///
/// # 关键业务约束
/// 基准行与当前行为同一版本行时该行不受影响；基准行无法确定的行不判定为受影响。
pub fn evaluate_source_impacts(
    facts: &ProcurementCoverageFacts,
    coverage: &SalesProcurementCoverage,
) -> Result<Vec<PurchaseSourceImpact>> {
    let context = ImpactContext::new(facts, coverage);
    let purchases = participating_purchases(facts)?;
    let assigned = cumulative_assignments(&purchases);
    let mut impacts = Vec::new();
    for (order, lines) in &purchases {
        let mut reasons = Vec::new();
        for line in lines {
            reasons.extend(context.line_reasons(order, line, &assigned)?);
        }
        if !reasons.is_empty() {
            reasons.sort_by_key(|reason| reason.line_no);
            impacts.push(PurchaseSourceImpact { purchase_order_id: order.base.id.clone(), reasons });
        }
    }
    Ok(impacts)
}

/// 以待审批销售变更的拟生效内容预判受影响采购。
///
/// # 参数
/// * `facts` - 当前销售版本的覆盖事实
/// * `proposed_revision_id` - 拟生效内容的身份，不得与当前版本相同
/// * `proposed` - 拟生效内容的商品/服务行；新增的稳定行被忽略
///
/// # 返回
/// 返回变更生效后将受影响的采购。
///
/// # 错误
/// 当前版本缺失、覆盖事实不完整或履约期限无法换算时返回领域错误。
///
/// # 关键业务约束
/// 第一期 §6.3“销售变更对采购的影响”第 9 条：预判仅供审批参考，生效时按当时事实
/// 重算。当前版本行转为历史行，原有采购与维持基准照常参与判定。
pub fn predict_source_impacts(
    facts: &ProcurementCoverageFacts,
    proposed_revision_id: &str,
    proposed: Vec<(SalesRevisionLineFact, SalesGoodsLineFact)>,
) -> Result<Vec<PurchaseSourceImpact>> {
    let mut hypothetical = facts.clone();
    let revision = hypothetical
        .revision
        .as_mut()
        .ok_or_else(|| Error::from("销售单当前版本不存在，无法预判采购影响"))?;
    let current_revision_id = std::mem::replace(&mut revision.base.id, proposed_revision_id.to_string());
    let mut goods = std::mem::take(&mut hypothetical.goods_lines)
        .into_iter()
        .map(|line| (line.revision_line_id.to_string(), line))
        .collect::<HashMap<_, _>>();
    let current = std::mem::take(&mut hypothetical.revision_lines);
    let existing = current.iter().map(|line| line.sales_order_line_id.to_string()).collect::<HashSet<_>>();
    for line in current {
        if let Some(goods_line) = goods.remove(&line.base.id) {
            hypothetical.historical_sales_lines.insert(
                line.base.id.clone(),
                HistoricalSalesLineFact {
                    sales_order_revision_id: current_revision_id.clone(),
                    revision_line: line,
                    goods_line,
                },
            );
        }
    }
    for (line, goods_line) in
        proposed.into_iter().filter(|(line, _)| existing.contains(line.sales_order_line_id.as_ref()))
    {
        hypothetical.revision_lines.push(line);
        hypothetical.goods_lines.push(goods_line);
    }
    let coverage = build_procurement_coverage(hypothetical.clone())?;
    evaluate_source_impacts(&hypothetical, &coverage)
}

/// 判定所需的一条采购商品行事实。
struct PurchaseLineFacts<'a> {
    /// 稳定销售行。
    stable_id: &'a str,
    /// 采购行引用的销售版本行。
    recorded_line_id: Option<&'a str>,
    /// 采购预计交付日。
    expected_delivery_date: Option<BusinessDate>,
    /// 本行承担的收件资料。
    delivery_allocations: &'a [PurchaseDeliveryAllocation],
}

/// 某一销售版本中一行的公共行与商品子行。
#[derive(Clone, Copy)]
struct SalesLineView<'a> {
    /// 销售版本行 ID。
    line_id: &'a str,
    /// 公共行。
    revision_line: &'a SalesRevisionLineFact,
    /// 商品子行。
    goods: &'a SalesGoodsLineFact,
}

/// 当前行及其覆盖超出数量。
#[derive(Clone, Copy)]
struct CurrentLine<'a> {
    /// 当前版本行。
    view: SalesLineView<'a>,
    /// 该行当前超出数量。
    excess: Quantity,
}

/// 收件资料累计承担键：稳定销售行、收货人、电话、地址。
type AssignmentKey = (String, String, String, String, String);

/// 判定上下文：当前行、历史行与维持基准索引。
struct ImpactContext<'a> {
    /// 当前版本 ID。
    current_revision_id: &'a str,
    /// 稳定行到当前行。
    current: HashMap<&'a str, CurrentLine<'a>>,
    /// 历史版本行 ID 到历史行。
    historical: HashMap<&'a str, SalesLineView<'a>>,
    /// （销售版本 ID，稳定行）到历史行。
    by_revision: HashMap<(&'a str, &'a str), SalesLineView<'a>>,
    /// 采购单 ID 到维持时的销售版本 ID。
    maintained: &'a HashMap<String, String>,
}

impl<'a> ImpactContext<'a> {
    /// 建立判定索引。
    fn new(facts: &'a ProcurementCoverageFacts, coverage: &'a SalesProcurementCoverage) -> Self {
        let current = coverage
            .lines
            .iter()
            .map(|line| {
                let view = SalesLineView {
                    line_id: line.revision_line.base.id.as_str(),
                    revision_line: &line.revision_line,
                    goods: &line.goods_line,
                };
                (
                    line.revision_line.sales_order_line_id.as_ref(),
                    CurrentLine { view, excess: line.summary.excess_quantity },
                )
            })
            .collect();
        let mut historical = HashMap::new();
        let mut by_revision = HashMap::new();
        for (line_id, line) in &facts.historical_sales_lines {
            let view = SalesLineView {
                line_id: line_id.as_str(),
                revision_line: &line.revision_line,
                goods: &line.goods_line,
            };
            historical.insert(line_id.as_str(), view);
            by_revision.insert(
                (line.sales_order_revision_id.as_str(), line.revision_line.sales_order_line_id.as_ref()),
                view,
            );
        }
        Self {
            current_revision_id: &coverage.revision.base.id,
            current,
            historical,
            by_revision,
            maintained: &facts.maintained_sales_revisions,
        }
    }

    /// 返回采购行的判定基准行。
    fn baseline(&self, order: &PurchaseOrder, line: &PurchaseLineFacts<'a>) -> Option<SalesLineView<'a>> {
        let current = self.current.get(line.stable_id).map(|current| current.view);
        if let Some(revision) = self.maintained.get(&order.base.id) {
            if revision == self.current_revision_id {
                return current;
            }
            if let Some(view) = self.by_revision.get(&(revision.as_str(), line.stable_id)) {
                return Some(*view);
            }
        }
        let recorded = line.recorded_line_id?;
        match current {
            Some(view) if view.line_id == recorded => Some(view),
            _ => self.historical.get(recorded).copied(),
        }
    }

    /// 计算一条采购行的影响原因。
    fn line_reasons(
        &self,
        order: &PurchaseOrder,
        line: &PurchaseLineFacts<'a>,
        assigned: &HashMap<AssignmentKey, Decimal>,
    ) -> Result<Vec<SourceImpactReason>> {
        let Some(baseline) = self.baseline(order, line) else {
            return Ok(Vec::new());
        };
        let current = self.current.get(line.stable_id).copied();
        if current.is_some_and(|current| current.view.line_id == baseline.line_id) {
            return Ok(Vec::new());
        }
        let kinds = match current {
            None => zeroed(baseline),
            Some(current) => changed_kinds(order, line, baseline, current, assigned)?,
        };
        let meta = current.map_or(baseline, |current| current.view).revision_line;
        Ok(kinds
            .into_iter()
            .map(|kind| SourceImpactReason {
                sales_order_line_id: line.stable_id.to_string(),
                line_no: meta.line_no,
                item_name: meta.item_name_snapshot.clone(),
                kind,
            })
            .collect())
    }
}

/// 基准行有数量而当前行不存在或为零时的归零原因。
fn zeroed(baseline: SalesLineView<'_>) -> Vec<SourceImpactKind> {
    if baseline.goods.quantity.to_decimal().is_zero() {
        return Vec::new();
    }
    vec![SourceImpactKind::Zeroed { baseline_quantity: baseline.goods.quantity }]
}

/// 比较基准行与当前行，返回与采购事实冲突的变化。
fn changed_kinds(
    order: &PurchaseOrder,
    line: &PurchaseLineFacts<'_>,
    baseline: SalesLineView<'_>,
    current: CurrentLine<'_>,
    assigned: &HashMap<AssignmentKey, Decimal>,
) -> Result<Vec<SourceImpactKind>> {
    let (before, after) = (baseline.goods, current.view.goods);
    if after.quantity.to_decimal().is_zero() {
        return Ok(zeroed(baseline));
    }
    let mut kinds = Vec::new();
    if after.quantity < before.quantity && !current.excess.to_decimal().is_zero() {
        kinds.push(SourceImpactKind::QuantityReduced {
            baseline_quantity: before.quantity,
            current_quantity: after.quantity,
            excess_quantity: current.excess,
        });
    }
    kinds.extend(due_earlier(before, after, line.expected_delivery_date)?);
    let responsibility = order.fulfillment_responsibility;
    let carries_addresses = matches!(
        responsibility,
        FulfillmentResponsibility::SupplierDirect | FulfillmentResponsibility::Service
    );
    if carries_addresses && delivery_changed(line, before, after, assigned) {
        kinds.push(SourceImpactKind::DeliveryChanged);
    }
    if responsibility == FulfillmentResponsibility::Service && before.service_region != after.service_region {
        kinds.push(SourceImpactKind::ServiceRegionChanged {
            baseline_region: before.service_region.clone(),
            current_region: after.service_region.clone(),
        });
    }
    Ok(kinds)
}

/// 履约期限提前且早于采购预计交付日时返回原因。
fn due_earlier(
    before: &SalesGoodsLineFact,
    after: &SalesGoodsLineFact,
    expected: Option<BusinessDate>,
) -> Result<Option<SourceImpactKind>> {
    let current_due = business_date_of(after.fulfillment_due_at)?;
    let baseline_due = business_date_of(before.fulfillment_due_at)?;
    Ok(expected
        .filter(|expected| current_due < baseline_due && *expected > current_due)
        .map(|expected_delivery_date| SourceImpactKind::DueEarlier { current_due, expected_delivery_date }))
}

/// 判断采购承担的收件资料是否被移除，或减量到低于各采购累计承担量。
fn delivery_changed(
    line: &PurchaseLineFacts<'_>,
    before: &SalesGoodsLineFact,
    after: &SalesGoodsLineFact,
    assigned: &HashMap<AssignmentKey, Decimal>,
) -> bool {
    line.delivery_allocations.iter().any(|row| {
        let baseline = recipient_quantity(&before.delivery_allocations, row);
        let current = recipient_quantity(&after.delivery_allocations, row);
        let total = assigned.get(&assignment_key(line.stable_id, row)).copied().unwrap_or(Decimal::ZERO);
        !baseline.is_zero() && current < baseline && total > current
    })
}

/// 汇总同一收件资料的数量。
fn recipient_quantity(rows: &[PurchaseDeliveryAllocation], target: &PurchaseDeliveryAllocation) -> Decimal {
    rows.iter().filter(|row| row.key() == target.key()).map(|row| row.quantity.to_decimal()).sum()
}

/// 构造收件资料累计承担键。
fn assignment_key(stable_id: &str, row: &PurchaseDeliveryAllocation) -> AssignmentKey {
    (
        stable_id.to_string(),
        row.recipient.clone(),
        row.phone.clone(),
        row.region_code.to_string(),
        row.address.clone(),
    )
}

/// 汇总全部参与覆盖采购对各收件资料的累计承担量。
fn cumulative_assignments(
    purchases: &[(&PurchaseOrder, Vec<PurchaseLineFacts<'_>>)],
) -> HashMap<AssignmentKey, Decimal> {
    let mut assigned = HashMap::new();
    for line in purchases.iter().flat_map(|(_, lines)| lines) {
        for row in line.delivery_allocations {
            *assigned.entry(assignment_key(line.stable_id, row)).or_insert(Decimal::ZERO) +=
                row.quantity.to_decimal();
        }
    }
    assigned
}

/// 按当前指针取出参与覆盖采购的商品行。
///
/// # 错误
/// 商品行缺少稳定销售行时返回一致性错误。
fn participating_purchases(
    facts: &ProcurementCoverageFacts,
) -> Result<Vec<(&PurchaseOrder, Vec<PurchaseLineFacts<'_>>)>> {
    facts
        .purchase_orders
        .iter()
        .filter(|order| order.base.deleted_at == 0)
        .filter_map(|order| purchase_lines(facts, order).map(|lines| lines.map(|lines| (order, lines))))
        .collect()
}

/// 草稿类读当前提交行，正式类读当前版本行；其他状态不参与。
fn purchase_lines<'a>(
    facts: &'a ProcurementCoverageFacts,
    order: &PurchaseOrder,
) -> Option<Result<Vec<PurchaseLineFacts<'a>>>> {
    match order.stable.status {
        PurchaseOrderStatus::Draft
        | PurchaseOrderStatus::PendingFinanceReview
        | PurchaseOrderStatus::InApproval => {
            let pointer = order.current_submission_id.as_deref()?;
            let rows = facts.submission_lines.iter().filter(|row| {
                row.purchase_order_submission_id.as_ref() == pointer
                    && row.line_type == PurchaseLineType::ItemService
            });
            Some(
                rows.map(|row| {
                    line_facts(
                        row.sales_order_line_id.as_ref(),
                        row.sales_order_revision_line_id.as_ref(),
                        row.expected_delivery_date,
                        &row.delivery_allocations,
                    )
                })
                .collect(),
            )
        },
        PurchaseOrderStatus::Effective
        | PurchaseOrderStatus::PartiallyExecuted
        | PurchaseOrderStatus::Completed => {
            let pointer = order.stable.current_revision_id.as_deref()?;
            let rows = facts.purchase_revision_lines.iter().filter(|row| {
                row.purchase_order_revision_id.as_ref() == pointer
                    && row.line_type == PurchaseLineType::ItemService
            });
            Some(
                rows.map(|row| {
                    line_facts(
                        row.sales_order_line_id.as_ref(),
                        row.sales_order_revision_line_id.as_ref(),
                        row.expected_delivery_date,
                        &row.delivery_allocations,
                    )
                })
                .collect(),
            )
        },
        _ => None,
    }
}

/// 由采购行字段构造判定事实。
fn line_facts<'a>(
    stable_id: Option<&'a SalesOrderLineId>,
    recorded: Option<&'a SalesOrderRevisionLineId>,
    expected_delivery_date: Option<BusinessDate>,
    delivery_allocations: &'a [PurchaseDeliveryAllocation],
) -> Result<PurchaseLineFacts<'a>> {
    let stable_id = stable_id.ok_or_else(|| Error::from("采购商品行缺少销售稳定行"))?;
    Ok(PurchaseLineFacts {
        stable_id: stable_id.as_ref(),
        recorded_line_id: recorded.map(AsRef::as_ref),
        expected_delivery_date,
        delivery_allocations,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{
        ProcurementConfirmationLineId, ProductId, PurchaseLineSalesAllocationId, PurchaseOrderId,
        PurchaseOrderRevisionId, PurchaseOrderRevisionLineId, PurchaseOrderSubmissionId,
        PurchaseOrderSubmissionLineId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId,
        SalesOrderRevisionLineId, SkuId, SkuRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_core::money::{Amount, Quantity, Rate, UnitPrice};

    use super::{PurchaseSourceImpact, SourceImpactKind, evaluate_source_impacts};
    use crate::entity::facts::{
        CurrentRevisionFact, FactIdentity, ProductFact, ProductKind, SalesCustomerSnapshotFact,
        SalesGoodsLineFact, SalesLineType, SalesRevisionFact, SalesRevisionLineFact, SkuFact,
    };
    use crate::entity::purchase_order::{
        FulfillmentResponsibility, HistoricalSalesLineFact, ProcurementCoverageFacts,
        PurchaseDeliveryAllocation, PurchaseLineSalesAllocation, PurchaseLineSalesAllocationData,
        PurchaseLineType, PurchaseOrder, PurchaseOrderData, PurchaseOrderRevisionLine,
        PurchaseOrderRevisionLineData, PurchaseOrderStatus, PurchaseOrderSubmissionLine,
        PurchaseOrderSubmissionLineData, PurchaseType, build_procurement_coverage,
    };

    /// 2026-10-20 00:00 上海时间。
    const DUE_LATE: i64 = 1_792_425_600;
    /// 2026-10-10 00:00 上海时间。
    const DUE_EARLY: i64 = 1_791_561_600;

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn date(value: &str) -> BusinessDate {
        BusinessDate::from_str(value).unwrap()
    }

    fn address(recipient: &str, quantity: &str) -> PurchaseDeliveryAllocation {
        PurchaseDeliveryAllocation {
            recipient: recipient.to_string(),
            phone: "13800000000".to_string(),
            region_code: erp_core::region::RegionCode::parse("310115").unwrap(),
            region_name: "上海市浦东新区".into(),
            address: format!("{recipient} 地址"),
            quantity: qty(quantity),
        }
    }

    /// 一行销售在某个版本中的采购相关事实。
    #[derive(Clone)]
    struct SalesLine {
        quantity: &'static str,
        due: i64,
        region: Option<&'static str>,
        addresses: Vec<PurchaseDeliveryAllocation>,
    }

    fn sales_line(quantity: &'static str) -> SalesLine {
        SalesLine { quantity, due: DUE_LATE, region: None, addresses: Vec::new() }
    }

    fn line_pair(line_id: &str, line: &SalesLine) -> (SalesRevisionLineFact, SalesGoodsLineFact) {
        (
            SalesRevisionLineFact {
                base: FactIdentity { id: line_id.to_string() },
                sales_order_line_id: SalesOrderLineId::new("sol-1"),
                line_no: 1,
                line_type: SalesLineType::GoodsService,
                item_name_snapshot: "礼盒".to_string(),
                spec_snapshot: None,
                unit_snapshot: Some("件".to_string()),
            },
            SalesGoodsLineFact {
                revision_line_id: SalesOrderRevisionLineId::new(line_id),
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: SkuRevisionId::new("skur-1"),
                quantity: qty(line.quantity),
                base_unit_code: "件".to_string(),
                fulfillment_due_at: Instant::from_unix_secs(line.due),
                service_region: line.region.map(str::to_string),
                delivery_allocations: line.addresses.clone(),
                assigned_delivery_allocations: Vec::new(),
            },
        )
    }

    /// 采购的一条承担行。
    struct PurchaseLine {
        quantity: &'static str,
        expected: Option<&'static str>,
        addresses: Vec<PurchaseDeliveryAllocation>,
    }

    fn purchase_line(quantity: &'static str) -> PurchaseLine {
        PurchaseLine { quantity, expected: None, addresses: Vec::new() }
    }

    fn order(
        id: &str,
        status: PurchaseOrderStatus,
        responsibility: FulfillmentResponsibility,
    ) -> PurchaseOrder {
        let mut order = PurchaseOrder::new(
            PurchaseOrderId::new(id),
            PurchaseOrderData {
                business_org_unit_id: "org".to_string(),
                purchase_no: format!("PO-{id}"),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("rev-old"),
                creation_basis_id: format!("basis-{id}"),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".to_string(),
                fulfillment_responsibility: responsibility,
                owner_user_id: "buyer".to_string(),
                target_warehouse_id: (responsibility == FulfillmentResponsibility::Warehouse)
                    .then(|| WarehouseId::new("warehouse-1")),
            },
            "buyer",
            crate::entity::test_support::payment_term,
        )
        .unwrap();
        order.stable.status = status;
        order
    }

    fn revision_line(
        id: &str,
        pointer: &str,
        recorded: &str,
        line: &PurchaseLine,
    ) -> PurchaseOrderRevisionLine {
        PurchaseOrderRevisionLine::new(
            PurchaseOrderRevisionLineId::new(id),
            PurchaseOrderRevisionLineData {
                purchase_order_revision_id: PurchaseOrderRevisionId::new(pointer),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: Some(SkuRevisionId::new("skur-1")),
                supplier_offering_source: None,
                delivery_allocations: line.addresses.clone(),
                product_name_snapshot: Some("礼盒".to_string()),
                specification_snapshot: None,
                quantity: Some(qty(line.quantity)),
                base_unit_code: Some("件".to_string()),
                unit_cost_gross: Some(UnitPrice::from_str("1").unwrap()),
                gross_amount: Amount::from_str(line.quantity).unwrap(),
                net_amount: Amount::from_str(line.quantity).unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                input_tax_rate: Some(Rate::from_str("0").unwrap()),
                expected_delivery_date: line.expected.map(date),
                sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new(recorded)),
                allocated_quantity: Some(qty(line.quantity)),
                expense: None,
            },
        )
        .unwrap()
    }

    fn submission_line(
        id: &str,
        pointer: &str,
        recorded: &str,
        line: &PurchaseLine,
    ) -> PurchaseOrderSubmissionLine {
        PurchaseOrderSubmissionLine::new(
            PurchaseOrderSubmissionLineId::new(id),
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: PurchaseOrderSubmissionId::new(pointer),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: Some(SkuRevisionId::new("skur-1")),
                supplier_offering_source: None,
                delivery_allocations: line.addresses.clone(),
                product_name_snapshot: Some("礼盒".to_string()),
                specification_snapshot: None,
                quantity: Some(qty(line.quantity)),
                base_unit_code: Some("件".to_string()),
                unit_cost_gross: Some(UnitPrice::from_str("1").unwrap()),
                gross_amount: Amount::from_str(line.quantity).unwrap(),
                net_amount: Amount::from_str(line.quantity).unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                input_tax_rate: Some(Rate::from_str("0").unwrap()),
                expected_delivery_date: line.expected.map(date),
                sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new(recorded)),
                sales_order_submission_line_id: None,
                allocated_quantity: Some(qty(line.quantity)),
                expense: None,
            },
        )
        .unwrap()
    }

    /// 覆盖事实构造器：当前版本 `rev-cur` 的 `cur-1` 行，历史版本行按需追加。
    struct Scenario {
        facts: ProcurementCoverageFacts,
    }

    impl Scenario {
        fn new(current: &SalesLine) -> Self {
            let (revision_line, goods_line) = line_pair("cur-1", current);
            let facts = ProcurementCoverageFacts {
                revision: Some(SalesRevisionFact {
                    base: FactIdentity { id: "rev-cur".to_string() },
                    customer_snapshot: SalesCustomerSnapshotFact { customer_name: "客户".to_string() },
                    contract_snapshot: None,
                }),
                revision_lines: vec![revision_line],
                goods_lines: vec![goods_line],
                skus: HashMap::from([(
                    "sku-1".to_string(),
                    SkuFact {
                        base: FactIdentity { id: "sku-1".to_string() },
                        product_id: ProductId::new("product-1"),
                    },
                )]),
                products: HashMap::from([(
                    "product-1".to_string(),
                    ProductFact {
                        base: FactIdentity { id: "product-1".to_string() },
                        stable: CurrentRevisionFact::default(),
                        product_kind: ProductKind::Physical,
                    },
                )]),
                ..ProcurementCoverageFacts::default()
            };
            Self { facts }
        }

        fn history(mut self, revision_id: &str, line_id: &str, line: &SalesLine) -> Self {
            let (revision_line, goods_line) = line_pair(line_id, line);
            self.facts.historical_sales_lines.insert(
                line_id.to_string(),
                HistoricalSalesLineFact {
                    sales_order_revision_id: revision_id.to_string(),
                    revision_line,
                    goods_line,
                },
            );
            self
        }

        fn effective(
            mut self,
            id: &str,
            responsibility: FulfillmentResponsibility,
            recorded: &str,
            line: PurchaseLine,
        ) -> Self {
            let mut purchase = order(id, PurchaseOrderStatus::Effective, responsibility);
            purchase.stable.current_revision_id = Some(format!("por-{id}"));
            let row = revision_line(&format!("porl-{id}"), &format!("por-{id}"), recorded, &line);
            self.facts.allocations.push(
                PurchaseLineSalesAllocation::new(
                    PurchaseLineSalesAllocationId::new(format!("alloc-{id}")),
                    PurchaseLineSalesAllocationData {
                        purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new(format!(
                            "porl-{id}"
                        )),
                        sales_order_revision_line_id: SalesOrderRevisionLineId::new(recorded),
                        allocated_quantity: qty(line.quantity),
                        allocated_cost_gross: Amount::from_str("0").unwrap(),
                        allocated_cost_net: Amount::from_str("0").unwrap(),
                    },
                )
                .unwrap(),
            );
            self.facts.purchase_revision_lines.push(row);
            self.facts.purchase_orders.push(purchase);
            self
        }

        fn in_approval(mut self, id: &str, recorded: &str, line: PurchaseLine) -> Self {
            let mut purchase =
                order(id, PurchaseOrderStatus::InApproval, FulfillmentResponsibility::Warehouse);
            purchase.current_submission_id = Some(format!("sub-{id}"));
            self.facts.submission_lines.push(submission_line(
                &format!("subl-{id}"),
                &format!("sub-{id}"),
                recorded,
                &line,
            ));
            self.facts.purchase_orders.push(purchase);
            self
        }

        fn maintained(mut self, purchase_id: &str, revision_id: &str) -> Self {
            self.facts.maintained_sales_revisions.insert(purchase_id.to_string(), revision_id.to_string());
            self
        }

        fn evaluate(self) -> Vec<PurchaseSourceImpact> {
            let coverage = build_procurement_coverage(self.facts.clone()).unwrap();
            evaluate_source_impacts(&self.facts, &coverage).unwrap()
        }
    }

    fn kinds(impacts: &[PurchaseSourceImpact]) -> Vec<(String, SourceImpactKind)> {
        impacts
            .iter()
            .flat_map(|impact| {
                impact.reasons.iter().map(|reason| (impact.purchase_order_id.clone(), reason.kind.clone()))
            })
            .collect()
    }

    /// 引用当前版本行的采购不受影响。
    #[test]
    fn purchase_on_current_line_is_not_affected() {
        let impacts = Scenario::new(&sales_line("8"))
            .effective("a", FulfillmentResponsibility::Warehouse, "cur-1", purchase_line("8"))
            .evaluate();

        assert!(impacts.is_empty());
    }

    /// 加量、改价等变化不影响采购，旧版本采购按稳定行计入覆盖。
    #[test]
    fn quantity_increase_does_not_affect_purchase() {
        let impacts = Scenario::new(&sales_line("12"))
            .history("rev-old", "old-1", &sales_line("10"))
            .effective("a", FulfillmentResponsibility::Warehouse, "old-1", purchase_line("10"))
            .evaluate();

        assert!(impacts.is_empty());
    }

    /// 减量后覆盖超出新数量时，该行全部采购受影响并暂停付款。
    #[test]
    fn reduction_with_excess_marks_every_purchase_on_line() {
        let impacts = Scenario::new(&sales_line("8"))
            .history("rev-old", "old-1", &sales_line("10"))
            .effective("a", FulfillmentResponsibility::Warehouse, "old-1", purchase_line("5"))
            .effective("b", FulfillmentResponsibility::Warehouse, "old-1", purchase_line("5"))
            .evaluate();

        let reduced = SourceImpactKind::QuantityReduced {
            baseline_quantity: qty("10"),
            current_quantity: qty("8"),
            excess_quantity: qty("2"),
        };
        assert_eq!(kinds(&impacts), vec![("a".to_string(), reduced.clone()), ("b".to_string(), reduced)]);
        assert!(impacts.iter().all(PurchaseSourceImpact::blocks_payment));
        assert_eq!(impacts[0].reasons[0].item_name, "礼盒");
    }

    /// 减量后覆盖仍不超过新数量时不受影响。
    #[test]
    fn reduction_within_coverage_is_not_affected() {
        let impacts = Scenario::new(&sales_line("8"))
            .history("rev-old", "old-1", &sales_line("10"))
            .effective("a", FulfillmentResponsibility::Warehouse, "old-1", purchase_line("5"))
            .evaluate();

        assert!(impacts.is_empty());
    }

    /// 销售行归零（含换品）时审批中采购受影响。
    #[test]
    fn zeroed_line_affects_pending_purchase() {
        let impacts = Scenario::new(&sales_line("0"))
            .history("rev-old", "old-1", &sales_line("10"))
            .in_approval("a", "old-1", purchase_line("4"))
            .evaluate();

        assert_eq!(
            kinds(&impacts),
            vec![("a".to_string(), SourceImpactKind::Zeroed { baseline_quantity: qty("10") })]
        );
        assert!(impacts[0].blocks_payment());
    }

    /// 期限提前且早于采购预计交付日才受影响，不暂停付款。
    #[test]
    fn earlier_due_affects_only_late_purchase() {
        let earlier = SalesLine { due: DUE_EARLY, ..sales_line("10") };
        let late = PurchaseLine { expected: Some("2026-10-15"), ..purchase_line("5") };
        let on_time = PurchaseLine { expected: Some("2026-10-08"), ..purchase_line("5") };
        let impacts = Scenario::new(&earlier)
            .history("rev-old", "old-1", &sales_line("10"))
            .effective("late", FulfillmentResponsibility::Warehouse, "old-1", late)
            .effective("ok", FulfillmentResponsibility::Warehouse, "old-1", on_time)
            .evaluate();

        let expected = SourceImpactKind::DueEarlier {
            current_due: date("2026-10-10"),
            expected_delivery_date: date("2026-10-15"),
        };
        assert_eq!(kinds(&impacts), vec![("late".to_string(), expected)]);
        assert!(!impacts[0].blocks_payment());
    }

    /// 直发采购承担的收件资料被移除时受影响；入仓采购不看地址。
    #[test]
    fn removed_recipient_affects_direct_shipping_only() {
        let before =
            SalesLine { addresses: vec![address("张三", "4"), address("李四", "6")], ..sales_line("10") };
        let after = SalesLine { addresses: vec![address("李四", "10")], ..sales_line("10") };
        let direct = PurchaseLine { addresses: vec![address("张三", "4")], ..purchase_line("4") };
        let warehouse = PurchaseLine { addresses: vec![address("张三", "4")], ..purchase_line("4") };
        let impacts = Scenario::new(&after)
            .history("rev-old", "old-1", &before)
            .effective("direct", FulfillmentResponsibility::SupplierDirect, "old-1", direct)
            .effective("store", FulfillmentResponsibility::Warehouse, "old-1", warehouse)
            .evaluate();

        assert_eq!(kinds(&impacts), vec![("direct".to_string(), SourceImpactKind::DeliveryChanged)]);
    }

    /// 线下服务采购的服务地区变化时受影响。
    #[test]
    fn service_region_change_affects_service_purchase() {
        let before = SalesLine { region: Some("EAST"), ..sales_line("10") };
        let after = SalesLine { region: Some("WEST"), ..sales_line("10") };
        let impacts = Scenario::new(&after)
            .history("rev-old", "old-1", &before)
            .effective("svc", FulfillmentResponsibility::Service, "old-1", purchase_line("10"))
            .evaluate();

        assert_eq!(
            kinds(&impacts),
            vec![(
                "svc".to_string(),
                SourceImpactKind::ServiceRegionChanged {
                    baseline_region: Some("EAST".to_string()),
                    current_region: Some("WEST".to_string()),
                }
            )]
        );
    }

    /// 维持登记在当前版本时不再受影响；维持在中间版本时以该版本为基准。
    #[test]
    fn maintained_revision_becomes_baseline() {
        let at_current = Scenario::new(&sales_line("8"))
            .history("rev-old", "old-1", &sales_line("10"))
            .effective("a", FulfillmentResponsibility::Warehouse, "old-1", purchase_line("10"))
            .maintained("a", "rev-cur")
            .evaluate();
        let at_middle = Scenario::new(&sales_line("8"))
            .history("rev-old", "old-1", &sales_line("10"))
            .history("rev-mid", "mid-1", &sales_line("8"))
            .effective("a", FulfillmentResponsibility::Warehouse, "old-1", purchase_line("10"))
            .maintained("a", "rev-mid")
            .evaluate();

        assert!(at_current.is_empty());
        assert!(at_middle.is_empty());
    }

    /// 作废采购不参与判定。
    #[test]
    fn voided_purchase_is_ignored() {
        let mut scenario = Scenario::new(&sales_line("0"))
            .history("rev-old", "old-1", &sales_line("10"))
            .effective("a", FulfillmentResponsibility::Warehouse, "old-1", purchase_line("4"));
        let coverage = build_procurement_coverage(scenario.facts.clone()).unwrap();
        scenario.facts.purchase_orders[0].stable.status = PurchaseOrderStatus::Voided;

        let impacts = evaluate_source_impacts(&scenario.facts, &coverage).unwrap();

        assert!(impacts.is_empty());
    }

    /// 影响原因按扁平字段序列化并可无损读回。
    #[test]
    fn reason_round_trips_through_flat_wire_shape() {
        let reason = super::SourceImpactReason {
            sales_order_line_id: "sol-1".into(),
            line_no: 2,
            item_name: "礼盒".into(),
            kind: SourceImpactKind::QuantityReduced {
                baseline_quantity: qty("10"),
                current_quantity: qty("8"),
                excess_quantity: qty("2"),
            },
        };

        let json = serde_json::to_value(&reason).unwrap();
        let back: super::SourceImpactReason = serde_json::from_value(json.clone()).unwrap();

        assert_eq!(json["kind"], "QUANTITY_REDUCED");
        assert_eq!(json["excess_quantity"], "2");
        assert!(json.get("current_due").is_none());
        assert_eq!(back, reason);
    }

    /// 预判以拟生效内容替换当前版本：减量超出时受影响，新增商品行被忽略。
    #[test]
    fn prediction_uses_proposed_content_as_current() {
        let scenario = Scenario::new(&sales_line("10")).effective(
            "a",
            FulfillmentResponsibility::Warehouse,
            "cur-1",
            purchase_line("10"),
        );
        let (reduced_line, reduced_goods) = line_pair("prop-1", &sales_line("8"));
        let (mut new_line, mut new_goods) = line_pair("prop-2", &sales_line("5"));
        new_line.sales_order_line_id = SalesOrderLineId::new("sol-new");
        new_goods.sku_id = SkuId::new("sku-unknown");

        let impacts = super::predict_source_impacts(
            &scenario.facts,
            "proposed",
            vec![(reduced_line, reduced_goods), (new_line, new_goods)],
        )
        .unwrap();

        assert_eq!(
            kinds(&impacts),
            vec![(
                "a".to_string(),
                SourceImpactKind::QuantityReduced {
                    baseline_quantity: qty("10"),
                    current_quantity: qty("8"),
                    excess_quantity: qty("2"),
                }
            )]
        );
    }

    /// 只改价格等不影响采购的内容时预判为空。
    #[test]
    fn prediction_without_purchase_relevant_change_is_empty() {
        let scenario = Scenario::new(&sales_line("10")).effective(
            "a",
            FulfillmentResponsibility::Warehouse,
            "cur-1",
            purchase_line("10"),
        );
        let (line, goods) = line_pair("prop-1", &sales_line("10"));

        let impacts =
            super::predict_source_impacts(&scenario.facts, "proposed", vec![(line, goods)]).unwrap();

        assert!(impacts.is_empty());
    }
}
