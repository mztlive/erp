use std::fmt;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};

/// 登录账号最小长度。
const ACCOUNT_MIN_LEN: usize = 3;
/// 登录账号最大长度。
const ACCOUNT_MAX_LEN: usize = 64;
/// Argon2 PHC 字符串前缀。
const ARGON2_PREFIX: &str = "$argon2";
/// 旧版 MD5 摘要的十六进制长度。
const LEGACY_MD5_LEN: usize = 32;
/// Dummy Argon2 使用的固定盐，仅用于平衡失败路径的计算成本。
const DUMMY_ARGON2_SALT: &[u8] = b"auth-dummy-salt";

/// 密码校验结果。
///
/// 该结果只暴露是否匹配及是否需要迁移，不暴露任何密码哈希内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordVerification {
    /// 密码不匹配，或持久化哈希格式无效。
    Mismatch,
    /// 密码匹配当前 Argon2 哈希。
    Current,
    /// 密码匹配旧 MD5 摘要，需要在认证成功前迁移。
    Legacy,
}

/// 已完成规范化和基础校验的登录账号。
///
/// 账号仅去除首尾空白，不改变大小写，避免改变既有账号的匹配语义。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LoginAccount(String);

impl LoginAccount {
    /// 规范化并校验登录账号。
    ///
    /// # 参数
    /// * `value` - 原始登录账号
    ///
    /// # 返回值
    /// 返回已去除首尾空白的登录账号。
    ///
    /// # 错误
    /// 当账号为空或字符数不在允许区间时返回错误。
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let value = value.trim();
        let length = value.chars().count();
        if !(ACCOUNT_MIN_LEN..=ACCOUNT_MAX_LEN).contains(&length) {
            return Err(Error::LogicError("账号长度不符合要求".to_string()));
        }

