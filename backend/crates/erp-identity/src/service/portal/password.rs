//! 门户自助密码修改：计算在事务外，最终身份重验及写入在调用方事务内。

use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::{PortalIdentityService, PreparedPortalPassword, invalid_session};
use crate::dto::PortalPasswordUpdate;
use crate::entity::portal::PortalActor;
use crate::service::auth::password;
use crate::{AccessControlExt, Error, LoginAccount, Result};

impl PortalIdentityService {
    /// 验证当前原密码并在事务外生成新凭证。
    ///
    /// # 参数
    /// `actor` 为已验证门户人；`input` 为执行登录同等密码策略的修改请求。
    /// # 返回
    /// 保留本次会话版本的待写入密码命令。
    /// # 错误
    /// 原密码错误、会话过期或密码策略失败时拒绝。
    pub async fn prepare_password(
        &self,
        actor: &PortalActor,
        input: &PortalPasswordUpdate,
    ) -> Result<PreparedPortalPassword> {
        input.validate()?;
        let current = self
            .validate_session(
                &actor.account_id,
                &actor.account,
                actor.account_version,
                actor.binding_version,
                &mut NoTransaction,
            )
            .await?;
        let account = self
            .db
            .accounts()
            .find_by_id(&current.account_id, &mut NoTransaction)
            .await?
            .ok_or_else(invalid_session)?;
        let check = password::verify_password(Some(account.secret), input.old_password.clone()).await?;
        if !check.is_match() {
            return Err(Error::Unauthenticated("原密码错误".into()));
        }
        let secret =
            password::hash_secret(LoginAccount::new(&current.account)?, input.new_password.clone()).await?;
        Ok(PreparedPortalPassword { actor: current, secret })
    }

    /// 在调用方事务中重验原会话并更新凭证，旧会话随账号版本失效。
    ///
    /// # 参数
    /// `prepared` 为已验证并哈希的命令；`executor` 为调用方事务。
    /// # 返回
    /// 更新版本后的门户身份，供协议层明确重新登录或签发新会话。
    /// # 错误
    /// 账号、绑定、岗位或版本变化及持久化失败时拒绝。
    pub async fn password_update(
        &self,
        prepared: PreparedPortalPassword,
        executor: &mut dyn Executor,
    ) -> Result<PortalActor> {
        let PreparedPortalPassword { actor, secret } = prepared;
        self.validate_session(
            &actor.account_id,
            &actor.account,
            actor.account_version,
            actor.binding_version,
            executor,
        )
        .await?;
        let mut account =
            self.db.accounts().find_by_id(&actor.account_id, executor).await?.ok_or_else(invalid_session)?;
        account.secret = secret;
        self.db.accounts().update(&mut account, executor).await?;
        self.validate_session(
            &actor.account_id,
            &actor.account,
            account.base.version,
            actor.binding_version,
            executor,
        )
        .await
    }
}
