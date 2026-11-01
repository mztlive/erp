use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierApiConnectionId;
use erp_integration::entity::supplier_message::SupplierPulledMessage;
use erp_integration::entity::supplier_message_consumer::SupplierMessageConsumer;
use erp_integration::repository::{
    SupplierMessageConsumerRepositoryExt, SupplierMessageExt, SupplierPulledMessageRepositoryExt,
};
use mongodb::Database;
use persistence_core::{Error as PersistenceError, NoTransaction, Transactional};

use super::SupplierMessageStore;
use super::intake::hash;
use crate::{Error, Result};

/// 已取得账号租期的持久化适配器，消息变更与账号围栏在同一事务提交。
pub struct MongoSupplierMessageStore {
    db: Database,
    consumer: SupplierMessageConsumer,
}

impl MongoSupplierMessageStore {
    /// 在本轮全部外部调用已经返回后释放消费租期，允许下一轮及时接收。
    /// # 参数
    /// 无。
    /// # 返回
    /// 原围栏仍有效时释放，不修改消息处理及未知结果。
    /// # 错误
    /// 数据库失败或已被替换时拒绝释放其他消费者租期。
    pub async fn release(&self) -> Result<()> {
        if !self
            .db
            .supplier_message_consumers()
            .release(&self.consumer, Instant::now(), &mut NoTransaction)
            .await?
        {
            return Err(Error::ConflictError("供应商消息消费资格已被替换".into()));
        }
        Ok(())
    }

    /// 建立不可转移的消费连接绑定，并原子领取有界账号租期。
    /// # 参数
    /// `account` 必须来自绑定适配器配置；`connection` 为已授权连接；`owner` 为本轮唯一消费者。
    /// `lease_seconds` 范围1—300，调用方必须在到期前停止新外部动作。
    /// # 返回
    /// 成功领取返回当前适配器；其他消费者仍占用时返回 None。
    /// # 错误
    /// 账号已被另一连接绑定、身份无效或数据库失败时拒绝。
    pub async fn claim(
        db: Database,
        account: &str,
        connection: SupplierApiConnectionId,
        owner: &str,
        lease_seconds: u32,
    ) -> Result<Option<Self>> {
        if !(1..=300).contains(&lease_seconds) || owner.is_empty() || owner.len() > 128 {
            return Err(Error::ValidationError("供应商消息消费期限或处理身份无效".into()));
        }
        let binding = SupplierMessageConsumer::bind(
            format!("smc-{}", hash(&[account.as_bytes()])),
            account.into(),
            connection,
        )?;
        ensure_binding(&db, &binding).await?;
        let now = Instant::now();
        let until = Instant::try_from_unix_secs(now.unix_secs().saturating_add(i64::from(lease_seconds)))?;
        let consumer = db
            .supplier_message_consumers()
            .claim(account, binding.connection_id.as_ref(), owner, now, until, &mut NoTransaction)
            .await?;
        Ok(consumer.map(|consumer| Self { db, consumer }))
    }

    fn ensure_message_binding(&self, message: &SupplierPulledMessage) -> Result<()> {
        if message.account_scope != self.consumer.account_scope
            || message.connection_id != self.consumer.connection_id
        {
            return Err(Error::Forbidden("供应商消息不属于当前账号连接".into()));
        }
        Ok(())
    }
}

#[async_trait]
impl SupplierMessageStore for MongoSupplierMessageStore {
    fn binding(&self) -> (&str, &SupplierApiConnectionId) {
        (&self.consumer.account_scope, &self.consumer.connection_id)
    }

    async fn receive(&self, incoming: SupplierPulledMessage) -> Result<SupplierPulledMessage> {
        self.ensure_message_binding(&incoming)?;
        let (db, consumer) = (self.db.clone(), self.consumer.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    if !db.supplier_message_consumers().fence(&consumer, Instant::now(), executor).await? {
                        return Err(Error::ConflictError("供应商消息消费资格已失效".into()));
                    }
                    let repository = db.supplier_pulled_messages();
                    if let Some(mut existing) = repository
                        .by_source(&incoming.account_scope, &incoming.source_message_id, executor)
                        .await?
                    {
                        if !existing.observe_replay(&incoming)? {
                            repository.update(&mut existing, executor).await?;
                        }
                        return Ok(existing);
                    }
                    repository.create(&incoming, executor).await?;
                    Ok(incoming)
                })
            })
            .await
    }

    async fn pending(&self) -> Result<Vec<SupplierPulledMessage>> {
        self.check_lease().await?;
        Ok(self
            .db
            .supplier_pulled_messages()
            .pending(
                &self.consumer.account_scope,
                self.consumer.connection_id.as_ref(),
                Instant::now(),
                &mut NoTransaction,
            )
            .await?)
    }

    async fn save(&self, message: &mut SupplierPulledMessage) -> Result<()> {
        self.ensure_message_binding(message)?;
        let (db, consumer, mut saved) = (self.db.clone(), self.consumer.clone(), message.clone());
        let updated = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    if !db.supplier_message_consumers().fence(&consumer, Instant::now(), executor).await? {
                        return Err(Error::ConflictError("供应商消息消费资格已失效".into()));
                    }
                    db.supplier_pulled_messages().update(&mut saved, executor).await?;
                    Ok::<_, Error>(saved)
                })
            })
            .await?;
        *message = updated;
        Ok(())
    }

    async fn check_lease(&self) -> Result<()> {
        let current = self
            .db
            .supplier_message_consumers()
            .find_by_id(&self.consumer.base.id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商消息账号绑定不存在".into()))?;
        if current.account_scope != self.consumer.account_scope
            || current.connection_id != self.consumer.connection_id
        {
            return Err(Error::Forbidden("供应商消息账号连接归属冲突".into()));
        }
        Ok(current.require_lease(
            self.consumer.lease_owner.as_deref().unwrap_or_default(),
            self.consumer.lease_epoch,
            Instant::now(),
        )?)
    }
}

async fn ensure_binding(db: &Database, binding: &SupplierMessageConsumer) -> Result<()> {
    let repository = db.supplier_message_consumers();
    if let Some(existing) =
        repository.find_by_id_including_deleted(&binding.base.id, &mut NoTransaction).await?
    {
        return validate_binding(&existing, binding);
    }
    match repository.create(binding, &mut NoTransaction).await {
        Ok(()) => Ok(()),
        Err(PersistenceError::DuplicateKey(_)) => {
            let existing = repository
                .find_by_id_including_deleted(&binding.base.id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::ConflictError("供应商消息账号绑定无法核实".into()))?;
            validate_binding(&existing, binding)
        },
        Err(error) => Err(error.into()),
    }
}

fn validate_binding(existing: &SupplierMessageConsumer, requested: &SupplierMessageConsumer) -> Result<()> {
    if existing.base.is_deleted()
        || existing.account_scope != requested.account_scope
        || existing.connection_id != requested.connection_id
    {
        return Err(Error::ConflictError("该供应商账号已绑定其他消息消费连接".into()));
    }
    Ok(())
}