        Ok(Self(value.to_string()))
    }

    /// 返回规范化后的账号字符串。
    ///
    /// # 返回值
    /// 返回账号的借用视图。
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// 消费值对象并返回账号字符串。
    ///
    /// # 返回值
    /// 返回规范化后的账号所有权。
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for LoginAccount {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl TryFrom<String> for LoginAccount {
    type Error = Error;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for LoginAccount {
    type Error = Error;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// 账号登录凭证。
///
/// 持久化字段名保持为 `account` 和 `password`。新密码使用 Argon2id PHC
/// 字符串，旧 MD5 摘要仅用于兼容验证和登录后的透明迁移。
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
    /// 使用已规范化账号和明文密码创建登录凭证。
    ///
    /// # 参数
    /// * `account` - 已规范化的登录账号
    /// * `password` - 明文密码
    ///
    /// # 返回值
    /// 返回采用 Argon2id 哈希的新凭证。
    ///
    /// # 错误
    /// 当密码为空或密码哈希失败时返回错误。
    pub fn new(account: LoginAccount, password: impl AsRef<str>) -> Result<Self> {
        Ok(Self {
            account: account.into_string(),
            password_hash: hash_password(password.as_ref())?,
        })
    }

    /// 返回规范化后的登录账号。
    ///
    /// # 返回值
    /// 返回账号字符串的借用视图。
    pub fn account(&self) -> &str {
        self.account.as_str()
    }

    /// 消费凭证并返回登录账号。
    ///
    /// # 返回值
    /// 返回账号字符串所有权；密码哈希会随其余凭证一起被丢弃。
    pub fn into_account(self) -> String {
        self.account
    }

    /// 使用已规范化账号替换当前登录账号。
    ///
    /// # 参数
    /// * `account` - 已规范化的新登录账号
    pub fn change_account(&mut self, account: LoginAccount) {
        self.account = account.into_string();
    }

    /// 使用 Argon2id 替换当前密码哈希。
    ///
    /// # 参数
    /// * `password` - 新的明文密码
    ///
    /// # 错误
    /// 当密码为空或密码哈希失败时返回错误。
    pub fn change_password(&mut self, password: impl AsRef<str>) -> Result<()> {
        self.password_hash = hash_password(password.as_ref())?;
        Ok(())
    }

    /// 验证明文密码，并在没有凭证时执行等成本的 Argon2 dummy work。
    ///
    /// Argon2 PHC 字符串按其中记录的参数验证。旧 MD5 摘要先完成兼容
    /// 校验，再执行一次默认参数的 Argon2 dummy work。没有凭证、未知格式
    /// 或损坏的 Argon2 哈希同样执行 dummy work，减少失败路径的时延差异。
    ///
    /// 该方法是 CPU 密集型同步能力，异步调用方必须通过有界阻塞任务执行。
    ///
    /// # 参数
    /// * `secret` - 已加载凭证；账号不存在或不可认证时传入 `None`
    /// * `password` - 待验证的明文密码
    ///
    /// # 返回值
    /// 返回匹配状态及是否需要迁移。
    pub fn verify_password_or_dummy(secret: Option<&Self>, password: &str) -> PasswordVerification {
        let Some(secret) = secret else {
            perform_dummy_argon2(password);
            return PasswordVerification::Mismatch;
        };

        if secret.password_hash.starts_with(ARGON2_PREFIX) {
            return match verify_argon2(password, &secret.password_hash) {
                Some(true) => PasswordVerification::Current,
                Some(false) => PasswordVerification::Mismatch,
                None => {
                    perform_dummy_argon2(password);
                    PasswordVerification::Mismatch
                }
            };
        }

        if secret.is_legacy_password_hash() {
            let matches = verify_legacy_md5(password, &secret.password_hash);
            perform_dummy_argon2(password);
            return if matches {
                PasswordVerification::Legacy
            } else {
                PasswordVerification::Mismatch
            };
        }

        perform_dummy_argon2(password);
        PasswordVerification::Mismatch
    }

    /// 判断当前凭证是否仍使用可迁移的旧 MD5 摘要。
    ///
    /// # 返回值
    /// 哈希是合法的 32 位十六进制 MD5 摘要时返回 `true`。
    pub fn is_legacy_password_hash(&self) -> bool {
        self.password_hash.len() == LEGACY_MD5_LEN
            && self.password_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

/// 使用 Argon2id 生成 PHC 格式的密码哈希。
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

/// 验证 Argon2 PHC 密码哈希，格式损坏时返回 `None`。
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

/// 使用与新密码哈希相同的默认参数执行一次 Argon2 工作。
fn perform_dummy_argon2(password: &str) {
    let salt = SaltString::encode_b64(DUMMY_ARGON2_SALT).expect("dummy Argon2 salt constant must be valid");
    let _ = Argon2::default().hash_password(password.as_bytes(), &salt);
}

/// 验证兼容期内的旧 MD5 密码摘要。
fn verify_legacy_md5(password: &str, encoded_hash: &str) -> bool {
    let candidate = format!("{:x}", md5::compute(password.as_bytes()));
    constant_time_eq(candidate.as_bytes(), encoded_hash.as_bytes())
}

/// 以固定遍历次数比较两个等长摘要，避免提前退出。
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

#[cfg(test)]
mod tests {
    use super::{LoginAccount, PasswordVerification, Secret};

    #[test]
    fn login_account_should_trim_without_changing_case() {
        let account = LoginAccount::new("  Admin01  ").unwrap();

        assert_eq!(account.as_str(), "Admin01");
    }

    #[test]
    fn login_account_should_reject_out_of_range_values() {
        assert!(LoginAccount::new("  ").is_err());
        assert!(LoginAccount::new("a".repeat(65)).is_err());
    }

    #[test]
    fn new_secret_should_use_argon2_and_match_password() {
        let secret = Secret::new(LoginAccount::new("admin01").unwrap(), "password123").unwrap();

        assert!(secret.password_hash.starts_with("$argon2id$"));
        assert_eq!(
            Secret::verify_password_or_dummy(Some(&secret), "password123"),
            PasswordVerification::Current
        );
        assert_eq!(
            Secret::verify_password_or_dummy(Some(&secret), "wrong-password"),
            PasswordVerification::Mismatch
        );
        assert!(!secret.is_legacy_password_hash());
    }

    #[test]
    fn password_whitespace_should_be_preserved_for_exact_matching() {
        let secret = Secret::new(LoginAccount::new("admin01").unwrap(), " password123 ").unwrap();

        assert_eq!(
            Secret::verify_password_or_dummy(Some(&secret), " password123 "),
            PasswordVerification::Current
        );
        assert_eq!(
            Secret::verify_password_or_dummy(Some(&secret), "password123"),
            PasswordVerification::Mismatch
        );
    }

    #[test]
    fn legacy_md5_secret_should_verify_for_migration() {
        let secret = Secret {
            account: "legacy01".to_string(),
            password_hash: format!("{:x}", md5::compute(b"password123")),
        };

        assert_eq!(
            Secret::verify_password_or_dummy(Some(&secret), "password123"),
            PasswordVerification::Legacy
        );
        assert_eq!(
            Secret::verify_password_or_dummy(Some(&secret), "wrong-password"),
            PasswordVerification::Mismatch
        );
        assert!(secret.is_legacy_password_hash());
    }

    #[test]
    fn change_password_should_upgrade_legacy_hash() {
        let mut secret = Secret {
            account: "legacy01".to_string(),
            password_hash: format!("{:x}", md5::compute(b"password123")),
        };

        secret.change_password("password123").unwrap();

        assert_eq!(
            Secret::verify_password_or_dummy(Some(&secret), "password123"),
            PasswordVerification::Current
        );
        assert!(!secret.is_legacy_password_hash());
    }

    #[test]
    fn missing_and_malformed_secret_should_fail_closed() {
        let malformed = Secret {
            account: "broken01".to_string(),
            password_hash: "$argon2id$malformed".to_string(),
        };

        assert_eq!(
            Secret::verify_password_or_dummy(None, "password123"),
            PasswordVerification::Mismatch
        );
        assert_eq!(
            Secret::verify_password_or_dummy(Some(&malformed), "password123"),
            PasswordVerification::Mismatch
        );
    }

    #[test]
    fn debug_output_should_redact_password_hash() {
        let secret = Secret::new(LoginAccount::new("admin01").unwrap(), "password123").unwrap();
        let output = format!("{secret:?}");

        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("$argon2"));
    }
}
