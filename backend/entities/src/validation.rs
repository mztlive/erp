use regex::Regex;
use std::sync::LazyLock;
use validator::ValidationError;

use crate::errors::{Error, Result};
use crate::FieldUpdate;

type ValidationResult = std::result::Result<(), ValidationError>;

// 定义常用的正则表达式
static PHONE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^1[3-9]\d{9}$").expect("invalid phone regex"));

static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").expect("invalid email regex")
});

// 自定义验证函数
/// 校验 `phone` 的格式或取值。
///
/// # 参数
/// * `phone` - 手机号
///
/// # 返回
/// 返回校验结果，`Ok(())` 表示通过，`Err` 表示未通过。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
pub fn validate_phone(phone: &str) -> ValidationResult {
    if !PHONE_REGEX.is_match(phone) {
        return Err(ValidationError::new("无效的手机号码格式"));
    }
    Ok(())
}

/// 校验 `email` 的格式或取值。
///
/// # 参数
/// * `email` - 邮箱地址
///
/// # 返回
/// 返回校验结果，`Ok(())` 表示通过，`Err` 表示未通过。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
pub fn validate_email(email: &str) -> ValidationResult {
    if !EMAIL_REGEX.is_match(email) {
        return Err(ValidationError::new("无效的邮箱格式"));
    }
    Ok(())
}

/// 去除首尾空白并校验非空，常用于领域对象的基础字符串验证。
///
/// # 参数
/// * `value` - 值
/// * `message` - 提示信息
///
/// # 返回
/// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
///
/// # 错误
/// 当验证失败或底层操作失败时返回错误。
pub fn non_empty_trimmed(value: String, message: &str) -> crate::errors::Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::from(message));
    }
    Ok(trimmed.to_string())
}

/// 规范化必填文本字段（去空白 + 非空 + 最大长度）。
///
/// # 参数
/// * `value` - 文本内容
/// * `empty_message` - 为空时错误信息
/// * `max_len` - 最大长度
/// * `too_long_message` - 超长时错误信息
///
/// # 返回值
/// 返回规范化后的文本
///
/// # 错误
/// 当字段为空或长度超限时返回错误
pub fn normalize_required_text(
    value: String,
    empty_message: &str,
    max_len: usize,
    too_long_message: &str,
) -> Result<String> {
    let value = non_empty_trimmed(value, empty_message)?;
    ensure_max_len(value.as_str(), max_len, too_long_message)?;
    Ok(value)
}

/// 规范化可选邮箱字段（去空白 + 长度 + 格式）。
///
/// # 参数
/// * `value` - 可选邮箱内容
/// * `max_len` - 最大长度
///
/// # 返回值
/// 返回规范化后的邮箱或 None
///
/// # 错误
/// 当邮箱超长或格式非法时返回错误
pub fn normalize_optional_email(value: Option<String>, max_len: usize) -> Result<Option<String>> {
    let value = normalize_optional_text(value, "邮箱", max_len).map_err(|_| Error::from("邮箱长度过长"))?;

    let Some(value) = value else {
        return Ok(None);
    };

    if validate_email(value.as_str()).is_err() {
        return Err(Error::from("邮箱格式不正确"));
    }

    Ok(Some(value))
}

/// 规范化可选手机号字段（去空白 + 长度 + 格式）。
///
/// # 参数
/// * `value` - 可选手机号内容
/// * `max_len` - 最大长度
///
/// # 返回值
/// 返回规范化后的手机号或 None
///
/// # 错误
/// 当手机号超长或格式非法时返回错误
pub fn normalize_optional_phone(value: Option<String>, max_len: usize) -> Result<Option<String>> {
    let value =
        normalize_optional_text(value, "手机号", max_len).map_err(|_| Error::from("手机号长度过长"))?;

    let Some(value) = value else {
        return Ok(None);
    };

    if validate_phone(value.as_str()).is_err() {
        return Err(Error::from("手机号格式不正确"));
    }

    Ok(Some(value))
}

