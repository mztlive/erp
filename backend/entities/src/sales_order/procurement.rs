//! 销售单采购数量覆盖值对象。
//!
//! 当前销售版本商品/服务数量是采购目标；草稿/审批提交与生效采购分配形成覆盖量。
//! 本模块只承载不依赖仓储的数量守恒、剩余数量与进度派生规则。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{Error, Result};
use crate::money::{Quantity, Rate};

/// 销售单当前版本的采购数量覆盖汇总。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcurementCoverageSummary {
    /// 当前销售版本商品/服务目标总数量。
    pub total_quantity: Quantity,
    /// 当前采购指针覆盖总数量。
    pub covered_quantity: Quantity,
    /// 尚可创建采购单的剩余总数量。
    pub remaining_quantity: Quantity,
    /// 覆盖进度，范围为 0 到 1。
    pub progress: Rate,
}

impl ProcurementCoverageSummary {
    /// 从目标数量与覆盖数量构造采购覆盖汇总。
    ///
    /// # 参数
    /// * `total_quantity` - 当前销售版本目标数量
    /// * `covered_quantity` - 按采购当前提交或当前版本汇总的覆盖数量
    ///
    /// # 返回
    /// 返回剩余数量与六位小数进度均已派生的覆盖汇总。
    ///
    /// # 错误
    /// 任一数量为负，或覆盖数量大于目标数量时返回一致性错误。
    ///
    /// # 关键业务约束
    /// 剩余数量恒等于目标数量减覆盖数量；零目标的进度固定为零。
    pub fn new(total_quantity: Quantity, covered_quantity: Quantity) -> Result<Self> {
        ensure_consistent(total_quantity, covered_quantity)?;
        let remaining = total_quantity.to_decimal() - covered_quantity.to_decimal();
        let progress = coverage_progress(total_quantity, covered_quantity)?;
        Ok(Self {
            total_quantity,
            covered_quantity,
            remaining_quantity: Quantity::try_from(remaining)?,
            progress,
        })
    }
}

/// 为稳定销售行集合构造采购任务责任键。
///
/// # 参数
/// * `line_ids` - 已按稳定身份排序去重的销售行 ID
///
/// # 返回
/// 返回长度边界安全的 `sales-lines:<sha256>` 责任键。
///
/// # 错误
/// 行集合为空时返回领域错误。
pub fn procurement_responsibility_key(line_ids: &[String]) -> Result<String> {
    if line_ids.is_empty() {
        return Err(Error::from("供给分配任务责任行不能为空"));
    }
    let mut digest = Sha256::new();
    for line_id in line_ids {
        digest.update((line_id.len() as u64).to_be_bytes());
        digest.update(line_id.as_bytes());
    }
    let digest = digest.finalize();
    let encoded = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("sales-lines:{encoded}"))
}

/// 校验采购目标与覆盖数量的一致性。
///
/// # 参数
/// * `total_quantity` - 当前销售版本目标数量
/// * `covered_quantity` - 当前采购覆盖数量
///
/// # 返回
/// 数量非负且覆盖不超过目标时返回 `Ok(())`。
///
/// # 错误
/// 数量为负或覆盖超过目标时返回领域错误。
///
/// # 关键业务约束
/// 超采不得被截断为零剩余，必须显式暴露数据一致性问题。
fn ensure_consistent(total_quantity: Quantity, covered_quantity: Quantity) -> Result<()> {
    if total_quantity.to_decimal().is_sign_negative() || covered_quantity.to_decimal().is_sign_negative() {
        return Err(Error::from("采购目标数量与覆盖数量不能为负"));
    }
    if covered_quantity > total_quantity {
        return Err(Error::from("采购覆盖数量超过销售当前版本目标数量"));
    }
    Ok(())
}

/// 计算采购数量覆盖进度。
///
/// # 参数
/// * `total_quantity` - 当前销售版本目标数量
/// * `covered_quantity` - 当前采购覆盖数量
///
/// # 返回
/// 返回范围为 0 到 1、保留最多六位小数的进度。
///
/// # 错误
/// 派生结果无法构造为 [`Rate`] 时返回领域错误。
///
/// # 关键业务约束
/// 零目标返回零进度，非零目标使用 `covered / total`。
fn coverage_progress(total_quantity: Quantity, covered_quantity: Quantity) -> Result<Rate> {
    if total_quantity.to_decimal().is_zero() {
        return Rate::try_from(Decimal::ZERO);
    }
    Rate::try_from((covered_quantity.to_decimal() / total_quantity.to_decimal()).round_dp(6))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{procurement_responsibility_key, ProcurementCoverageSummary};
    use crate::money::{Quantity, Rate};

    #[test]
    fn responsibility_key_is_stable_and_boundary_safe() {
        let first = procurement_responsibility_key(&["line-1".to_string(), "line-23".to_string()]).unwrap();
        let repeated =
            procurement_responsibility_key(&["line-1".to_string(), "line-23".to_string()]).unwrap();
        let different =
            procurement_responsibility_key(&["line-12".to_string(), "line-3".to_string()]).unwrap();
        assert_eq!(first, repeated);
        assert_ne!(first, different);
        assert!(first.starts_with("sales-lines:"));
        assert!(procurement_responsibility_key(&[]).is_err());
    }

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

    /// 覆盖超过当前销售目标必须返回一致性错误。
    #[test]
    fn summary_rejects_over_coverage() {
        let result = ProcurementCoverageSummary::new(
            Quantity::from_str("2").unwrap(),
            Quantity::from_str("2.000001").unwrap(),
        );

        assert!(result.is_err());
    }
}
