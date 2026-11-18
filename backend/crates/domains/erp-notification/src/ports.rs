use async_trait::async_trait;

use crate::{NotificationData, Result};

/// 站外渠道适配端口。实现必须按收件人及 dedup_key 幂等，并设置超时。
#[async_trait]
pub trait NotificationChannel: Send + Sync {
    /// 在业务事务外按收件人和去重键幂等投递规范化消息；提供方确认接收才返回成功。可重试故障、永久拒绝和渠道未配置分别返回 `Retryable`、`Rejected`、`ChannelUnavailable`。
    async fn deliver(&self, message: &NotificationData) -> Result<()>;
}
