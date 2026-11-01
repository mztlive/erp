//! 采购行跨版本累计（第一期 §6.3 采购改单）。
//!
//! 入库等履约事实记录在形成时的采购版本行上；采购改单生效后形成新版本行。累计
//! 已发生数量必须按稳定销售行跨版本汇总，不能因版本行主键变化而清零。

use std::collections::HashMap;

use erp_core::ids::PurchaseOrderRevisionLineId;
use erp_core::money::Quantity;
use erp_core::{Error, Result};
use rust_decimal::Decimal;

use super::{ProgressStatus, PurchaseLineType, PurchaseOrderRevisionLine};

/// 返回采购行跨版本的稳定键。
///
/// # 参数
/// * `line` - 任一版本的采购行
///
/// # 返回
/// 商品行返回销售稳定行（历史数据缺失时取采购二次确认分行）；物流费用行返回版本行主键。
///
/// # 错误
/// 不返回错误。
pub fn purchase_line_stable_key(line: &PurchaseOrderRevisionLine) -> String {
    if line.line_type == PurchaseLineType::ItemService
        && let Some(stable) = line
            .sales_order_line_id
            .as_ref()
            .map(ToString::to_string)
            .or_else(|| line.procurement_confirmation_line_id.as_ref().map(ToString::to_string))
    {
        return stable;
    }
    line.base.id.clone()
}

/// 把按版本行记录的数量汇总到稳定键。
///
/// # 参数
/// * `all_lines` - 本采购全部版本的版本行
/// * `by_revision_line` - 版本行主键到数量
///
/// # 返回
/// 返回稳定键到累计数量；找不到版本行的记录按其主键单独计。
///
/// # 错误
/// 累计超出数量范围时返回领域错误。
pub fn rollup_by_stable_key(
    all_lines: &[PurchaseOrderRevisionLine],
    by_revision_line: &HashMap<PurchaseOrderRevisionLineId, Quantity>,
) -> Result<HashMap<String, Quantity>> {
    let keys = all_lines
        .iter()
        .map(|line| (line.base.id.as_str(), purchase_line_stable_key(line)))
        .collect::<HashMap<_, _>>();
    let mut totals: HashMap<String, Decimal> = HashMap::new();
    for (line_id, value) in by_revision_line {
        let key = keys.get(line_id.as_ref()).cloned().unwrap_or_else(|| line_id.to_string());
        let total = totals.entry(key).or_default();
        *total = total.checked_add(value.to_decimal()).ok_or_else(|| Error::from("累计数量超出范围"))?;
    }
    totals.into_iter().map(|(key, value)| Ok((key, Quantity::try_from(value)?))).collect()
}

/// 把跨版本累计数量归到当前版本行主键。
///
/// # 参数
/// * `current_lines` - 采购当前生效版本行
/// * `all_lines` - 本采购全部版本的版本行
/// * `by_revision_line` - 版本行主键到数量
///
/// # 返回
/// 返回当前版本行主键到同一稳定键的历史累计数量；没有累计的行不出现。
///
/// # 错误
/// 累计超出数量范围时返回领域错误。
pub fn rekey_to_current_lines(
    current_lines: &[PurchaseOrderRevisionLine],
    all_lines: &[PurchaseOrderRevisionLine],
    by_revision_line: &HashMap<PurchaseOrderRevisionLineId, Quantity>,
) -> Result<HashMap<PurchaseOrderRevisionLineId, Quantity>> {
    let totals = rollup_by_stable_key(all_lines, by_revision_line)?;
    Ok(current_lines
        .iter()
        .filter_map(|line| {
            totals
                .get(&purchase_line_stable_key(line))
                .map(|value| (PurchaseOrderRevisionLineId::new(line.base.id.clone()), *value))
        })
        .collect())
}

