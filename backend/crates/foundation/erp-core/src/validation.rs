//! 可选联系方式与文本规范化原语。
//!
//! 错误分层：`validator::ValidationError` 仅用于 `validator` 派生所需的
//! 纯格式校验（`validate_phone`/`validate_email`）；其余 `normalize_*`
//! 入口统一返回 `crate::Error`，格式校验经由内部适配收敛为业务文案，
//! 调用方只需处理 `crate::Error`。

use std::sync::LazyLock;

use regex::Regex;
use validator::ValidationError;

use crate::FieldUpdate;
use crate::errors::{Error, Result};

type ValidationResult = std::result::Result<(), ValidationError>;

// 定义常用的正则表达式
static PHONE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^1[3-9]\d{9}$").expect("invalid phone regex"));

static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").expect("invalid email regex")
});

// 自定义验证函数
/// 供 validator 派生校验直接检查手机号。只匹配 `^1[3-9]\d{9}$`，不裁剪输入；不匹配时返回「无效的手机号码格式」。
pub fn validate_phone(phone: &str) -> ValidationResult {
    if !PHONE_REGEX.is_match(phone) {
        return Err(ValidationError::new("无效的手机号码格式"));
    }
    Ok(())
}

/// 供 validator 派生校验邮箱文本，不做裁剪或送达验证。仅接受 `^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$`，
/// 不匹配时返回「无效的邮箱格式」。
pub fn validate_email(email: &str) -> ValidationResult {
    if !EMAIL_REGEX.is_match(email) {
        return Err(ValidationError::new("无效的邮箱格式"));
    }
    Ok(())
}

/// 构造领域值时从借用或自有文本取去除首尾空白的内容；裁剪后为空则使用调用方给出的错误说明。
pub fn non_empty_trimmed(value: impl AsRef<str>, message: &str) -> Result<String> {
    let trimmed = value.as_ref().trim();
    if trimmed.is_empty() {
        return Err(Error::from(message));
    }
    Ok(trimmed.to_string())
}

/// 消费待保存的必填文本，裁剪两端并按字符数限长。全空白与超长分别使用领域提供的文案。
pub fn normalize_required_text(
    value: String,
    empty_message: &str,
    max_len: usize,
    too_long_message: &str,
) -> Result<String> {
    normalize_required_text_ref(&value, empty_message, max_len, too_long_message)
}

/// 在调用方仍需保留输入时，借用原文来规范化必填字段，再返回裁剪后的自有值；先校验非空，再按字符数校验长度。
pub fn normalize_required_text_ref(
    value: &str,
    empty_message: &str,
    max_len: usize,
    too_long_message: &str,
) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::from(empty_message));
    }
    ensure_max_len(value, max_len, || too_long_message.to_owned())?;
    Ok(value.to_owned())
}

/// 保存可选联系邮箱前裁剪首尾空白，未提供或纯空白视为没有地址；有值时先按字符数限长，再验证格式。
pub fn normalize_optional_email(value: Option<String>, max_len: usize) -> Result<Option<String>> {
    normalize_optional_contact(value, max_len, "邮箱", "邮箱长度过长", validate_email, "邮箱格式不正确")
}

/// 保存可选联系电话前裁剪首尾空白，未提供或纯空白视为没有手机号；有值时先按字符数限长，再检查手机号格式。
pub fn normalize_optional_phone(value: Option<String>, max_len: usize) -> Result<Option<String>> {
    normalize_optional_contact(value, max_len, "手机号", "手机号长度过长", validate_phone, "手机号格式不正确")
}

// 让邮箱和手机号共用裁剪、缺省和先长度后格式的顺序。长度错误保留底层原因，validator 格式错误换成领域文案，
// 使调用方只处理业务值错误。
fn normalize_optional_contact(
    value: Option<String>,
    max_len: usize,
    label: &str,
    too_long_message: &str,
    validate_format: fn(&str) -> ValidationResult,
    invalid_format_message: &str,
) -> Result<Option<String>> {
    let value = normalize_optional_text(value, label, max_len)
        .map_err(|error| Error::from(format!("{too_long_message}：{error}")))?;
    let Some(value) = value else {
        return Ok(None);
    };

    if validate_format(value.as_str()).is_err() {
        return Err(Error::from(invalid_format_message));
    }
    Ok(Some(value))
}

/// 保存备注等可选文本前裁剪两端，缺省或纯空白不保存；裁剪后字符数超限时把字段标签写入业务值错误。
pub fn normalize_optional_text(value: Option<String>, label: &str, max_len: usize) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }

    ensure_max_len(value, max_len, || format!("{label}长度不符合要求"))?;
    Ok(Some(value.to_owned()))
}

/// 保存邮箱补丁时保留缺省和清空的区别；只规范化设置值，空白改为清空，超长或格式不合法则拒绝更新。
pub fn normalize_optional_email_update(
    update: FieldUpdate<String>,
    max_len: usize,
) -> Result<FieldUpdate<String>> {
    normalize_field_update(update, |value| normalize_optional_email(Some(value), max_len))
}

/// 保存手机号补丁时只校验设置的新值，空白改为清空；未提交或显式清空原样保留，超长和格式错误向上返回。
pub fn normalize_optional_phone_update(
    update: FieldUpdate<String>,
    max_len: usize,
) -> Result<FieldUpdate<String>> {
    normalize_field_update(update, |value| normalize_optional_phone(Some(value), max_len))
}

// 联系方式补丁只在 `Set` 时调用规范化器，避免缺省字段被清空；规范化结果缺失则清空，失败则不产生补丁。
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

