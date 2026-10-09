//! 回调产生刷新通知或完整观察事实；未经正文认证的载荷只能作为补查线索。

use erp_core::common::time::Instant;

use super::common::{ConnectorResult, Snapshot, SourceRevision, SupplierSku};
use super::offer::{Availability, OfferChange, RegionCoverage};
use super::order::{OrderKey, OrderSnapshot, RefundReceipt};

/// HTTP 层提供未经修改的字节；个人资料和签名不得写入 Debug 日志。
pub struct CallbackRequest<'a> {
    pub method: &'a str,
    pub path_and_query: &'a str,
    pub headers: &'a [(String, Vec<u8>)],
    pub body: &'a [u8],
    pub received_at: Instant,
}

/// 声明验签实际保护范围，避免只校验渠道及时间戳就声称正文已认证。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackIntegrity {
    BodyAuthenticated,
    EnvelopeOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshTarget {
    /// 品牌等父级变化；消费方刷新本连接下已有绑定，禁止自动创建新商品。
    Brand(String),
    Product(String),
    Sku(SupplierSku),
    Order(OrderKey),
    /// 无原动作身份时只能刷新订单并登记待核验线索，不能编造退款编号。
    Refund {
        order: OrderKey,
        action_id: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeTopic {
    Product,
    Availability,
    Price,
    Region,
    Order,
    Refund,
}

/// 一个回调可以包含多个对象；没有来源 ID 时不得把接收编号冒充来源事件 ID。
#[derive(Clone, PartialEq, Eq)]
pub struct ChangeNotice {
    pub source_event_id: Option<String>,
    pub revision: SourceRevision,
    pub occurred_at: Option<Instant>,
    pub target: RefreshTarget,
    pub topic: ChangeTopic,
    /// 仅正文认证且数据完整时提供；否则必须为 None 并补查。
    /// 仅推送供应商可提供完整事实，但仍由领域校验版本、幂等及业务生效条件。
    pub observation: Option<Observation>,
}

/// 来源观察值，不是已经生效的 ERP 业务事件。对象身份必须与 target 一致。
#[derive(Clone, PartialEq, Eq)]
pub enum Observation {
    Offer(Box<Snapshot<OfferChange>>),
    Availability(Snapshot<Availability>),
    /// target 必须是对应规格；品牌/商品级局部城市变更仍走刷新线索。
    Regions(Snapshot<RegionCoverage>),
    Order(Box<Snapshot<OrderSnapshot>>),
    Refund(Snapshot<RefundReceipt>),
}

/// 成功应答在可靠保存原始接收证据和刷新意图后才能发送。
#[derive(Clone, PartialEq, Eq)]
pub struct CallbackReply {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct VerifiedCallback {
    pub integrity: CallbackIntegrity,
    pub notices: Vec<ChangeNotice>,
    pub durable_ack: CallbackReply,
}

/// 纯协议校验接口；密钥由装配注入，不从请求的 channel_no 切换连接。
pub trait Callbacks: Send + Sync {
    /// 验证绑定连接、签名和允许的时间窗口，转换为需要刷新对象的通知。
    ///
    /// # 参数
    /// `request` 为原始 HTTP 报文和服务端接收时间，验签前禁止重编码正文。
    /// # 返回
    /// 校验范围、标准通知/完整观察值及延迟发送的成功应答；不直接推进业务状态。
    /// # 错误
    /// 签名/来源错误返回 AuthSignature，字段语义错误返回 MappingError；失败不发成功应答。
    fn verify(&self, request: &CallbackRequest<'_>) -> ConnectorResult<VerifiedCallback>;
}