/// 按当前版本数量与跨版本累计入库判断入库进度。
///
/// # 参数
/// * `current_lines` - 采购当前生效版本行
/// * `received` - 当前版本行主键到跨版本累计合格入库数量
///
/// # 返回
/// 商品数量合计为零或每行累计都达到当前数量时返回已完成；有入库但未完成返回部分；没有入库返回未开始。
///
/// # 错误
/// 不返回错误。
pub fn receipt_progress_after_change(
    current_lines: &[PurchaseOrderRevisionLine],
    received: &HashMap<PurchaseOrderRevisionLineId, Quantity>,
) -> ProgressStatus {
    let mut total = Decimal::ZERO;
    let mut reached = Decimal::ZERO;
    let mut any_received = false;
    for line in current_lines.iter().filter(|line| line.line_type == PurchaseLineType::ItemService) {
        let ordered = line.quantity.map_or(Decimal::ZERO, |value| value.to_decimal());
        let got = received
            .get(&PurchaseOrderRevisionLineId::new(line.base.id.clone()))
            .map_or(Decimal::ZERO, |value| value.to_decimal());
        any_received |= got > Decimal::ZERO;
        total += ordered;
        reached += got.min(ordered);
    }
    if reached >= total {
        ProgressStatus::Completed
    } else if any_received {
        ProgressStatus::Partial
    } else {
        ProgressStatus::None
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{
        ProcurementConfirmationLineId, PurchaseOrderRevisionId, SalesOrderLineId, SalesOrderRevisionLineId,
        SkuId,
    };
    use erp_core::money::{Rate, UnitPrice, line_amounts};

    use super::*;
    use crate::entity::purchase_order::PurchaseOrderRevisionLineData;

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn line(id: &str, revision: &str, stable: &str, quantity: &str) -> PurchaseOrderRevisionLine {
        let quantity = qty(quantity);
        let cost = UnitPrice::from_str("5").unwrap();
        let tax = Rate::from_str("0").unwrap();
        let (gross, net, tax_amount) = line_amounts(cost, quantity, tax);
        PurchaseOrderRevisionLine::new(
            PurchaseOrderRevisionLineId::new(id),
            PurchaseOrderRevisionLineData {
                purchase_order_revision_id: PurchaseOrderRevisionId::new(revision),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new(stable)),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: None,
                supplier_offering_source: None,
                delivery_allocations: Vec::new(),
                product_name_snapshot: Some("礼盒".into()),
                specification_snapshot: None,
                quantity: Some(quantity),
                base_unit_code: Some("件".into()),
                unit_cost_gross: Some(cost),
                gross_amount: gross,
                net_amount: net,
                tax_amount,
                input_tax_rate: Some(tax),
                expected_delivery_date: None,
                sales_order_line_id: Some(SalesOrderLineId::new(stable)),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sorl-1")),
                allocated_quantity: Some(quantity),
                expense: None,
            },
        )
        .unwrap()
    }

    fn received(rows: &[(&str, &str)]) -> HashMap<PurchaseOrderRevisionLineId, Quantity> {
        rows.iter().map(|(id, value)| (PurchaseOrderRevisionLineId::new(*id), qty(value))).collect()
    }

    /// 改单形成新版本行后，旧版本行上的入库仍计入同一稳定行。
    #[test]
    fn received_on_old_revision_counts_for_current_line() {
        let old = line("v1-a", "por-1", "sol-a", "10");
        let current = line("v2-a", "por-2", "sol-a", "8");
        let rows = received(&[("v1-a", "6"), ("v2-a", "1")]);

        let rekeyed =
            rekey_to_current_lines(std::slice::from_ref(&current), &[old, current.clone()], &rows).unwrap();

        assert_eq!(rekeyed.get(&PurchaseOrderRevisionLineId::new("v2-a")), Some(&qty("7")));
    }

    /// 入库进度按当前数量判断：调减到已入库数量即完成；全部归零也视为完成。
    #[test]
    fn progress_follows_current_quantities() {
        let partial = line("v2-a", "por-2", "sol-a", "8");
        let rows = received(&[("v2-a", "7")]);
        assert_eq!(receipt_progress_after_change(&[partial], &rows), ProgressStatus::Partial);

        let reduced = line("v2-a", "por-2", "sol-a", "7");
        assert_eq!(receipt_progress_after_change(&[reduced], &rows), ProgressStatus::Completed);

        let untouched = line("v2-a", "por-2", "sol-a", "7");
        assert_eq!(receipt_progress_after_change(&[untouched], &HashMap::new()), ProgressStatus::None);
    }
}
