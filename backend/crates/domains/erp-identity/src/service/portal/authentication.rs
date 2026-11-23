//! 门户固定外部类型的密码认证及当前会话重验。

use erp_core::AccountKind;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::{PortalIdentityService, invalid_session};
use crate::dto::PortalLoginPayload;
use crate::entity::portal::PortalActor;
use crate::repository::portal::{PortalBindingRepositoryExt, PortalIdentityExt};
use crate::service::auth::{authenticate_account, invalid_credentials};
use crate::{AccessControlExt, LoginAccount, Result};

impl PortalIdentityService {
    /// 门户登录只按允许的登录字段认证供应商账号，不接收客户端指定的身份类型。
    /// 不存在、类型不符、停用、绑定失效和密码错误均返回同一凭证错误。
    /// 返回当前账号及绑定身份，组合层须再确认供应商已启用才能签发会话。
    pub async fn authenticate(&self, request: &PortalLoginPayload) -> Result<PortalActor> {
        request.validate()?;
        let login = LoginAccount::new(&request.account)?;
        let stored = authenticate_account(
            &self.db,
            login.as_str(),
            request.password.clone(),
            AccountKind::Supplier,
            true,
        )
        .await?;

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

    /// 每次门户访问及调用方写事务中，按已验证会话的账号 ID、登录名及两个版本重读账号和绑定。
    /// 供应商归属及岗位只取当前绑定，不接受客户端指定；任一事实失效即拒绝，供应商启用另由组合层检查。
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

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use crate::service::auth::password::verify_password;
    use crate::{AccountCore, AccountCoreData, AccountStatus, LoginAccount, Secret};

    // 验证内部账号即使用正确密码，也只能经过门户占位验密并得到不匹配。
    #[tokio::test]
    async fn internal_credentials_use_dummy_boundary_and_cannot_authenticate_portal() {
        let account = AccountCore::new(
            "internal".into(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("buyer01").unwrap(), "password123").unwrap(),
                name: "采购".into(),
                kind: AccountKind::Admin,
                status: AccountStatus::Active,
            },
        )
        .unwrap();
        let secret = account.authentication_secret_for(AccountKind::Supplier).cloned();
        assert!(!verify_password(secret, "password123".into()).await.unwrap());
    }
}
