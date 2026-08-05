use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::{
    errors::{Error, Result},
    FieldUpdate, LoginAccount, Secret,
};

const ACCOUNT_MIN_LEN: usize = 4;
const ACCOUNT_MAX_LEN: usize = 64;
const PASSWORD_MIN_LEN: usize = 6;
const PASSWORD_MAX_LEN: usize = 64;
const NICKNAME_MIN_LEN: usize = 1;
const NICKNAME_MAX_LEN: usize = 32;

/// 消费者账号更新数据。
///
/// 账号在进入实体前已完成通用规范化；消费者专属长度规则、密码规则和昵称
/// 规范化仍由 [`Consumer::update`] 统一保证。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumerUpdate {
    /// 待替换的规范化登录账号；`None` 表示保持不变。
    pub account: Option<LoginAccount>,
    /// 待替换的明文密码；`None` 表示保持不变。
    pub password: Option<String>,
    /// 昵称更新意图；支持保持、清除与替换。
    pub nickname: FieldUpdate<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, Entity, PartialEq, Eq)]
#[serde(default)]
pub struct Consumer {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub secret: Secret,
    pub nickname: Option<String>,
    pub is_active: bool,
}

impl Consumer {
    /// 创建 Consumer 实例。
    ///
    /// # 参数
    /// * `id` - 标识符
    /// * `account` - 已完成通用规范化的登录账号
    /// * `password` - 明文密码
    /// * `nickname` - 昵称
    ///
    /// # 返回
    /// 返回创建的实例。
    ///
    /// # 错误
    /// 当账号、密码或昵称不符合消费者账号规则时返回错误。
    pub fn new(
        id: String,
        account: LoginAccount,
        password: impl AsRef<str>,
        nickname: Option<String>,
    ) -> Result<Self> {
        ensure_account(&account)?;
        ensure_password(password.as_ref())?;
        let nickname = nickname.map(normalize_nickname).transpose()?;
        let secret = Secret::new(account, password)?;

        Ok(Self {
            base: BaseModel::new(id),
            secret,
            nickname,
            is_active: true,
        })
    }

    /// 更新消费者账号。
    ///
    /// 所有字段会先完成校验和密码处理，再一次性替换实体状态；失败时实体保持
    /// 原样。
    ///
    /// # 参数
    /// * `update` - 消费者账号更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当账号、密码或昵称不符合消费者账号规则时返回错误。
    pub fn update(&mut self, update: ConsumerUpdate) -> Result<()> {
        let ConsumerUpdate {
            account,
            password,
            nickname,
        } = update;
        if let Some(account) = account.as_ref() {
            ensure_account(account)?;
        }
        if let Some(password) = password.as_deref() {
            ensure_password(password)?;
        }
        let nickname = match nickname {
            FieldUpdate::Set(nickname) => FieldUpdate::Set(normalize_nickname(nickname)?),
            update => update,
        };

        let mut secret = self.secret.clone();
        if let Some(account) = account {
            secret.change_account(account);
        }
        if let Some(password) = password {
            secret.change_password(password)?;
        }

        self.secret = secret;
        nickname.apply_to(&mut self.nickname);
        Ok(())
    }

    /// 校验昵称是否满足消费者 DTO 与领域模型共享的规则。
    ///
    /// 校验基于去除首尾空白后的 Unicode 字符数。
    ///
    /// # 参数
    /// * `nickname` - 待校验昵称
    ///
    /// # 返回
    /// 符合规则返回 `Ok(())`。
    ///
    /// # 错误
    /// 当昵称去除首尾空白后不在 1–32 个字符之间时返回错误。
    pub fn validate_nickname(nickname: &str) -> Result<()> {
        let length = nickname.trim().chars().count();
        if !(NICKNAME_MIN_LEN..=NICKNAME_MAX_LEN).contains(&length) {
            return Err(Error::from("昵称长度必须在1-32个字符之间"));
        }

        Ok(())
    }
}

/// 校验消费者登录账号的专属长度规则。
fn ensure_account(account: &LoginAccount) -> Result<()> {
    let length = account.as_str().chars().count();
    if !(ACCOUNT_MIN_LEN..=ACCOUNT_MAX_LEN).contains(&length) {
        return Err(Error::from("账号长度必须在4-64个字符之间"));
    }

    Ok(())
}

/// 校验消费者密码长度，不改变具有业务意义的首尾空白。
fn ensure_password(password: &str) -> Result<()> {
    let length = password.chars().count();
    if !(PASSWORD_MIN_LEN..=PASSWORD_MAX_LEN).contains(&length) {
        return Err(Error::from("密码长度必须在6-64个字符之间"));
    }

    Ok(())
}

/// 规范化已通过规则校验的消费者昵称。
fn normalize_nickname(nickname: String) -> Result<String> {
    Consumer::validate_nickname(nickname.as_str())?;
    Ok(nickname.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::{Consumer, ConsumerUpdate};
    use crate::{FieldUpdate, LoginAccount, PasswordVerification, Secret};

    fn consumer() -> Consumer {
        Consumer::new(
            "consumer-1".to_string(),
            LoginAccount::new("consumer01").unwrap(),
            "password123",
            Some("测试用户".to_string()),
        )
        .unwrap()
    }

    #[test]
    fn new_should_normalize_account_and_nickname() {
        let consumer = Consumer::new(
            "consumer-1".to_string(),
            LoginAccount::new("  Consumer01  ").unwrap(),
            "password123",
            Some("  测试用户  ".to_string()),
        )
        .unwrap();

        assert_eq!(consumer.secret.account(), "Consumer01");
        assert_eq!(consumer.nickname.as_deref(), Some("测试用户"));
        assert!(consumer.is_active);
    }

    #[test]
    fn new_should_reject_nickname_longer_than_32_unicode_characters() {
        let result = Consumer::new(
            "consumer-1".to_string(),
            LoginAccount::new("consumer01").unwrap(),
            "password123",
            Some("名".repeat(33)),
        );

        assert!(result.is_err());
    }

    #[test]
    fn update_should_normalize_and_apply_all_fields() {
        let mut consumer = consumer();

        consumer
            .update(ConsumerUpdate {
                account: Some(LoginAccount::new("  next_consumer  ").unwrap()),
                password: Some("next-password".to_string()),
                nickname: FieldUpdate::Set("  新昵称  ".to_string()),
            })
            .unwrap();

        assert_eq!(consumer.secret.account(), "next_consumer");
        assert_eq!(
            Secret::verify_password_or_dummy(Some(&consumer.secret), "next-password"),
            PasswordVerification::Current
        );
        assert_eq!(consumer.nickname.as_deref(), Some("新昵称"));
    }

    #[test]
    fn update_should_keep_entity_unchanged_when_password_is_invalid() {
        let mut consumer = consumer();
        let original = consumer.clone();

        let result = consumer.update(ConsumerUpdate {
            account: Some(LoginAccount::new("next_consumer").unwrap()),
            password: Some("short".to_string()),
            nickname: FieldUpdate::Set("新昵称".to_string()),
        });

        assert!(result.is_err());
        assert_eq!(consumer, original);
    }

    #[test]
    fn update_should_clear_nickname() {
        let mut consumer = consumer();

        consumer
            .update(ConsumerUpdate {
                account: None,
                password: None,
                nickname: FieldUpdate::Clear,
            })
            .unwrap();

        assert_eq!(consumer.nickname, None);
    }
}
