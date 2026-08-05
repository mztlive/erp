use database::DatabaseExt;
use entities::{Consumer, LoginAccount, Secret};
use mongodb::Database;
use validator::Validate;

use crate::{
    auth::{password, PasswordLoginPayload},
    errors::{Error, Result},
};

/// 返回登录结果，供上层生成令牌。
#[derive(Debug, Clone)]
pub struct ConsumerAuthResult {
    pub user_id: String,
    pub account: String,
}

/// 消费者登录服务，基于账户密码完成鉴权。
pub struct ConsumerAuthService {
    db: Database,
}

impl ConsumerAuthService {
    /// 创建 ConsumerAuthService 实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回创建的实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 使用账户密码登录，成功时返回用户标识与账户名。
    ///
    /// # 参数
    /// * `payload` - 账号密码登录参数
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当内部逻辑或依赖操作失败时返回错误。
    pub async fn login_with_password(&self, payload: PasswordLoginPayload) -> Result<ConsumerAuthResult> {
        payload.validate()?;
        let PasswordLoginPayload { account, password } = payload;
        let mut consumer = self.find_consumer(account).await?;
        let secret = secret_for_authentication(consumer.as_ref());
        let password_check = password::verify_password(secret, password).await?;

        let Some(mut consumer) = consumer.take().filter(|consumer| consumer.is_active) else {
            return Err(invalid_credentials());
        };
        if !password_check.is_match() {
            return Err(invalid_credentials());
        }
        if let Some(secret) = password_check.into_upgraded_secret() {
            consumer.secret = secret;
            self.db.consumers().update(&mut consumer).await?;
        }

        Ok(ConsumerAuthResult {
            user_id: consumer.base.id,
            account: consumer.secret.into_account(),
        })
    }

    /// 规范化账号并查询消费者；无效账号按不存在处理并继续 dummy 校验。
    async fn find_consumer(&self, account: String) -> Result<Option<Consumer>> {
        let Ok(account) = LoginAccount::new(account) else {
            return Ok(None);
        };

        Ok(self.db.consumers().find_by_account(account.as_str()).await?)
    }
}

/// 仅为启用的消费者复制凭证；禁用或不存在时进入 dummy 校验。
fn secret_for_authentication(consumer: Option<&Consumer>) -> Option<Secret> {
    consumer
        .filter(|consumer| consumer.is_active)
        .map(|consumer| consumer.secret.clone())
}

/// 构造不泄露账号存在性的消费者凭证错误。
fn invalid_credentials() -> Error {
    Error::Unauthenticated("用户名或密码错误".into())
}

#[cfg(test)]
mod tests {
    use super::{password, secret_for_authentication};
    use entities::{Consumer, LoginAccount};

    fn consumer(is_active: bool) -> Consumer {
        let mut consumer = Consumer::new(
            "consumer-1".to_string(),
            LoginAccount::new("consumer01").unwrap(),
            "password123",
            None,
        )
        .unwrap();
        consumer.is_active = is_active;
        consumer
    }

    #[tokio::test]
    async fn active_consumer_should_verify_current_password() {
        let consumer = consumer(true);
        let secret = secret_for_authentication(Some(&consumer));

        let result = password::verify_password(secret, "password123".to_string())
            .await
            .unwrap();

        assert!(result.is_match());
    }

    #[tokio::test]
    async fn inactive_and_missing_consumers_should_use_dummy_boundary() {
        let inactive = consumer(false);

        for secret in [
            secret_for_authentication(Some(&inactive)),
            secret_for_authentication(None),
        ] {
            let result = password::verify_password(secret, "password123".to_string())
                .await
                .unwrap();
            assert!(!result.is_match());
        }
    }
}
