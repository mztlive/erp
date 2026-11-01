//! 在途采购改单对销售覆盖的预判（第一期 §6.3 采购改单执行规则）。
//!
//! 按当前冻结提交与销售当前覆盖计算每条改动数量的商品行：改后缺口在生效后进入
//! 供给分配，由采购负责人重新安排；生效时按当时事实复核。

use erp_core::money::Quantity;
use erp_procurement::entity::purchase_order::{
    ChangeQuantityBound, PurchaseChangeOrder, PurchaseLineType, PurchaseOrder,
};
use erp_procurement::repository::PurchaseOrderExt;
use mongodb::Database;
use persistence_core::Executor;

use super::super::change_bounds::load_change_quantity_facts;
use super::super::dto::PurchaseChangeCoverageEffectView;
use crate::{Error, Result};

/// 读取在途改单当前提交的销售覆盖影响。
///
/// # 参数
/// * `db` - 数据库
/// * `change` - 采购变更单
/// * `order` - 已证明可见的来源采购单
/// * `executor` - 调用方执行器
///
/// # 返回
/// 草稿已提交过或审批中时返回数量有变化的行；其余情况返回空列表。
///
/// # 错误
/// 提交行、覆盖事实或已发生数量读取失败时返回错误。
pub(super) async fn load_coverage_effects(
    db: &Database,
    change: &PurchaseChangeOrder,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseChangeCoverageEffectView>> {
    let Some(submission_id) = change.current_submission_id.as_ref() else {
        return Ok(Vec::new());
    };
    if !change.stable.status.is_in_progress() || !order.stable.status.allows_change() {
        return Ok(Vec::new());
    }
    let lines = db.purchase_order().list_change_submission_lines(submission_id, executor).await?;
    let target = lines
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
        .filter_map(|line| Some((line.sales_order_line_id.as_ref()?.to_string(), line.quantity?)))
        .collect::<Vec<_>>();
    let facts = load_change_quantity_facts(db, order, executor).await?;
    coverage_effects(&facts.bounds, &target)
}

/// 由数量范围与改后数量计算变化行的覆盖影响。
///
/// # 参数
/// * `bounds` - 采购当前生效版本商品行的数量范围
/// * `target` - 改单目标中的（稳定销售行，数量）
///
/// # 返回
/// 按采购行号返回数量有变化的行。
///
/// # 错误
/// 数量越界时返回错误。
fn coverage_effects(
    bounds: &[ChangeQuantityBound],
    target: &[(String, Quantity)],
) -> Result<Vec<PurchaseChangeCoverageEffectView>> {
    bounds
        .iter()
        .filter_map(|bound| {
            let requested = target.iter().find(|(line, _)| *line == bound.sales_order_line_id)?.1;
            (requested != bound.base_quantity).then_some((bound, requested))
        })
        .map(|(bound, requested)| {
            Ok(PurchaseChangeCoverageEffectView {
                sales_order_line_id: bound.sales_order_line_id.clone(),
                line_no: bound.line_no,
                item_name: bound.item_name.clone(),
                base_quantity: bound.base_quantity,
                target_quantity: requested,
                sales_quantity: bound.sales_quantity,
                other_covered_quantity: bound.other_covered_quantity,
                gap_quantity: bound.gap_after(requested).map_err(Error::Logic)?,
                excess_quantity: bound.excess_after(requested).map_err(Error::Logic)?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn bound(line: &str, base: &str, sales: &str, other: &str) -> ChangeQuantityBound {
        ChangeQuantityBound {
            sales_order_line_id: line.into(),
            line_no: 1,
            item_name: "礼盒".into(),
            base_quantity: qty(base),
            sales_quantity: qty(sales),
            other_covered_quantity: qty(other),
            fulfilled_quantity: qty("0"),
            quantity_affected: false,
        }
    }

    /// 只列出数量变化的行；改为 0 时整行需要重新供给。
    #[test]
    fn only_changed_lines_report_gap_after_change() {
        let bounds = vec![bound("sol-a", "5", "10", "5"), bound("sol-b", "3", "3", "0")];
        let target = vec![("sol-a".to_string(), qty("0")), ("sol-b".to_string(), qty("3"))];

        let effects = coverage_effects(&bounds, &target).unwrap();

        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].sales_order_line_id, "sol-a");
        assert_eq!(effects[0].gap_quantity, qty("5"));
        assert_eq!(effects[0].excess_quantity, qty("0"));
    }
}
