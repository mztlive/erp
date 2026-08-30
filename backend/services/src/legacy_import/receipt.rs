use crate::errors::{Error, Result};

/// 解析收据中的整数版本。
pub(super) fn parse_receipt_number<T>(value: &str, field: &str) -> Result<T>
where
    T: std::str::FromStr,
{
    value
        .parse()
        .map_err(|_| Error::Internal(format!("导入确认幂等收据{field}非法")))
}

/// 归一化必填文本。
pub(super) fn required_text(value: &str, message: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(message.to_string()));
    }
    Ok(value.to_string())
}

/// 把 HTTP 边界的字符串版本严格解析为正整数。
pub(super) fn parse_command_version<T>(value: &str, field: &str) -> Result<T>
where
    T: std::str::FromStr + PartialEq + From<u8>,
{
    let value = required_text(value, &format!("{field}不能为空"))?;
    let parsed = value
        .parse::<T>()
        .map_err(|_| Error::ValidationError(format!("{field}必须是正整数")))?;
    if parsed == T::from(0) {
        return Err(Error::ValidationError(format!("{field}必须是正整数")));
    }
    Ok(parsed)
}

/// 归一化可选文本，空白值折叠为空。
pub(super) fn optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}
