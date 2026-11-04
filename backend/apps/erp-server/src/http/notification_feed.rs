//! 每个 API 进程共用一条数据库订阅；失败显式中断，重建后完整同步。
use std::time::Duration;

use erp_notification::{RefreshHub, RefreshSignal, RefreshTopic};
use futures_util::StreamExt;
use persistence_core::change_feed::{self, ChangeSource};
use tokio::sync::watch;

use super::errors::Error;
use super::tasks::RequestTasks;
use crate::wiring::{Application, notification_sources};

/// HTTP 专属订阅生命周期，与 worker 的业务投递解耦。
#[derive(Clone)]
pub(crate) struct NotificationFeed {
    pub(crate) hub: RefreshHub,
    stop: watch::Sender<bool>,
}
impl Default for NotificationFeed {
    fn default() -> Self {
        let (stop, _) = watch::channel(false);
        Self { hub: RefreshHub::default(), stop }
    }
}
impl NotificationFeed {
    /// 由进程启动入口调用一次，任务交给 HTTP 排空管理器。
    /// # 参数
    /// app 为共享装配，tasks 为生命周期管理器。
    /// # 返回
    /// 后台订阅已经登记；建立连接前 hub 保持不可用。
    /// # 错误
    /// 停机或任务管理器拒绝时返回错误。
    pub(crate) fn start(&self, app: Application, tasks: &RequestTasks) -> Result<(), Error> {
        let hub = self.hub.clone();
        let mut stop = self.stop.subscribe();
        let sources = notification_sources::sources(&app);
        tasks.spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = stop.changed() => break,
                    result = consume(&app, &sources, &hub) => {
                        hub.set_available(false);
                        tracing::warn!(error = ?result.err(), "通知变更订阅中断，将重建并同步全部计数");
                    }
                }
                tokio::select! {
                    _ = stop.changed() => break,
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {},
                }
            }
            hub.set_available(false);
        })
    }
    /// 停止订阅并唤醒 SSE，使 HTTP 可以及时排空。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已广播停止信号。
    /// # 错误
    /// 无。
    pub(crate) fn stop(&self) {
        self.stop.send_replace(true);
        self.hub.set_available(false);
    }
}

async fn consume(
    app: &Application,
    sources: &[ChangeSource<RefreshTopic>],
    hub: &RefreshHub,
) -> persistence_core::Result<()> {
    let mut changes = change_feed::subscribe(&app.db(), sources).await?;
    hub.set_available(true);
    while let Some(change) = changes.next().await {
        let change = change?;
        hub.publish(match change.topic {
            Some(topic) => RefreshSignal::Changed { topic, recipient: change.recipient },
            None => RefreshSignal::Resync,
        });
    }
    Ok(())
}
