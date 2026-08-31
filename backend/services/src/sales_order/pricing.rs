//! 金额静态值。

use std::str::FromStr;

use entities::money::Amount;

/// 返回静态零金额。
///
/// # 返回
/// 返回 `0.00` 金额值。
pub(super) fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("静态零值必须合法")
}
