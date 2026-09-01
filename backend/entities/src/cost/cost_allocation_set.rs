//! `CostAllocationSet` 成本分配计划（W16 手工成本入账，数据模型 §6.10）。
//!
//! 与 [`CostAllocation`](crate::cost::CostAllocation) 实体的分工：本计划负责
//! gross／net 双口径守恒、零／负分配行拒绝、溢出安全的受检求和与尾差归属解析
//! 的确定性构造；`CostAllocation::new` 继续守卫实体自身的归属目标互斥与
//! 含税 ≥ 不含税形态不变量。计划不生成 ID、不查询销售订单、不做跨聚合存在性
//! 判断——销售事实由 Service 确认后传入，实体 ID 由 Service 注入。

use rust_decimal::Decimal;

use crate::errors::{Error, Result};
use crate::ids::{SalesOrderId, SalesOrderLineId};
use crate::money::Amount;

/// 成本分配计划输入行（尾差归属尚未解析）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostAllocationLineInput {
    /// 经营归属销售单（一期成本归属销售单必填）。
    pub sales_order_id: SalesOrderId,
    /// 经营归属销售明细（可空）。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 是否承担最后一笔尾差；`None` 时默认由第一条分配行承担。
    pub rounding_residual_flag: Option<bool>,
}

/// 成本分配计划行（尾差归属已解析；顺序与输入一致）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostAllocationPlanLine {
    /// 经营归属销售单。
    pub sales_order_id: SalesOrderId,
    /// 经营归属销售明细（可空）。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 分配含税金额。
    pub allocated_gross_amount: Amount,
    /// 分配不含税金额。
    pub allocated_net_amount: Amount,
    /// 是否承担最后一笔尾差。
    pub rounding_residual_flag: bool,
}

/// 成本分配计划：gross／net 双口径守恒与尾差归属的一等领域对象。
///
/// 构造一次完成守恒校验、精度校验（十进制精确相等，不经过浮点或舍入）与
/// 尾差归属解析；任一校验失败都不产生部分计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostAllocationSet {
    lines: Vec<CostAllocationPlanLine>,
}

impl CostAllocationSet {
    /// 构造成本分配计划。
    ///
    /// 规则（与 `CostService::create_cost_entry` 原实现一致，责任归位到本
    /// 类型后 Service 不再重复求和）：
    /// - 至少一条分配行；
    /// - 每条分配行的含税与不含税金额必须为正数（零／负值直接拒绝，满足
    ///   FIN-E01 关闭验收「零／负值」维度）；含税 ≥ 不含税与归属目标互斥
    ///   仍由 [`CostAllocation::new`](crate::cost::CostAllocation::new) 承担；
    /// - 含税合计与不含税合计分别精确等于成本事实金额（含税与不含税两个
    ///   口径独立校验，任一不一致即失败）；
    /// - 合计使用十进制受检加法，极端大额溢出返回错误而非 panic；
    /// - 尾差归属解析：显式值优先；`None` 默认落到第一条分配行
    ///   （`index == 0`），其余 `None` 行解析为 `false`；
    /// - 顺序确定性：计划行保持输入顺序。
    ///
    /// # 参数
    /// * `gross_amount` - 成本事实含税金额
    /// * `net_amount` - 成本事实不含税金额
    /// * `lines` - 分配输入行（顺序即计划顺序）
    ///
    /// # 返回
    /// 返回守恒校验通过、行金额均为正数且尾差归属已解析的成本分配计划。
    ///
    /// # 错误
    /// 空分配、零／负分配行、合计溢出，或含税／不含税任一口径合计与成本事实
    /// 金额不一致时返回 [`Error::LogicError`]，且不产生部分计划。
    pub fn new(
        gross_amount: Amount,
        net_amount: Amount,
        lines: Vec<CostAllocationLineInput>,
    ) -> Result<Self> {
        if lines.is_empty() {
            return Err(Error::from("至少提供一条成本分配"));
        }
        let mut gross_sum = Decimal::ZERO;
        let mut net_sum = Decimal::ZERO;
        for line in &lines {
            let line_gross = line.allocated_gross_amount.to_decimal();
            let line_net = line.allocated_net_amount.to_decimal();
            if line_gross.is_sign_negative()
                || line_gross.is_zero()
                || line_net.is_sign_negative()
                || line_net.is_zero()
            {
                return Err(Error::from("分配金额必须为正数"));
            }
            let Some(next_gross) = gross_sum.checked_add(line_gross) else {
                return Err(Error::from("成本分配金额合计溢出"));
            };
            gross_sum = next_gross;
            let Some(next_net) = net_sum.checked_add(line_net) else {
                return Err(Error::from("成本分配金额合计溢出"));
            };
            net_sum = next_net;
        }
        if gross_sum != gross_amount.to_decimal() || net_sum != net_amount.to_decimal() {
            return Err(Error::from("成本分配合计必须等于成本事实金额"));
        }
        let planned = lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| CostAllocationPlanLine {
                sales_order_id: line.sales_order_id,
                sales_order_line_id: line.sales_order_line_id,
                allocated_gross_amount: line.allocated_gross_amount,
                allocated_net_amount: line.allocated_net_amount,
                rounding_residual_flag: line.rounding_residual_flag.unwrap_or(index == 0),
            })
            .collect();
        Ok(Self { lines: planned })
    }

    /// 返回按输入顺序排列的计划行。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回计划内部行切片，供 Service 生成 ID 并执行持久化写入。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 调用方不能修改计划；事务与持久化职责仍属于 Service。
    pub fn lines(&self) -> &[CostAllocationPlanLine] {
        &self.lines
    }

    /// 消费计划并返回按输入顺序排列的计划行。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回计划内部行；计划被消费后不可再次使用。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn into_lines(self) -> Vec<CostAllocationPlanLine> {
        self.lines
    }
}

