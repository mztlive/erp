//! 采购数量覆盖汇总值对象。
//!
//! 当前销售版本商品/服务数量是采购目标；草稿/审批提交与生效采购分配形成覆盖量。
//! 销售减量后覆盖可以超过目标，超出部分单独表达，剩余数量不为负。
//! 本模块只承载不依赖仓储的数量守恒、剩余数量、超出数量与进度派生规则。

use erp_core::money::{Quantity, Rate};
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 销售单当前版本的采购数量覆盖汇总。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcurementCoverageSummary {
    /// 当前销售版本商品/服务目标总数量。
    pub total_quantity: Quantity,
    /// 当前采购指针覆盖总数量。
    pub covered_quantity: Quantity,
    /// 尚可创建采购单的剩余总数量。
    pub remaining_quantity: Quantity,
    /// 覆盖超过当前销售目标的数量；销售减量后出现，等待采购调减。
    #[serde(default = "zero_quantity")]
    pub excess_quantity: Quantity,
    /// 覆盖进度，范围为 0 到 1。
    pub progress: Rate,
}

impl ProcurementCoverageSummary {
    /// 从单行目标数量与覆盖数量构造采购覆盖汇总。
    ///
    /// # 参数
    /// * `total_quantity` - 当前销售版本目标数量
    /// * `covered_quantity` - 按采购当前提交或当前版本汇总的覆盖数量
    ///
    /// # 返回
    /// 返回剩余数量、超出数量与六位小数进度均已派生的覆盖汇总。
    ///
    /// # 错误
    /// 任一数量为负、派生数量无法构造为 [`Quantity`]，或进度无法构造为 [`Rate`]
    /// 时返回一致性错误。
    ///
    /// # 关键业务约束
    /// 剩余数量与超出数量至多一个非零；进度按不超过目标的覆盖计算，封顶为 1，
    /// 零目标的进度固定为零。
    pub fn new(total_quantity: Quantity, covered_quantity: Quantity) -> Result<Self> {
        ensure_non_negative(total_quantity, covered_quantity)?;
        let difference = total_quantity.to_decimal() - covered_quantity.to_decimal();
        let effective = covered_quantity.min(total_quantity);
        Ok(Self {
            total_quantity,
            covered_quantity,
            remaining_quantity: Quantity::try_from(difference.max(Decimal::ZERO))?,
            excess_quantity: Quantity::try_from((-difference).max(Decimal::ZERO))?,
            progress: coverage_progress(total_quantity, effective)?,
        })
    }

    /// 汇总多行覆盖，剩余与超出逐行累计，不在行间互相抵消。
    ///
    /// # 参数
    /// * `lines` - 逐行覆盖汇总
    ///
    /// # 返回
    /// 返回整单目标、覆盖、剩余、超出与进度。
    ///
    /// # 错误
    /// 累计结果超过数量精度或进度无法构造时返回一致性错误。
    ///
    /// # 关键业务约束
    /// 一行超出不得冲抵另一行缺口；进度只按各行不超过目标的覆盖计算。
    pub fn aggregate<'a>(lines: impl IntoIterator<Item = &'a Self>) -> Result<Self> {
        let mut sums = [Decimal::ZERO; 5];
        for line in lines {
            let effective = line.covered_quantity.min(line.total_quantity);
            for (sum, value) in sums.iter_mut().zip([
                line.total_quantity,
                line.covered_quantity,
                line.remaining_quantity,
                line.excess_quantity,
                effective,
            ]) {
                *sum += value.to_decimal();
            }
        }
        let [total, covered, remaining, excess, effective] = sums.map(Quantity::try_from);
        let (total, effective) = (total?, effective?);
        Ok(Self {
            total_quantity: total,
            covered_quantity: covered?,
            remaining_quantity: remaining?,
            excess_quantity: excess?,
            progress: coverage_progress(total, effective)?,
        })
    }
}

/// 校验采购目标与覆盖数量非负。
///
/// # 参数
/// * `total_quantity` - 当前销售版本目标数量
/// * `covered_quantity` - 当前采购覆盖数量
///
/// # 返回
/// 数量非负时返回 `Ok(())`。
///
/// # 错误
/// 任一数量为负时返回领域错误。
fn ensure_non_negative(total_quantity: Quantity, covered_quantity: Quantity) -> Result<()> {
    if total_quantity.to_decimal().is_sign_negative() || covered_quantity.to_decimal().is_sign_negative() {
        return Err(Error::from("采购目标数量与覆盖数量不能为负"));
    }
    Ok(())
}

