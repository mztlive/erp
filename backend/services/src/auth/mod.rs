use database::{AccessControlExt, NoTransaction};
use entities::{AccountCore, AccountKind as DomainAccountKind, LoginAccount, Secret};
use mongodb::Database;
use validator::Validate;

use crate::errors::{Error, Result};

mod dto;
pub(crate) mod password;

/// 后台账号认证成功后的最小身份信息。
///
/// 该结果不携带密码哈希，避免凭证离开认证服务边界。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackofficeAuthResult {
    account_id: String,
    account: String,
    account_kind: DomainAccountKind,
    account_version: u64,
}

impl BackofficeAuthResult {
    /// 返回账号 ID。
    ///
    /// # 返回值
    /// 返回认证成功的账号 ID。
    pub fn account_id(&self) -> &str {
        self.account_id.as_str()
    }

    /// 返回规范化后的登录账号。
    ///
    /// # 返回值
    /// 返回认证成功的登录账号。
    pub fn account(&self) -> &str {
        self.account.as_str()
    }

    /// 返回后台账号类型。
    ///
    /// # 返回值
    /// 返回认证成功的账号类型。
    pub fn account_kind(&self) -> DomainAccountKind {
        self.account_kind
    }

    /// 返回签发身份时的账号持久化版本。
    ///
    /// # 返回值
    /// 返回用于撤销旧 token 的账号版本。
    pub fn account_version(&self) -> u64 {
        self.account_version
    }
}

impl From<&AccountCore> for BackofficeAuthResult {
    fn from(account: &AccountCore) -> Self {
        Self {
            account_id: account.base.id.clone(),
            account: account.secret.account().to_string(),
            account_kind: account.kind,
            account_version: account.base.version,
        }
    }
}

/// 后台账号密码认证服务。
///
/// 该服务统一负责账号规范化、账号类型与状态判断、密码验证，以及旧 MD5
/// 摘要在认证成功后的 Argon2id 迁移。
pub struct BackofficeAuthService {
    db: Database,
}

impl BackofficeAuthService {
    /// 创建后台账号认证服务。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回值
    /// 返回认证服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 使用账号、密码及账号类型完成后台认证。
    ///
    /// 账号不存在、类型不符、状态不可登录和密码错误均返回相同凭证错误，
    /// 避免对外泄露账号状态。旧 MD5 摘要只有在验证成功并完成持久化升级后
    /// 才会返回认证成功；升级失败时不会签发登录身份。
    ///
    /// # 参数
    /// * `request` - 账号、密码和后台账号类型
    ///
    /// # 返回值
    /// 返回不含密码哈希的后台身份信息。
    ///
    /// # 错误
    /// 当凭证无效、账号不可登录、数据库访问或旧哈希升级失败时返回错误。
    pub async fn authenticate(&self, request: &dto::AuthRequest) -> Result<BackofficeAuthResult> {
        request.validate()?;
        let mut stored_account = self.find_account(&request.account).await?;
        let secret = secret_for_authentication(stored_account.as_ref(), request.account_kind);
        let is_authenticatable = secret.is_some();
        let password_check = password::verify_password(secret, request.password.clone()).await?;
        if !is_authenticatable || !password_check.is_match() {
            return Err(invalid_credentials());
        }

        let Some(mut stored_account) = stored_account.take() else {
            return Err(invalid_credentials());
        };
        if let Some(secret) = password_check.into_upgraded_secret() {
            stored_account.secret = secret;
            self.db
                .accounts()
                .update(&mut stored_account, &mut NoTransaction)
                .await?;
        }
        Ok(BackofficeAuthResult::from(&stored_account))
    }

    /// 校验 JWT 对应的后台账号当前仍然有效。
    ///
    /// 每次受保护请求都从持久化层读取账号，确保软删除、停用、账号改名或
    /// 类型变化能够立即撤销既有 token，而不是等到 token 自然过期。
    ///
    /// # 参数
    /// * `account_id` - token 中的账号 ID
    /// * `account` - token 中的登录账号
    /// * `account_kind` - token 中的后台账号类型
    /// * `account_version` - token 签发时的账号版本
    ///
    /// # 返回值
    /// 当前身份仍可使用时返回最小后台身份信息。
    ///
    /// # 错误
    /// 账号不存在或身份状态不匹配时返回统一认证错误；数据库失败时返回仓储错误。
    pub async fn validate_session(
        &self,
        account_id: &str,
        account: &str,
        account_kind: DomainAccountKind,
        account_version: u64,
    ) -> Result<BackofficeAuthResult> {
        let stored_account = self
            .db
            .accounts()
            .find_by_id(account_id, &mut NoTransaction)
            .await?;
        let Some(stored_account) =
            valid_session_account(stored_account.as_ref(), account, account_kind, account_version)
        else {
            return Err(invalid_session());
        };

        Ok(BackofficeAuthResult::from(stored_account))
    }

