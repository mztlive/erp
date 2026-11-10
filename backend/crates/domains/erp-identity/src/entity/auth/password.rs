use validator::ValidationError;

/// 校验管理员新密码的字符长度，保留空白与原始内容。
/// # 参数
/// password 为待设置的明文密码。
/// # 返回
/// 字符数位于 6–32 时返回成功。
/// # 错误
/// 长度不合法时返回不含密码内容的字段校验错误。
pub(crate) fn validate_admin_password(password: &str) -> Result<(), ValidationError> {
    if (6..=32).contains(&password.chars().count()) {
        return Ok(());
    }
    Err(ValidationError::new("length").with_message("密码长度必须在6-32个字符之间".into()))
}

#[cfg(test)]
mod tests {
    use super::validate_admin_password;

    #[test]
    fn password_length_counts_characters_and_preserves_spaces() {
        for (length, valid) in [(5, false), (6, true), (32, true), (33, false)] {
            for character in ['a', '密'] {
                assert_eq!(validate_admin_password(&character.to_string().repeat(length)).is_ok(), valid);
            }
        }
        assert!(validate_admin_password(" abcde").is_ok());
        assert!(validate_admin_password("abcde ").is_ok());
        assert!(validate_admin_password("").is_err());
    }
}
