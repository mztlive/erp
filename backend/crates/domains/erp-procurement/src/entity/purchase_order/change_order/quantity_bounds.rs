//! 采购改单逐行可填数量范围（第一期 §6.3“实物与服务采购改单执行规则”）。
//!
//! 下限是本采购在该销售行上扣除已核实退回后的净入库、发货和交付数量，原事实保留；上限是
//! 销售当前目标扣除其他来源覆盖后的剩余量。未因销售减量受影响的行允许保持原数量，
//! 因销售归零或减量受影响的行必须调减到剩余量以内，否则应登记维持原采购。
//! 下限高于上限时以已发生数量为准，超出部分走采购退货。

use std::collections::{HashMap, HashSet};

use erp_core::money::Quantity;
use erp_core::{Error, Result};
use rust_decimal::Decimal;

use super::super::{
    PurchaseLineType, PurchaseOrderRevisionLine, PurchaseSourceImpact, SalesProcurementCoverage,
    SourceImpactKind,
};

/// 一条采购商品行在改单中的数量事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeQuantityBound {
    /// 稳定销售行。
    pub sales_order_line_id: String,
    /// 采购当前版本行号。
    pub line_no: u32,
    /// 商品名称快照。
    pub item_name: String,
    /// 采购当前生效数量。
    pub base_quantity: Quantity,
    /// 销售当前版本目标数量；销售行已不在当前版本时为零。
    pub sales_quantity: Quantity,
    /// 本采购之外其他采购与库存的覆盖数量。
    pub other_covered_quantity: Quantity,
    /// 本采购在该行已发生的入库、发货和交付数量。
    pub fulfilled_quantity: Quantity,
    /// 是否因销售归零或减量而受影响。
    pub quantity_affected: bool,
}

impl ChangeQuantityBound {
    /// 返回可填写的最小数量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回已发生数量。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn min_quantity(&self) -> Quantity {
        self.fulfilled_quantity
    }

    /// 返回可填写的最大数量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 受影响行返回剩余量；未受影响行返回剩余量与原数量的较大者；两者都低于已发生数量时返回已发生数量。
    ///
    /// # 错误
    /// 数量超出精度范围时返回错误。
    pub fn max_quantity(&self) -> Result<Quantity> {
        let capacity = self.capacity();
        let upper =
            if self.quantity_affected { capacity } else { capacity.max(self.base_quantity.to_decimal()) };
        quantity(upper.max(self.fulfilled_quantity.to_decimal()))
    }

    /// 返回改为指定数量后该销售行仍待覆盖的数量。
    ///
    /// # 参数
    /// * `requested` - 本行改后数量
    ///
    /// # 返回
    /// 返回非负的缺口数量；缺口生效后进入供给分配。
    ///
    /// # 错误
    /// 数量超出精度范围时返回错误。
    pub fn gap_after(&self, requested: Quantity) -> Result<Quantity> {
        quantity((self.capacity() - requested.to_decimal()).max(Decimal::ZERO))
    }

    /// 返回改为指定数量后该销售行超出目标的数量。
    ///
    /// # 参数
    /// * `requested` - 本行改后数量
    ///
    /// # 返回
    /// 返回非负的超出数量。
    ///
    /// # 错误
    /// 数量超出精度范围时返回错误。
    pub fn excess_after(&self, requested: Quantity) -> Result<Quantity> {
        let covered = self.other_covered_quantity.to_decimal() + requested.to_decimal();
        quantity((covered - self.sales_quantity.to_decimal()).max(Decimal::ZERO))
    }

    /// 校验本行改后数量。
    ///
    /// # 参数
    /// * `requested` - 本行改后数量
    ///
    /// # 返回
    /// 数量在可填范围内时返回 `Ok(())`。
    ///
    /// # 错误
    /// 低于已发生数量、高于可填上限或数量越界时返回领域错误，文案指明行和下一步。
    pub fn check(&self, requested: Quantity) -> Result<()> {
        let value = requested.to_decimal();
        if value < self.fulfilled_quantity.to_decimal() {
            return Err(Error::from(format!(
                "第 {} 行 {} 扣除已登记退回后，已履约 {}，改后数量不能少于该数量；多出的货物请办理采购退货",
                self.line_no,
                self.item_name,
                self.fulfilled_quantity.to_decimal().normalize()
            )));
        }
        let max = self.max_quantity()?;
        if value > max.to_decimal() {
            return Err(Error::from(self.above_max_message(max)));
        }
        Ok(())
    }

    /// 销售目标扣除其他来源覆盖后的剩余量。
    fn capacity(&self) -> Decimal {
        (self.sales_quantity.to_decimal() - self.other_covered_quantity.to_decimal()).max(Decimal::ZERO)
    }

    /// 超出上限时的提示：受影响行指向维持登记，其余指向销售剩余量。
    fn above_max_message(&self, max: Quantity) -> String {
        let max = max.to_decimal().normalize();
        if self.quantity_affected {
            format!(
                "第 {} 行 {} 的销售数量已调减，本采购最多保留 {}；供应商不接受调减时请登记维持原采购",
                self.line_no, self.item_name, max
            )
        } else {
            format!(
                "第 {} 行 {} 最多可采购 {}（销售需要扣除其他采购和库存后的剩余量），超出部分不能通过改单增加",
                self.line_no, self.item_name, max
            )
        }
    }
}