#[cfg(test)]
mod tests {
    use super::{CostAllocationLineInput, CostAllocationSet};
    use crate::ids::{SalesOrderId, SalesOrderLineId};
    use crate::money::Amount;
    use std::str::FromStr;

    fn line(
        id: &str,
        gross: &str,
        net: &str,
        rounding_residual_flag: Option<bool>,
    ) -> CostAllocationLineInput {
        CostAllocationLineInput {
            sales_order_id: SalesOrderId::new(id),
            sales_order_line_id: Some(SalesOrderLineId::new(format!("{id}-l1"))),
            allocated_gross_amount: Amount::from_str(gross).unwrap(),
            allocated_net_amount: Amount::from_str(net).unwrap(),
            rounding_residual_flag,
        }
    }

    #[test]
    fn plan_accepts_multi_allocation_preserving_input_order() {
        let plan = CostAllocationSet::new(
            Amount::from_str("113.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            vec![
                line("so-1", "56.50", "50.00", None),
                line("so-2", "28.25", "25.00", None),
                line("so-3", "28.25", "25.00", None),
            ],
        )
        .unwrap();

        let lines = plan.lines();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].sales_order_id, SalesOrderId::new("so-1"));
        assert_eq!(lines[1].sales_order_id, SalesOrderId::new("so-2"));
        assert_eq!(lines[2].sales_order_id, SalesOrderId::new("so-3"));
        assert_eq!(
            lines[2].allocated_gross_amount,
            Amount::from_str("28.25").unwrap()
        );
        assert_eq!(lines[2].allocated_net_amount, Amount::from_str("25.00").unwrap());
    }

    #[test]
    fn plan_rejects_gross_mismatch_without_partial_plan() {
        let result = CostAllocationSet::new(
            Amount::from_str("113.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            vec![
                line("so-1", "56.50", "50.00", None),
                line("so-2", "28.25", "25.00", None),
                line("so-3", "28.24", "25.00", None),
            ],
        );
        assert_eq!(
            result.unwrap_err().to_string(),
            "成本分配合计必须等于成本事实金额"
        );
    }

    #[test]
    fn plan_rejects_net_mismatch() {
        let result = CostAllocationSet::new(
            Amount::from_str("113.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            vec![
                line("so-1", "56.50", "50.00", None),
                line("so-2", "56.50", "49.99", None),
            ],
        );
        assert!(result.is_err());
    }

    #[test]
    fn plan_resolves_explicit_residual_over_default() {
        let plan = CostAllocationSet::new(
            Amount::from_str("113.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            vec![
                line("so-1", "56.50", "50.00", Some(false)),
                line("so-2", "28.25", "25.00", Some(true)),
                line("so-3", "28.25", "25.00", Some(false)),
            ],
        )
        .unwrap();

        let flags = plan
            .lines()
            .iter()
            .map(|line| line.rounding_residual_flag)
            .collect::<Vec<_>>();
        assert_eq!(flags, vec![false, true, false]);
    }

    #[test]
    fn plan_defaults_residual_to_first_line() {
        let plan = CostAllocationSet::new(
            Amount::from_str("113.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            vec![
                line("so-1", "56.50", "50.00", None),
                line("so-2", "28.25", "25.00", None),
                line("so-3", "28.25", "25.00", None),
            ],
        )
        .unwrap();

        let flags = plan
            .lines()
            .iter()
            .map(|line| line.rounding_residual_flag)
            .collect::<Vec<_>>();
        assert_eq!(flags, vec![true, false, false]);
    }

    #[test]
    fn plan_defaults_residual_only_when_first_line_has_no_explicit_flag() {
        let plan = CostAllocationSet::new(
            Amount::from_str("113.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            vec![
                line("so-1", "56.50", "50.00", Some(false)),
                line("so-2", "28.25", "25.00", None),
                line("so-3", "28.25", "25.00", None),
            ],
        )
        .unwrap();

        let flags = plan
            .lines()
            .iter()
            .map(|line| line.rounding_residual_flag)
            .collect::<Vec<_>>();
        assert_eq!(flags, vec![false, false, false]);
    }

    #[test]
    fn plan_rounding_boundary_split_conserves_exactly() {
        let plan = CostAllocationSet::new(
            Amount::from_str("100.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            vec![
                line("so-1", "33.33", "33.33", None),
                line("so-2", "33.33", "33.33", None),
                line("so-3", "33.34", "33.34", None),
            ],
        )
        .unwrap();
        assert_eq!(plan.lines().len(), 3);

        let under_split = CostAllocationSet::new(
            Amount::from_str("100.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            vec![
                line("so-1", "33.33", "33.33", None),
                line("so-2", "33.33", "33.33", None),
                line("so-3", "33.33", "33.33", None),
            ],
        );
        assert!(under_split.is_err());
    }

    #[test]
    fn plan_rejects_empty_lines() {
        let result = CostAllocationSet::new(
            Amount::from_str("113.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            Vec::new(),
        );
        assert_eq!(result.unwrap_err().to_string(), "至少提供一条成本分配");
    }

    #[test]
    fn plan_rejects_zero_or_negative_lines() {
        // 零金额行（gross 为 0.00）直接拒绝。
        let zero_gross = CostAllocationSet::new(
            Amount::from_str("100.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            vec![
                line("so-1", "100.00", "100.00", None),
                line("so-2", "0.00", "0.00", None),
            ],
        );
        assert_eq!(zero_gross.unwrap_err().to_string(), "分配金额必须为正数");

        // 负含税行即使守恒可通过（100.00 = 150.00 + (-50.00)）也必须被拒绝。
        let negative_gross = CostAllocationSet::new(
            Amount::from_str("100.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            vec![
                line("so-1", "150.00", "100.00", None),
                line("so-2", "-50.00", "0.00", None),
            ],
        );
        assert_eq!(negative_gross.unwrap_err().to_string(), "分配金额必须为正数");

        // 负不含税行（净额口径守恒同样成立）也必须被拒绝。
        let negative_net = CostAllocationSet::new(
            Amount::from_str("200.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            vec![
                line("so-1", "100.00", "150.00", None),
                line("so-2", "100.00", "-50.00", None),
            ],
        );
        assert_eq!(negative_net.unwrap_err().to_string(), "分配金额必须为正数");
    }

    #[test]
    fn plan_rejects_sum_overflow_with_logic_error() {
        // Decimal 96 位尾数上限（scale 0，Amount 允许的小数位下限）再累加 1
        // 触发溢出，`Decimal::checked_add` 返回 None，必须映射为 LogicError
        // 而非 panic（旧 `Amount::checked_add` 走未受检的 `Add`，该路径
        // 会 panic）。
        let result = CostAllocationSet::new(
            Amount::from_str("79228162514264337593543950335").unwrap(),
            Amount::from_str("79228162514264337593543950335").unwrap(),
            vec![
                line(
                    "so-1",
                    "79228162514264337593543950335",
                    "79228162514264337593543950335",
                    None,
                ),
                line("so-2", "1", "1", None),
            ],
        );
        assert_eq!(result.unwrap_err().to_string(), "成本分配金额合计溢出");
    }

    #[test]
    fn plan_construction_is_deterministic() {
        let input = vec![
            line("so-1", "56.50", "50.00", None),
            line("so-2", "28.25", "25.00", Some(true)),
            line("so-3", "28.25", "25.00", None),
        ];
        let first = CostAllocationSet::new(
            Amount::from_str("113.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            input.clone(),
        )
        .unwrap();
        let second = CostAllocationSet::new(
            Amount::from_str("113.00").unwrap(),
            Amount::from_str("100.00").unwrap(),
            input,
        )
        .unwrap();
        assert_eq!(first, second);
    }
}
