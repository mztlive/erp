use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::validation::normalize_required_text;
use erp_core::{AccountKind, Error, Result};
use serde::{Deserialize, Serialize};

use crate::entity::auth::{LoginAccount, Secret};

/// 账号名称最大长度。
const NAME_MAX_LEN: usize = 64;
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

/// 已存在超级管理员的账号状态（初始化路径的纯状态分类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExistingSuperAdminState {
    Active,
    Inactive,
    Deleted,
}

impl ExistingSuperAdminState {
    /// 超管初始化时先按软删除标记识别待恢复账号，其余按登录状态区分启用和停用。
    pub(crate) fn from_state(is_deleted: bool, is_active: bool) -> Self {
        if is_deleted {
            return Self::Deleted;
        }
        if is_active { Self::Active } else { Self::Inactive }
    }

    /// 初始化结果用此标记记录原先停用或已软删除的超管账号是否被恢复。
    pub(crate) fn reactivates(self) -> bool {
        !matches!(self, Self::Active)
    }
}

impl AccountStatus {
    /// 判断账号状态是否允许登录，只有启用状态通过。
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 新建统一账号时传入实体的凭证、显示名称、身份类型及初始状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountCoreData {
    /// 已处理的登录凭证，内含登录名和密码哈希。
    pub secret: Secret,
    /// 待实体裁剪首尾空白并校验的显示名称。
    pub name: String,
    /// 新账号所属的身份域。
    pub kind: AccountKind,
    /// 新账号的初始登录状态。
    pub status: AccountStatus,
}

impl AccountCoreData {
    /// 用已处理的凭证准备启用状态的后台管理员账号，显示名称留给实体校验。
    pub fn admin(secret: Secret, name: String) -> Self {
        Self { secret, name, kind: AccountKind::Admin, status: AccountStatus::Active }
    }

    /// 用已处理的凭证准备启用状态的供应商账号，显示名称留给实体校验。
    pub fn supplier(secret: Secret, name: String) -> Self {
        Self { secret, name, kind: AccountKind::Supplier, status: AccountStatus::Active }
    }
}

/// 统一账号的局部更新，未提供的字段沿用原值。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AccountCoreUpdate {
    /// 待规范化的新显示名称。
    pub name: Option<String>,
    /// 用于生成替换哈希的新明文密码。
    pub password: Option<String>,
    /// 决定后续登录资格的目标账号状态。
    pub status: Option<AccountStatus>,
}

/// 后台与供应商共用的持久化账号，承载登录凭证、身份类型、状态和个人资料。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct AccountCore {
    /// 持久化元数据，含稳定账号 ID、版本和软删除标记。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 展开为账号记录顶层字段的登录名和密码哈希。
    #[serde(flatten, default)]
    pub secret: Secret,
    /// 用于展示的账号名称。
    pub name: String,
    /// 用于区分后台和供应商认证入口的身份类型。
    pub kind: AccountKind,
    /// 登录状态，旧记录缺省时视为启用。
    #[serde(default)]
    pub status: AccountStatus,
    /// 账号的可选联系邮箱。
    #[serde(default)]
    pub email: Option<String>,
    /// 账号的可选联系电话。
    #[serde(default)]
    pub phone: Option<String>,
    /// 账号的可选头像地址。
    #[serde(default)]
    pub avatar: Option<String>,
}

impl AccountCore {
    /// 创建账号时裁剪显示名称并校验非空及 64 字符上限，登录名额外遵守后台 32 字符上限。
    /// 任一校验失败时返回错误；新账号的联系方式和头像为空。
    pub fn new(id: String, data: AccountCoreData) -> Result<Self> {
        let mut secret = data.secret;
        let name = normalize_required_text(data.name, "账号名称不能为空", NAME_MAX_LEN, "账号名称过长")?;
        let account = backoffice_login_account(secret.account())?;
        secret.change_account(account);

        Ok(Self {
            base: BaseModel::new(id),
            secret,
            name,
            kind: data.kind,
            status: data.status,
            email: None,
            phone: None,
            avatar: None,
        })
    }

    /// 就地依次更新给定的名称、密码和状态，省略的字段沿用原值。
    /// 名称校验或密码处理失败时中止；密码处理失败前名称可能已更新。
    pub fn update(&mut self, update: AccountCoreUpdate) -> Result<()> {
        self.apply_name(update.name)?;
        self.apply_password(update.password)?;
        self.apply_status(update.status);
        Ok(())
    }

    /// 仅按启用状态判断登录资格；软删除由调用方检查。
    pub fn can_login(&self) -> bool {
        self.status.is_active()
    }

