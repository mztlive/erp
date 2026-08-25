//! 销售当前版本的采购数量覆盖查询。
//!
//! 草稿、旧待财务与审批中采购只读取 `current_submission_id`；生效、部分执行与
//! 已完成采购只读取 `current_revision_id` 及其销售分配。历史提交、历史采购版本
//! 与作废采购单均不进入覆盖量。

use std::collections::{HashMap, HashSet};

use database::{CatalogExt, Executor, PurchaseOrderExt, SalesOrderExt};
use entities::catalog::ProductKind;
use entities::ids::{
    PurchaseOrderRevisionId, PurchaseOrderRevisionLineId, PurchaseOrderSubmissionId, SalesOrderRevisionId,
    SalesOrderRevisionLineId, SkuId,
};
use entities::money::Quantity;
use entities::purchase_order::{PurchaseLineType, PurchaseOrder, PurchaseOrderStatus};
use entities::sales_order::{
    LineType, ProcurementCoverageSummary, SalesOrder, SalesOrderGoodsServiceLineRevision, SalesOrderRevision,
    SalesOrderRevisionLine,
};
use rust_decimal::Decimal;

use crate::errors::{Error, Result};

/// 当前销售版本单行的采购覆盖信息。
#[derive(Debug, Clone)]
pub(crate) struct SalesProcurementCoverageLine {
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
#[derive(Debug, Clone)]
pub(crate) struct SalesProcurementCoverage {
    /// 销售当前版本头。
    pub revision: SalesOrderRevision,
    /// 当前版本商品/服务行及逐行覆盖。
    pub lines: Vec<SalesProcurementCoverageLine>,
    /// 当前版本全部商品/服务行汇总。
    pub summary: ProcurementCoverageSummary,
}

/// 加载销售单当前版本及采购覆盖数量。
///
/// # 参数
/// * `db` - MongoDB 数据库实例
/// * `order` - 已加载的销售稳定单
/// * `executor` - 数据访问执行器；创建命令必须传入事务会话
///
/// # 返回
/// 返回当前销售版本商品/服务目标行、逐行覆盖与总汇总。
///
/// # 错误
/// 当前版本缺失、当前采购指针缺失、正式分配未绑定销售当前版本行、覆盖超过目标
/// 或仓储查询失败时返回错误。
///
/// # 关键业务约束
/// 只沿销售与采购的当前指针读取，稳定关联键为 `sales_order_line_id`。
pub(crate) async fn load_sales_procurement_coverage(
    db: &mongodb::Database,
    order: &SalesOrder,
    executor: &mut dyn Executor,
) -> Result<SalesProcurementCoverage> {
    let revision = load_current_sales_revision(db, order, executor).await?;
    let target_lines = load_target_lines(db, &revision, executor).await?;
    let product_kinds = load_product_kinds(db, &target_lines, executor).await?;
    let purchase_orders = db
        .purchase_orders()
        .find_covering_by_sales_order(&order.base.id.clone().into(), executor)
        .await?;
    let covered = load_covered_quantities(db, &target_lines, &purchase_orders, executor).await?;
    build_coverage(revision, target_lines, covered, product_kinds)
}

/// 批量解析采购目标行 SKU 对应的商品业务类型。
///
/// # 参数
/// * `db` - MongoDB 数据库实例
/// * `targets` - 当前版本商品目标行
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回以 SKU 稳定 ID 为键的商品业务类型。
///
/// # 错误
/// SKU、商品缺失或仓储查询失败时返回错误。
///
/// # 关键业务约束
/// 采购类型只读取商品稳定主表的 `product_kind`，不得从销售字段或分类名称推导。
async fn load_product_kinds(
    db: &mongodb::Database,
    targets: &[(SalesOrderRevisionLine, SalesOrderGoodsServiceLineRevision)],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, ProductKind>> {
    let sku_ids = targets
        .iter()
        .map(|(_, goods)| goods.sku_id.clone())
        .collect::<Vec<SkuId>>();
    let skus = db.skus().find_by_ids(&sku_ids, executor).await?;
    let sku_to_product = skus
        .into_iter()
        .map(|sku| (sku.base.id, sku.product_id))
        .collect::<HashMap<_, _>>();
    let product_ids = sku_to_product.values().cloned().collect::<Vec<_>>();
    let products = db.products().find_by_ids(&product_ids, executor).await?;
    let product_kinds = products
        .into_iter()
        .map(|product| (product.base.id, product.product_kind))
        .collect::<HashMap<_, _>>();
    targets
        .iter()
        .map(|(_, goods)| {
            let product_id = sku_to_product.get(goods.sku_id.as_ref()).ok_or_else(|| {
                Error::BusinessLogicError(format!("销售当前版本 SKU {} 不存在", goods.sku_id))
            })?;
            let product_kind = product_kinds.get(product_id.as_ref()).copied().ok_or_else(|| {
                Error::BusinessLogicError(format!("销售当前版本 SKU {} 所属商品不存在", goods.sku_id))
            })?;
            Ok((goods.sku_id.to_string(), product_kind))
        })
        .collect()
}

/// 加载销售单当前版本头。
///
/// # 参数
/// * `db` - MongoDB 数据库实例
/// * `order` - 销售稳定单
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回 `stable.current_revision_id` 指向的销售版本。
///
/// # 错误
/// 当前版本指针或版本文档缺失、仓储查询失败时返回错误。
///
/// # 关键业务约束
/// 不允许回退到最近提交或最近创建版本。
async fn load_current_sales_revision(
    db: &mongodb::Database,
    order: &SalesOrder,
    executor: &mut dyn Executor,
) -> Result<SalesOrderRevision> {
    let revision_id = order
        .stable
        .current_revision_id
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本，无法计算采购剩余量".to_string()))?;
    db.sales_order_revisions()
        .find_by_id(revision_id, executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("销售单当前版本不存在，无法计算采购剩余量".to_string()))
}

/// 加载当前销售版本的商品/服务目标行。
///
/// # 参数
/// * `db` - MongoDB 数据库实例
/// * `revision` - 销售当前版本头
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回按销售版本行号升序排列的公共行与商品子类型行。
///
/// # 错误
/// 商品公共行缺少对应子类型行或仓储查询失败时返回错误。
///
/// # 关键业务约束
/// 仅 `GOODS_SERVICE` 行形成采购目标数量。
async fn load_target_lines(
    db: &mongodb::Database,
    revision: &SalesOrderRevision,
    executor: &mut dyn Executor,
) -> Result<Vec<(SalesOrderRevisionLine, SalesOrderGoodsServiceLineRevision)>> {
    let revision_id = SalesOrderRevisionId::new(revision.base.id.clone());
    let lines = db
        .sales_order_revision_lines()
        .list_lines_by_revision(&revision_id, executor)
        .await?;
    let goods_ids = lines
        .iter()
        .filter(|line| line.line_type == LineType::GoodsService)
        .map(|line| SalesOrderRevisionLineId::new(line.base.id.clone()))
        .collect::<Vec<_>>();
    let goods = db
        .sales_order_goods_service_line_revisions()
        .list_by_revision_line_ids(&goods_ids, executor)
        .await?;
    join_target_lines(lines, goods)
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
                .ok_or_else(|| Error::BusinessLogicError("销售当前版本商品行缺少子类型数据".to_string()))?;
            Ok((line, goods_line))
        })
        .collect()
}

