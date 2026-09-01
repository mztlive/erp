//! 采购覆盖领域值对象。
//!
//! 当前销售版本商品/服务数量是采购目标；草稿、旧待财务与审批中采购只读取
//! `current_submission_id`，生效、部分执行与已完成采购只读取
//! `current_revision_id` 及其销售分配；现有库存直接分配形成的预占按
//! `reserved + consumed` 计入。历史提交、历史采购版本与作废采购单均不进入覆盖量。
//!
//! Repository 只负责批量返回当前销售目标与全部当前覆盖来源的原始持久化事实
//! （[`ProcurementCoverageFacts`]）；本模块负责当前行关联、指针选择、覆盖累计、
//! 超覆盖拒绝、剩余量与进度派生的纯业务规则，不依赖任何 I/O、时钟或 ID 生成器。

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;

use crate::catalog::{Product, ProductKind, Sku};
use crate::errors::{Error, Result};
use crate::ids::{PurchaseOrderRevisionId, PurchaseOrderSubmissionId};
use crate::inventory::StockReservation;
use crate::money::Quantity;
use crate::sales_order::{
    LineType, ProcurementCoverageSummary, SalesOrderGoodsServiceLineRevision, SalesOrderRevision,
    SalesOrderRevisionLine,
};

use super::allocation::PurchaseLineSalesAllocation;
use super::order::{PurchaseOrder, PurchaseOrderStatus};
use super::purchase_revision::PurchaseOrderRevisionLine;
use super::purchase_submission::PurchaseOrderSubmissionLine;
use super::types::PurchaseLineType;

/// 当前销售版本单行的采购覆盖信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesProcurementCoverageLine {
    /// 销售当前版本公共行。
    pub revision_line: SalesOrderRevisionLine,
    /// 销售当前版本商品/服务子类型行。
    pub goods_line: SalesOrderGoodsServiceLineRevision,
    /// SKU 所属商品的稳定业务类型。
    pub product_kind: ProductKind,
    /// 单行目标、覆盖、剩余与进度。
    pub summary: ProcurementCoverageSummary,
}

/// 当前销售版本的采购覆盖聚合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesProcurementCoverage {
    /// 销售当前版本头。
    pub revision: SalesOrderRevision,
    /// 当前版本商品/服务行及逐行覆盖。
    pub lines: Vec<SalesProcurementCoverageLine>,
    /// 当前版本全部商品/服务行汇总。
    pub summary: ProcurementCoverageSummary,
}

/// 采购覆盖计算所需的最小持久化事实集合。
///
/// Repository 一次批量返回当前销售目标与全部当前覆盖来源；本结构不承载任何
/// 计算规则，完整性、指针选择与超覆盖校验由 [`build_procurement_coverage`] 负责。
#[derive(Debug, Clone, Default)]
pub struct ProcurementCoverageFacts {
    /// 销售单当前版本文档；指针缺失或版本文档缺失时为空。
    pub revision: Option<SalesOrderRevision>,
    /// 销售当前版本全部公共行（Repository 已按行号升序返回）。
    pub revision_lines: Vec<SalesOrderRevisionLine>,
    /// 销售当前版本商品/服务子类型行。
    pub goods_lines: Vec<SalesOrderGoodsServiceLineRevision>,
    /// SKU 稳定 ID 到 SKU 事实的映射。
    pub skus: HashMap<String, Sku>,
    /// 商品 ID 到商品事实的映射。
    pub products: HashMap<String, Product>,
    /// 未作废且参与覆盖的采购单。
    pub purchase_orders: Vec<PurchaseOrder>,
    /// 草稿类采购单当前提交行（Repository 已按当前提交指针批量读取）。
    pub submission_lines: Vec<PurchaseOrderSubmissionLine>,
    /// 正式采购单当前版本行（Repository 已按当前版本指针批量读取）。
    pub purchase_revision_lines: Vec<PurchaseOrderRevisionLine>,
    /// 正式采购版本行的销售分配。
    pub allocations: Vec<PurchaseLineSalesAllocation>,
    /// 现有库存直接分配形成的预占（Repository 已排除采购入库预占）。
    pub reservations: Vec<StockReservation>,
}

/// 由采购覆盖事实构造逐行与总体采购覆盖结果。
///
/// # 参数
/// * `facts` - Repository 批量返回的当前销售目标与全部当前覆盖来源
///
/// # 返回
/// 返回逐行剩余量与总体汇总；结果只依赖输入事实，重复构造结果完全一致。
///
/// # 错误
/// 销售当前版本文档缺失、商品公共行缺少子类型行、SKU/商品/商品类型缺失、
/// 草稿类或正式状态缺少当前指针、正式行缺少唯一销售分配、分配未绑定销售当前
/// 版本行、覆盖超过目标或数量精度非法时返回领域错误。
///
/// # 关键业务约束
/// 只沿销售与采购的当前指针计算，稳定关联键为 `sales_order_line_id`；作废与
/// 历史采购不参与；现有库存预占只按 `reserved + consumed` 计入一次，采购入库
/// 预占由 Repository 过滤；覆盖超过目标不得截断，必须显式失败。
pub fn build_procurement_coverage(facts: ProcurementCoverageFacts) -> Result<SalesProcurementCoverage> {
    let revision = facts
        .revision
        .ok_or_else(|| Error::from("销售单当前版本不存在，无法计算采购剩余量"))?;
    let targets = join_target_lines(facts.revision_lines, facts.goods_lines)?;
    let product_kinds = resolve_product_kinds(&targets, &facts.skus, &facts.products)?;
    let mut covered = HashMap::new();
    accumulate_submission_coverage(
        &targets,
        &facts.purchase_orders,
        &facts.submission_lines,
        &mut covered,
    )?;
    accumulate_revision_coverage(
        &targets,
        &facts.purchase_orders,
        &facts.purchase_revision_lines,
        &facts.allocations,
        &mut covered,
    )?;
    accumulate_stock_coverage(&facts.reservations, &mut covered)?;
    build_coverage(revision, targets, covered, product_kinds)
}

