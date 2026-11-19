use std::fmt;

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

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

/// 用于创建凭证和登录查找的规范账号名，保留原大小写以延续既有匹配规则。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LoginAccount(
    // 裁剪首尾空白后通过通用长度校验的登录名。
    String
);

impl LoginAccount {
    /// 后台账号在通用区间上进一步收敛的最大长度（[`crate::entity::AccountCore`] 复用）。
    pub const BACKOFFICE_MAX_LEN: usize = 32;

    /// 裁剪登录名首尾空白并保留大小写，裁剪后少于 3 或多于 64 字符时报错。
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let value = value.trim();
        let length = value.chars().count();
        if !(ACCOUNT_MIN_LEN..=ACCOUNT_MAX_LEN).contains(&length) {
            return Err(Error::LogicError("账号长度不符合要求".to_string()));
        }
        Ok(Self(value.to_string()))
    }

    /// 借用规范登录名，供账号查找和精确匹配。
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// 交出规范登录名的所有权，供凭证保存。
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for LoginAccount {
    // 向接受字符串借用的接口提供规范登录名，无需复制。
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// 持久化登录凭证，沿用 `account` 和 `password` 字段名。
/// 新密码存为 Argon2id PHC 字符串，旧 MD5 摘要仅用于兼容验证和登录迁移。
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Secret {
    // 用于登录及凭证归属核对的账号名，持久化为 `account`。
    account: String,
    // 存于 `password` 的验密哈希，可能是 Argon2 PHC 字符串或待迁移的 MD5 摘要。
    #[serde(rename = "password")]
    password_hash: String,
}

impl fmt::Debug for Secret {
    // 调试时显示登录名，密码字段固定为 `[REDACTED]`；格式化失败时传递错误。
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

    /// 借用凭证中的登录名，供认证身份比较或展示。
    pub fn account(&self) -> &str {
        self.account.as_str()
    }

    /// 消费凭证并交出登录名，密码哈希随凭证丢弃。
    pub fn into_account(self) -> String {
        self.account
    }

    /// 用已规范化的登录名更新凭证归属，保留原密码哈希。
    pub fn change_account(&mut self, account: LoginAccount) {
        self.account = account.into_string();
    }

    /// 设置新密码或迁移旧凭证时生成 Argon2id 哈希；空密码或哈希失败时返回错误并保留旧值。
    pub fn change_password(&mut self, password: impl AsRef<str>) -> Result<()> {
        self.password_hash = hash_password(password.as_ref())?;
        Ok(())
    }

    /// 登录时按持久化 PHC 参数验证 Argon2 凭证；无账号或账号不可认证时传入 `None`。
    /// 旧 MD5 验证后，以及凭证缺失、格式未知或 Argon2 哈希损坏时，执行一次默认参数的 Argon2 占位计算以缩小时延差异。
    ///
    /// Argon2 匹配返回 `Current`，MD5 匹配返回待迁移的 `Legacy`，其余返回 `Mismatch`。
    /// 此方法会同步消耗 CPU；异步调用方必须使用有界阻塞任务。
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
                },
            };
        }

        if secret.is_legacy_password_hash() {
            let matches = verify_legacy_md5(password, &secret.password_hash);
            perform_dummy_argon2(password);
            return if matches { PasswordVerification::Legacy } else { PasswordVerification::Mismatch };
        }

        perform_dummy_argon2(password);
        PasswordVerification::Mismatch
    }

    /// 按 32 个 ASCII 十六进制字符识别旧 MD5 摘要格式，供验密选择兼容路径。
    pub fn is_legacy_password_hash(&self) -> bool {
        self.password_hash.len() == LEGACY_MD5_LEN
            && self.password_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

// 创建或换密时以随机盐生成 Argon2id PHC 文本，空密码和生成失败均返回业务错误。
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

// 按 PHC 参数验密并返回是否匹配；解析失败、缺少盐或摘要、以及其他验证错误均返回 `None`。
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

// 用固定盐执行一次默认参数的 Argon2 占位计算并丢弃结果；固定盐无法编码成 Base64 时 panic。
fn perform_dummy_argon2(password: &str) {
    let salt = SaltString::encode_b64(DUMMY_ARGON2_SALT).expect("dummy Argon2 salt constant must be valid");
    let _ = Argon2::default().hash_password(password.as_bytes(), &salt);
}

// 仅在旧凭证验密时计算输入的 MD5 小写十六进制摘要，并逐字节核对持久化值。
fn verify_legacy_md5(password: &str, encoded_hash: &str) -> bool {
    let candidate = format!("{:x}", md5::compute(password.as_bytes()));
    constant_time_eq(candidate.as_bytes(), encoded_hash.as_bytes())
}

// 长度不同直接判定不匹配；等长时逐字节累积差异，遍历完成后判定。
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter().zip(right).fold(0_u8, |difference, (left, right)| difference | (left ^ right)) == 0
}

#[cfg(test)]
mod tests {
    use super::{LoginAccount, PasswordVerification, Secret};

    // 验证登录名规范化裁剪两端空白并保留大小写。
    #[test]
    fn login_account_should_trim_without_changing_case() {
        let account = LoginAccount::new("  Admin01  ").unwrap();
        assert_eq!(account.as_str(), "Admin01");
    }

    // 验证通用登录名规则拒绝全空白及 65 字符输入。
    #[test]
    fn login_account_should_reject_out_of_range_values() {
        assert!(LoginAccount::new("  ").is_err());
        assert!(LoginAccount::new("a".repeat(65)).is_err());
    }

    // 验证登录名先裁剪再校验，并接受 3 和 64 字符边界。
    #[test]
    fn account_constructor_keeps_normalization_and_length_boundaries() {
        assert_eq!(LoginAccount::new("  Admin01  ").unwrap().as_str(), "Admin01");
        for value in ["abc".to_string(), "a".repeat(64)] {
            assert_eq!(LoginAccount::new(&value).unwrap().as_str(), value);
        }
        for value in [String::new(), "  ab  ".to_string(), "a".repeat(65)] {
            assert!(LoginAccount::new(value).is_err());
        }
    }

    // 验证新凭证采用 Argon2id，正确密码匹配当前格式，错误密码验证失败。
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

    // 验证密码保留首尾空白，省略空白后的输入无法匹配原凭证。
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

    // 验证旧 MD5 凭证仅在密码正确时返回待迁移标记。
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

    // 验证换密将旧 MD5 哈希替换为可按当前格式验证的凭证。
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

    // 验证缺少凭证和损坏的 Argon2 哈希均返回不匹配。
    #[test]
    fn missing_and_malformed_secret_should_fail_closed() {
        let malformed =
            Secret { account: "broken01".to_string(), password_hash: "$argon2id$malformed".to_string() };

        assert_eq!(Secret::verify_password_or_dummy(None, "password123"), PasswordVerification::Mismatch);
        assert_eq!(
            Secret::verify_password_or_dummy(Some(&malformed), "password123"),
            PasswordVerification::Mismatch
        );
    }

    // 验证调试文本使用脱敏占位值，不包含 Argon2 哈希。
    #[test]
    fn debug_output_should_redact_password_hash() {
        let secret = Secret::new(LoginAccount::new("admin01").unwrap(), "password123").unwrap();
        let output = format!("{secret:?}");

        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("$argon2"));
    }
}
