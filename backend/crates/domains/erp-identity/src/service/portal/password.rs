//! 门户自助密码修改：计算在事务外，最终身份重验及写入在调用方事务内。

use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::{PortalIdentityService, PreparedPortalPassword, invalid_session};
use crate::dto::PortalPasswordUpdate;
use crate::entity::portal::PortalActor;
use crate::service::auth::password;
use crate::{AccessControlExt, Error, LoginAccount, Result};

impl PortalIdentityService {
    /// 门户自助改密前，在事务外重验会话和原密码，按登录密码策略生成新凭证。
    /// 原密码错误、会话失效或新密码不合规时拒绝；有效命令保留账号与绑定版本供写入时核对。
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
        if !password::verify_password(Some(account.secret), input.old_password.clone()).await? {
            return Err(Error::Unauthenticated("原密码错误".into()));
        }

        let secret =
            password::hash_secret(LoginAccount::new(&current.account)?, input.new_password.clone()).await?;
        Ok(PreparedPortalPassword { actor: current, secret })
    }

    /// 收到已验密并哈希的命令后，在调用方事务中重验原会话，再更新凭证使旧会话失效。
    /// 账号、绑定、岗位或版本不再匹配及写入失败时拒绝；成功时返回新版本身份，供协议层要求重登或签发新会话。
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