/// 将当前版本公共行与商品子类型行一一连接。
///
/// # 参数
/// * `lines` - 当前版本公共行
/// * `goods` - 商品/服务子类型行
///
/// # 返回
/// 返回只包含商品/服务行的稳定有序目标集合。
///
/// # 错误
/// 任一商品公共行缺少子类型行时返回一致性错误。
///
/// # 关键业务约束
/// 连接键为当前销售版本行 ID，不按 SKU 猜测。
fn join_target_lines(
    lines: Vec<SalesOrderRevisionLine>,
    goods: Vec<SalesOrderGoodsServiceLineRevision>,
) -> Result<Vec<(SalesOrderRevisionLine, SalesOrderGoodsServiceLineRevision)>> {
    let mut goods_by_line = goods
        .into_iter()
        .map(|goods_line| (goods_line.revision_line_id.to_string(), goods_line))
        .collect::<HashMap<_, _>>();
    lines
        .into_iter()
        .filter(|line| line.line_type == LineType::GoodsService)
        .map(|line| {
            let goods_line = goods_by_line
                .remove(&line.base.id)
                .ok_or_else(|| Error::from("销售当前版本商品行缺少子类型数据"))?;
            Ok((line, goods_line))
        })
        .collect()
}

/// 解析采购目标行 SKU 对应的商品业务类型。
///
/// # 参数
/// * `targets` - 当前版本商品目标行
/// * `skus` - SKU 稳定 ID 到 SKU 事实的映射
/// * `products` - 商品 ID 到商品事实的映射
///
/// # 返回
/// 返回以 SKU 稳定 ID 为键的商品业务类型。
///
/// # 错误
/// SKU 或商品缺失时返回一致性错误。
///
/// # 关键业务约束
/// 采购类型只读取商品稳定主表的 `product_kind`，不得从销售字段或分类名称推导。
fn resolve_product_kinds(
    targets: &[(SalesOrderRevisionLine, SalesOrderGoodsServiceLineRevision)],
    skus: &HashMap<String, Sku>,
    products: &HashMap<String, Product>,
) -> Result<HashMap<String, ProductKind>> {
    targets
        .iter()
        .map(|(_, goods)| {
            let sku = skus
                .get(goods.sku_id.as_ref())
                .ok_or_else(|| Error::from(format!("销售当前版本 SKU {} 不存在", goods.sku_id)))?;
            let product_kind = products
                .get(sku.product_id.as_ref())
                .map(|product| product.product_kind)
                .ok_or_else(|| Error::from(format!("销售当前版本 SKU {} 所属商品不存在", goods.sku_id)))?;
            Ok((goods.sku_id.to_string(), product_kind))
        })
        .collect()
}

/// 汇总草稿、旧待财务与审批中采购的当前提交行。
///
/// # 参数
/// * `targets` - 当前销售版本商品目标行
/// * `orders` - 参与覆盖的采购单
/// * `lines` - 草稿类采购单当前提交行
/// * `covered` - 待累加的稳定销售行覆盖映射
///
/// # 返回
/// 汇总成功返回 `Ok(())`。
///
/// # 错误
/// 草稿类状态缺少当前提交指针、提交行缺少销售稳定行或分配数量时返回一致性错误。
///
/// # 关键业务约束
/// 禁止读取同一采购单的历史提交行；已从销售当前版本移除的目标行不参与。
fn accumulate_submission_coverage(
    targets: &[(SalesOrderRevisionLine, SalesOrderGoodsServiceLineRevision)],
    orders: &[PurchaseOrder],
    lines: &[PurchaseOrderSubmissionLine],
    covered: &mut HashMap<String, Quantity>,
) -> Result<()> {
    let target_ids = target_stable_ids(targets);
    current_submission_ids(orders)?;
    for line in lines
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
    {
        let stable_id = line
            .sales_order_line_id
            .as_ref()
            .ok_or_else(|| Error::from("采购当前提交行缺少销售稳定行"))?
            .to_string();
        if !target_ids.contains_key(&stable_id) {
            continue;
        }
        let quantity = line
            .allocated_quantity
            .ok_or_else(|| Error::from("采购当前提交行缺少分配数量"))?;
        add_covered(covered, &stable_id, quantity)?;
    }
    Ok(())
}

/// 汇总生效、部分执行与已完成采购的当前版本销售分配。
///
/// # 参数
/// * `targets` - 当前销售版本商品目标行
/// * `orders` - 参与覆盖的采购单
/// * `purchase_lines` - 正式采购单当前版本行
/// * `allocations` - 正式采购版本行的销售分配
/// * `covered` - 待累加的稳定销售行覆盖映射
///
/// # 返回
/// 汇总成功返回 `Ok(())`。
///
/// # 错误
/// 正式状态缺少当前版本指针、正式行或分配未绑定销售当前版本行、正式行缺少
/// 唯一销售分配时返回一致性错误。
///
/// # 关键业务约束
/// 禁止累计历史采购版本或脱离 allocation 直接使用采购行数量；正式覆盖必须同时
/// 由采购当前版本行和 allocation 指向同一销售当前版本行。
fn accumulate_revision_coverage(
    targets: &[(SalesOrderRevisionLine, SalesOrderGoodsServiceLineRevision)],
    orders: &[PurchaseOrder],
    purchase_lines: &[PurchaseOrderRevisionLine],
    allocations: &[PurchaseLineSalesAllocation],
    covered: &mut HashMap<String, Quantity>,
) -> Result<()> {
    let targets = target_stable_ids(targets);
    current_revision_ids(orders)?;
    let lines = purchase_lines
        .iter()
        .map(|line| (line.base.id.clone(), line.clone()))
        .collect::<HashMap<_, _>>();
    let expected = lines
        .iter()
        .filter(|(_, line)| line.line_type == PurchaseLineType::ItemService)
        .map(|(id, _)| id.clone())
        .collect::<HashSet<_>>();
    let mut allocated = HashSet::new();
    for allocation in allocations {
        allocated.insert(allocation.purchase_order_revision_line_id.to_string());
        add_current_allocation(&targets, &lines, allocation.clone(), covered)?;
    }
    if allocated != expected {
        return Err(Error::from("采购当前版本商品行缺少唯一销售分配"));
    }
    Ok(())
}

