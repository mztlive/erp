//! 无正文刷新通道；业务计数始终通过原有授权查询重新计算。
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// 跨进程协议使用稳定主题；不得携带业务内容或对象标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshTopic {
    Inbox,
    Tasks,
    Approvals,
    Copies,
    Inquiries,
    Selection,
    Jobs,
    Sales,
    Purchase,
    Finance,
    Fulfillment,
    Authorization,
}
impl RefreshTopic {
    /// 返回首次连接及恢复时必须同步的全部主题。
    /// # 参数
    /// 无。
    /// # 返回
    /// 稳定主题集合。
    /// # 错误
    /// 无。
    pub fn all() -> BTreeSet<Self> {
        use RefreshTopic::*;
        [
            Inbox,
            Tasks,
            Approvals,
            Copies,
            Inquiries,
            Selection,
            Jobs,
            Sales,
            Purchase,
            Finance,
            Fulfillment,
            Authorization,
        ]
        .into()
    }
}

/// 内部投递；个人资源只交给本人，丢失来源时要求完整同步。
#[derive(Debug, Clone)]
pub enum RefreshSignal {
    Changed { topic: RefreshTopic, recipient: Option<String> },
    Resync,
    Unavailable,
}
impl RefreshSignal {
    /// 将内部信号折叠为本人的刷新主题。
    /// # 参数
    /// account 为认证账号；topics 为本次待刷新集合。
    /// # 返回
    /// 通道可用返回 true；故障返回 false，不能伪装为零。
    /// # 错误
    /// 无。
    pub fn collect(&self, account: &str, topics: &mut BTreeSet<RefreshTopic>) -> bool {
        match self {
            Self::Changed { topic, recipient } if recipient.as_deref().is_none_or(|id| id == account) => {
                topics.insert(*topic);
            },
            Self::Resync => topics.extend(RefreshTopic::all()),
            Self::Unavailable => return false,
            _ => {},
        }
        true
    }
}

/// 每个 API 进程一个有界广播器；数据库订阅生命周期由应用管理。
#[derive(Clone)]
pub struct RefreshHub {
    sender: broadcast::Sender<RefreshSignal>,
    available: Arc<AtomicBool>,
}
impl Default for RefreshHub {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender, available: Arc::new(AtomicBool::new(false)) }
    }
}
impl RefreshHub {
    /// 先订阅再读取快照，避免两者之间丢失变化。
    /// # 参数
    /// 无。
    /// # 返回
    /// 本页面独立的有界接收器。
    /// # 错误
    /// 接收落后时消费者必须完整同步。
    pub fn subscribe(&self) -> broadcast::Receiver<RefreshSignal> {
        self.sender.subscribe()
    }

    /// 读取变更流是否已经建立。
    /// # 参数
    /// 无。
    /// # 返回
    /// false 表示未建立或中断。
    /// # 错误
    /// 无。
    pub fn is_available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }

    /// 发布内部刷新信号；没有在线页面时无需保留。
    /// # 参数
    /// signal 为不包含业务正文的信号。
    /// # 返回
    /// 无。离线页面重连必须完整同步。
    /// # 错误
    /// 无接收器不算业务投递失败。
    pub fn publish(&self, signal: RefreshSignal) {
        let _ = self.sender.send(signal);
    }

    /// 设置订阅健康状态；恢复总是要求完整同步。
    /// # 参数
    /// ready 表示已建立数据库订阅。
    /// # 返回
    /// 无。
    /// # 错误
    /// 无。
    pub fn set_available(&self, ready: bool) {
        self.available.store(ready, Ordering::Release);
        self.publish(if ready { RefreshSignal::Resync } else { RefreshSignal::Unavailable });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn personal_events_are_isolated_and_equal_counts_still_refresh() {
        let mut topics = BTreeSet::new();
        let event = RefreshSignal::Changed { topic: RefreshTopic::Copies, recipient: Some("alice".into()) };
        assert!(event.collect("bob", &mut topics));
        assert!(topics.is_empty());

        assert!(event.collect("alice", &mut topics));
        assert!(event.collect("alice", &mut topics));
        assert_eq!(topics, [RefreshTopic::Copies].into());

        assert!(
            RefreshSignal::Changed { topic: RefreshTopic::Tasks, recipient: None }
                .collect("alice", &mut topics)
        );
        assert!(topics.contains(&RefreshTopic::Tasks));
    }
    #[tokio::test]
    async fn lag_requires_resync_and_recovery_is_visible_to_every_subscriber() {
        let hub = RefreshHub::default();
        let mut first = hub.subscribe();
        let mut second = hub.subscribe();
        assert!(!hub.is_available());

        let publisher = hub.clone();
        publisher.set_available(true);
        assert!(hub.is_available());
        assert!(matches!(first.recv().await.unwrap(), RefreshSignal::Resync));
        assert!(matches!(second.recv().await.unwrap(), RefreshSignal::Resync));

        for _ in 0..300 {
            hub.publish(RefreshSignal::Changed { topic: RefreshTopic::Jobs, recipient: None });
        }
        assert!(matches!(first.recv().await, Err(broadcast::error::RecvError::Lagged(_))));
        let mut topics = BTreeSet::new();
        RefreshSignal::Resync.collect("alice", &mut topics);
        assert_eq!(topics, RefreshTopic::all());

        let mut current = hub.subscribe();
        publisher.set_available(false);
        assert!(!hub.is_available());
        assert!(!current.recv().await.unwrap().collect("alice", &mut topics));
    }
}
