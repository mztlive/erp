//! 仅在配送电话和交付日期列转换 Excel 数值，不执行公式或猜测其他标识字段。

use chrono::{Datelike, Days, NaiveDate};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use super::parse::Row;

/// 将可明确识别的数字手机号和 Excel 日期转换为领域输入文本。
/// # 参数
/// `rows` 为已映射的配送行；`date1904` 为工作簿声明的日期系统。
/// # 返回
/// 原位转换成功的单元格；保留文本电话的前导零。
/// # 错误
/// 无法可靠转换时保留原值，交由领域校验定位错误；公式不参与转换。
pub(super) fn normalize(rows: &mut [Row], date1904: bool) {
    for row in rows {
        for index in [2, 8] {
            let Some(cell) = row.cells.get_mut(index).filter(|cell| cell.numeric && !cell.formula) else {
                continue;
            };
            let value = if index == 2 { mobile(&cell.value) } else { date(&cell.value, date1904) };
            if let Some(value) = value {
                cell.value = value;
                cell.text = true;
                cell.numeric = false;
            }
        }
    }
}

/// Excel 数值只兼容可精确表示的 11 位大陆手机号；其他电话须保留文本原样。
fn mobile(value: &str) -> Option<String> {
    let value = integer(value)?.to_string();
    (value.len() == 11 && value.starts_with('1') && matches!(value.as_bytes()[1], b'3'..=b'9'))
        .then_some(value)
}

/// 日期序号必须为整天，拒绝 Excel 虚构的 1900-02-29 及超出四位年份的日期。
fn date(value: &str, date1904: bool) -> Option<String> {
    let serial = integer(value)?;
    let (epoch, days) = if date1904 {
        (NaiveDate::from_ymd_opt(1904, 1, 1)?, serial)
    } else {
        if serial == 0 || serial == 60 {
            return None;
        }
        (NaiveDate::from_ymd_opt(1899, 12, 31)?, serial - u64::from(serial > 60))
    };
    let date = epoch.checked_add_days(Days::new(days))?;
    (date.year() <= 9999).then(|| date.format("%Y-%m-%d").to_string())
}

/// 精确读取 Excel 数值（含科学记数法），禁止舍入小数或截断溢出值。
fn integer(value: &str) -> Option<u64> {
    if let Some((base, _)) = value.split_once(['e', 'E']) {
        // from_scientific 的底数解析允许舍入，先用严格入口排除精度损失。
        Decimal::from_str_exact(base).ok()?;
    }
    let number = Decimal::from_str_exact(value).or_else(|_| Decimal::from_scientific(value)).ok()?;
    if !number.fract().is_zero() || number.is_sign_negative() {
        return None;
    }
    number.to_u64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sales_inquiry_import::parse::Cell;

    #[test]
    fn numeric_mobiles_are_exact_and_ambiguous_numbers_are_rejected() {
        for value in ["13800000000", "13800000000.0", "1.3800000000E+10"] {
            assert_eq!(mobile(value).as_deref(), Some("13800000000"));
        }
        for value in [
            "",
            "12345678",
            "2100123456",
            "13800000000.1",
            "-13800000000",
            "9999999999999999",
            "1e100",
            "13800000000.0000000000000000001e0",
        ] {
            assert_eq!(mobile(value), None, "{value}");
        }
    }

    #[test]
    fn both_excel_epochs_and_leap_day_boundary_are_handled_without_rounding() {
        for (value, epoch, expected) in [
            ("46296", false, "2026-10-01"),
            ("44834", true, "2026-10-01"),
            ("0", true, "1904-01-01"),
            ("1", false, "1900-01-01"),
            ("59", false, "1900-02-28"),
            ("61", false, "1900-03-01"),
        ] {
            assert_eq!(date(value, epoch).as_deref(), Some(expected));
        }
        for value in ["0", "60", "-1", "46296.5", "2958466", "1e100", "2026-10-01"] {
            assert_eq!(date(value, false), None, "{value}");
        }
    }

    #[test]
    fn text_identifiers_formulas_and_non_numeric_types_are_preserved() {
        let mut row = Row { number: 2, cells: vec![Cell::default(); 10] };
        row.cells[0] = Cell { value: "00123".into(), text: true, ..Default::default() };
        row.cells[2] = Cell { value: "02100123456".into(), text: true, ..Default::default() };
        row.cells[8] = Cell { value: "46296".into(), numeric: true, formula: true, ..Default::default() };
        normalize(std::slice::from_mut(&mut row), false);
        assert_eq!(row.cells[0].value, "00123");
        assert_eq!(row.cells[2].value, "02100123456");
        assert_eq!(row.cells[8].value, "46296");
        row.cells[8].formula = false;
        row.cells[8].numeric = false;
        normalize(std::slice::from_mut(&mut row), false);
        assert_eq!(row.cells[8].value, "46296");
    }
}