/// 校验并累加一条正式采购分配。
///
/// # 参数
/// * `targets` - 稳定销售行到当前销售版本行的映射
/// * `purchase_lines` - 当前采购版本行映射
/// * `allocation` - 待汇总的正式采购分配
/// * `covered` - 待累加覆盖映射
///
/// # 返回
/// 分配属于当前目标并完成累加时返回 `Ok(())`；目标行已从销售当前版本移除时忽略。
///
/// # 错误
/// 分配对应采购行缺失，或采购行/分配未绑定销售当前版本行时返回一致性错误。
fn add_current_allocation(
    targets: &HashMap<String, String>,
    purchase_lines: &HashMap<String, PurchaseOrderRevisionLine>,
    allocation: PurchaseLineSalesAllocation,
    covered: &mut HashMap<String, Quantity>,
) -> Result<()> {
    let purchase_line = purchase_lines
        .get(&allocation.purchase_order_revision_line_id.to_string())
        .ok_or_else(|| Error::from("采购正式分配缺少当前采购版本行"))?;
    let stable_id = purchase_line
        .sales_order_line_id
        .as_ref()
        .ok_or_else(|| Error::from("采购当前版本行缺少销售稳定行"))?
        .to_string();
    let Some(current_sales_line_id) = targets.get(&stable_id) else {
        return Ok(());
    };
    let recorded_sales_line_id = purchase_line
        .sales_order_revision_line_id
        .as_ref()
        .ok_or_else(|| Error::from("采购当前版本行缺少销售当前版本行"))?;
    if recorded_sales_line_id.to_string() != *current_sales_line_id
        || allocation.sales_order_revision_line_id.to_string() != *current_sales_line_id
    {
        return Err(Error::from("采购正式分配未绑定销售当前版本行"));
    }
    add_covered(covered, &stable_id, allocation.allocated_quantity)
}

/// 将现有库存直接分配形成的预占计入供给覆盖。
///
/// # 参数
/// * `reservations` - 现有库存预占（Repository 已排除采购入库预占）
/// * `covered` - 待累加的稳定销售行覆盖映射
///
/// # 返回
/// 累加成功返回 `Ok(())`。
///
/// # 错误
/// 预占数量精度非法时返回一致性错误。
///
/// # 关键业务约束
/// 采购入库形成的预占已经由采购单覆盖量承载，必须排除以免重复累计；现有库存
/// 预占按 `reserved + consumed` 计入：仍锁定和已经仓发的数量都已满足供给；释放
/// 数量不再覆盖，释放后任务同步会重新出现缺口。
fn accumulate_stock_coverage(
    reservations: &[StockReservation],
    covered: &mut HashMap<String, Quantity>,
) -> Result<()> {
    for reservation in reservations {
        add_covered(
            covered,
            reservation.sales_order_line_id.as_ref(),
            reservation_covered_quantity(reservation)?,
        )?;
    }
    Ok(())
}

/// 返回一条现有库存预占仍然满足销售供给的数量。
fn reservation_covered_quantity(reservation: &StockReservation) -> Result<Quantity> {
    quantity_of(reservation.reserved_quantity.to_decimal() + reservation.consumed_quantity.to_decimal())
}

/// 提取草稿类采购单的当前提交指针。
///
/// # 参数
/// * `orders` - 参与覆盖的采购单
///
/// # 返回
/// 返回草稿、旧待财务与审批中采购的当前提交 ID。
///
/// # 错误
/// 草稿类状态缺少当前提交指针时返回一致性错误。
///
/// # 关键业务约束
/// 其他状态不进入本集合。
fn current_submission_ids(orders: &[PurchaseOrder]) -> Result<Vec<PurchaseOrderSubmissionId>> {
    orders
        .iter()
        .filter(|order| {
            matches!(
                order.stable.status,
                PurchaseOrderStatus::Draft
                    | PurchaseOrderStatus::PendingFinanceReview
                    | PurchaseOrderStatus::InApproval
            )
        })
        .map(|order| {
            order
                .current_submission_id
                .as_ref()
                .map(|id| PurchaseOrderSubmissionId::new(id.clone()))
                .ok_or_else(|| Error::from("采购草稿类状态缺少当前提交指针"))
        })
        .collect()
}

/// 提取正式采购单的当前版本指针。
///
/// # 参数
/// * `orders` - 参与覆盖的采购单
///
/// # 返回
/// 返回生效、部分执行与已完成采购的当前版本 ID。
///
/// # 错误
/// 正式状态缺少当前版本指针时返回一致性错误。
///
/// # 关键业务约束
/// 其他状态不进入本集合。
fn current_revision_ids(orders: &[PurchaseOrder]) -> Result<Vec<PurchaseOrderRevisionId>> {
    orders
        .iter()
        .filter(|order| {
            matches!(
                order.stable.status,
                PurchaseOrderStatus::Effective
                    | PurchaseOrderStatus::PartiallyExecuted
                    | PurchaseOrderStatus::Completed
            )
        })
        .map(|order| {
            order
                .stable
                .current_revision_id
                .as_ref()
                .map(|id| PurchaseOrderRevisionId::new(id.clone()))
                .ok_or_else(|| Error::from("采购正式状态缺少当前版本指针"))
        })
        .collect()
}

