use database::{repository::ConsumerFilter, DatabaseExt};
use entities::{Consumer, ConsumerUpdate, LoginAccount};
use mongodb::Database;

use crate::{
    audit::run_audited_transaction,
    audit::AuditActor,
    auth::password,
    errors::{Error, Result},
    Page,
};

use super::dto::{
    ConsumerItem, ConsumerListParams, CreateConsumerParams, NormalizedConsumerListParams,
    UpdateConsumerParams,
};

/// 管理端消费者账号服务。
pub struct ConsumerAccountService {
    db: Database,
}

impl ConsumerAccountService {
    /// 创建 ConsumerAccountService 实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回创建的实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 创建消费者账号，确保账号唯一。
    ///
    /// # 参数
    /// * `params` - 参数集合
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当内部逻辑或依赖操作失败时返回错误。
    pub async fn create(&self, params: CreateConsumerParams, actor: AuditActor) -> Result<()> {
        let account = LoginAccount::new(params.account)?;
        if self
            .db
            .consumers()
            .find_by_account_including_deleted(account.as_str())
            .await?
            .is_some()
        {
            return Err(Error::ConflictError("账号已存在".into()));
        }

        let consumer =
            new_consumer(id_generator::next_id(), account, params.password, params.nickname).await?;
        let audit = actor.resource_log("consumer.create", "consumer", consumer.base.id.clone())?;
        let db = self.db.clone();
        run_audited_transaction(self.db.clone(), audit, move |session| {
            Box::pin(async move {
                db.consumers().create_with_session(&consumer, session).await?;
                Ok(())
            })
        })
        .await
    }

    /// 更新消费者账号，可修改账号、密码及昵称。
    ///
    /// # 参数
    /// * `params` - 参数集合
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当内部逻辑或依赖操作失败时返回错误。
    pub async fn update(&self, params: UpdateConsumerParams, actor: AuditActor) -> Result<()> {
        let (id, update) = params.into_update()?;
        let consumer = self
            .db
            .consumers()
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound("用户不存在".into()))?;

        if let Some(account) = update.account.as_ref() {
            if account.as_str() != consumer.secret.account() {
                let existing = self
                    .db
                    .consumers()
                    .find_by_account_including_deleted(account.as_str())
                    .await?;
                if existing.is_some_and(|existing| existing.base.id != consumer.base.id) {
                    return Err(Error::ConflictError("账号已存在".into()));
                }
            }
        }

        let mut consumer = apply_consumer_update(consumer, update).await?;
        let audit = actor.resource_log("consumer.update", "consumer", consumer.base.id.clone())?;
        let db = self.db.clone();
        run_audited_transaction(self.db.clone(), audit, move |session| {
            Box::pin(async move {
                db.consumers().update_with_session(&mut consumer, session).await?;
                Ok(())
            })
        })
        .await
    }

    /// 获取消费者账号列表（分页）。
    ///
    /// # 参数
    /// * `params` - 参数集合
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当内部逻辑或依赖操作失败时返回错误。
    pub async fn consumer_list(&self, params: &ConsumerListParams) -> Result<Page<ConsumerItem>> {
        let NormalizedConsumerListParams {
            account,
            nickname,
            page,
            page_size,
        } = params.normalized();
        let filter = ConsumerFilter {
            account,
            nickname,
            page,
            page_size,
        };
        let page = self.db.consumers().search_consumers(&filter).await?;
        let items = page.items.into_iter().map(Into::into).collect();
        Ok(Page::new(items, page.total))
    }
}

/// 构建消费者实体，并通过共享有界阻塞边界完成密码哈希。
async fn new_consumer(
    id: String,
    account: LoginAccount,
    password: String,
    nickname: Option<String>,
) -> Result<Consumer> {
    password::run_hashing(move || Consumer::new(id, account, password, nickname)).await
}

/// 应用消费者更新；仅在包含密码时占用共享 Argon2 工作槽。
async fn apply_consumer_update(mut consumer: Consumer, update: ConsumerUpdate) -> Result<Consumer> {
    if update.password.is_none() {
        consumer.update(update)?;
        return Ok(consumer);
    }

    password::run_hashing(move || {
        consumer.update(update)?;
        Ok(consumer)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::{apply_consumer_update, new_consumer};
    use crate::auth::password::{verify_password, PasswordCheck};
    use entities::{ConsumerUpdate, FieldUpdate, LoginAccount};

    #[tokio::test]
    async fn password_update_keeps_nullable_nickname_semantics() {
        let consumer = new_consumer(
            "consumer-1".to_string(),
            LoginAccount::new("consumer01").unwrap(),
            "password123".to_string(),
            Some("测试用户".to_string()),
        )
        .await
        .unwrap();

        let consumer = apply_consumer_update(
            consumer,
            ConsumerUpdate {
                account: None,
                password: Some("next-password".to_string()),
                nickname: FieldUpdate::Clear,
            },
        )
        .await
        .unwrap();

        assert_eq!(consumer.nickname, None);
        assert!(matches!(
            verify_password(Some(consumer.secret), "next-password".to_string())
                .await
                .unwrap(),
            PasswordCheck::Current
        ));
    }
}
