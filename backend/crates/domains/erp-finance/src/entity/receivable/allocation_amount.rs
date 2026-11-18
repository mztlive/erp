//! 回款核销与冲正共用的精确金额运算。

use erp_core::money::Amount;
use erp_core::{Error, Result};

use super::{AllocationAction, ReceiptAllocation};

// 计算回款分配净已核销合计。
// 返回 APPLY 加、REVERSE 减后的净额。
// 失败条件：合计溢出时返回 [`Error::LogicError`]。
// 调用边界：供账本与快照共用，避免两套净额算法。
//
// 输入约定：
// - `allocations`：回款核销分配
pub(crate) fn net_receipt_allocated(allocations: &[ReceiptAllocation]) -> Result<Amount> {
    allocations.iter().try_fold(zero_amount(), |sum, line| match line.allocation_action {
        AllocationAction::Apply => checked_add_amount(sum, line.allocated_amount),
        AllocationAction::Reverse => checked_sub_amount(sum, line.allocated_amount),
    })
}

// 校验发票分配金额为正且满足含税金额恒等式。
// 结果：金额合法时返回 `Ok(())`。
// 失败条件：金额非正或恒等式不成立时返回原分配校验错误。
//
// 输入约定：
// - `gross`：分配含税金额。
// - `net`：分配不含税金额。
// - `tax`：分配税额。
pub(crate) fn validate_invoice_amounts(gross: Amount, net: Amount, tax: Amount) -> Result<()> {
    if gross.to_decimal().is_sign_negative()
        || gross.to_decimal().is_zero()
        || net.to_decimal().is_sign_negative()
        || tax.to_decimal().is_sign_negative()
    {
        return Err(Error::from("分配金额必须为正数"));
    }
    if gross != net.checked_add(tax) {
        return Err(Error::from("分配含税金额必须等于不含税金额加税额"));
    }
    Ok(())
}

// 返回固定零金额。
// 返回 `0.00`。
pub(crate) fn zero_amount() -> Amount {
    Amount::zero()
}

// 精确相加两个金额（调用方指定溢出文案）。
// 返回精确和。
// 失败条件：定点运算溢出或结果超出金额精度时返回以 `message` 构造的 [`Error::LogicError`]。
//
// 输入约定：
// - `left`：加数
// - `right`：加数
// - `message`：溢出时的错误文案（调用方领域语义，保持原对外文案不变）
pub(crate) fn checked_add_with_message(left: Amount, right: Amount, message: &str) -> Result<Amount> {
    let sum = left.to_decimal().checked_add(right.to_decimal()).ok_or_else(|| Error::from(message))?;
    Amount::try_from(sum).map_err(|_| Error::from(message))
}

// 精确相加两个金额。
// 返回精确和。
// 失败条件：溢出时返回 [`Error::LogicError`]。
//
// 输入约定：
// - `left`：加数
// - `right`：加数
pub(crate) fn checked_add_amount(left: Amount, right: Amount) -> Result<Amount> {
    checked_add_with_message(left, right, "票款金额合计溢出")
}

// 精确相减两个金额。
// 返回精确差。
// 失败条件：溢出时返回 [`Error::LogicError`]。
//
// 输入约定：
// - `left`：被减数
// - `right`：减数
pub(crate) fn checked_sub_amount(left: Amount, right: Amount) -> Result<Amount> {
    let diff =
        left.to_decimal().checked_sub(right.to_decimal()).ok_or_else(|| Error::from("票款金额合计溢出"))?;
    Amount::try_from(diff).map_err(|_| Error::from("票款金额合计溢出"))
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;

    // 回归检查：`Amount::zero()` 与原 `"0.00"` 解析拼写的零金额逐项一致。
    #[test]
    fn zero_matches_legacy_spelling() {
        let zero = zero_amount();
        assert_eq!(zero, Amount::zero());
        assert_eq!(zero.to_string(), "0.00");
        assert_eq!(zero.to_decimal().scale(), 2);
    }

    #[test]
    fn invoice_amounts_keep_zero_components_and_error_precedence() {
        for (gross, net, tax, error) in [
            ("10", "10", "0", None),
            ("10", "0", "10", None),
            ("0", "0", "0", Some("分配金额必须为正数")),
            ("-10", "-10", "0", Some("分配金额必须为正数")),
            ("10", "-1", "11", Some("分配金额必须为正数")),
            ("10", "11", "-1", Some("分配金额必须为正数")),
            ("10", "8", "1", Some("分配含税金额必须等于不含税金额加税额")),
        ] {
            let result =
                validate_invoice_amounts(gross.parse().unwrap(), net.parse().unwrap(), tax.parse().unwrap());
            match error {
                Some(message) => assert!(
                    matches!(result, Err(Error::LogicError(actual)) if actual == message),
                    "{gross} / {net} / {tax}"
                ),
                None => result.unwrap(),
            }
        }
    }

    // 回归检查：调用方指定的溢出文案被原样保留。
    #[test]
    fn checked_add_with_message_keeps_caller_message() {
        let max = Amount::try_from(Decimal::MAX).unwrap();
        let err = checked_add_with_message(max, max, "发票分配金额合计溢出").unwrap_err();
        assert_eq!(err.to_string(), "发票分配金额合计溢出");
    }
}
