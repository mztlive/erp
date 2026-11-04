use async_trait::async_trait;

use crate::{NotificationData, Result};

/// 站外渠道适配端口。实现必须按收件人及 dedup_key 幂等，并设置超时。
#[async_trait]
pub trait NotificationChannel: Send + Sync {
    /// 投递规范化消息；不得在数据库事务中调用。
    /// # 参数
    /// message 包含稳定去重键、内部收件人 ID 与展示内容。
    /// # 返回
    /// 提供方确认接收后返回成功。
    /// # 错误
    /// 可重试失败返回 Retryable；永久拒绝返回 Rejected；未配置返回 ChannelUnavailable。
    async fn deliver(&self, message: &NotificationData) -> Result<()>;
}