/// 构造稳定销售行到当前销售版本行的映射。
///
/// # 参数
/// * `targets` - 当前销售版本商品目标行
///
/// # 返回
/// 返回稳定销售行 ID 到当前版本行 ID 的映射。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 稳定销售行是跨销售版本关联采购覆盖的唯一键。
fn target_stable_ids(
    targets: &[(SalesOrderRevisionLine, SalesOrderGoodsServiceLineRevision)],
) -> HashMap<String, String> {
    targets
        .iter()
        .map(|(line, _)| (line.sales_order_line_id.to_string(), line.base.id.clone()))
        .collect()
}

/// 向稳定销售行累加覆盖数量。
///
/// # 参数
/// * `covered` - 覆盖数量映射
/// * `sales_order_line_id` - 销售稳定行 ID
/// * `quantity` - 本次增加的覆盖数量
///
/// # 返回
/// 累加成功返回 `Ok(())`。
///
/// # 错误
/// 数量和超过 [`Quantity`] 精度约束时返回一致性错误。
///
/// # 关键业务约束
/// 使用十进制定点数精确相加，不做舍入。
fn add_covered(
    covered: &mut HashMap<String, Quantity>,
    sales_order_line_id: &str,
    quantity: Quantity,
) -> Result<()> {
    let current = covered
        .get(sales_order_line_id)
        .copied()
        .unwrap_or_else(zero_quantity);
    let value = Quantity::try_from(current.to_decimal() + quantity.to_decimal())
        .map_err(|error| Error::from(format!("采购覆盖数量精度非法: {error}")))?;
    covered.insert(sales_order_line_id.to_string(), value);
    Ok(())
}

/// 构造逐行与总体采购覆盖结果。
///
/// # 参数
/// * `revision` - 销售当前版本头
/// * `targets` - 当前版本商品目标行
/// * `covered` - 稳定销售行覆盖数量
/// * `product_kinds` - SKU 到商品业务类型的映射
///
/// # 返回
/// 返回逐行剩余量与总体汇总。
///
/// # 错误
/// 任一行或总体覆盖超过目标时返回一致性错误。
///
/// # 关键业务约束
/// `remaining = current sales quantity - covered`，不得截断负数。
fn build_coverage(
    revision: SalesOrderRevision,
    targets: Vec<(SalesOrderRevisionLine, SalesOrderGoodsServiceLineRevision)>,
    covered: HashMap<String, Quantity>,
    product_kinds: HashMap<String, ProductKind>,
) -> Result<SalesProcurementCoverage> {
    let mut total = Decimal::ZERO;
    let mut total_covered = Decimal::ZERO;
    let mut lines = Vec::with_capacity(targets.len());
    for (revision_line, goods_line) in targets {
        let product_kind = product_kinds
            .get(goods_line.sku_id.as_ref())
            .copied()
            .ok_or_else(|| Error::from(format!("销售当前版本 SKU {} 缺少商品类型", goods_line.sku_id)))?;
        let covered_quantity = covered
            .get(&revision_line.sales_order_line_id.to_string())
            .copied()
            .unwrap_or_else(zero_quantity);
        let summary = coverage_summary(goods_line.quantity, covered_quantity)?;
        total += summary.total_quantity.to_decimal();
        total_covered += summary.covered_quantity.to_decimal();
        lines.push(SalesProcurementCoverageLine {
            revision_line,
            goods_line,
            product_kind,
            summary,
        });
    }
    let summary = coverage_summary(quantity_of(total)?, quantity_of(total_covered)?)?;
    Ok(SalesProcurementCoverage {
        revision,
        lines,
        summary,
    })
}

/// 构造覆盖值对象并统一映射为一致性错误。
///
/// # 参数
/// * `total` - 目标数量
/// * `covered` - 覆盖数量
///
/// # 返回
/// 返回采购覆盖值对象。
///
/// # 错误
/// 数量不一致时返回领域错误。
///
/// # 关键业务约束
/// 所有查询入口共享同一错误语义。
fn coverage_summary(total: Quantity, covered: Quantity) -> Result<ProcurementCoverageSummary> {
    ProcurementCoverageSummary::new(total, covered)
        .map_err(|error| Error::from(format!("采购数量一致性错误: {error}")))
}

/// 从十进制构造数量并统一映射错误。
///
/// # 参数
/// * `value` - 待封装十进制数量
///
/// # 返回
/// 返回符合六位小数约束的数量。
///
/// # 错误
/// 有效小数位超过六位时返回一致性错误。
///
/// # 关键业务约束
/// 不做隐式舍入。
fn quantity_of(value: Decimal) -> Result<Quantity> {
    Quantity::try_from(value).map_err(|error| Error::from(format!("采购数量精度非法: {error}")))
}

