use erp_core::AccountKind as DomainAccountKind;
use mongodb::Database;
use persistence_core::NoTransaction;
use validator::Validate;

use crate::AccessControlExt;
use crate::entity::account_core::AccountCore;
use crate::entity::auth::{LoginAccount, Secret};
use crate::error::{Error, Result};
use crate::repository::prelude::*;

pub(crate) mod password;

/// 后台登录或会话重验通过后交付的身份快照，不携带密码哈希。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackofficeAuthResult {
    // 持久化账号 ID，用于标识当前身份。
    account_id: String,
    // 规范化后的登录名，用于核对会话身份。
    account: String,
    // 认证通过的账号类型，用于区分后台与门户身份。
    account_kind: DomainAccountKind,
    // 认证时读取的账号版本，变更后旧令牌不再有效。
    account_version: u64,
    // 同次认证读取的显示名称。
    name: String,
}

impl BackofficeAuthResult {
    /// 构造会话身份时取得已认证账号的持久化 ID。
    pub fn account_id(&self) -> &str {
        self.account_id.as_str()
    }

    /// 签发或读取会话身份时取得规范化登录名。
    pub fn account(&self) -> &str {
        self.account.as_str()
    }

    /// 组装响应时使用认证快照中的名称，无须再查账号。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 组装后台会话身份时取得认证过的账号类型。
    pub fn account_kind(&self) -> DomainAccountKind {
        self.account_kind
    }

    /// 签发令牌时绑定当前账号版本，供后续请求拒绝旧令牌。
    pub fn account_version(&self) -> u64 {
        self.account_version
    }
}

impl From<&AccountCore> for BackofficeAuthResult {
    // 在登录或会话重验通过后从账号提取身份，不复制密码哈希。
    fn from(account: &AccountCore) -> Self {
        Self {
            account_id: account.base.id.clone(),
            account: account.secret.account().to_string(),
            account_kind: account.kind,
            account_version: account.base.version,
            name: account.name.clone(),
        }
    }
}

/// 处理后台登录和会话重验，统一规范化账号、检查类型与状态，并在验密通过后将旧 MD5 凭证迁移为 Argon2id。
pub struct BackofficeAuthService {
    // 登录和会话重验所读的账号库，也用于持久化升级后的凭证。
    db: Database,
}

impl BackofficeAuthService {
    /// 应用装配时绑定身份数据库，构造时不查询账号。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 后台登录时校验请求中的账号、密码和类型，只返回不含密码哈希的身份。
    /// 不存在、类型不符、不可登录及密码错误统一为凭证错误；数据库错误单独返回。
    /// 旧 MD5 凭证匹配后必须先成功写入 Argon2id 凭证，才能交付登录身份。
    pub async fn authenticate(&self, request: &crate::dto::AuthRequest) -> Result<BackofficeAuthResult> {
        request.validate()?;
        let stored_account = self.find_account(&request.account).await?;
        let secret = stored_account
            .as_ref()
            .and_then(|account| backoffice_secret(account, request.account_kind))
            .cloned();
        let is_authenticatable = secret.is_some();
        let password_check = password::verify_password(secret, request.password.clone()).await?;
        let mut stored_account = match (stored_account, password_check.is_match()) {
            (Some(account), true) => account,
            (account, _) => {
                tracing::info!(
                    account = request.account.as_str(),
                    account_kind = ?request.account_kind,
                    found = account.is_some(),
                    authenticatable = is_authenticatable,
                    "backoffice authenticate rejected"
                );
                return Err(invalid_credentials());
            },
        };

        if let Some(secret) = password_check.into_upgraded_secret() {
            stored_account.secret = secret;
            self.db.accounts().update(&mut stored_account, &mut NoTransaction).await?;
        }
        Ok(BackofficeAuthResult::from(&stored_account))
    }

    /// 每次受保护请求验签后重读账号，按 JWT 中的 ID、登录名、后台类型和版本核对当前身份。
    /// 软删除、停用、改名、类型或版本变化均使旧令牌失效；身份失效统一报认证错误，仓储错误原样返回。
    pub async fn validate_session(
        &self,
        account_id: &str,
        account: &str,
        account_kind: DomainAccountKind,
        account_version: u64,
    ) -> Result<BackofficeAuthResult> {
        let stored_account = self.db.accounts().find_by_id(account_id, &mut NoTransaction).await?;
        let Some(stored_account) = stored_account.as_ref().filter(|stored| {
            stored.is_active_backoffice()
                && account_kind == DomainAccountKind::Admin
                && stored.matches_session_identity(account, account_kind, account_version)
        }) else {
            return Err(invalid_session());
        };

        Ok(BackofficeAuthResult::from(stored_account))
    }

    // 登录时按规范化账号查询；格式无效视为未找到，交给密码校验做占位计算。
    async fn find_account(&self, account: &str) -> Result<Option<AccountCore>> {
        let Ok(account) = LoginAccount::new(account) else {
            return Ok(None);
        };

        Ok(self.db.accounts().find_by_account(account.as_str(), &mut NoTransaction).await?)
    }
}

// 后台密码认证失败时使用统一错误，不对外区分账号状态或密码不匹配。
fn invalid_credentials() -> Error {
    Error::Unauthenticated("用户名或密码错误".to_string())
}