    /// 仅在账号启用且类型符合认证入口要求时借出凭证，否则返回 `None`。
    /// 密码验证、验证成本平衡和对外错误脱敏由认证服务负责。
    pub fn authentication_secret_for(&self, expected_kind: AccountKind) -> Option<&Secret> {
        (self.kind == expected_kind && self.can_login()).then_some(&self.secret)
    }

    /// 复验会话签发时的登录名、账号类型和版本，并要求当前状态仍允许登录；软删除由调用方检查。
    pub fn matches_session_identity(&self, account: &str, kind: AccountKind, version: u64) -> bool {
        self.authentication_secret_for(kind)
            .is_some_and(|secret| secret.account() == account && self.base.version == version)
    }

    /// 业务操作要求可登录身份时检查账号状态，冻结或归档返回业务错误。
    pub fn ensure_can_login(&self) -> Result<()> {
        if self.can_login() {
            return Ok(());
        }
        Err(Error::from("账号已停用，不能参与需要可登录身份的业务操作"))
    }

    /// 判断持久化账号类型是否符合调用方要求的身份域。
    pub fn is_kind(&self, expected: AccountKind) -> bool {
        self.kind == expected
    }

    /// 筛选审批责任人等后台职责的候选账号，同时要求后台管理员类型和启用状态。
    pub fn is_active_backoffice(&self) -> bool {
        self.is_kind(AccountKind::Admin) && self.can_login()
    }

    /// 授权前确认操作人类型与持久化账号一致，且该后台账号启用、未软删除。
    pub fn is_authorization_actor(&self, kind: AccountKind) -> bool {
        !self.base.is_deleted() && self.is_active_backoffice() && self.kind == kind
    }

    // 提供新名称时先裁剪并校验非空及长度，再替换显示名称。
    fn apply_name(&mut self, name: Option<String>) -> Result<()> {
        if let Some(name) = name {
            self.name = normalize_required_text(name, "账号名称不能为空", NAME_MAX_LEN, "账号名称过长")?;
        }
        Ok(())
    }

    // 提供新密码时先计算哈希再替换凭证；空密码或哈希失败保留原密码哈希。
    fn apply_password(&mut self, password: Option<String>) -> Result<()> {
        if let Some(password) = password {
            self.secret.change_password(password)?;
        }
        Ok(())
    }

    // 直接应用给定的账号状态；省略时保留原登录资格。
    fn apply_status(&mut self, status: Option<AccountStatus>) {
        if let Some(status) = status {
            self.status = status;
        }
    }
}

// 在通用登录名规范化后检查后台 32 字符上限，供统一账号创建复用。
fn backoffice_login_account(account: impl Into<String>) -> Result<LoginAccount> {
    let account = LoginAccount::new(account)?;
    if account.as_str().chars().count() > LoginAccount::BACKOFFICE_MAX_LEN {
        return Err(Error::from("账号长度不符合要求"));
    }
    Ok(account)
}

#[cfg(test)]
mod tests {

    use super::{AccountCore, AccountCoreData, AccountCoreUpdate, AccountKind, AccountStatus};
    use crate::entity::auth::{LoginAccount, Secret};