/// 由采购当前版本、销售覆盖和已发生数量构造逐行数量范围。
///
/// # 参数
/// * `base_lines` - 采购当前生效版本行
/// * `coverage` - 销售当前版本覆盖（含本采购当前版本的分配）
/// * `impact` - 本采购当前受影响原因；未受影响为空
/// * `fulfilled` - 稳定销售行到本采购已发生数量
///
/// # 返回
/// 按采购行号返回商品/服务行的数量范围；物流费用行不参与。
///
/// # 错误
/// 商品行缺少数量或销售稳定行，或数量越界时返回领域错误。
pub fn build_change_quantity_bounds(
    base_lines: &[PurchaseOrderRevisionLine],
    coverage: &SalesProcurementCoverage,
    impact: Option<&PurchaseSourceImpact>,
    fulfilled: &HashMap<String, Quantity>,
) -> Result<Vec<ChangeQuantityBound>> {
    let affected = quantity_affected_lines(impact);
    let zero = quantity(Decimal::ZERO)?;
    let mut bounds = base_lines
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
        .map(|line| {
            let stable_id = line
                .sales_order_line_id
                .as_ref()
                .map(ToString::to_string)
                .ok_or_else(|| Error::from("采购商品行缺少销售稳定行"))?;
            let base_quantity = line.quantity.ok_or_else(|| Error::from("采购商品行缺少数量"))?;
            let current =
                coverage.lines.iter().find(|row| row.revision_line.sales_order_line_id.as_ref() == stable_id);
            let (sales_quantity, covered) =
                current.map_or((zero, zero), |row| (row.goods_line.quantity, row.summary.covered_quantity));
            let other = (covered.to_decimal() - base_quantity.to_decimal()).max(Decimal::ZERO);
            Ok(ChangeQuantityBound {
                quantity_affected: affected.contains(stable_id.as_str()),
                fulfilled_quantity: fulfilled.get(&stable_id).copied().unwrap_or(zero),
                sales_order_line_id: stable_id,
                line_no: line.line_no,
                item_name: line.product_name_snapshot.clone().unwrap_or_default(),
                base_quantity,
                sales_quantity,
                other_covered_quantity: quantity(other)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    bounds.sort_by_key(|bound| bound.line_no);
    Ok(bounds)
}

/// 校验改单目标覆盖每条原商品行一次且数量都在可填范围内。
///
/// # 参数
/// * `bounds` - 原商品行数量范围
/// * `requested` - 改单目标中的（稳定销售行，数量）
///
/// # 返回
/// 全部商品行齐全且数量合规时返回 `Ok(())`。
///
/// # 错误
/// 新增商品、遗漏或重复原商品行，或任一行数量越界时返回领域错误。
pub fn validate_change_quantities(
    bounds: &[ChangeQuantityBound],
    requested: &[(String, Quantity)],
) -> Result<()> {
    let by_line =
        bounds.iter().map(|bound| (bound.sales_order_line_id.as_str(), bound)).collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    for (stable_id, value) in requested {
        let bound = by_line
            .get(stable_id.as_str())
            .ok_or_else(|| Error::from("采购改单不能新增商品；需要其他商品请另行采购"))?;
        if !seen.insert(stable_id.as_str()) {
            return Err(Error::from("同一商品在改单中重复出现，请刷新后重试"));
        }
        bound.check(*value)?;
    }
    if seen.len() != bounds.len() {
        return Err(Error::from("原采购商品须保留记录；不再采购的商品请把数量改为 0"));
    }
    Ok(())
}

/// 本采购因销售归零或减量受影响的稳定销售行。
fn quantity_affected_lines(impact: Option<&PurchaseSourceImpact>) -> HashSet<&str> {
    impact
        .into_iter()
        .flat_map(|impact| impact.reasons.iter())
        .filter(|reason| {
            matches!(reason.kind, SourceImpactKind::Zeroed { .. } | SourceImpactKind::QuantityReduced { .. })
        })
        .map(|reason| reason.sales_order_line_id.as_str())
        .collect()
}

/// 构造数量；超出精度或范围时返回领域错误。
fn quantity(value: Decimal) -> Result<Quantity> {
    Quantity::try_from(value.round_dp(6))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn bound(base: &str, sales: &str, other: &str, fulfilled: &str, affected: bool) -> ChangeQuantityBound {
        ChangeQuantityBound {
            sales_order_line_id: "sol-1".into(),
            line_no: 1,
            item_name: "礼盒".into(),
            base_quantity: qty(base),
            sales_quantity: qty(sales),
            other_covered_quantity: qty(other),
            fulfilled_quantity: qty(fulfilled),
            quantity_affected: affected,
        }
    }

    /// 未受影响行可保持或调减原数量，增加不能超过销售剩余量；改为 0 形成缺口。
    #[test]
    fn unaffected_line_keeps_base_and_caps_increase_by_capacity() {
        let line = bound("5", "10", "3", "0", false);

        assert_eq!(line.max_quantity().unwrap(), qty("7"));
        line.check(qty("7")).unwrap();
        line.check(qty("0")).unwrap();
        assert_eq!(line.gap_after(qty("0")).unwrap(), qty("7"));
        assert_eq!(line.gap_after(qty("5")).unwrap(), qty("2"));
        assert!(line.check(qty("7.000001")).unwrap_err().to_string().contains("最多可采购 7"));
    }

    /// 已维持的超量采购仍可保持原数量（例如只改价格），但不能继续增加。
    #[test]
    fn unaffected_overcovered_line_may_keep_base_quantity() {
        let line = bound("5", "4", "2", "0", false);

        assert_eq!(line.max_quantity().unwrap(), qty("5"));
        line.check(qty("5")).unwrap();
        assert!(line.check(qty("6")).is_err());
        assert_eq!(line.excess_after(qty("5")).unwrap(), qty("3"));
    }

    /// 受影响行必须调减到剩余量以内，原数量不再被接受。
    #[test]
    fn affected_line_must_be_reduced_within_capacity() {
        let zeroed = bound("5", "0", "0", "0", true);
        assert_eq!(zeroed.max_quantity().unwrap(), qty("0"));
        zeroed.check(qty("0")).unwrap();
        let message = zeroed.check(qty("5")).unwrap_err().to_string();
        assert!(message.contains("登记维持"));

        let reduced = bound("5", "6", "3", "0", true);
        assert_eq!(reduced.max_quantity().unwrap(), qty("3"));
        assert!(reduced.check(qty("4")).is_err());
    }

    /// 已发生数量是下限；高于上限时以已发生数量为准。
    #[test]
    fn fulfilled_quantity_is_floor_and_wins_over_capacity() {
        let line = bound("5", "0", "0", "2", true);

        assert_eq!(line.min_quantity(), qty("2"));
        assert_eq!(line.max_quantity().unwrap(), qty("2"));
        line.check(qty("2")).unwrap();
        let message = line.check(qty("1")).unwrap_err().to_string();
        assert!(message.contains("不能少于") && message.contains("采购退货"));
    }

    /// 目标必须恰好覆盖每条原商品行一次，不得新增。
    #[test]
    fn requested_lines_must_match_base_lines() {
        let bounds = vec![bound("5", "10", "0", "0", false)];

        validate_change_quantities(&bounds, &[("sol-1".into(), qty("0"))]).unwrap();
        assert!(validate_change_quantities(&bounds, &[]).unwrap_err().to_string().contains("数量改为 0"));
        assert!(
            validate_change_quantities(&bounds, &[("sol-x".into(), qty("1"))])
                .unwrap_err()
                .to_string()
                .contains("不能新增")
        );
        assert!(
            validate_change_quantities(&bounds, &[("sol-1".into(), qty("1")), ("sol-1".into(), qty("1"))])
                .is_err()
        );
    }
}
