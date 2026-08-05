use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::validation::{
    normalize_optional_email, normalize_optional_email_update, normalize_optional_phone,
    normalize_optional_phone_update, normalize_optional_text, normalize_required_text,
};
use crate::{FieldUpdate, LoginAccount, Secret};

/// 账号名称最大长度。
const NAME_MAX_LEN: usize = 64;
/// 账号最大长度。
const ACCOUNT_MAX_LEN: usize = 32;
/// 邮箱最大长度。
const EMAIL_MAX_LEN: usize = 128;
/// 电话最大长度。
const PHONE_MAX_LEN: usize = 32;
/// 头像地址最大长度。
const AVATAR_MAX_LEN: usize = 512;

/// 后台账号种类。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    /// 系统管理员账号。
    Admin,
}

impl AccountKind {
    /// 返回账号类型字符串表示。
    ///
    /// # 返回值
    /// 返回稳定的账号类型字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
        }
    }

    /// 从字符串解析账号类型。
    ///
    /// # 参数
    /// * `value` - 账号类型字符串
    ///
    /// # 返回值
    /// 成功返回账号类型，失败返回空错误。
    #[allow(clippy::result_unit_err)]
    pub fn parse(value: &str) -> std::result::Result<Self, ()> {
        match value {
            "admin" => Ok(Self::Admin),
            _ => Err(()),
        }
    }
}

impl TryFrom<&str> for AccountKind {
    type Error = ();

    /// 从字符串解析账号类型。
    ///
    /// # 参数
    /// * `value` - 账号类型字符串
    ///
    /// # 返回值
    /// 成功返回账号类型，失败返回空错误。
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        Self::parse(value)
    }
}

/// 统一账号状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    /// 正常可用。
    #[default]
    Active,
    /// 冻结状态。
    Suspended,
    /// 归档状态。
    Archived,
}

impl AccountStatus {
    /// 判断状态是否可登录。
    ///
    /// # 返回值
    /// 处于 `Active` 时返回 true。
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// 返回状态在数据库中的字符串值。
    ///
    /// # 返回值
    /// 返回用于持久化的状态值。
    pub fn as_db_value(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Archived => "archived",
        }
    }
}

/// 统一账号创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountCoreData {
    pub secret: Secret,
    pub name: String,
    pub kind: AccountKind,
    pub status: AccountStatus,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub avatar: Option<String>,
}

/// 统一账号更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AccountCoreUpdate {
    pub name: Option<String>,
    pub account: Option<String>,
    pub password: Option<String>,
    pub status: Option<AccountStatus>,
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub email: FieldUpdate<String>,
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub phone: FieldUpdate<String>,
    pub avatar: Option<String>,
}

/// 统一账号实体。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct AccountCore {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten, default)]
    pub secret: Secret,
    pub name: String,
    pub kind: AccountKind,
    #[serde(default)]
    pub status: AccountStatus,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
}

impl AccountCore {
    /// 创建统一账号。
    ///
    /// # 参数
    /// * `id` - 账号ID
    /// * `data` - 账号创建数据
    ///
    /// # 返回值
    /// 返回新建的账号实体。
    ///
    /// # 错误
    /// 当账号数据校验失败时返回错误。
    pub fn new(id: String, data: AccountCoreData) -> Result<Self> {
        let mut secret = data.secret;
        let name = normalize_required_text(data.name, "账号名称不能为空", NAME_MAX_LEN, "账号名称过长")?;
        let account = backoffice_login_account(secret.account())?;
        secret.change_account(account);
        let email = normalize_optional_email(data.email, EMAIL_MAX_LEN)?;
        let phone = normalize_optional_phone(data.phone, PHONE_MAX_LEN)?;
        let avatar = normalize_optional_text(data.avatar, "头像", AVATAR_MAX_LEN)?;

        Ok(Self {
            base: BaseModel::new(id),
            secret,
            name,
            kind: data.kind,
            status: data.status,
            email,
            phone,
            avatar,
        })
    }

    /// 更新统一账号。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回值
    /// 更新成功返回 Ok。
    ///
    /// # 错误
    /// 当更新数据校验失败时返回错误。
    pub fn update(&mut self, update: AccountCoreUpdate) -> Result<()> {
        self.apply_name(update.name)?;
        self.apply_account(update.account)?;
        self.apply_password(update.password)?;
        self.apply_status(update.status);
        self.apply_email(update.email)?;
        self.apply_phone(update.phone)?;
        self.apply_avatar(update.avatar)?;

        Ok(())
    }

    /// 判断账号是否可登录。
    ///
    /// # 返回值
    /// 返回账号状态是否允许登录。
    pub fn can_login(&self) -> bool {
        self.status.is_active()
    }

    /// 判断账号类型是否匹配期望值。
    ///
    /// # 参数
    /// * `expected` - 期望账号类型
    ///
    /// # 返回值
    /// 类型匹配时返回 `true`。
    pub fn is_kind(&self, expected: AccountKind) -> bool {
        self.kind == expected
    }

