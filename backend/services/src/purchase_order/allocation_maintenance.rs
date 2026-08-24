//! 采购当前版本到销售当前版本的正式分配维护。
//!
//! 正式 allocation 必须按采购版本行携带的稳定 `sales_order_line_id`，绑定来源
//! 销售单 `current_revision_id` 下的当前销售版本行。采购变更新当前版本时重新生成
//! 该版本自己的 allocation，历史采购版本 allocation 保持不可变且不参与当前覆盖。

use std::collections::HashMap;

use database::{Executor, PurchaseOrderExt, SalesOrderExt};
use entities::ids::{PurchaseLineSalesAllocationId, PurchaseOrderRevisionLineId, SalesOrderRevisionId};
use entities::purchase_order::{
    PurchaseLineSalesAllocation, PurchaseLineSalesAllocationData, PurchaseLineType, PurchaseOrder,
    PurchaseOrderRevisionLine,
};
use id_generator::next_id;

use crate::errors::{Error, Result};

/// 待写入的当前采购版本销售分配。
pub(super) struct PreparedSalesAllocations {
    /// 采购版本行 ID 到 allocation ID，供履约草稿引用。
    pub by_purchase_line: HashMap<String, PurchaseLineSalesAllocationId>,
    /// 待与采购版本同事务写入的 allocation 实体。
    allocations: Vec<PurchaseLineSalesAllocation>,
}