/// 按采购状态和当前指针汇总逐销售稳定行覆盖数量。
///
/// # 参数
/// * `db` - MongoDB 数据库实例
/// * `targets` - 当前销售版本商品目标行
/// * `orders` - 未作废且参与覆盖的采购单
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回以稳定 `sales_order_line_id` 为键的覆盖数量。
///
/// # 错误
/// 当前采购指针缺失、正式分配未指向销售当前版本行或仓储查询失败时返回错误。
///
/// # 关键业务约束
/// 草稿类状态只读当前提交；正式状态只读当前采购版本及 allocation。
async fn load_covered_quantities(
    db: &mongodb::Database,
    targets: &[(SalesOrderRevisionLine, SalesOrderGoodsServiceLineRevision)],
    orders: &[PurchaseOrder],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, Quantity>> {
    let mut covered = HashMap::new();
    add_submission_coverage(db, targets, orders, &mut covered, executor).await?;
    add_revision_coverage(db, targets, orders, &mut covered, executor).await?;
    Ok(covered)
}

/// 汇总草稿、旧待财务与审批中采购的当前提交行。
///
/// # 参数
/// * `db` - MongoDB 数据库实例
/// * `targets` - 当前销售版本商品目标行
/// * `orders` - 参与覆盖的采购单
/// * `covered` - 待累加的稳定销售行覆盖映射
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 汇总成功返回 `Ok(())`。
///
/// # 错误
/// 当前提交指针缺失或仓储查询失败时返回错误。
///
/// # 关键业务约束
/// 禁止读取同一采购单的历史提交行。
async fn add_submission_coverage(
    db: &mongodb::Database,
    targets: &[(SalesOrderRevisionLine, SalesOrderGoodsServiceLineRevision)],
    orders: &[PurchaseOrder],
    covered: &mut HashMap<String, Quantity>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let target_ids = target_stable_ids(targets);
    let submission_ids = current_submission_ids(orders)?;
    let lines = db
        .purchase_order_submission_lines()
        .find_lines_by_submission_ids(&submission_ids, executor)
        .await?;
    for line in lines
        .into_iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
    {
        let stable_id = line
            .sales_order_line_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("采购当前提交行缺少销售稳定行".to_string()))?
            .to_string();
        if !target_ids.contains_key(&stable_id) {
            continue;
        }
        let quantity = line
            .allocated_quantity
            .ok_or_else(|| Error::BusinessLogicError("采购当前提交行缺少分配数量".to_string()))?;
        add_covered(covered, &stable_id, quantity)?;
    }
    Ok(())
}