/// 返回采购数量零值。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回合法的零数量。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 零值只用于缺省覆盖，不代表缺失采购指针。
fn zero_quantity() -> Quantity {
    Quantity::try_from(Decimal::ZERO).expect("零数量合法")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::catalog::product::ProductData;
    use crate::catalog::sku::SkuData;
    use crate::catalog::{EnableStatus, ListingStatus, Product, Sku};
    use crate::common::revision::RevisionBase;
    use crate::common::time::Instant;
    use crate::ids::{
        ProductId, PurchaseLineSalesAllocationId, PurchaseOrderId, PurchaseOrderRevisionId,
        PurchaseOrderRevisionLineId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, SalesOrderId,
        SalesOrderLineId, SalesOrderRevisionId, SalesOrderRevisionLineId, SkuId, StockReservationId,
        SupplierAccountId, UnitOfMeasureId, WarehouseId,
    };
    use crate::inventory::stock_reservation::{
        ReservationStatus, StockReservation, StockReservationData, StockReservationSourceType,
    };
    use crate::money::{Amount, Quantity, Rate};
    use crate::sales_order::revision::{
        SalesOrderGoodsServiceLineRevision, SalesOrderRevision, SalesOrderRevisionLine,
    };
    use crate::sales_order::{LineType, ProcurementCoverageSummary, RevisionSource};

    use super::{
        add_covered, build_procurement_coverage, coverage_summary, current_revision_ids,
        current_submission_ids, reservation_covered_quantity, zero_quantity, ProcurementCoverageFacts,
    };

    use crate::purchase_order::allocation::{PurchaseLineSalesAllocation, PurchaseLineSalesAllocationData};
    use crate::purchase_order::order::{PurchaseOrder, PurchaseOrderData, PurchaseOrderStatus};
    use crate::purchase_order::purchase_revision::{
        PurchaseOrderRevisionLine, PurchaseOrderRevisionLineData,
    };
    use crate::purchase_order::purchase_submission::{
        PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData,
    };
    use crate::purchase_order::{FulfillmentResponsibility, PurchaseLineType, PurchaseType};

    /// 构造指定状态和当前指针的采购单。
    fn purchase_order(
        id: &str,
        status: PurchaseOrderStatus,
        submission_id: Option<&str>,
        revision_id: Option<&str>,
    ) -> PurchaseOrder {
        let mut order = PurchaseOrder::new(
            PurchaseOrderId::new(id),
            PurchaseOrderData {
                purchase_no: String::new(),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("sor-1"),
                creation_basis_id: format!("basis-{id}"),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".to_string(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer-1".to_string(),
                target_warehouse_id: Some(WarehouseId::new("warehouse-1")),
            },
            "buyer-1",
        )
        .unwrap();
        order.stable.status = status;
        order.current_submission_id = submission_id.map(str::to_string);
        order.stable.current_revision_id = revision_id.map(str::to_string);
        order
    }

    /// 构造销售当前版本头。
    fn revision(id: &str) -> SalesOrderRevision {
        SalesOrderRevision {
            base: entity_core::BaseModel::new(format!("rev-{id}")),
            revision: RevisionBase::new(1),
            sales_order_id: SalesOrderId::new("so-1"),
            revision_source: RevisionSource::ErpApproval,
            source_snapshot_id: None,
            previous_revision_id: None,
            content_hash: format!("hash-{id}"),
            customer_revision_id: None,
            contract_revision_id: None,
            customer_snapshot: crate::sales_order::snapshot::CustomerSnapshot::new("客户").unwrap(),
            contract_snapshot: None,
            settlement_party_snapshot: None,
            payment_term_snapshot: crate::sales_order::snapshot::PaymentTermSnapshot::new("NET-30", "净30天")
                .unwrap(),
            invoice_requirement_snapshot: crate::sales_order::snapshot::InvoiceRequirementSnapshot::new(
                "增值税专用发票",
                "13",
            )
            .unwrap(),
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: Amount::from_str("100").unwrap(),
            net_amount: Amount::from_str("100").unwrap(),
            tax_amount: Amount::from_str("0").unwrap(),
            effective_at: Instant::from_unix_secs(1_800_000_000),
            recorded_at: Instant::from_unix_secs(1_800_000_000),
        }
    }

    /// 构造销售当前版本公共行。
    fn revision_line(id: &str, stable_line_id: &str, line_no: u32) -> SalesOrderRevisionLine {
        SalesOrderRevisionLine {
            base: entity_core::BaseModel::new(id.to_string()),
            sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
            sales_order_line_id: SalesOrderLineId::new(stable_line_id),
            line_no,
            line_type: LineType::GoodsService,
            gross_amount: Amount::from_str("10").unwrap(),
            net_amount: Amount::from_str("10").unwrap(),
            tax_amount: Amount::from_str("0").unwrap(),
            sales_tax_rate: Rate::from_str("0").unwrap(),
            item_name_snapshot: "商品".to_string(),
            spec_snapshot: Some("规格".to_string()),
            unit_snapshot: Some("件".to_string()),
        }
    }

    /// 构造销售当前版本商品/服务子类型行。
    fn goods_line(
        revision_line_id: &str,
        sku_id: &str,
        quantity: &str,
    ) -> SalesOrderGoodsServiceLineRevision {
        SalesOrderGoodsServiceLineRevision {
            base: entity_core::BaseModel::new(format!("goods-{revision_line_id}")),
            revision_line_id: SalesOrderRevisionLineId::new(revision_line_id),
            sku_id: SkuId::new(sku_id),
            sku_revision_id: crate::ids::SkuRevisionId::new(format!("skur-{sku_id}")),
            welfare_scenario: None,
            service_region: None,
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: Quantity::from_str(quantity).unwrap(),
            base_unit_code: "件".to_string(),
            unit_price_gross: crate::money::UnitPrice::from_str("5").unwrap(),
        }
    }

    /// 构造 SKU 事实。
    fn sku(id: &str) -> Sku {
        Sku::new(
            SkuId::new(id),
            SkuData {
                sku_no: format!("SKU-{id}"),
                product_id: ProductId::new(format!("product-{id}")),
                base_unit_id: UnitOfMeasureId::new("unit-1"),
                specification_signature: format!("spec-{id}"),
                status: EnableStatus::Active,
                listing_status: ListingStatus::Listed,
            },
            "test",
        )
        .unwrap()
    }

    /// 构造商品事实。
    fn product(id: &str) -> Product {
        Product::new(
            ProductId::new(id),
            ProductData {
                product_no: format!("P-{id}"),
                product_kind: crate::catalog::ProductKind::Physical,
                status: EnableStatus::Active,
            },
            "test",
        )
        .unwrap()
    }

    /// 构造草稿类采购提交行。
    fn submission_line(
        id: &str,
        submission_id: &str,
        stable_line_id: Option<&str>,
        quantity: Option<&str>,
    ) -> PurchaseOrderSubmissionLine {
        PurchaseOrderSubmissionLine::new(
            PurchaseOrderSubmissionLineId::new(id),
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: PurchaseOrderSubmissionId::new(submission_id),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(crate::ids::ProcurementConfirmationLineId::new(
                    "pcl-1",
                )),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: Some(crate::ids::SkuRevisionId::new("skur-1")),
                product_name_snapshot: Some("商品".to_string()),
                specification_snapshot: Some("规格".to_string()),
                quantity: Some(Quantity::from_str("2").unwrap()),
                base_unit_code: Some("件".to_string()),
                unit_cost_gross: Some(crate::money::UnitPrice::from_str("5").unwrap()),
                gross_amount: Amount::from_str("10").unwrap(),
                net_amount: Amount::from_str("10").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                input_tax_rate: Some(Rate::from_str("0").unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: stable_line_id.map(SalesOrderLineId::new),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sorl-1")),
                sales_order_submission_line_id: None,
                allocated_quantity: quantity.map(|q| Quantity::from_str(q).unwrap()),
            },
        )
        .unwrap()
    }

    /// 构造正式采购版本行。
    fn purchase_revision_line(
        id: &str,
        revision_id: &str,
        stable_line_id: Option<&str>,
        current_line_id: Option<&str>,
    ) -> PurchaseOrderRevisionLine {
        PurchaseOrderRevisionLine::new(
            PurchaseOrderRevisionLineId::new(id),
            PurchaseOrderRevisionLineData {
                purchase_order_revision_id: PurchaseOrderRevisionId::new(revision_id),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(crate::ids::ProcurementConfirmationLineId::new(
                    "pcl-1",
                )),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: Some(crate::ids::SkuRevisionId::new("skur-1")),
                product_name_snapshot: Some("商品".to_string()),
                specification_snapshot: Some("规格".to_string()),
                quantity: Some(Quantity::from_str("2").unwrap()),
                base_unit_code: Some("件".to_string()),
                unit_cost_gross: Some(crate::money::UnitPrice::from_str("5").unwrap()),
                gross_amount: Amount::from_str("10").unwrap(),
                net_amount: Amount::from_str("10").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                input_tax_rate: Some(Rate::from_str("0").unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: stable_line_id.map(SalesOrderLineId::new),
                sales_order_revision_line_id: current_line_id.map(SalesOrderRevisionLineId::new),
                allocated_quantity: Some(Quantity::from_str("2").unwrap()),
            },
        )
        .unwrap()
    }

    /// 构造正式采购分配。
    fn allocation(
        id: &str,
        purchase_line_id: &str,
        current_line_id: &str,
        quantity: &str,
    ) -> PurchaseLineSalesAllocation {
        PurchaseLineSalesAllocation::new(
            PurchaseLineSalesAllocationId::new(id),
            PurchaseLineSalesAllocationData {
                purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new(purchase_line_id),
                sales_order_revision_line_id: SalesOrderRevisionLineId::new(current_line_id),
                allocated_quantity: Quantity::from_str(quantity).unwrap(),
                allocated_cost_gross: Amount::from_str("10").unwrap(),
                allocated_cost_net: Amount::from_str("10").unwrap(),
            },
        )
        .unwrap()
    }

    /// 构造一条现有库存预占。
    fn reservation(id: &str, stable_line_id: &str, reserved: &str, consumed: &str) -> StockReservation {
        StockReservation::new(
            StockReservationId::new(id),
            StockReservationData {
                warehouse_id: WarehouseId::new("warehouse-1"),
                sku_id: SkuId::new("sku-1"),
                sales_order_line_id: SalesOrderLineId::new(stable_line_id),
                source_type: StockReservationSourceType::ExistingStock,
                purchase_line_sales_allocation_id: None,
                source_receipt_line_id: None,
                source_allocation_id: Some("allocation-1".to_string()),
                reserved_quantity: Quantity::from_str(reserved).unwrap(),
                consumed_quantity: Quantity::from_str(consumed).unwrap(),
                released_quantity: Quantity::from_str("0").unwrap(),
                status: ReservationStatus::PartiallyConsumed,
            },
        )
        .unwrap()
    }

    /// 构造覆盖事实：一行目标、一个草稿提交、一个正式版本及现有库存预占。
    fn facts_with_two_coverage_sources() -> ProcurementCoverageFacts {
        ProcurementCoverageFacts {
            revision: Some(revision("1")),
            revision_lines: vec![revision_line("sorl-1", "sol-1", 1)],
            goods_lines: vec![goods_line("sorl-1", "sku-1", "10")],
            skus: std::iter::once(("sku-1".to_string(), sku("sku-1"))).collect(),
            products: std::iter::once(("product-sku-1".to_string(), product("product-sku-1"))).collect(),
            purchase_orders: vec![
                purchase_order("po-draft", PurchaseOrderStatus::Draft, Some("sub-1"), None),
                purchase_order(
                    "po-effective",
                    PurchaseOrderStatus::Effective,
                    None,
                    Some("rev-1"),
                ),
            ],
            submission_lines: vec![submission_line("subl-1", "sub-1", Some("sol-1"), Some("2"))],
            purchase_revision_lines: vec![purchase_revision_line(
                "porl-1",
                "rev-1",
                Some("sol-1"),
                Some("sorl-1"),
            )],
            allocations: vec![allocation("alloc-1", "porl-1", "sorl-1", "3")],
            reservations: vec![reservation("rsv-1", "sol-1", "1", "1")],
        }
    }

    /// 覆盖累加使用稳定销售行键且保持定点精度。
    #[test]
    fn covered_quantities_accumulate_by_stable_line() {
        let mut covered = std::collections::HashMap::new();
        add_covered(&mut covered, "sol-1", Quantity::from_str("1.25").unwrap()).unwrap();
        add_covered(&mut covered, "sol-1", Quantity::from_str("0.75").unwrap()).unwrap();

        assert_eq!(covered["sol-1"], Quantity::from_str("2").unwrap());
        assert_eq!(zero_quantity(), Quantity::from_str("0").unwrap());
    }

    /// 现有库存覆盖保留已仓发数量，并排除已经释放的剩余部分。
    #[test]
    fn existing_stock_coverage_is_reserved_plus_consumed() {
        let reservation = reservation("rsv-direct-1", "line-1", "6", "4");

        assert_eq!(
            reservation_covered_quantity(&reservation).unwrap(),
            Quantity::from_str("10").unwrap()
        );
    }

    /// 覆盖超过销售当前版本数量时统一返回一致性错误。
    #[test]
    fn over_coverage_is_consistency_error() {
        let result = coverage_summary(
            Quantity::from_str("1").unwrap(),
            Quantity::from_str("1.000001").unwrap(),
        );

        assert!(result.unwrap_err().to_string().contains("采购数量一致性错误"));
    }

    /// 状态分流只提取草稿类当前提交和正式类当前版本指针。
    #[test]
    fn coverage_pointer_selection_uses_current_status_pointer_only() {
        let orders = vec![
            purchase_order("po-draft", PurchaseOrderStatus::Draft, Some("sub-current"), None),
            purchase_order(
                "po-finance",
                PurchaseOrderStatus::PendingFinanceReview,
                Some("sub-finance"),
                None,
            ),
            purchase_order(
                "po-approval",
                PurchaseOrderStatus::InApproval,
                Some("sub-approval"),
                None,
            ),
            purchase_order(
                "po-effective",
                PurchaseOrderStatus::Effective,
                Some("sub-history"),
                Some("rev-current"),
            ),
            purchase_order(
                "po-partial",
                PurchaseOrderStatus::PartiallyExecuted,
                None,
                Some("rev-partial"),
            ),
            purchase_order(
                "po-completed",
                PurchaseOrderStatus::Completed,
                None,
                Some("rev-completed"),
            ),
            purchase_order(
                "po-voided",
                PurchaseOrderStatus::Voided,
                Some("sub-voided"),
                Some("rev-voided"),
            ),
        ];

        let submission_ids = current_submission_ids(&orders)
            .unwrap()
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        let revision_ids = current_revision_ids(&orders)
            .unwrap()
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();

        assert_eq!(submission_ids, vec!["sub-current", "sub-finance", "sub-approval"]);
        assert_eq!(revision_ids, vec!["rev-current", "rev-partial", "rev-completed"]);
    }

    /// 当前销售版本文档缺失时按一致性错误失败。
    #[test]
    fn missing_current_revision_is_consistency_error() {
        let facts = ProcurementCoverageFacts::default();
        let error = build_procurement_coverage(facts).unwrap_err().to_string();
        assert_eq!(error, "销售单当前版本不存在，无法计算采购剩余量");
    }

    /// 草稿类状态缺少当前提交指针时按一致性错误失败。
    #[test]
    fn missing_submission_pointer_is_consistency_error() {
        let mut facts = facts_with_two_coverage_sources();
        facts.purchase_orders = vec![purchase_order("po-draft", PurchaseOrderStatus::Draft, None, None)];
        let error = build_procurement_coverage(facts).unwrap_err().to_string();
        assert_eq!(error, "采购草稿类状态缺少当前提交指针");
    }

    /// 正式状态缺少当前版本指针时按一致性错误失败。
    #[test]
    fn missing_revision_pointer_is_consistency_error() {
        let mut facts = facts_with_two_coverage_sources();
        facts.purchase_orders = vec![purchase_order(
            "po-effective",
            PurchaseOrderStatus::Effective,
            None,
            None,
        )];
        let error = build_procurement_coverage(facts).unwrap_err().to_string();
        assert_eq!(error, "采购正式状态缺少当前版本指针");
    }

    /// 零覆盖、部分覆盖与完整覆盖均正确派生剩余量与进度。
    #[test]
    fn coverage_build_handles_zero_partial_and_full() {
        let mut facts = facts_with_two_coverage_sources();
        facts.purchase_orders = vec![purchase_order(
            "po-draft",
            PurchaseOrderStatus::Draft,
            Some("sub-1"),
            None,
        )];
        facts.submission_lines = vec![submission_line("subl-1", "sub-1", Some("sol-1"), Some("2"))];
        facts.purchase_revision_lines.clear();
        facts.allocations.clear();
        facts.reservations.clear();

        let coverage = build_procurement_coverage(facts).unwrap();
        assert_eq!(coverage.lines.len(), 1);
        assert_eq!(
            coverage.lines[0].summary.total_quantity,
            Quantity::from_str("10").unwrap()
        );
        assert_eq!(
            coverage.lines[0].summary.covered_quantity,
            Quantity::from_str("2").unwrap()
        );
        assert_eq!(
            coverage.lines[0].summary.remaining_quantity,
            Quantity::from_str("8").unwrap()
        );
        assert_eq!(
            coverage.summary.covered_quantity,
            Quantity::from_str("2").unwrap()
        );

        let mut full = facts_with_two_coverage_sources();
        full.submission_lines = vec![submission_line("subl-1", "sub-1", Some("sol-1"), Some("10"))];
        full.purchase_revision_lines.clear();
        full.allocations.clear();
        full.reservations.clear();
        let full = build_procurement_coverage(full).unwrap();
        assert_eq!(
            full.lines[0].summary.remaining_quantity,
            Quantity::from_str("0").unwrap()
        );
        assert_eq!(full.lines[0].summary.progress, Rate::from_str("1").unwrap());

        let mut zero = facts_with_two_coverage_sources();
        zero.submission_lines.clear();
        zero.purchase_revision_lines.clear();
        zero.allocations.clear();
        zero.reservations.clear();
        let zero = build_procurement_coverage(zero).unwrap();
        assert_eq!(
            zero.lines[0].summary.covered_quantity,
            Quantity::from_str("0").unwrap()
        );
        assert_eq!(zero.lines[0].summary.progress, Rate::from_str("0").unwrap());
    }

    /// 覆盖超过销售当前版本数量时构造失败且不截断为负剩余。
    #[test]
    fn coverage_build_rejects_over_coverage() {
        let mut facts = facts_with_two_coverage_sources();
        facts.submission_lines = vec![submission_line("subl-1", "sub-1", Some("sol-1"), Some("11"))];
        facts.purchase_revision_lines.clear();
        facts.allocations.clear();
        facts.reservations.clear();
        let error = build_procurement_coverage(facts).unwrap_err().to_string();
        assert!(error.contains("采购数量一致性错误"));
    }

    /// 已从销售当前版本移除的稳定行：提交行与分配不再累计且不报错。
    #[test]
    fn removed_sales_line_is_ignored() {
        let mut facts = facts_with_two_coverage_sources();
        facts.submission_lines = vec![submission_line("subl-1", "sub-1", Some("sol-removed"), Some("2"))];
        facts.purchase_revision_lines = vec![purchase_revision_line(
            "porl-1",
            "rev-1",
            Some("sol-removed"),
            Some("sorl-other"),
        )];
        facts.allocations = vec![allocation("alloc-1", "porl-1", "sorl-other", "3")];
        facts.reservations.clear();

        let coverage = build_procurement_coverage(facts).unwrap();
        assert_eq!(coverage.lines.len(), 1);
        assert_eq!(
            coverage.lines[0].summary.covered_quantity,
            Quantity::from_str("0").unwrap()
        );
        assert_eq!(
            coverage.summary.remaining_quantity,
            Quantity::from_str("10").unwrap()
        );
    }

    /// 正式分配绑定到错误的销售当前版本行时按一致性错误失败。
    #[test]
    fn wrong_version_allocation_is_rejected() {
        let mut facts = facts_with_two_coverage_sources();
        facts.allocations = vec![allocation("alloc-1", "porl-1", "sorl-stale", "3")];
        let error = build_procurement_coverage(facts).unwrap_err().to_string();
        assert_eq!(error, "采购正式分配未绑定销售当前版本行");
    }

    /// 正式商品行缺少唯一销售分配时按一致性错误失败。
    #[test]
    fn missing_unique_allocation_is_rejected() {
        let mut facts = facts_with_two_coverage_sources();
        facts.allocations.clear();
        let error = build_procurement_coverage(facts).unwrap_err().to_string();
        assert_eq!(error, "采购当前版本商品行缺少唯一销售分配");
    }

    /// 现有库存预占按 reserved + consumed 计入且与采购覆盖互不重复累计。
    #[test]
    fn stock_coverage_counts_reserved_plus_consumed_once() {
        let facts = facts_with_two_coverage_sources();
        let coverage = build_procurement_coverage(facts).unwrap();
        // 提交行 2 + 正式分配 3 + 现有库存预占 2 = 7。
        assert_eq!(
            coverage.lines[0].summary.covered_quantity,
            Quantity::from_str("7").unwrap()
        );
        assert_eq!(
            coverage.lines[0].summary.remaining_quantity,
            Quantity::from_str("3").unwrap()
        );
    }

    /// 相同事实重复构造产生完全一致的结果（确定性）。
    #[test]
    fn coverage_build_is_deterministic() {
        let first = build_procurement_coverage(facts_with_two_coverage_sources()).unwrap();
        let second = build_procurement_coverage(facts_with_two_coverage_sources()).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first.lines[0].summary.remaining_quantity,
            Quantity::from_str("3").unwrap()
        );
    }

    /// 商品类型缺失时按一致性错误失败。
    #[test]
    fn missing_product_kind_is_rejected() {
        let mut facts = facts_with_two_coverage_sources();
        facts.products.clear();
        let error = build_procurement_coverage(facts).unwrap_err().to_string();
        assert!(error.contains("SKU sku-1 所属商品不存在"));
    }

    /// 测试构造确保当前版本目标模型以稳定行和版本行双重定位。
    #[test]
    fn target_line_model_keeps_stable_and_revision_identity() {
        let revision_line = revision_line("sorl-1", "sol-1", 1);
        let goods = goods_line("sorl-1", "sku-1", "2");
        let summary =
            ProcurementCoverageSummary::new(goods.quantity, Quantity::from_str("1").unwrap()).unwrap();

        assert_eq!(revision_line.sales_order_line_id.to_string(), "sol-1");
        assert_eq!(summary.remaining_quantity, Quantity::from_str("1").unwrap());
    }
}
