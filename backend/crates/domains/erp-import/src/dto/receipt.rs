use crate::error::{Error, Result};

/// 按 `FromStr` 解析回执字段，不限定为整数。
///
/// # 参数
/// * `value` - 待解析文本。
/// * `field` - 拼进错误文案的字段名。
///
/// # 返回
/// 返回解析后的值。
///
/// # 错误
/// `FromStr` 失败时返回 [`Error::Internal`]，文案为导入确认幂等收据字段非法。
pub fn parse_receipt_number<T>(value: &str, field: &str) -> Result<T>
where
    T: std::str::FromStr,
{
    value.parse().map_err(|_| Error::Internal(format!("导入确认幂等收据{field}非法")))
}

/// 归一化必填文本。
///
/// # 参数
/// * `value` - 原始文本。
/// * `message` - 去空白后为空时的校验文案。
///
/// # 返回
/// 返回去首尾空白后的非空文本。
///
/// # 错误
/// 去空白后为空时返回 [`Error::ValidationError`]。
pub fn required_text(value: &str, message: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(message.to_string()));
    }
    Ok(value.to_string())
}

/// 把 HTTP 边界的字符串版本严格解析为正整数。
///
/// # 参数
/// * `value` - 版本文本。
/// * `field` - 拼进错误文案的字段名。
///
/// # 返回
/// 返回大于 0 的版本值。
///
/// # 错误
/// 空白、无法按 `FromStr` 解析或值为 0 时返回 [`Error::ValidationError`]。
pub fn parse_command_version<T>(value: &str, field: &str) -> Result<T>
where
    T: std::str::FromStr + PartialEq + From<u8>,
{
    let value = required_text(value, &format!("{field}不能为空"))?;
    let parsed = value.parse::<T>().map_err(|_| Error::ValidationError(format!("{field}必须是正整数")))?;
    if parsed == T::from(0) {
        return Err(Error::ValidationError(format!("{field}必须是正整数")));
    }
    Ok(parsed)
}

/// 归一化可选文本，空白值折叠为 `None`。
///
/// # 参数
/// * `value` - 可选原始文本。
///
/// # 返回
/// 去首尾空白后非空时返回该文本，否则返回 `None`。
///
/// # 错误
/// 不返回错误。
pub fn optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}
