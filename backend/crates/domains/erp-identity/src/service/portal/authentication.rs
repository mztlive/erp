//! 门户固定外部类型的密码认证及当前会话重验。

use erp_core::AccountKind;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::{PortalIdentityService, invalid_session};
use crate::dto::PortalLoginPayload;
use crate::entity::portal::PortalActor;
use crate::repository::portal::{PortalBindingRepositoryExt, PortalIdentityExt};
use crate::repository::prelude::*;
use crate::service::auth::password;
use crate::{AccessControlExt, Error, LoginAccount, Result};

impl PortalIdentityService {
    /// 从独立门户入口认证外部账号，不接受客户端身份类型。
    ///
    /// # 参数
    /// `request` 为固定登录允许字段。
    /// # 返回
    /// 当前账号和绑定身份；组合层还须检查供应商启用事实后才能签发会话。
    /// # 错误
    /// 不存在、类型错误、停用、绑定撤销和密码错误均返回同一凭证错误。
    pub async fn authenticate(&self, request: &PortalLoginPayload) -> Result<PortalActor> {
        request.validate()?;
        let login = LoginAccount::new(&request.account)?;
        let stored = self.db.accounts().find_by_account(login.as_str(), &mut NoTransaction).await?;
        let secret = stored
            .as_ref()
            .and_then(|account| account.authentication_secret_for(AccountKind::Supplier))
            .cloned();
        let password_check = password::verify_password(secret, request.password.clone()).await?;
        if !password_check.is_match() {
            return Err(invalid_credentials());
        }
        let mut stored = stored.ok_or_else(invalid_credentials)?;
        if let Some(secret) = password_check.into_upgraded_secret() {
            stored.secret = secret;
            self.db.accounts().update(&mut stored, &mut NoTransaction).await?;
        }
        let binding = self
            .db
            .portal_bindings()
            .binding_for_account(&stored.base.id, &mut NoTransaction)
            .await?
            .ok_or_else(invalid_credentials)?;
        binding
            .identity(&stored, login.as_str(), stored.base.version, binding.base.version)
            .map_err(|_| invalid_credentials())
    }

    /// 在每次门户访问或调用方写事务中重新读取账号和绑定。
    ///
    /// # 参数
    /// `account_id`、`account` 和两个版本来自已验证会话；`executor` 由调用方指定。
    /// # 返回
    /// 基于当前绑定的供应商归属及门户岗位，绝不采用客户端 supplier_id。
    /// # 错误
    /// 任一账号或绑定事实失效均拒绝；供应商停用仍由组合层继续检查。
    pub async fn validate_session(
        &self,
        account_id: &str,
        account: &str,
        account_version: u64,
        binding_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<PortalActor> {
        let stored =
            self.db.accounts().find_by_id(account_id, executor).await?.ok_or_else(invalid_session)?;
        let binding = self
            .db
            .portal_bindings()
            .binding_for_account(account_id, executor)
            .await?
            .ok_or_else(invalid_session)?;
        binding.identity(&stored, account, account_version, binding_version)
    }
}

/// 统一凭证失败，不暴露内部账号是否存在或供应商绑定状态。
fn invalid_credentials() -> Error {
    Error::Unauthenticated("用户名或密码错误".into())
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use crate::service::auth::password::{PasswordCheck, verify_password};
    use crate::{AccountCore, AccountCoreData, AccountStatus, LoginAccount, Secret};

    #[tokio::test]
    async fn internal_credentials_use_dummy_boundary_and_cannot_authenticate_portal() {
        let account = AccountCore::new(
            "internal".into(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("buyer01").unwrap(), "password123").unwrap(),
                name: "采购".into(),
                kind: AccountKind::Admin,
                status: AccountStatus::Active,
                email: None,
                phone: None,
                avatar: None,
            },
        )
        .unwrap();
        let secret = account.authentication_secret_for(AccountKind::Supplier).cloned();
        assert!(matches!(
            verify_password(secret, "password123".into()).await.unwrap(),
            PasswordCheck::Mismatch
        ));
    }
}
