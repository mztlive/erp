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
    /// 提供首次连接、重连或信号丢失后需要重新查询的全部刷新主题。
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
    /// 将共享信号和属于当前认证账号的个人信号并入待刷新集合，重同步则加入全部主题。订阅不可用时返回 `false`，调用方不能将故障视为无更新。
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
    /// 为当前页面取得有界广播接收器；应先订阅再读快照，避免漏掉期间的变化。接收落后时必须完整同步。
    pub fn subscribe(&self) -> broadcast::Receiver<RefreshSignal> {
        self.sender.subscribe()
    }

    /// 检查数据库变更订阅是否就绪；尚未建立或已中断均返回 `false`，避免将不可用当作正常空流。
    pub fn is_available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }

    /// 向本进程在线连接广播无正文信号，不持久化，也不把无接收器视为业务投递失败；离线页面通过重连完整同步。
    pub fn publish(&self, signal: RefreshSignal) {
        let _ = self.sender.send(signal);
    }

    /// 更新数据库订阅的可用状态，并通知在线连接完整同步或结束不可用连接，避免静默丢失变更。
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
