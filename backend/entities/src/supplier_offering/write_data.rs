//! 供给写入字符串解析：修订条款与可供数量的 I/O-free 转换。
//!
//! Service 的写 DTO 只负责形状与传输，解析边界（空白、非法数值、有效期、
//! 零数量、可空金额）统一收敛在本模块，保证首版与后续修订共用同一规则。

use std::str::FromStr;

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::money::{Amount, Quantity, Rate, UnitPrice};

/// 解析进项税率字符串。
///
/// # 参数
/// * `raw` - 原始税率字符串，允许首尾空白
///
/// # 返回
/// 返回类型化税率。
///
/// # 错误
/// 空白或非法数值时返回领域错误，文案为 `非法进项税率: {raw}`。
///
/// # 约束
/// 纯内存转换，不触碰 I/O、时钟、ID 生成器或密钥。
pub fn parse_input_tax_rate(raw: &str) -> Result<Rate> {
    Rate::from_str(raw.trim()).map_err(|_| Error::from(format!("非法进项税率: {raw}")))
}

/// 按标签解析单价字符串。
///
/// # 参数
/// * `value` - 原始单价字符串，允许首尾空白
/// * `label` - 字段标签，用于错误文案（如 `一件代发供给价`）
///
/// # 返回
/// 返回类型化单价。
///
/// # 错误
/// 空白或非法数值时返回领域错误，文案为 `非法{label}: {value}`。
///
/// # 约束
/// 纯内存转换，不触碰 I/O、时钟、ID 生成器或密钥。
pub fn parse_unit_price(value: &str, label: &str) -> Result<UnitPrice> {
    UnitPrice::from_str(value.trim()).map_err(|_| Error::from(format!("非法{label}: {value}")))
}

/// 解析可空金额字符串。
///
/// # 参数
/// * `value` - 原始金额字符串；`None` 或空白表示缺省
///
/// # 返回
/// 缺省时返回 `None`，否则返回类型化金额。
///
/// # 错误
/// 非法数值时返回领域错误，文案为 `非法金额: {value}`。
///
/// # 约束
/// 纯内存转换，不触碰 I/O、时钟、ID 生成器或密钥。
pub fn parse_optional_amount(value: Option<&str>) -> Result<Option<Amount>> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| Amount::from_str(value).map_err(|_| Error::from(format!("非法金额: {value}"))))
        .transpose()
}

/// 解析可空数量字符串。
///
/// # 参数
/// * `value` - 原始数量字符串；`None` 或空白表示缺省
///
/// # 返回
/// 缺省时返回 `None`，否则返回类型化数量。
///
/// # 错误
/// 非法数值时返回领域错误，文案为 `非法数量: {value}`。
///
/// # 约束
/// 纯内存转换，不触碰 I/O、时钟、ID 生成器或密钥。
pub fn parse_optional_quantity(value: Option<&str>) -> Result<Option<Quantity>> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| Quantity::from_str(value).map_err(|_| Error::from(format!("非法数量: {value}"))))
        .transpose()
}

/// 解析集采起订量字符串。
///
/// # 参数
/// * `value` - 原始起订量字符串，允许首尾空白
///
/// # 返回
/// 返回类型化数量；正负与零值判定由修订实体继续校验。
///
/// # 错误
/// 空白或非法数值时返回领域错误，文案为 `非法集采起订量: {value}`。
///
/// # 约束
/// 纯内存转换，不触碰 I/O、时钟、ID 生成器或密钥。
pub fn parse_minimum_order_quantity(value: &str) -> Result<Quantity> {
    Quantity::from_str(value.trim()).map_err(|_| Error::from(format!("非法集采起订量: {value}")))
}