// 后台验密前只为可登录的内部账号取凭证，请求中的账号类型不能切换身份域。
fn backoffice_secret(account: &AccountCore, requested_kind: DomainAccountKind) -> Option<&Secret> {
    (requested_kind == DomainAccountKind::Admin)
        .then(|| account.authentication_secret_for(DomainAccountKind::Admin))
        .flatten()
}

#[cfg(test)]
mod login_failure_class_tests {
    use super::{invalid_credentials, invalid_session};
    use crate::error::Error;

    // 验证统一凭证错误的类别和消息，不暴露密码处理细节。
    #[test]
    fn missing_wrong_kind_and_bad_password_share_unauthenticated_class() {
        let error = invalid_credentials();
        assert!(matches!(error, Error::Unauthenticated(_)));
        assert_eq!(error.to_string(), "认证失败: 用户名或密码错误");
        assert!(!error.to_string().contains("password"));
        assert!(!error.to_string().contains("argon2"));
    }

    // 验证后台会话失效只返回通用认证错误。
    #[test]
    fn session_failure_does_not_leak_account_state() {
        let error = invalid_session();
        assert!(matches!(error, Error::Unauthenticated(_)));
        assert_eq!(error.to_string(), "认证失败: 认证已失效");
    }
}

// 会话重验失败时统一报告身份失效，不披露账号当前状态。
fn invalid_session() -> Error {
    Error::Unauthenticated("认证已失效".to_string())
}

pub use crate::dto::{AuthRequest, AuthResponse};

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::password::{PasswordCheck, verify_password};
    use super::{BackofficeAuthResult, backoffice_secret};
    use crate::entity::account_core::{AccountCore, AccountCoreData, AccountStatus};
    use crate::entity::auth::{LoginAccount, Secret};

    // 按测试场景构造指定类型和状态的账号，复用固定登录名和密码。
    fn account(kind: AccountKind, status: AccountStatus) -> AccountCore {
        AccountCore::new(
            "account-1".to_string(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("admin01").unwrap(), "password123").unwrap(),
                name: "测试账号".to_string(),
                kind,
                status,
            },
        )
        .unwrap()
    }

    // 验证后台只接受内部账号，客户端指定门户类型也不能越过身份边界。
    #[test]
    fn backoffice_entry_rejects_supplier_credentials_and_client_kind_selection() {
        let admin = account(AccountKind::Admin, AccountStatus::Active);
        let supplier = account(AccountKind::Supplier, AccountStatus::Active);
        assert!(backoffice_secret(&admin, AccountKind::Admin).is_some());
        assert!(backoffice_secret(&admin, AccountKind::Supplier).is_none());
        assert!(backoffice_secret(&supplier, AccountKind::Admin).is_none());
        assert!(backoffice_secret(&supplier, AccountKind::Supplier).is_none());
    }

    // 验证启用的后台账号用正确密码可通过当前格式凭证校验。
    #[tokio::test]
    async fn authentication_rules_should_accept_matching_active_account() {
        let account = account(AccountKind::Admin, AccountStatus::Active);
        let secret = account.authentication_secret_for(AccountKind::Admin).cloned();
        let result = verify_password(secret, "password123".to_string()).await.unwrap();

        assert!(matches!(result, PasswordCheck::Current));
    }

    // 验证停用账号仍经过密码校验边界，但正确密码也不能登录。
    #[tokio::test]
    async fn inactive_account_should_still_use_password_boundary() {
        let suspended = account(AccountKind::Admin, AccountStatus::Suspended);
        let secret = suspended.authentication_secret_for(AccountKind::Admin).cloned();
        let result = verify_password(secret, "password123".to_string()).await.unwrap();
        assert!(matches!(result, PasswordCheck::Mismatch));
    }

    // 验证身份结果保留会话所需信息，调试输出不暴露 Argon2 哈希。
    #[test]
    fn auth_result_should_not_expose_password_hash() {
        let account = account(AccountKind::Admin, AccountStatus::Active);
        let result = BackofficeAuthResult::from(&account);
        let debug = format!("{result:?}");

        assert_eq!(result.account_id(), "account-1");
        assert_eq!(result.account(), "admin01");
        assert_eq!(result.name(), "测试账号");
        assert_eq!(result.account_kind(), AccountKind::Admin);
        assert_eq!(result.account_version(), 1);
        assert!(!debug.contains("$argon2"));
    }

    // 验证当前身份可以匹配，登录名或版本不符则拒绝。
    #[test]
    fn session_rules_should_accept_only_current_active_identity() {
        let active = account(AccountKind::Admin, AccountStatus::Active);

        assert!(active.matches_session_identity(
            active.secret.account(),
            AccountKind::Admin,
            active.base.version,
        ));
        assert!(!active.matches_session_identity("renamed-account", AccountKind::Admin, active.base.version));
        assert!(!active.matches_session_identity(
            active.secret.account(),
            AccountKind::Admin,
            active.base.version + 1,
        ));
    }

    // 验证停用和归档账号即使登录名及版本匹配也不能沿用会话。
    #[test]
    fn session_rules_should_reject_missing_or_inactive_accounts() {
        let suspended = account(AccountKind::Admin, AccountStatus::Suspended);
        let archived = account(AccountKind::Admin, AccountStatus::Archived);

        assert!(!suspended.matches_session_identity(
            suspended.secret.account(),
            AccountKind::Admin,
            suspended.base.version,
        ));
        assert!(!archived.matches_session_identity(
            archived.secret.account(),
            AccountKind::Admin,
            archived.base.version,
        ));
    }
}