/// 汇总生效、部分执行与已完成采购的当前版本销售分配。
///
/// # 参数
/// * `db` - MongoDB 数据库实例
/// * `targets` - 当前销售版本商品目标行
/// * `orders` - 参与覆盖的采购单
/// * `covered` - 待累加的稳定销售行覆盖映射
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 汇总成功返回 `Ok(())`。
///
/// # 错误
/// 当前版本指针缺失、正式行或 allocation 未绑定销售当前版本行、仓储查询失败时返回错误。
///
/// # 关键业务约束
/// 禁止累计历史采购版本或脱离 allocation 直接使用采购行数量。
async fn add_revision_coverage(
    db: &mongodb::Database,
    targets: &[(SalesOrderRevisionLine, SalesOrderGoodsServiceLineRevision)],
    orders: &[PurchaseOrder],
    covered: &mut HashMap<String, Quantity>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let targets = target_stable_ids(targets);
    let revision_ids = current_revision_ids(orders)?;
    let lines = db
        .purchase_order_revision_lines()
        .find_lines_by_revision_ids(&revision_ids, executor)
        .await?;
    let line_ids = lines
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
        .map(|line| PurchaseOrderRevisionLineId::new(line.base.id.clone()))
        .collect::<Vec<_>>();
    let allocations = db
        .purchase_line_sales_allocations()
        .find_by_purchase_revision_line_ids(&line_ids, executor)
        .await?;
    let lines = lines
        .into_iter()
        .map(|line| (line.base.id.clone(), line))
        .collect::<HashMap<_, _>>();
    let expected = lines
        .iter()
        .filter(|(_, line)| line.line_type == PurchaseLineType::ItemService)
        .map(|(id, _)| id.clone())
        .collect::<HashSet<_>>();
    let mut allocated = HashSet::new();
    for allocation in allocations {
        allocated.insert(allocation.purchase_order_revision_line_id.to_string());
        add_current_allocation(&targets, &lines, allocation, covered)?;
    }
    if allocated != expected {
        return Err(Error::BusinessLogicError(
            "采购当前版本商品行缺少唯一销售分配".to_string(),
        ));
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
///
/// # 关键业务约束
/// 正式覆盖必须同时由采购当前版本行和 allocation 指向同一销售当前版本行。
fn add_current_allocation(
    targets: &HashMap<String, String>,
    purchase_lines: &HashMap<String, entities::purchase_order::PurchaseOrderRevisionLine>,
    allocation: entities::purchase_order::PurchaseLineSalesAllocation,
    covered: &mut HashMap<String, Quantity>,
) -> Result<()> {
    let purchase_line = purchase_lines
        .get(&allocation.purchase_order_revision_line_id.to_string())
        .ok_or_else(|| Error::BusinessLogicError("采购正式分配缺少当前采购版本行".to_string()))?;
    let stable_id = purchase_line
        .sales_order_line_id
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("采购当前版本行缺少销售稳定行".to_string()))?
        .to_string();
    let Some(current_sales_line_id) = targets.get(&stable_id) else {
        return Ok(());
    };
    let recorded_sales_line_id = purchase_line
        .sales_order_revision_line_id
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("采购当前版本行缺少销售当前版本行".to_string()))?;
    if recorded_sales_line_id.to_string() != *current_sales_line_id
        || allocation.sales_order_revision_line_id.to_string() != *current_sales_line_id
    {
        return Err(Error::BusinessLogicError(
            "采购正式分配未绑定销售当前版本行".to_string(),
        ));
    }
    add_covered(covered, &stable_id, allocation.allocated_quantity)
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
                .ok_or_else(|| Error::BusinessLogicError("采购草稿类状态缺少当前提交指针".to_string()))
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
                .ok_or_else(|| Error::BusinessLogicError("采购正式状态缺少当前版本指针".to_string()))
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
        .map_err(|error| Error::BusinessLogicError(format!("采购覆盖数量精度非法: {error}")))?;
    covered.insert(sales_order_line_id.to_string(), value);
    Ok(())
}