    /// 应用名称更新。
    ///
    /// # 参数
    /// * `name` - 可选名称
    ///
    /// # 错误
    /// 当名称不合法时返回错误。
    fn apply_name(&mut self, name: Option<String>) -> Result<()> {
        if let Some(name) = name {
            self.name = normalize_required_text(name, "账号名称不能为空", NAME_MAX_LEN, "账号名称过长")?;
        }

        Ok(())
    }

    /// 应用账号更新。
    ///
    /// # 参数
    /// * `account` - 可选账号
    ///
    /// # 错误
    /// 当账号不合法时返回错误。
    fn apply_account(&mut self, account: Option<String>) -> Result<()> {
        if let Some(account) = account {
            self.secret.change_account(backoffice_login_account(account)?);
        }

        Ok(())
    }

    /// 应用密码更新。
    ///
    /// # 参数
    /// * `password` - 可选密码
    ///
    /// # 错误
    /// 当密码不符合策略时返回错误。
    fn apply_password(&mut self, password: Option<String>) -> Result<()> {
        if let Some(password) = password {
            self.secret.change_password(password)?;
        }

        Ok(())
    }

    /// 应用状态更新。
    ///
    /// # 参数
    /// * `status` - 可选状态
    fn apply_status(&mut self, status: Option<AccountStatus>) {
        if let Some(status) = status {
            self.status = status;
        }
    }

    /// 应用邮箱更新。
    ///
    /// # 参数
    /// * `email` - 可选邮箱
    ///
    /// # 错误
    /// 当邮箱不合法时返回错误。
    fn apply_email(&mut self, email: FieldUpdate<String>) -> Result<()> {
        let email = normalize_optional_email_update(email, EMAIL_MAX_LEN)?;
        email.apply_to(&mut self.email);
        Ok(())
    }

    /// 应用手机号更新。
    ///
    /// # 参数
    /// * `phone` - 可选手机号
    ///
    /// # 错误
    /// 当手机号不合法时返回错误。
    fn apply_phone(&mut self, phone: FieldUpdate<String>) -> Result<()> {
        let phone = normalize_optional_phone_update(phone, PHONE_MAX_LEN)?;
        phone.apply_to(&mut self.phone);
        Ok(())
    }

    /// 应用头像更新。
    ///
    /// # 参数
    /// * `avatar` - 可选头像地址
    ///
    /// # 错误
    /// 当头像地址不合法时返回错误。
    fn apply_avatar(&mut self, avatar: Option<String>) -> Result<()> {
        if let Some(avatar) = avatar {
            self.avatar = normalize_optional_text(Some(avatar), "头像", AVATAR_MAX_LEN)?;
        }

        Ok(())
    }
}

/// 构造符合后台账号长度约束的规范化登录账号。
fn backoffice_login_account(account: impl Into<String>) -> Result<LoginAccount> {
    let account = LoginAccount::new(account)?;
    if account.as_str().chars().count() > ACCOUNT_MAX_LEN {
        return Err(Error::from("账号长度不符合要求"));
    }

    Ok(account)
}

#[cfg(test)]
mod tests {
    use super::{AccountCore, AccountCoreData, AccountCoreUpdate, AccountKind, AccountStatus};
    use crate::{FieldUpdate, LoginAccount, Secret};

    fn sample_account(kind: AccountKind, status: AccountStatus) -> AccountCore {
        AccountCore::new(
            "account-1".to_string(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("sample").unwrap(), "password123").unwrap(),
                name: "Sample".to_string(),
                kind,
                status,
                email: None,
                phone: None,
                avatar: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn is_kind_should_match_only_expected_kind() {
        let account = sample_account(AccountKind::Admin, AccountStatus::Active);

        assert!(account.is_kind(AccountKind::Admin));
    }

    #[test]
    fn new_should_reject_account_longer_than_backoffice_limit() {
        let secret = Secret::new(LoginAccount::new("a".repeat(33)).unwrap(), "password123").unwrap();
        let result = AccountCore::new(
            "account-1".to_string(),
            AccountCoreData {
                secret,
                name: "Sample".to_string(),
                kind: AccountKind::Admin,
                status: AccountStatus::Active,
                email: None,
                phone: None,
                avatar: None,
            },
        );

        assert!(result.is_err());
    }

    #[test]
    fn update_should_distinguish_unchanged_clear_and_set_optional_contacts() {
        let mut account = sample_account(AccountKind::Admin, AccountStatus::Active);
        account.email = Some("old@example.com".to_string());
        account.phone = Some("13900000000".to_string());

        account
            .update(AccountCoreUpdate {
                email: FieldUpdate::Unchanged,
                phone: FieldUpdate::Clear,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(account.email.as_deref(), Some("old@example.com"));
        assert_eq!(account.phone, None);

        account
            .update(AccountCoreUpdate {
                email: FieldUpdate::Set(" next@example.com ".to_string()),
                phone: FieldUpdate::Set("13800000000".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(account.email.as_deref(), Some("next@example.com"));
        assert_eq!(account.phone.as_deref(), Some("13800000000"));
    }
}
