use std::collections::HashMap;
use std::sync::Arc;

use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::NoTransaction;
use persistence_core::change_feed::ChangeSource;

use crate::ports::NotificationChannel;
use crate::{Channel, Error, InboxPage, InboxQuery, NotificationData, Result, repository};

/// 所有生产者使用的统一通知入口。
#[derive(Clone)]
pub struct NotificationService {
    db: Database,
    channels: HashMap<String, Arc<dyn NotificationChannel>>,
}

impl NotificationService {
    /// 创建通知服务。
    /// # 参数
    /// db 为通知仓储数据库，channels 为组合根已配置的站外渠道。
    /// # 返回
    /// 支持站内信及指定站外渠道的服务。
    /// # 错误
    /// 不执行 I/O。
    pub fn new(db: Database, channels: HashMap<String, Arc<dyn NotificationChannel>>) -> Self {
        Self { db, channels }
    }

    /// 投递单个收件人的通知。生产者须从提交后的 durable outbox 调用。
    /// # 参数
    /// message 为稳定内容，channel 为本次目标渠道。
    /// # 返回
    /// 站内持久化完成或站外提供方接收完成。
    /// # 错误
    /// 输入无效、异载荷重放、未配置渠道和持久化/提供方失败原样返回。
    pub async fn notify(&self, message: NotificationData, channel: &Channel) -> Result<()> {
        let message = message.normalized()?;
        match channel {
            Channel::Inbox => repository::receive(&self.db, message, &mut NoTransaction).await,
            Channel::External(name) => {
                self.channels.get(name).ok_or(Error::ChannelUnavailable)?.deliver(&message).await
            },
        }
    }

    /// 声明站内信新增和已读变化的订阅来源。
    /// # 参数
    /// topic 为消费方刷新主题。
    /// # 返回
    /// 只带不可变收件账号的来源声明。
    /// # 错误
    /// 不执行 I/O。
    pub fn changes<T>(&self, topic: T) -> ChangeSource<T> {
        ChangeSource::personal(repository::COLLECTION, topic, "recipient_id")
    }

    /// 查询当前账号每页 20 条消息及全部未读数。
    /// # 参数
    /// recipient 必须来自已认证会话；query 只允许继续读取游标。
    /// # 返回
    /// 最近消息与未读计数。
    /// # 错误
    /// 仓储失败返回 Repository。
    pub async fn inbox(&self, recipient: &str, query: InboxQuery) -> Result<InboxPage> {
        let items = repository::inbox(&self.db, recipient, &query, &mut NoTransaction).await?;
        Ok(InboxPage::from_records(items, self.unread_count(recipient).await?))
    }

    /// 查询当前账号未读总数。
    /// # 参数
    /// recipient 为会话账号。
    /// # 返回
    /// 未读总数。
    /// # 错误
    /// 仓储失败返回 Repository。
    pub async fn unread_count(&self, recipient: &str) -> Result<u64> {
        repository::unread(&self.db, recipient, &mut NoTransaction).await
    }

    /// 幂等标记本人的指定消息已读。
    /// # 参数
    /// recipient 为会话账号，id 为消息 ID。
    /// # 返回
    /// 首次已读时间保留。
    /// # 错误
    /// 消息不存在或不属于本人均返回 NotFound；仓储失败透传。
    pub async fn mark_read(&self, recipient: &str, id: &str) -> Result<()> {
        repository::mark_read(&self.db, recipient, id, Instant::now().unix_secs(), &mut NoTransaction).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use mongodb::Client;

    use super::*;

    struct RecordingChannel(Mutex<Vec<NotificationData>>);

    #[async_trait]
    impl NotificationChannel for RecordingChannel {
        async fn deliver(&self, message: &NotificationData) -> Result<()> {
            self.0.lock().unwrap().push(message.clone());
            if message.dedup_key == "retry" { Err(Error::Retryable) } else { Ok(()) }
        }
    }

    #[tokio::test]
    async fn external_dispatch_validates_routes_and_preserves_retry_classification() {
        // 只构造惰性客户端；站外分支不读取或写入数据库。
        let db = Client::with_options(Default::default()).unwrap().database("unused");
        let channel = Arc::new(RecordingChannel(Mutex::new(Vec::new())));
        let mut channels: HashMap<String, Arc<dyn NotificationChannel>> = HashMap::new();
        channels.insert("test".into(), channel.clone());
        let service = NotificationService::new(db, channels);
        let message = NotificationData {
            dedup_key: " event ".into(),
            recipient_id: " u1 ".into(),
            title: " 标题 ".into(),
            body: "正文".into(),
            href: None,
        };
        let target = Channel::External("test".into());

        service.notify(message.clone(), &target).await.unwrap();
        let recorded = channel.0.lock().unwrap().clone();
        assert_eq!(recorded[0].recipient_id, "u1");
        assert_eq!(recorded[0].dedup_key, "event");
        assert_eq!(recorded[0].title, "标题");

        assert!(matches!(
            service.notify(message.clone(), &Channel::External("missing".into())).await,
            Err(Error::ChannelUnavailable)
        ));

        let mut invalid = message.clone();
        invalid.title.clear();
        assert!(matches!(service.notify(invalid, &target).await, Err(Error::Invalid(_))));
        assert_eq!(channel.0.lock().unwrap().len(), 1);

        let mut retry = message;
        retry.dedup_key = "retry".into();
        assert!(matches!(service.notify(retry, &target).await, Err(Error::Retryable)));
    }
}