/// 规范化可选文本字段。
///
/// # 参数
/// * `value` - 可选文本内容
/// * `label` - 字段说明
/// * `max_len` - 最大长度
///
/// # 返回值
/// 返回规范化后的文本或 None
pub fn normalize_optional_text(value: Option<String>, label: &str, max_len: usize) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };

    let value = value.trim().to_string();
    if value.is_empty() {
        return Ok(None);
    }

    ensure_max_len(
        value.as_str(),
        max_len,
        format!("{}长度不符合要求", label).as_str(),
    )?;

    Ok(Some(value))
}

/// 规范化可空邮箱字段的更新意图。
///
/// # 参数
/// * `update` - 区分未提供、清除与设置的更新意图
/// * `max_len` - 最大长度
///
/// # 返回值
/// 返回规范化后的更新意图。
///
/// # 错误
/// 当邮箱长度或格式不合法时返回错误。
pub fn normalize_optional_email_update(
    update: FieldUpdate<String>,
    max_len: usize,
) -> Result<FieldUpdate<String>> {
    normalize_field_update(update, |value| normalize_optional_email(Some(value), max_len))
}

/// 规范化可空手机号字段的更新意图。
///
/// # 参数
/// * `update` - 区分未提供、清除与设置的更新意图
/// * `max_len` - 最大长度
///
/// # 返回值
/// 返回规范化后的更新意图。
///
/// # 错误
/// 当手机号长度或格式不合法时返回错误。
pub fn normalize_optional_phone_update(
    update: FieldUpdate<String>,
    max_len: usize,
) -> Result<FieldUpdate<String>> {
    normalize_field_update(update, |value| normalize_optional_phone(Some(value), max_len))
}

/// 对具体值执行规范化，并保留字段更新意图。
fn normalize_field_update(
    update: FieldUpdate<String>,
    normalize: impl FnOnce(String) -> Result<Option<String>>,
) -> Result<FieldUpdate<String>> {
    let FieldUpdate::Set(value) = update else {
        return Ok(update);
    };

    Ok(match normalize(value)? {
        Some(value) => FieldUpdate::Set(value),
        None => FieldUpdate::Clear,
    })
}

/// 校验文本长度不超过上限。
///
/// # 参数
/// * `value` - 文本内容
/// * `max_len` - 最大长度
/// * `message` - 长度超限时错误信息
///
/// # 返回值
/// 通过返回 Ok
///
/// # 错误
/// 当长度超限时返回错误
fn ensure_max_len(value: &str, max_len: usize, message: &str) -> Result<()> {
    if value.chars().count() > max_len {
        return Err(Error::from(message));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 `non_empty_trimmed_returns_trimmed_value` 行为。
    ///
    /// # 返回
    /// 不返回数据，仅表示执行结果。
    #[test]
    fn non_empty_trimmed_returns_trimmed_value() {
        let result = non_empty_trimmed("  hello ".to_string(), "should not be empty").unwrap();
        assert_eq!(result, "hello");
    }

    /// 校验可选邮箱规范化规则。
    ///
    /// # 返回
    /// 不返回数据，仅表示执行结果。
    #[test]
    fn normalize_optional_email_should_trim_and_validate() {
        let result = normalize_optional_email(Some("  user@example.com  ".to_string()), 128).unwrap();
        assert_eq!(result.as_deref(), Some("user@example.com"));

        let result = normalize_optional_email(Some("  ".to_string()), 128).unwrap();
        assert!(result.is_none());

        assert!(normalize_optional_email(Some("invalid".to_string()), 128).is_err());
    }

    /// 校验可选手机号规范化规则。
    ///
    /// # 返回
    /// 不返回数据，仅表示执行结果。
    #[test]
    fn normalize_optional_phone_should_trim_and_validate() {
        let result = normalize_optional_phone(Some(" 13900000000 ".to_string()), 32).unwrap();
        assert_eq!(result.as_deref(), Some("13900000000"));

        let result = normalize_optional_phone(Some("  ".to_string()), 32).unwrap();
        assert!(result.is_none());

        assert!(normalize_optional_phone(Some("12345".to_string()), 32).is_err());
    }
}