/// 解析业务日期字符串。
///
/// # 参数
/// * `value` - 原始业务日期字符串，允许首尾空白
///
/// # 返回
/// 返回类型化业务日期。
///
/// # 错误
/// 空白或非法形态时返回领域错误，文案为 `非法业务日期: {value}`。
///
/// # 约束
/// 纯内存转换，不触碰 I/O、时钟、ID 生成器或密钥。
pub fn parse_business_date(value: &str) -> Result<BusinessDate> {
    BusinessDate::from_str(value.trim()).map_err(|_| Error::from(format!("非法业务日期: {value}")))
}

/// 解析可选业务日期字符串。
///
/// # 参数
/// * `value` - 原始业务日期字符串；`None` 或空白表示缺省
///
/// # 返回
/// 缺省时返回 `None`，否则返回类型化业务日期。
///
/// # 错误
/// 非法形态时返回领域错误，文案为 `非法业务日期: {value}`。
///
/// # 约束
/// 纯内存转换，不触碰 I/O、时钟、ID 生成器或密钥。
pub fn parse_optional_business_date(value: Option<&str>) -> Result<Option<BusinessDate>> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_business_date)
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::{
        parse_business_date, parse_input_tax_rate, parse_minimum_order_quantity, parse_optional_amount,
        parse_optional_quantity, parse_unit_price,
    };

    #[test]
    fn blank_and_illegal_inputs_fail_closed() {
        assert!(parse_input_tax_rate("  ").is_err());
        assert!(parse_input_tax_rate("abc").is_err());
        assert!(parse_unit_price("  ", "一件代发供给价").is_err());
        assert!(parse_unit_price("abc", "一件代发供给价").is_err());
        assert!(parse_minimum_order_quantity(" ").is_err());
        assert!(parse_business_date("not-a-date").is_err());
        assert!(parse_optional_amount(Some("abc")).is_err());
        assert!(parse_optional_quantity(Some("abc")).is_err());
    }

    #[test]
    fn whitespace_is_normalized_and_empty_is_absent() {
        assert!(parse_input_tax_rate(" 0.13 ").is_ok());
        assert!(parse_unit_price(" 10.00 ", "一件代发供给价").is_ok());
        assert!(parse_optional_amount(None).unwrap().is_none());
        assert!(parse_optional_amount(Some("  ")).unwrap().is_none());
        assert!(parse_optional_quantity(None).unwrap().is_none());
        assert!(parse_optional_quantity(Some(" ")).unwrap().is_none());
        assert!(parse_optional_amount(Some(" 1.50 ")).unwrap().is_some());
        assert!(parse_optional_quantity(Some(" 8 ")).unwrap().is_some());
    }

    #[test]
    fn zero_quantity_parses_and_defers_policy_to_entity() {
        let parsed = parse_optional_quantity(Some("0")).unwrap();
        assert!(parsed.is_some());
        let moq = parse_minimum_order_quantity("0").unwrap();
        assert_eq!(moq.to_string(), "0");
    }

    #[test]
    fn validity_window_boundaries_are_preserved() {
        let from = parse_business_date("2026-01-01").unwrap();
        let same = parse_business_date("2026-01-01").unwrap();
        assert_eq!(from, same);
        assert!(parse_business_date("2026-02-01").unwrap() > from);
    }

    #[test]
    fn error_messages_keep_api_contract() {
        assert_eq!(
            parse_input_tax_rate("abc").unwrap_err().to_string(),
            "非法进项税率: abc"
        );
        assert_eq!(
            parse_unit_price("abc", "一件代发供给价").unwrap_err().to_string(),
            "非法一件代发供给价: abc"
        );
        assert_eq!(
            parse_optional_amount(Some("abc")).unwrap_err().to_string(),
            "非法金额: abc"
        );
        assert_eq!(
            parse_optional_quantity(Some("abc")).unwrap_err().to_string(),
            "非法数量: abc"
        );
        assert_eq!(
            parse_minimum_order_quantity("abc").unwrap_err().to_string(),
            "非法集采起订量: abc"
        );
        assert_eq!(
            parse_business_date("abc").unwrap_err().to_string(),
            "非法业务日期: abc"
        );
    }
}