/// 计算采购数量覆盖进度。
///
/// # 参数
/// * `total_quantity` - 当前销售版本目标数量
/// * `effective_quantity` - 不超过目标的覆盖数量
///
/// # 返回
/// 返回范围为 0 到 1、保留最多六位小数的进度。
///
/// # 错误
/// 派生结果无法构造为 [`Rate`] 时返回领域错误。
///
/// # 关键业务约束
/// 零目标返回零进度，非零目标使用 `effective / total`。
fn coverage_progress(total_quantity: Quantity, effective_quantity: Quantity) -> Result<Rate> {
    if total_quantity.to_decimal().is_zero() {
        return Rate::try_from(Decimal::ZERO);
    }
    Rate::try_from((effective_quantity.to_decimal() / total_quantity.to_decimal()).round_dp(6))
}

/// 返回合法的零数量，供缺省反序列化使用。
///
/// # Panics
/// 数量类型拒绝零值时 panic；零数量合法，panic 只表示数量类型不变量被破坏。
fn zero_quantity() -> Quantity {
    Quantity::try_from(Decimal::ZERO).expect("零数量合法")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::money::{Quantity, Rate};

    use super::ProcurementCoverageSummary;

    /// 部分覆盖时应精确派生剩余数量与进度。
    #[test]
    fn summary_derives_remaining_and_progress() {
        let summary = ProcurementCoverageSummary::new(
            Quantity::from_str("10").unwrap(),
            Quantity::from_str("4").unwrap(),
        )
        .unwrap();

        assert_eq!(summary.remaining_quantity, Quantity::from_str("6").unwrap());
        assert_eq!(summary.progress, Rate::from_str("0.4").unwrap());
    }

    /// 零目标保持零进度，完全覆盖保持零剩余。
    #[test]
    fn summary_handles_zero_and_complete_boundaries() {
        let zero = ProcurementCoverageSummary::new(
            Quantity::from_str("0").unwrap(),
            Quantity::from_str("0").unwrap(),
        )
        .unwrap();
        let complete = ProcurementCoverageSummary::new(
            Quantity::from_str("3").unwrap(),
            Quantity::from_str("3").unwrap(),
        )
        .unwrap();

        assert_eq!(zero.progress, Rate::from_str("0").unwrap());
        assert_eq!(complete.remaining_quantity, Quantity::from_str("0").unwrap());
        assert_eq!(complete.progress, Rate::from_str("1").unwrap());
    }

    /// 销售减量后覆盖超过目标时剩余为零、超出单独计量、进度封顶。
    #[test]
    fn summary_reports_excess_without_negative_remaining() {
        let summary = ProcurementCoverageSummary::new(
            Quantity::from_str("2").unwrap(),
            Quantity::from_str("2.000001").unwrap(),
        )
        .unwrap();

        assert_eq!(summary.remaining_quantity, Quantity::from_str("0").unwrap());
        assert_eq!(summary.excess_quantity, Quantity::from_str("0.000001").unwrap());
        assert_eq!(summary.progress, Rate::from_str("1").unwrap());
    }

    /// 整单汇总不得用一行超出冲抵另一行缺口。
    #[test]
    fn aggregate_keeps_line_gaps_and_excess_separate() {
        let over = ProcurementCoverageSummary::new(
            Quantity::from_str("5").unwrap(),
            Quantity::from_str("8").unwrap(),
        )
        .unwrap();
        let gap = ProcurementCoverageSummary::new(
            Quantity::from_str("10").unwrap(),
            Quantity::from_str("4").unwrap(),
        )
        .unwrap();

        let total = ProcurementCoverageSummary::aggregate([&over, &gap]).unwrap();

        assert_eq!(total.total_quantity, Quantity::from_str("15").unwrap());
        assert_eq!(total.covered_quantity, Quantity::from_str("12").unwrap());
        assert_eq!(total.remaining_quantity, Quantity::from_str("6").unwrap());
        assert_eq!(total.excess_quantity, Quantity::from_str("3").unwrap());
        assert_eq!(total.progress, Rate::from_str("0.6").unwrap());
    }

    /// 空集合汇总为全零。
    #[test]
    fn aggregate_of_no_lines_is_zero() {
        let total = ProcurementCoverageSummary::aggregate([]).unwrap();

        assert_eq!(total.total_quantity, Quantity::from_str("0").unwrap());
        assert_eq!(total.progress, Rate::from_str("0").unwrap());
    }
}
