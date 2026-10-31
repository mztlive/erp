//! 主动拉取消息只提供刷新线索；不声明回调验签或直接形成订单、库存和资金事实。

use std::num::NonZeroU32;
use std::time::Duration;

use async_trait::async_trait;

use super::callback::{ChangeTopic, RefreshTarget};
use super::common::ConnectorResult;

/// 适配器已经解析的定向补查意图，不包含可直接应用的业务观察值。
#[derive(Clone, PartialEq, Eq)]
pub struct MessageRefreshHint {
    pub topic: ChangeTopic,
    pub target: RefreshTarget,
    /// 仅账号协议明确证明对应真实资金交易号时提供，不能填消息号或订单号。
    pub funds_transaction_id: Option<String>,
}

/// 未识别类型或正文无效也必须保留密文证据，不得丢弃或直接删除。
#[derive(Clone, PartialEq, Eq)]
pub enum MessageInterpretation {
    Refresh(Vec<MessageRefreshHint>),
    Quarantine { reason_code: String },
}

/// 原始正文可能含个人资料，禁止派生 Debug；本地保存前必须加密。
#[derive(Clone, PartialEq, Eq)]
pub struct PulledMessage {
    pub source_id: String,
    pub message_type: String,
    pub body: Vec<u8>,
    pub interpretation: MessageInterpretation,
}

/// 请求发出后不能确定删除结果时保留 Unknown，禁止在客户端隐式重试。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageAcknowledgement {
    Confirmed,
    Unknown,
}

/// 每个实例绑定一个账号范围和连接；账号范围不得随请求载荷改变。
#[async_trait]
pub trait SupplierMessageSource: Send + Sync {
    /// 返回账号消息的最短轮询间隔。
    /// # 参数
    /// 无。
    /// # 返回
    /// 缺省一分钟；Worker 检查节奏不等于供应商请求频率。
    /// # 错误
    /// 不返回错误。
    fn poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    /// 返回服务端配置绑定的账号命名空间，不包含密钥或令牌。
    /// # 参数
    /// 无。
    /// # 返回
    /// 稳定账号范围；同账号的不同连接必须返回相同值。
    /// # 错误
    /// 不返回错误。
    fn account_scope(&self) -> &str;

    /// 判断官方协议是否明确将该类型仅定义为原订单补查线索。
    /// # 参数
    /// `message_type` 为已持久化的来源类型，供旧版错误资金提示恢复使用。
    /// # 返回
    /// 缺少单笔资金身份时可以只完成原单补查返回真；默认假保持既有资金处理要求。
    /// # 错误
    /// 不返回错误；真实资金交易身份始终由消费方单独核验，不因本策略跳过。
    fn order_only_notice(&self, _message_type: &str) -> bool {
        false
    }

    /// 拉取有界消息，不删除远端队列。
    /// # 参数
    /// `limit` 是本次最多接收数量。
    /// # 返回
    /// 保留真实来源身份和原始正文的消息。
    /// # 错误
    /// 读取失败按连接错误分类返回，不伪造空队列。
    async fn pull(&self, limit: NonZeroU32) -> ConnectorResult<Vec<PulledMessage>>;

    /// 对已经可靠保存并结束补查应用的单条消息执行一次远端确认。
    /// # 参数
    /// `source_id` 为该账号原始消息号。
    /// # 返回
    /// 已确认或结果未知；未知不能解释为安全重发。
    /// # 错误
    /// 不得隐藏重试；请求可能送达时使用 ResultUnknown 分类。
    async fn acknowledge(&self, source_id: &str) -> ConnectorResult<MessageAcknowledgement>;
}
