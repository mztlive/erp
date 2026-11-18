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
    /// 绑定站内信数据库和组合层已注册的站外渠道，供生产者复用统一投递入口；构造不触发 I/O。
    pub fn new(db: Database, channels: HashMap<String, Arc<dyn NotificationChannel>>) -> Self {
        Self { db, channels }
    }

    /// 从已提交的持久化 outbox 投递一个收件人的通知，调用方重试须保持原去重键和内容。站内以保存成功、站外以提供方确认接收为准；非法输入、异内容重放、未配置渠道或仓储和提供方失败直接返回。
    pub async fn notify(&self, message: NotificationData, channel: &Channel) -> Result<()> {
        let message = message.normalized()?;
        match channel {
            Channel::Inbox => repository::receive(&self.db, message, &mut NoTransaction).await,
            Channel::External(name) => {
                self.channels.get(name).ok_or(Error::ChannelUnavailable)?.deliver(&message).await
            },
        }
    }

    /// 声明站内信新增和已读变化的订阅来源，仅按不可变的收件账号分发指定主题，不读取消息正文或立即订阅数据库。
    pub fn changes<T>(&self, topic: T) -> ChangeSource<T> {
        ChangeSource::personal(repository::COLLECTION, topic, "recipient_id")
    }

    /// 以认证会话账号限定收件箱范围，用继续游标返回每页最多 20 条近期消息，并附该账号全部未读数；仓储失败返回 `Repository`。
    pub async fn inbox(&self, recipient: &str, query: InboxQuery) -> Result<InboxPage> {
        let items = repository::inbox(&self.db, recipient, &query, &mut NoTransaction).await?;
        Ok(InboxPage::from_records(items, self.unread_count(recipient).await?))
    }

    /// 统计会话账号的未读消息总数，供收件箱及角标展示；仓储失败返回 `Repository`，不以零代替。
    pub async fn unread_count(&self, recipient: &str) -> Result<u64> {
        repository::unread(&self.db, recipient, &mut NoTransaction).await
    }

    /// 幂等标记会话账号的指定消息已读，重复操作保留首次已读时间。找不到消息或消息不属于本人均返回 `NotFound`，仓储错误直接返回。
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
