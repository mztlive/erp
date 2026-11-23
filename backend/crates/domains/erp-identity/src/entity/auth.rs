//! 登录账号名、Argon2id 密码凭证及管理员密码长度校验。
use std::fmt;

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};
use validator::ValidationError;

const ACCOUNT_MIN_LEN: usize = 3;
const ACCOUNT_MAX_LEN: usize = 64;
const ARGON2_PREFIX: &str = "$argon2";
const DUMMY_ARGON2_SALT: &[u8] = b"auth-dummy-salt";

/// 设置管理员密码前按原文计算字符数，首尾空白也计入 6–32 字符的长度限制；越界时返回不含密码内容的字段校验错误。
pub(crate) fn validate_admin_password(password: &str) -> std::result::Result<(), ValidationError> {
    if (6..=32).contains(&password.chars().count()) {
        return Ok(());
    }
    Err(ValidationError::new("length").with_message("密码长度必须在6-32个字符之间".into()))
}

/// 用于创建凭证和登录查找的规范账号名，保留原大小写以延续既有匹配规则。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LoginAccount(String);

impl LoginAccount {
    /// 后台账号在通用区间上进一步收敛的最大长度（[`crate::entity::AccountCore`] 复用）。
    pub const BACKOFFICE_MAX_LEN: usize = 32;

    /// 裁剪登录名首尾空白并保留大小写，裁剪后少于 3 或多于 64 字符时报错。
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let value = value.trim();
        if !(ACCOUNT_MIN_LEN..=ACCOUNT_MAX_LEN).contains(&value.chars().count()) {
            return Err(Error::LogicError("账号长度不符合要求".to_string()));
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for LoginAccount {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// 持久化登录凭证，沿用 `account` 和 `password` 字段名，使用 Argon2id PHC 字符串。
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Secret {
    account: String,
    #[serde(rename = "password")]
    password_hash: String,
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Secret")
            .field("account", &self.account)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

impl Secret {
    /// 为规范登录名创建凭证，用明文生成 Argon2id 哈希；空密码或哈希失败时返回错误。
    pub fn new(account: LoginAccount, password: impl AsRef<str>) -> Result<Self> {
        Ok(Self { account: account.into_string(), password_hash: hash_password(password.as_ref())? })
    }

    pub fn account(&self) -> &str {
        self.account.as_str()
    }

    pub fn into_account(self) -> String {
        self.account
    }

    pub fn change_account(&mut self, account: LoginAccount) {
        self.account = account.into_string();
    }

    /// 设置新密码时生成 Argon2id 哈希；空密码或哈希失败时返回错误并保留旧值。
    pub fn change_password(&mut self, password: impl AsRef<str>) -> Result<()> {
        self.password_hash = hash_password(password.as_ref())?;
        Ok(())
    }

    /// 登录时按持久化 PHC 参数验证 Argon2 凭证；无账号或凭证损坏时执行一次占位计算以缩小时延差异。
    /// 此方法同步消耗 CPU；异步调用方必须使用有界阻塞任务。
    pub fn verify_password_or_dummy(secret: Option<&Self>, password: &str) -> bool {
        if let Some(secret) = secret
            && secret.password_hash.starts_with(ARGON2_PREFIX)
            && let Some(matched) = verify_argon2(password, &secret.password_hash)
        {
            return matched;
        }
        perform_dummy_argon2(password);
        false
    }
}

fn hash_password(password: &str) -> Result<String> {
    if password.is_empty() {
        return Err(Error::LogicError("密码不能为空".to_string()));
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| Error::LogicError("密码处理失败".to_string()))
}

fn verify_argon2(password: &str, encoded_hash: &str) -> Option<bool> {
    let hash = PasswordHash::new(encoded_hash).ok()?;
    if hash.salt.is_none() || hash.hash.is_none() {
        return None;
    }
    match Argon2::default().verify_password(password.as_bytes(), &hash) {
        Ok(()) => Some(true),
        Err(argon2::password_hash::Error::Password) => Some(false),
        Err(_) => None,
    }
}

fn perform_dummy_argon2(password: &str) {
    let salt = SaltString::encode_b64(DUMMY_ARGON2_SALT).expect("dummy Argon2 salt constant must be valid");
    let _ = Argon2::default().hash_password(password.as_bytes(), &salt);
}

#[cfg(test)]
mod tests {
    use super::{LoginAccount, Secret, validate_admin_password};

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

    #[test]
    fn account_constructor_keeps_normalization_and_length_boundaries() {
        assert_eq!(LoginAccount::new("  Admin01  ").unwrap().as_str(), "Admin01");
        for value in ["abc".to_string(), "a".repeat(64)] {
            assert_eq!(LoginAccount::new(&value).unwrap().as_str(), value);
        }
        for value in [String::new(), "  ".to_string(), "  ab  ".to_string(), "a".repeat(65)] {
            assert!(LoginAccount::new(value).is_err());
        }
    }

    #[test]
    fn secret_verifies_argon2_exact_match_and_fails_closed() {
        let secret = Secret::new(LoginAccount::new("admin01").unwrap(), " password123 ").unwrap();
        assert!(secret.password_hash.starts_with("$argon2id$"));
        assert!(Secret::verify_password_or_dummy(Some(&secret), " password123 "));
        assert!(!Secret::verify_password_or_dummy(Some(&secret), "password123"));
        assert!(!Secret::verify_password_or_dummy(Some(&secret), "wrong-password"));

        let malformed =
            Secret { account: "broken01".to_string(), password_hash: "$argon2id$malformed".to_string() };
        assert!(!Secret::verify_password_or_dummy(None, "password123"));
        assert!(!Secret::verify_password_or_dummy(Some(&malformed), "password123"));

        let output = format!("{secret:?}");
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("$argon2"));
    }
}
