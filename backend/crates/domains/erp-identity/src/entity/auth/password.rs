use validator::ValidationError;

/// 设置管理员密码前按原文计算字符数，首尾空白也计入 6–32 字符的长度限制。
/// 越界时返回不含密码内容的字段校验错误。
pub(crate) fn validate_admin_password(password: &str) -> Result<(), ValidationError> {
    if (6..=32).contains(&password.chars().count()) {
        return Ok(());
    }
    Err(ValidationError::new("length").with_message("密码长度必须在6-32个字符之间".into()))
}

#[cfg(test)]
mod tests {
    use super::validate_admin_password;

    // 验证中英文按字符数应用相同边界，首尾空格计入长度，空密码被拒绝。
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
