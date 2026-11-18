use crate::error::{Error, Result};

/// 用 `FromStr` 还原回执字段，不限于整数；解析失败返回 [`Error::Internal`]，并在导入确认幂等收据的错误中指出字段名。
pub fn parse_receipt_number<T>(value: &str, field: &str) -> Result<T>
where
    T: std::str::FromStr,
{
    value.parse().map_err(|_| Error::Internal(format!("导入确认幂等收据{field}非法")))
}

/// 清理命令必填文本的首尾空白；清理后为空时用调用方提供的文案返回 [`Error::ValidationError`]，不让缺失字段进入命令处理。
pub fn required_text(value: &str, message: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(message.to_string()));
    }
    Ok(value.to_string())
}

/// 把 HTTP 命令携带的文本版本转成对应版本类型，以便对照冻结版本。去空白后为空、解析失败或值为零时返回带字段名的 [`Error::ValidationError`]。
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

/// 清理可选说明、原因等文本的首尾空白，将空白折叠为 `None`，使缺失值比较和命令指纹保持一致。
pub fn optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}