/// 按来源销售单当前版本重建采购版本行关联并构造正式 allocation。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 来源采购单
/// * `revision_lines` - 即将成为当前采购版本的行；函数会刷新销售当前版本行字段
/// * `executor` - 数据访问执行器，正式化与变更生效必须传入事务会话
///
/// # 返回
/// 返回待写入 allocation 及采购版本行到 allocation 的引用映射。
///
/// # 错误
/// 来源销售单、当前销售版本或稳定销售行缺失，分配数量与采购数量不一致，或仓储
/// 查询失败时返回错误。
///
/// # 关键业务约束
/// 只查询来源销售单 `stable.current_revision_id`，不在历史销售版本中猜测匹配。
pub(super) async fn prepare_current_sales_allocations(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    revision_lines: &mut [PurchaseOrderRevisionLine],
    executor: &mut dyn Executor,
) -> Result<PreparedSalesAllocations> {
    let sales_order = db
        .sales_orders()
        .find_by_id(&order.sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
    let revision_id = sales_order
        .stable
        .current_revision_id
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前版本".to_string()))?;
    let sales_lines = db
        .sales_order_revision_lines()
        .list_lines_by_revision(&SalesOrderRevisionId::new(revision_id.clone()), executor)
        .await?;
    let sales_lines = sales_lines
        .into_iter()
        .map(|line| (line.sales_order_line_id.to_string(), line))
        .collect::<HashMap<_, _>>();
    build_allocations(revision_lines, &sales_lines)
}

/// 将已准备的 allocation 写入当前事务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `prepared` - 已按销售当前版本构造的 allocation
/// * `executor` - 数据访问执行器，必须与采购版本写入使用同一事务
///
/// # 返回
/// 全部写入成功返回 `Ok(())`。
///
/// # 错误
/// 唯一键冲突或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 每个采购版本行只形成一条当前版本 allocation。
pub(super) async fn persist_current_sales_allocations(
    db: &mongodb::Database,
    prepared: &PreparedSalesAllocations,
    executor: &mut dyn Executor,
) -> Result<()> {
    for allocation in &prepared.allocations {
        db.purchase_line_sales_allocations()
            .create(allocation, executor)
            .await?;
    }
    Ok(())
}

/// 依据稳定销售行刷新采购版本行并构造 allocation 实体。
///
/// # 参数
/// * `revision_lines` - 即将生效的采购版本行
/// * `sales_lines` - 稳定销售行到销售当前版本行的映射
///
/// # 返回
/// 返回全部商品/服务采购行的正式 allocation。
///
/// # 错误
/// 商品行缺少稳定关联、当前销售版本行或合法分配数量时返回一致性错误。
///
/// # 关键业务约束
/// 分配数量必须等于该采购版本行数量，物流费用行不形成 allocation。
fn build_allocations(
    revision_lines: &mut [PurchaseOrderRevisionLine],
    sales_lines: &HashMap<String, entities::sales_order::SalesOrderRevisionLine>,
) -> Result<PreparedSalesAllocations> {
    let mut by_purchase_line = HashMap::new();
    let mut allocations = Vec::new();
    for line in revision_lines {
        if line.line_type != PurchaseLineType::ItemService {
            continue;
        }
        let stable_id = line
            .sales_order_line_id
            .as_ref()
            .map(ToString::to_string)
            .or_else(|| {
                line.procurement_confirmation_line_id
                    .as_ref()
                    .map(ToString::to_string)
            })
            .ok_or_else(|| Error::BusinessLogicError("采购版本行缺少销售稳定行".to_string()))?;
        let sales_line = sales_lines.get(&stable_id).ok_or_else(|| {
            Error::BusinessLogicError("采购版本行在销售当前版本中没有对应稳定行".to_string())
        })?;
        let quantity = line
            .quantity
            .ok_or_else(|| Error::BusinessLogicError("采购版本行缺少数量".to_string()))?;
        let allocated = line
            .allocated_quantity
            .ok_or_else(|| Error::BusinessLogicError("采购版本行缺少分配数量".to_string()))?;
        if allocated != quantity {
            return Err(Error::BusinessLogicError(
                "采购版本行分配数量必须等于采购数量".to_string(),
            ));
        }
        line.sales_order_line_id = Some(sales_line.sales_order_line_id.clone());
        line.sales_order_revision_line_id = Some(sales_line.base.id.clone().into());
        let allocation_id = PurchaseLineSalesAllocationId::new(next_id());
        let allocation = PurchaseLineSalesAllocation::new(
            allocation_id.clone(),
            PurchaseLineSalesAllocationData {
                purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new(line.base.id.clone()),
                sales_order_revision_line_id: sales_line.base.id.clone().into(),
                allocated_quantity: allocated,
                allocated_cost_gross: line.gross_amount,
                allocated_cost_net: line.net_amount,
            },
        )
        .map_err(Error::Logic)?;
        by_purchase_line.insert(line.base.id.clone(), allocation_id);
        allocations.push(allocation);
    }
    Ok(PreparedSalesAllocations {
        by_purchase_line,
        allocations,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use entities::common::time::BusinessDate;
    use entities::ids::{
        ProcurementConfirmationLineId, PurchaseOrderRevisionId, PurchaseOrderRevisionLineId,
        SalesOrderLineId, SalesOrderRevisionId, SalesOrderRevisionLineId, SkuId,
    };
    use entities::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};
    use entities::purchase_order::{
        PurchaseLineType, PurchaseOrderRevisionLine, PurchaseOrderRevisionLineData,
    };
    use entities::sales_order::{LineType, SalesOrderRevisionLine, SalesOrderRevisionLineData};

    use super::build_allocations;

    /// 测试采购版本行。
    fn purchase_line(allocated: &str) -> PurchaseOrderRevisionLine {
        let quantity = Quantity::from_str("2").unwrap();
        let cost = UnitPrice::from_str("5").unwrap();
        let tax = Rate::from_str("0").unwrap();
        let (gross, net, tax_amount) = line_amounts(cost, quantity, tax);
        PurchaseOrderRevisionLine::new(
            PurchaseOrderRevisionLineId::new("porl-1"),
            PurchaseOrderRevisionLineData {
                purchase_order_revision_id: PurchaseOrderRevisionId::new("por-1"),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("sol-1")),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: None,
                product_name_snapshot: Some("商品".to_string()),
                specification_snapshot: Some("规格".to_string()),
                quantity: Some(quantity),
                base_unit_code: Some("件".to_string()),
                unit_cost_gross: Some(cost),
                gross_amount: gross,
                net_amount: net,
                tax_amount,
                input_tax_rate: Some(tax),
                expected_delivery_date: Some(BusinessDate::from_ymd(2026, 8, 24).unwrap()),
                sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("old-sorl")),
                allocated_quantity: Some(Quantity::from_str(allocated).unwrap()),
            },
        )
        .unwrap()
    }

    /// 测试销售当前版本行。
    fn sales_line() -> SalesOrderRevisionLine {
        SalesOrderRevisionLine::new(
            SalesOrderRevisionLineId::new("current-sorl"),
            SalesOrderRevisionLineData {
                sales_order_revision_id: SalesOrderRevisionId::new("current-sor"),
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
        .unwrap()
    }

    /// 正式分配刷新为销售当前版本行并保持稳定销售行。
    #[test]
    fn allocations_bind_current_sales_revision_line() {
        let mut purchase_lines = vec![purchase_line("2")];
        let sales_lines = HashMap::from([("sol-1".to_string(), sales_line())]);

        let prepared = build_allocations(&mut purchase_lines, &sales_lines).unwrap();

        assert_eq!(
            purchase_lines[0]
                .sales_order_revision_line_id
                .as_ref()
                .unwrap()
                .to_string(),
            "current-sorl"
        );
        assert_eq!(
            prepared.allocations[0].sales_order_revision_line_id.to_string(),
            "current-sorl"
        );
    }

    /// 分配数量与采购数量不一致时拒绝形成正式 allocation。
    #[test]
    fn allocations_reject_quantity_mismatch() {
        let mut purchase_lines = vec![purchase_line("1")];
        let sales_lines = HashMap::from([("sol-1".to_string(), sales_line())]);

        assert!(build_allocations(&mut purchase_lines, &sales_lines).is_err());
    }
}