// 共用字符数而非字节数上限，以免多字节文本提前超限；仅在超限时构造文案。
fn ensure_max_len(value: &str, max_len: usize, message: impl FnOnce() -> String) -> Result<()> {
    if value.chars().count() > max_len {
        return Err(Error::from(message()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // 确认必填值校验返回裁剪后的文本，不让入库字符串保留两端空白。
    #[test]
    fn non_empty_trimmed_returns_trimmed_value() {
        let result = non_empty_trimmed("  hello ", "should not be empty").unwrap();
        assert_eq!(result, "hello");
    }

    // 核对借用和自有文本入口使用同样的裁剪、Unicode 字符数上限及空值错误优先级。
    #[test]
    fn required_text_accepts_owned_and_borrowed_inputs() {
        let input = "  商品  ".to_string();
        assert_eq!(normalize_required_text_ref(&input, "不能为空", 2, "过长").unwrap(), "商品");
        assert_eq!(normalize_required_text(input, "不能为空", 2, "过长").unwrap(), "商品");

        assert_eq!(
            normalize_required_text_ref("  ", "不能为空", 2, "过长").unwrap_err().to_string(),
            "不能为空"
        );
        assert_eq!(
            normalize_required_text_ref(" 商品名 ", "不能为空", 2, "过长").unwrap_err().to_string(),
            "过长"
        );
        assert_eq!(
            normalize_required_text("  ".into(), "不能为空", 2, "过长").unwrap_err().to_string(),
            "不能为空"
        );
        assert_eq!(
            normalize_required_text(" 商品名 ".into(), "不能为空", 2, "过长").unwrap_err().to_string(),
            "过长"
        );
        assert!(matches!(
            normalize_required_text_ref("\u{3000}\t ", "不能为空", 0, "过长"),
            Err(Error::LogicError(message)) if message == "不能为空"
        ));
    }

    #[test]
    fn optional_text_preserves_blank_and_unicode_character_limits() {
        assert_eq!(normalize_optional_text(None, "备注", 0).unwrap(), None);
        assert_eq!(normalize_optional_text(Some("\u{3000}\t ".into()), "备注", 0).unwrap(), None);

        assert_eq!(
            normalize_optional_text(Some("\u{3000}商品🙂 \t".into()), "备注", 3).unwrap(),
            Some("商品🙂".into())
        );
        assert!(matches!(
            normalize_optional_text(Some(" 商品🙂 ".into()), "备注", 2),
            Err(Error::LogicError(message)) if message == "备注长度不符合要求"
        ));
    }

    #[test]
    fn contact_length_errors_precede_format_errors() {
        assert!(matches!(
            normalize_optional_email(Some("invalid".into()), 3),
            Err(Error::LogicError(message)) if message == "邮箱长度过长：邮箱长度不符合要求"
        ));
        assert!(matches!(
            normalize_optional_phone(Some("12345".into()), 3),
            Err(Error::LogicError(message)) if message == "手机号长度过长：手机号长度不符合要求"
        ));

        assert!(matches!(
            normalize_optional_email(Some("invalid".into()), 7),
            Err(Error::LogicError(message)) if message == "邮箱格式不正确"
        ));
    }

    #[test]
    fn contact_updates_preserve_unchanged_clear_and_normalized_set() {
        for update in [FieldUpdate::Unchanged, FieldUpdate::Clear] {
            assert_eq!(normalize_optional_email_update(update.clone(), 0).unwrap(), update);
            assert_eq!(normalize_optional_phone_update(update.clone(), 0).unwrap(), update);
        }

        assert_eq!(
            normalize_optional_email_update(FieldUpdate::Set("\u{3000} ".into()), 0).unwrap(),
            FieldUpdate::Clear
        );
        assert_eq!(
            normalize_optional_phone_update(FieldUpdate::Set("\u{3000} ".into()), 0).unwrap(),
            FieldUpdate::Clear
        );

        assert_eq!(
            normalize_optional_email_update(FieldUpdate::Set(" user@example.com ".into()), 16).unwrap(),
            FieldUpdate::Set("user@example.com".into())
        );
        assert_eq!(
            normalize_optional_phone_update(FieldUpdate::Set(" 13900000000 ".into()), 11).unwrap(),
            FieldUpdate::Set("13900000000".into())
        );

        assert!(matches!(
            normalize_optional_phone_update(FieldUpdate::Set("12345".into()), 11),
            Err(Error::LogicError(message)) if message == "手机号格式不正确"
        ));
    }

    // 确认有值的邮箱裁剪后保存，纯空白视为缺省，非法邮箱仍被拒绝。
    #[test]
    fn normalize_optional_email_should_trim_and_validate() {
        let result = normalize_optional_email(Some("  user@example.com  ".to_string()), 128).unwrap();
        assert_eq!(result.as_deref(), Some("user@example.com"));

        let result = normalize_optional_email(Some("  ".to_string()), 128).unwrap();
        assert!(result.is_none());

        assert!(normalize_optional_email(Some("invalid".to_string()), 128).is_err());
    }

    // 确认有值的手机号裁剪后保存，纯空白视为缺省，号码格式不符仍报错。
    #[test]
    fn normalize_optional_phone_should_trim_and_validate() {
        let result = normalize_optional_phone(Some(" 13900000000 ".to_string()), 32).unwrap();
        assert_eq!(result.as_deref(), Some("13900000000"));

        let result = normalize_optional_phone(Some("  ".to_string()), 32).unwrap();
        assert!(result.is_none());

        assert!(normalize_optional_phone(Some("12345".to_string()), 32).is_err());
    }
}