    /// 规范化账号并查询记录；无效账号按不存在处理，由后续统一执行 dummy work。
    async fn find_account(&self, account: &str) -> Result<Option<AccountCore>> {
        let Ok(account) = LoginAccount::new(account) else {
            return Ok(None);
        };

        Ok(self
            .db
            .accounts()
            .find_by_account(account.as_str(), &mut NoTransaction)
            .await?)
    }
}

/// 仅为类型与状态均可登录的后台账号复制凭证，其余场景进入 dummy 校验。
fn secret_for_authentication(
    account: Option<&AccountCore>,
    expected_kind: DomainAccountKind,
) -> Option<Secret> {
    account
        .filter(|account| account.kind == expected_kind && account.can_login())
        .map(|account| account.secret.clone())
}

/// 返回与 token 身份完全一致且当前可用的后台账号。
fn valid_session_account<'a>(
    stored_account: Option<&'a AccountCore>,
    token_account: &str,
    expected_kind: DomainAccountKind,
    expected_version: u64,
) -> Option<&'a AccountCore> {
    stored_account.filter(|account| {
        account.kind == expected_kind
            && account.can_login()
            && account.secret.account() == token_account
            && account.base.version == expected_version
    })
}

/// 构造不泄露具体失败原因的凭证错误。
fn invalid_credentials() -> Error {
    Error::Unauthenticated("用户名或密码错误".to_string())
}

/// 构造不泄露账号状态的会话失效错误。
fn invalid_session() -> Error {
    Error::Unauthenticated("认证已失效".to_string())
}

pub use dto::{AuthRequest, AuthResponse, PasswordLoginPayload};

#[cfg(test)]
mod tests {
    use super::{
        password::verify_password, password::PasswordCheck, secret_for_authentication, valid_session_account,
        BackofficeAuthResult,
    };
    use entities::{AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Secret};

    fn account(kind: AccountKind, status: AccountStatus) -> AccountCore {
        AccountCore::new(
            "account-1".to_string(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("admin01").unwrap(), "password123").unwrap(),
                name: "测试账号".to_string(),
                kind,
                status,
                email: None,
                phone: None,
                avatar: None,
            },
        )
        .unwrap()
    }

    #[tokio::test]
    async fn authentication_rules_should_accept_matching_active_account() {
        let account = account(AccountKind::Admin, AccountStatus::Active);
        let secret = secret_for_authentication(Some(&account), AccountKind::Admin);
        let result = verify_password(secret, "password123".to_string()).await.unwrap();

        assert!(matches!(result, PasswordCheck::Current));
    }

    #[tokio::test]
    async fn inactive_account_should_still_use_password_boundary() {
        let suspended = account(AccountKind::Admin, AccountStatus::Suspended);

        let secret = secret_for_authentication(Some(&suspended), AccountKind::Admin);
        let result = verify_password(secret, "password123".to_string()).await.unwrap();
        assert!(matches!(result, PasswordCheck::Mismatch));
    }

    #[test]
    fn auth_result_should_not_expose_password_hash() {
        let account = account(AccountKind::Admin, AccountStatus::Active);
        let result = BackofficeAuthResult::from(&account);
        let debug = format!("{result:?}");

        assert_eq!(result.account_id(), "account-1");
        assert_eq!(result.account(), "admin01");
        assert_eq!(result.account_kind(), AccountKind::Admin);
        assert_eq!(result.account_version(), 1);
        assert!(!debug.contains("$argon2"));
    }

    #[test]
    fn session_rules_should_accept_only_current_active_identity() {
        let active = account(AccountKind::Admin, AccountStatus::Active);

        assert!(valid_session_account(
            Some(&active),
            active.secret.account(),
            AccountKind::Admin,
            active.base.version,
        )
        .is_some());
        assert!(valid_session_account(
            Some(&active),
            "renamed-account",
            AccountKind::Admin,
            active.base.version,
        )
        .is_none());
        assert!(valid_session_account(
            Some(&active),
            active.secret.account(),
            AccountKind::Admin,
            active.base.version + 1,
        )
        .is_none());
    }

    #[test]
    fn session_rules_should_reject_missing_or_inactive_accounts() {
        let suspended = account(AccountKind::Admin, AccountStatus::Suspended);
        let archived = account(AccountKind::Admin, AccountStatus::Archived);

        assert!(valid_session_account(None, "admin01", AccountKind::Admin, 1).is_none());
        assert!(valid_session_account(
            Some(&suspended),
            suspended.secret.account(),
            AccountKind::Admin,
            suspended.base.version,
        )
        .is_none());
        assert!(valid_session_account(
            Some(&archived),
            archived.secret.account(),
            AccountKind::Admin,
            archived.base.version,
        )
        .is_none());
    }
}
