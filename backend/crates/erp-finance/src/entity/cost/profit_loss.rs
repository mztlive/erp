//! 一期实际经营盈亏的纯金额规则；冲减为正数事实，在公式中只加回一次。
use super::{CostStage, CostType};
use erp_core::{Error, Result};
use rust_decimal::Decimal;

/// 一组成本事实的阶段汇总。预计、确认不进入实际金额。
#[derive(Debug, Clone, Default)]
pub struct ProfitLossAmounts {
    pub procurement: Decimal,
    pub fulfillment: Decimal,
    pub reductions: Decimal,
    pub expected_procurement: Decimal,
    pub expected_fulfillment: Decimal,
    pub confirmed_procurement: Decimal,
    pub confirmed_fulfillment: Decimal,
}
impl ProfitLossAmounts {
    /// 采用已分配的不含税金额；不接受负数冲减，防止重复反向。
    pub fn add(&mut self, stage: CostStage, cost_type: CostType, net: Decimal) -> Result<()> {
        if net.is_sign_negative() {
            return Err(Error::from("成本分配金额不能为负"));
        }
        let procurement = cost_type == CostType::Product;
        let target = match (stage, procurement) {
            (CostStage::Reduction, _) => &mut self.reductions,
            (CostStage::Actual, true) => &mut self.procurement,
            (CostStage::Actual, false) => &mut self.fulfillment,
            (CostStage::Expected, true) => &mut self.expected_procurement,
            (CostStage::Expected, false) => &mut self.expected_fulfillment,
            (CostStage::Confirmed, true) => &mut self.confirmed_procurement,
            (CostStage::Confirmed, false) => &mut self.confirmed_fulfillment,
        };
        *target = target
            .checked_add(net)
            .ok_or_else(|| Error::from("盈亏金额超出范围"))?;
        Ok(())
    }
    /// 收入减实际成本再加冲减；不完整的成本由调用方禁止输出利润。
    pub fn profit(&self, revenue: Decimal) -> Result<Decimal> {
        revenue
            .checked_sub(self.procurement)
            .and_then(|n| n.checked_sub(self.fulfillment))
            .and_then(|n| n.checked_add(self.reductions))
            .ok_or_else(|| Error::from("盈亏金额超出范围"))
    }
}
/// 供货成本覆盖证据由销售读取适配，财务领域执行统一可信性规则。
pub struct CoverageEvidence<'a> {
    pub fulfillment_completed: bool,
    pub current_lines: std::collections::BTreeSet<&'a str>,
    pub supplied_lines: &'a std::collections::BTreeSet<String>,
}
/// 可执行的成本完整性缺口。
pub struct CoverageGap {
    pub code: &'static str,
    pub message: String,
}
impl ProfitLossAmounts {
    /// 非卡券完整性不能只凭一笔费用存在判断；多行要求逐行供货成本。
    pub fn coverage_gaps(&self, evidence: CoverageEvidence<'_>) -> Result<Vec<CoverageGap>> {
        let mut gaps = evidence.gaps();
        let mut actual = self.procurement;
        actual = actual
            .checked_add(self.fulfillment)
            .ok_or_else(|| Error::from("实际成本超出范围"))?;
        if self.reductions > actual {
            gaps.push(CoverageGap {
                code: "EXCESS_REDUCTION",
                message: "成本冲减超过实际发生金额，请财务核查".into(),
            });
        }
        Ok(gaps)
    }
    /// 实际成本扣除冲减，负数留给完整性校验解释，不取绝对值。
    pub fn net_cost(&self) -> Result<Decimal> {
        self.procurement
            .checked_add(self.fulfillment)
            .and_then(|n| n.checked_sub(self.reductions))
            .ok_or_else(|| Error::from("实际成本超出范围"))
    }
}
impl CoverageEvidence<'_> {
    /// 单行允许明确整单成本覆盖；多行禁止猜测各行覆盖范围。
    fn gaps(&self) -> Vec<CoverageGap> {
        let mut gaps = Vec::new();
        if !self.fulfillment_completed {
            gaps.push(CoverageGap {
                code: "FULFILLMENT_OPEN",
                message: "履约尚未完成，实际成本可能继续发生".into(),
            });
        }
        let whole_single_line = self.current_lines.len() == 1 && self.supplied_lines.contains("");
        let missing = self
            .current_lines
            .iter()
            .filter(|line| !whole_single_line && !self.supplied_lines.contains(**line))
            .count();
        if self.current_lines.is_empty() || missing > 0 {
            gaps.push(CoverageGap {
                code: "SUPPLY_COST_MISSING",
                message: format!("{} 条销售明细缺少已分配的实际供货成本", missing.max(1)),
            });
        }
        if self
            .supplied_lines
            .iter()
            .any(|line| !line.is_empty() && !self.current_lines.contains(line.as_str()))
        {
            gaps.push(CoverageGap {
                code: "REMOVED_LINE_COST",
                message: "存在已移除销售明细的成本，请核对销售变更与成本冲减".into(),
            });
        }
        gaps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profit_excludes_estimates_and_adds_reductions_once() {
        let mut a = ProfitLossAmounts::default();
        for (stage, kind, value) in [
            (CostStage::Actual, CostType::Product, 60),
            (CostStage::Actual, CostType::Delivery, 10),
            (CostStage::Reduction, CostType::Product, 5),
            (CostStage::Expected, CostType::Product, 90),
            (CostStage::Confirmed, CostType::Product, 80),
        ] {
            a.add(stage, kind, Decimal::from(value)).unwrap();
        }
        assert_eq!(a.profit(Decimal::from(100)).unwrap(), Decimal::from(35));
        assert!(a
            .add(CostStage::Reduction, CostType::Product, Decimal::NEGATIVE_ONE)
            .is_err());
    }
    #[test]
    fn zero_and_negative_margin_are_valid() {
        let a = ProfitLossAmounts {
            procurement: Decimal::from(110),
            ..Default::default()
        };
        assert_eq!(a.profit(Decimal::from(100)).unwrap(), Decimal::from(-10));
        assert_eq!(a.profit(Decimal::ZERO).unwrap(), Decimal::from(-110));
    }
}
