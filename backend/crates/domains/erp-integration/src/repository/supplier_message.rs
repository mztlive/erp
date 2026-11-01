//! 拉取消息持久化与账号消费围栏；不访问其他领域集合。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::supplier_message::SupplierPulledMessage;
use crate::entity::supplier_message_consumer::SupplierMessageConsumer;

pub trait SupplierMessageExt {
    const SUPPLIER_PULLED_MESSAGES: &'static str = "supplier_pulled_messages";
    const SUPPLIER_MESSAGE_CONSUMERS: &'static str = "supplier_message_consumers";

    /// 取得独立于回调的拉取消息证据仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 本域消息仓储。
    /// # 错误
    /// 无。
    fn supplier_pulled_messages(&self) -> Repository<'_, SupplierPulledMessage>;

    /// 取得账号唯一消费绑定与租期仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 本域账号消费者仓储。
    /// # 错误
    /// 无。
    fn supplier_message_consumers(&self) -> Repository<'_, SupplierMessageConsumer>;
}

impl SupplierMessageExt for Database {
    fn supplier_pulled_messages(&self) -> Repository<'_, SupplierPulledMessage> {
        Repository::new(self, Self::SUPPLIER_PULLED_MESSAGES)
    }

    fn supplier_message_consumers(&self) -> Repository<'_, SupplierMessageConsumer> {
        Repository::new(self, Self::SUPPLIER_MESSAGE_CONSUMERS)
    }
}

#[allow(async_fn_in_trait)]
pub trait SupplierPulledMessageRepositoryExt {
    /// 按真实账号和来源消息号读取去重证据。
    /// # 参数
    /// `account` 是服务端绑定范围；`source_id` 是来源身份；`executor` 为调用方执行器。
    /// # 返回
    /// 原接收记录，包含已结束记录。
    /// # 错误
    /// 数据库读取失败时返回原错误。
    async fn by_source(
        &self,
        account: &str,
        source_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierPulledMessage>>;

    /// 查询当前绑定账号和连接的有界可恢复消息。
    /// # 参数
    /// `account`、`connection` 来自服务端连接，`now` 为当前时间。
    /// # 返回
    /// 最早至多50条未结束消息，不包含隔离及已知删除未知记录。
    /// # 错误
    /// 数据库查询失败时返回原错误。
    async fn pending(
        &self,
        account: &str,
        connection: &str,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierPulledMessage>>;
}

impl SupplierPulledMessageRepositoryExt for Repository<'_, SupplierPulledMessage> {
    async fn by_source(
        &self,
        account: &str,
        source_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierPulledMessage>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "account_scope":account, "source_message_id":source_id,
                "deleted_at":NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    async fn pending(
        &self,
        account: &str,
        connection: &str,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierPulledMessage>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "account_scope":account, "connection_id":connection,
                "state":{"$in":["RECEIVED","APPLYING","ACK_PENDING","ACK_SENDING"]},
                "deleted_at":NOT_DELETED_TIMESTAMP_BSON,
                "$or":[{"next_attempt_at":null},{"next_attempt_at":{"$lte":now.unix_secs()}}],
            },
            FindOptions::builder().sort(doc! {"received_at":1,"id":1}).limit(50).build(),
            executor,
        )
        .await
    }
}

#[allow(async_fn_in_trait)]
pub trait SupplierMessageConsumerRepositoryExt {
    /// 原子领取已绑定连接的账号消费资格，旧租期到期后才允许更换消费者。
    /// # 参数
    /// `account`、`connection` 是已持久化绑定；`owner` 是本次消费者唯一身份。
    /// `now`、`until` 为有界租期；事务执行器由调用方提供。
    /// # 返回
    /// 成功领取后的递增围栏；未命中时返回 None。
    /// # 错误
    /// 数据库或版本范围错误保留原分类。
    async fn claim(
        &self,
        account: &str,
        connection: &str,
        owner: &str,
        now: Instant,
        until: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierMessageConsumer>>;

    /// 写入消息前在同一事务内触碰账号围栏，防止过期消费者提交。
    /// # 参数
    /// `consumer` 为领取快照；`now` 为当前时间，`executor` 必须属于调用方事务。
    /// # 返回
    /// 仍持有账号消费资格时返回 true。
    /// # 错误
    /// 数据库写入失败时返回原错误。
    async fn fence(
        &self,
        consumer: &SupplierMessageConsumer,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 结束已经完成外部调用的本轮消费，不允许旧消费者释放新租期。
    /// # 参数
    /// `consumer` 为领取快照；`now` 为结束时间，执行器由调用方提供。
    /// # 返回
    /// 围栏仍匹配时返回true。
    /// # 错误
    /// 数据库更新失败返回原错误。
    async fn release(
        &self,
        consumer: &SupplierMessageConsumer,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
}

impl SupplierMessageConsumerRepositoryExt for Repository<'_, SupplierMessageConsumer> {
    async fn claim(
        &self,
        account: &str,
        connection: &str,
        owner: &str,
        now: Instant,
        until: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierMessageConsumer>> {
        if until <= now
            || until.unix_secs().saturating_sub(now.unix_secs()) > 300
            || owner.is_empty()
            || owner.len() > 128
        {
            return Ok(None);
        }
        mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            doc! {
                "account_scope":account,"connection_id":connection,"deleted_at":NOT_DELETED_TIMESTAMP_BSON,
                "lease_epoch":{"$lt":i64::MAX},
                "$or":[{"lease_until":{"$lte":now.unix_secs()}},{"lease_owner":owner}],
            },
            vec![doc! {"$set":{
                "lease_owner":{"$literal":owner},"lease_until":until.unix_secs(),
                "lease_epoch":{"$add":["$lease_epoch",1]},"version":{"$add":["$version",1]},
                "updated_at":now.unix_secs(),
            }}],
            executor,
        )
        .await
    }

    async fn fence(
        &self,
        consumer: &SupplierMessageConsumer,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let result = mongo_ops::update_one(
            &self.collection(),
            doc! {
                "id":&consumer.base.id,"account_scope":&consumer.account_scope,
                "connection_id":consumer.connection_id.as_ref(),"lease_owner":consumer.lease_owner.as_deref(),
                "lease_epoch":consumer.lease_epoch,"lease_until":{"$gt":now.unix_secs()},
                "deleted_at":NOT_DELETED_TIMESTAMP_BSON,
            },
            doc! {"$inc":{"version":1},"$set":{"updated_at":now.unix_secs()}},
            false,
            executor,
        )
        .await?;
        Ok(result.matched_count == 1)
    }

    async fn release(
        &self,
        consumer: &SupplierMessageConsumer,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let result = mongo_ops::update_one(
            &self.collection(),
            doc! {
                "id":&consumer.base.id,"connection_id":consumer.connection_id.as_ref(),
                "lease_owner":consumer.lease_owner.as_deref(),"lease_epoch":consumer.lease_epoch,
                "deleted_at":NOT_DELETED_TIMESTAMP_BSON,
            },
            doc! {"$set":{"lease_until":now.unix_secs()},"$inc":{"version":1}},
            false,
            executor,
        )
        .await?;
        Ok(result.matched_count == 1)
    }
}