/// 构造逐行与总体采购覆盖结果。
///
/// # 参数
/// * `revision` - 销售当前版本头
/// * `targets` - 当前版本商品目标行
/// * `covered` - 稳定销售行覆盖数量
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
            .ok_or_else(|| {
                Error::BusinessLogicError(format!("销售当前版本 SKU {} 缺少商品类型", goods_line.sku_id))
            })?;
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

/// 构造覆盖值对象并统一映射为业务一致性错误。
///
/// # 参数
/// * `total` - 目标数量
/// * `covered` - 覆盖数量
///
/// # 返回
/// 返回采购覆盖值对象。
///
/// # 错误
/// 数量不一致时返回 `BusinessLogicError`。
///
/// # 关键业务约束
/// 所有查询入口共享同一错误语义。
fn coverage_summary(total: Quantity, covered: Quantity) -> Result<ProcurementCoverageSummary> {
    ProcurementCoverageSummary::new(total, covered)
        .map_err(|error| Error::BusinessLogicError(format!("采购数量一致性错误: {error}")))
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
    Quantity::try_from(value).map_err(|error| Error::BusinessLogicError(format!("采购数量精度非法: {error}")))
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

    use entities::ids::{
        PurchaseOrderId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId, SalesOrderRevisionLineId,
        SupplierAccountId,
    };
    use entities::money::{Amount, Quantity, Rate};
    use entities::purchase_order::{
        FulfillmentResponsibility, PurchaseOrder, PurchaseOrderData, PurchaseOrderStatus, PurchaseType,
    };
    use entities::sales_order::{
        LineType, ProcurementCoverageSummary, SalesOrderGoodsServiceLineRevision,
        SalesOrderGoodsServiceLineRevisionData, SalesOrderRevisionLine, SalesOrderRevisionLineData,
    };

    use super::{add_covered, coverage_summary, current_revision_ids, current_submission_ids, zero_quantity};

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
            },
            "buyer-1",
        )
        .unwrap();
        order.stable.status = status;
        order.current_submission_id = submission_id.map(str::to_string);
        order.stable.current_revision_id = revision_id.map(str::to_string);
        order
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

    /// 覆盖超过销售当前版本数量时统一返回业务一致性错误。
    #[test]
    fn over_coverage_is_business_consistency_error() {
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

    /// 测试构造确保当前版本目标模型以稳定行和版本行双重定位。
    #[test]
    fn target_line_model_keeps_stable_and_revision_identity() {
        let revision_line = SalesOrderRevisionLine::new(
            SalesOrderRevisionLineId::new("sorl-1"),
            SalesOrderRevisionLineData {
                sales_order_revision_id: SalesOrderRevisionId::new("sor-1"),
                sales_order_line_id: SalesOrderLineId::new("sol-1"),
                line_no: 1,
                line_type: LineType::GoodsService,
                gross_amount: Amount::from_str("10").unwrap(),
                net_amount: Amount::from_str("10").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                sales_tax_rate: Rate::from_str("0").unwrap(),
                item_name_snapshot: "商品".to_string(),
                spec_snapshot: Some("规格".to_string()),
                unit_snapshot: Some("件".to_string()),
            },
        )
        .unwrap();
        let goods = SalesOrderGoodsServiceLineRevision::new(
            entities::ids::SalesOrderGoodsServiceLineRevisionId::new("sogslr-1"),
            SalesOrderGoodsServiceLineRevisionData {
                revision_line_id: SalesOrderRevisionLineId::new(revision_line.base.id.clone()),
                sku_id: entities::ids::SkuId::new("sku-1"),
                sku_revision_id: entities::ids::SkuRevisionId::new("skur-1"),
                welfare_scenario: None,
                service_region: None,
                fulfillment_due_at: entities::common::time::Instant::from_unix_secs(1_800_000_000),
                quantity: Quantity::from_str("2").unwrap(),
                base_unit_code: "件".to_string(),
                unit_price_gross: entities::money::UnitPrice::from_str("5").unwrap(),
            },
        )
        .unwrap();
        let summary =
            ProcurementCoverageSummary::new(goods.quantity, Quantity::from_str("1").unwrap()).unwrap();

        assert_eq!(revision_line.sales_order_line_id.to_string(), "sol-1");
        assert_eq!(summary.remaining_quantity, Quantity::from_str("1").unwrap());
    }
}