    // 用固定登录凭证搭配指定类型和状态，构造账号规则测试数据。
    fn sample_account(kind: AccountKind, status: AccountStatus) -> AccountCore {
        AccountCore::new(
            "account-1".to_string(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("sample").unwrap(), "password123").unwrap(),
                name: "Sample".to_string(),
                kind,
                status,
            },
        )
        .unwrap()
    }

    // 验证后台与供应商账号按正确类型启用、裁剪名称，并留空联系方式和头像。
    #[test]
    fn constructors_create_active_accounts_without_contacts() {
        let secret = Secret::new(LoginAccount::new("sample").unwrap(), "password123").unwrap();
        for (data, kind) in [
            (AccountCoreData::admin(secret.clone(), " Admin ".into()), AccountKind::Admin),
            (AccountCoreData::supplier(secret, " Supplier ".into()), AccountKind::Supplier),
        ] {
            let account = AccountCore::new("id".into(), data).unwrap();
            assert_eq!(account.kind, kind);
            assert_eq!(account.status, AccountStatus::Active);
            assert_eq!(account.name, account.name.trim());
            assert_eq!((account.email, account.phone, account.avatar), (None, None, None));
        }
    }

    // 验证后台授权拒绝身份类型不符、冻结、归档和软删除的账号。
    #[test]
    fn authorization_actor_rejects_inactive_deleted_and_wrong_kind() {
        let mut account = sample_account(AccountKind::Admin, AccountStatus::Active);
        assert!(account.is_authorization_actor(AccountKind::Admin));
        assert!(!account.is_authorization_actor(AccountKind::Supplier));

        for status in [AccountStatus::Suspended, AccountStatus::Archived] {
            account.status = status;
            assert!(!account.is_authorization_actor(AccountKind::Admin));
        }

        account.status = AccountStatus::Active;
        account.base.deleted_at = 1;
        assert!(!account.is_authorization_actor(AccountKind::Admin));
    }

    // 验证后台账号可匹配管理员身份类型。
    #[test]
    fn is_kind_should_match_only_expected_kind() {
        let account = sample_account(AccountKind::Admin, AccountStatus::Active);
        assert!(account.is_kind(AccountKind::Admin));
    }

    // 验证后台账号只有处于启用状态才能承担后台职责。
    #[test]
    fn active_backoffice_requires_login_eligibility() {
        let active = sample_account(AccountKind::Admin, AccountStatus::Active);
        let suspended = sample_account(AccountKind::Admin, AccountStatus::Suspended);
        let archived = sample_account(AccountKind::Admin, AccountStatus::Archived);

        assert!(active.is_active_backoffice());
        assert!(!suspended.is_active_backoffice());
        assert!(!archived.is_active_backoffice());
    }

    // 验证后台认证能借用启用账号的凭证，冻结账号返回空。
    #[test]
    fn authentication_secret_requires_matching_active_kind() {
        let active = sample_account(AccountKind::Admin, AccountStatus::Active);
        let suspended = sample_account(AccountKind::Admin, AccountStatus::Suspended);

        assert!(active.authentication_secret_for(AccountKind::Admin).is_some());
        assert!(suspended.authentication_secret_for(AccountKind::Admin).is_none());
    }

    // 验证登录名、版本或登录状态变化时，原会话身份复验失败。
    #[test]
    fn session_identity_requires_account_kind_status_and_version() {
        let active = sample_account(AccountKind::Admin, AccountStatus::Active);

        assert!(active.matches_session_identity("sample", AccountKind::Admin, 1));
        assert!(!active.matches_session_identity("renamed", AccountKind::Admin, 1));
        assert!(!active.matches_session_identity("sample", AccountKind::Admin, 2));
        let suspended = sample_account(AccountKind::Admin, AccountStatus::Suspended);
        assert!(!suspended.matches_session_identity("sample", AccountKind::Admin, 1));
    }

    // 验证登录资格检查通过启用账号，并拒绝冻结和归档状态。
    #[test]
    fn login_eligibility_accepts_active_and_rejects_inactive_states() {
        assert!(sample_account(AccountKind::Admin, AccountStatus::Active).ensure_can_login().is_ok());
        assert!(sample_account(AccountKind::Admin, AccountStatus::Suspended).ensure_can_login().is_err());
        assert!(sample_account(AccountKind::Admin, AccountStatus::Archived).ensure_can_login().is_err());
    }

    // 验证通过通用登录名校验的 33 字符账号仍超出后台上限。
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
            },
        );

        assert!(result.is_err());
    }

    // 验证更新名称、密码和状态时保留已有邮箱、电话和头像。
    #[test]
    fn update_preserves_existing_contacts() {
        let mut account = sample_account(AccountKind::Admin, AccountStatus::Active);
        account.email = Some("old@example.com".into());
        account.phone = Some("13900000000".into());
        account.avatar = Some("https://example.com/avatar.png".into());

        account
            .update(AccountCoreUpdate {
                name: Some("New Name".into()),
                password: Some("new-password".into()),
                status: Some(AccountStatus::Suspended),
            })
            .unwrap();

        assert_eq!(account.name, "New Name");
        assert!(!account.can_login());
        assert_eq!(account.email.as_deref(), Some("old@example.com"));
        assert_eq!(account.phone.as_deref(), Some("13900000000"));
        assert_eq!(account.avatar.as_deref(), Some("https://example.com/avatar.png"));
    }

    // 验证冻结会关闭登录资格，重新启用后恢复登录资格。
    #[test]
    fn update_should_suspend_and_reactivate_login_eligibility() {
        let mut account = sample_account(AccountKind::Admin, AccountStatus::Active);

        account
            .update(AccountCoreUpdate { status: Some(AccountStatus::Suspended), ..Default::default() })
            .unwrap();
        assert!(!account.can_login());
        assert!(account.ensure_can_login().is_err());

        account
            .update(AccountCoreUpdate { status: Some(AccountStatus::Active), ..Default::default() })
            .unwrap();
        assert!(account.can_login());
    }
}
