//! 回款核销与冲正共用的精确金额运算。

use std::str::FromStr;

use erp_core::money::Amount;
use erp_core::{Error, Result};

use super::{AllocationAction, ReceiptAllocation};

/// 计算回款分配净已核销合计。
///
/// # 参数
/// * `allocations` - 回款核销分配
///
/// # 返回
/// 返回 APPLY 加、REVERSE 减后的净额。
///
/// # 错误
/// 合计溢出时返回 [`Error::LogicError`]。
///
/// # 约束
/// 供账本与快照共用，避免两套净额算法。
pub(crate) fn net_receipt_allocated(allocations: &[ReceiptAllocation]) -> Result<Amount> {
    allocations.iter().try_fold(zero_amount(), |sum, line| match line.allocation_action {
        AllocationAction::Apply => checked_add_amount(sum, line.allocated_amount),
        AllocationAction::Reverse => checked_sub_amount(sum, line.allocated_amount),
    })
}

/// 返回固定零金额。
///
/// # 返回
/// 返回 `0.00`。
pub(crate) fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}

/// 精确相加两个金额。
///
/// # 参数
/// * `left` - 加数
/// * `right` - 加数
///
/// # 返回
/// 返回精确和。
///
/// # 错误
/// 溢出时返回 [`Error::LogicError`]。
pub(crate) fn checked_add_amount(left: Amount, right: Amount) -> Result<Amount> {
    let sum =
        left.to_decimal().checked_add(right.to_decimal()).ok_or_else(|| Error::from("票款金额合计溢出"))?;
    Amount::try_from(sum).map_err(|_| Error::from("票款金额合计溢出"))
}

/// 精确相减两个金额。
///
/// # 参数
/// * `left` - 被减数
/// * `right` - 减数
///
/// # 返回
/// 返回精确差。
///
/// # 错误
/// 溢出时返回 [`Error::LogicError`]。
pub(crate) fn checked_sub_amount(left: Amount, right: Amount) -> Result<Amount> {
    let diff =
        left.to_decimal().checked_sub(right.to_decimal()).ok_or_else(|| Error::from("票款金额合计溢出"))?;
    Amount::try_from(diff).map_err(|_| Error::from("票款金额合计溢出"))
}
