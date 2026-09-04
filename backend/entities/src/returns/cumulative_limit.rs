//! 退款与冲正共享累计限额：历史已过账累计加本次不得超过原收付金额。
//!
//! 四类过账（客户退款、供应商退款、回款冲正、付款冲正）复用同一精确金额规则；
//! 权威累计由 Repository 按原单聚合已过账净额提供，本值对象只做纯判断。

use crate::errors::{Error, Result};
use crate::money::Amount;

/// 退款与冲正共享的累计限额判断。
pub struct CumulativeAmountLimit;

impl CumulativeAmountLimit {
    /// 校验历史已过账累计加本次金额是否在原金额内。
    ///
    /// # 参数
    /// * `source` - 原收/付款金额
    /// * `posted_before` - 同一原单下已过账单据的合计（不含本次）
    /// * `current` - 本次申请过账金额
    ///
    /// # 返回
    /// 累计未超限时返回 `Ok(())`。
    ///
    /// # 错误
    /// 历史已超、加上本次刚好超限或超过时返回 `LogicError`；调用方按单据类型映射为面向用户的业务文案。
    ///
    /// # 关键业务约束
    /// 纯内存判断，不做查询、不区分单据类型；等额放行、超额拒绝，零金额由调用方的前置正数校验保证。
    pub fn ensure_within_limit(source: Amount, posted_before: Amount, current: Amount) -> Result<()> {
        if posted_before.checked_add(current) > source {
            return Err(Error::from("累计金额不得超过原金额"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::CumulativeAmountLimit;
    use crate::money::Amount;

    /// 把字符串解析为测试金额。
    ///
    /// # 参数
    /// * `value` - 定点数值字符串
    ///
    /// # 返回
    /// 返回解析后的金额；用例输入均合法。
    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    /// 小于上限放行。
    #[test]
    fn below_limit_passes() {
        assert!(
            CumulativeAmountLimit::ensure_within_limit(amt("10000.00"), amt("6000.00"), amt("3000.00"))
                .is_ok()
        );
    }

    /// 等于上限放行。
    #[test]
    fn equal_to_limit_passes() {
        assert!(
            CumulativeAmountLimit::ensure_within_limit(amt("10000.00"), amt("6000.00"), amt("4000.00"))
                .is_ok()
        );
    }

    /// 超过上限拒绝。
    #[test]
    fn above_limit_fails() {
        assert!(
            CumulativeAmountLimit::ensure_within_limit(amt("10000.00"), amt("6000.00"), amt("4000.01"))
                .is_err()
        );
    }

    /// 历史已超拒绝，本次为零也不放行。
    #[test]
    fn history_already_exceeded_fails() {
        assert!(
            CumulativeAmountLimit::ensure_within_limit(amt("10000.00"), amt("10000.01"), amt("0.00"))
                .is_err()
        );
    }

    /// 零金额边界：空累计加零放行。
    #[test]
    fn zero_amount_boundary() {
        assert!(
            CumulativeAmountLimit::ensure_within_limit(amt("10000.00"), amt("0.00"), amt("0.00")).is_ok()
        );
    }

    /// 精度边界：分位差异必须精确判定。
    #[test]
    fn precision_boundary() {
        assert!(CumulativeAmountLimit::ensure_within_limit(amt("100.00"), amt("99.99"), amt("0.01")).is_ok());
        assert!(
            CumulativeAmountLimit::ensure_within_limit(amt("100.00"), amt("99.99"), amt("0.02")).is_err()
        );
    }
}
