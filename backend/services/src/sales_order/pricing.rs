//! 行金额汇总（§4.2 铁律 2：表头只汇总已舍入的行金额）。

use std::str::FromStr;

use entities::money::Amount;
use entities::sales_order::{SalesOrderSubmissionLine, SalesOrderWorkingCopyLine};

/// 汇总已舍入的行金额三元组（§4.2 铁律 2：表头只汇总已舍入的行金额）。
///
/// # 参数
/// * `lines` - 行实体（金额已在实体 `new` 内逐行舍入）
///
/// # 返回
/// 返回 `(含税合计, 不含税合计, 税额合计)`。
pub(super) fn line_totals<T: LineAmounts>(lines: &[T]) -> (Amount, Amount, Amount) {
    let zero = zero_amount();
    let gross = lines
        .iter()
        .fold(zero, |acc, line| acc.checked_add(line.gross_amount()));
    let net = lines
        .iter()
        .fold(zero, |acc, line| acc.checked_add(line.net_amount()));
    let tax = lines
        .iter()
        .fold(zero, |acc, line| acc.checked_add(line.tax_amount()));
    (gross, net, tax)
}

/// 行金额访问器（工作副本行与提交行共用）。
pub(super) trait LineAmounts {
    /// 返回行含税金额。
    fn gross_amount(&self) -> Amount;
    /// 返回行不含税金额。
    fn net_amount(&self) -> Amount;
    /// 返回行税额。
    fn tax_amount(&self) -> Amount;
}

impl LineAmounts for SalesOrderWorkingCopyLine {
    fn gross_amount(&self) -> Amount {
        self.gross_amount
    }
    fn net_amount(&self) -> Amount {
        self.net_amount
    }
    fn tax_amount(&self) -> Amount {
        self.tax_amount
    }
}

impl LineAmounts for SalesOrderSubmissionLine {
    fn gross_amount(&self) -> Amount {
        self.gross_amount
    }
    fn net_amount(&self) -> Amount {
        self.net_amount
    }
    fn tax_amount(&self) -> Amount {
        self.tax_amount
    }
}

/// 返回静态零金额。
///
/// # 返回
/// 返回 `0.00` 金额值。
pub(super) fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("静态零值必须合法")
}
