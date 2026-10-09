//! 外部订单的可恢复步骤。方法成功只证明该步骤完成，不等于接单、退款或结算完成。

use async_trait::async_trait;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::money::{Amount, Quantity, UnitPrice};

use super::common::{ActionKey, ConnectorResult, Lookup, ReplayProtection, Snapshot, SupplierSku};
use super::offer::DeliveryContext;
use crate::entity::supplier_fulfillment::{CancelStatus, FulfillmentStatus, RefundStatus};

/// 履约个人信息；禁止 Debug 或把整个请求写入普通日志。
#[derive(Clone, PartialEq, Eq)]
pub struct Recipient {
    pub name: String,
    pub phone: String,
    pub region: String,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderLine {
    /// 我方稳定明细身份，拆单和回执分摊均以此关联。
    pub line_id: String,
    pub sku: SupplierSku,
    /// 必须为正且符合商品单位精度。
    pub quantity: Quantity,
    /// 下单批准时冻结的人民币含税成本；超出批准价格必须拒绝，不隐式接受涨价。
    pub approved_unit_price: UnitPrice,
    /// 已声明且校验过的商品选项，例如口味；不能承载业务指令或任意 JSON。
    pub attributes: Vec<SelectedAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedAttribute {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryMethod {
    Courier,
    LocalDelivery { date: BusinessDate, slot: String },
    Pickup { store_id: String, date: BusinessDate, slot: String },
}

/// 查询取得的选项，必须与连接、地址、明细、数量及有效期绑定后才可提交。
#[derive(Clone, PartialEq, Eq)]
pub struct DeliveryChoice {
    pub method: DeliveryMethod,
    pub shipping_fee: Amount,
    /// 供应商配送规则及所选时段的封装引用；不承载密钥或待执行 HTTP 请求。
    pub reference: String,
}

/// 一个已确定拆单组；适配器不得在 create 中再次静默拆单。
#[derive(Clone, PartialEq, Eq)]
pub struct CreateOrder {
    pub action: ActionKey,
    /// 例如供应商 out_order_no；每个拆单组一个，重试时保持不变。
    pub merchant_order_no: String,
    pub lines: Vec<OrderLine>,
    pub recipient: Recipient,
    pub delivery_context: DeliveryContext,
    pub delivery: DeliveryChoice,
    /// 含所有费用的人民币含税最大批准金额；不含商城卡券/微信扣款指令。
    pub approved_total: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderKey {
    Merchant(String),
    External(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderReference {
    pub merchant_order_no: String,
    pub external_order_no: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentState {
    Pending,
    Confirmed,
}

/// 每条轨道独立；None 表示本次未取得该事实，禁止推导“无取消/无退款”。
#[derive(Clone, PartialEq, Eq)]
pub struct OrderSnapshot {
    pub reference: OrderReference,
    /// 来源未返回可核实金额时为 None；不得用批准金额伪造供应商实际金额。
    pub amounts: Option<OrderAmounts>,
    /// 只映射来源可证明的状态；HTTP 200 和创建成功均不是 Accepted。
    pub fulfillment: Option<FulfillmentStatus>,
    pub cancellation: Option<CancelStatus>,
    pub refund: Option<RefundStatus>,
    pub payment: Option<PaymentState>,
    /// None 表示未取得物流信息；Some(empty) 才表示来源明确返回当前无包裹。
    pub shipments: Option<Vec<Shipment>>,
}

/// 供应商当前确认的金额，分别保存应支付额和采购成本，不能混用商城售价。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderAmounts {
    pub payable: Amount,
    /// 三项成本均为人民币含税、互不包含；无法拆分时保留 None。
    pub goods_cost: Option<Amount>,
    pub shipping_cost: Option<Amount>,
    pub service_cost: Option<Amount>,
}

/// 配送员手机号属于敏感资料，不派生 Debug；包裹可有多条明细。
#[derive(Clone, PartialEq, Eq)]
pub struct Shipment {
    pub external_shipment_id: Option<String>,
    pub line_ids: Vec<String>,
    pub carrier: Option<String>,
    pub tracking_no: Option<String>,
    pub courier_phone: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationNext {
    ObserveOrder,
    ConfirmPayment,
}

/// 外部订单创建已确认；返回后必须先持久化，才能执行 next。
#[derive(Clone, PartialEq, Eq)]
pub struct CreatedOrder {
    pub order: Snapshot<OrderSnapshot>,
    pub next: CreationNext,
}

#[async_trait]
pub trait Orders: Send + Sync {
    /// 声明供应商对重复创建的保护窗口。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 实际供应商保证；Unverified 禁止未知结果自动重放。
    /// # 错误
    /// 不返回错误。
    fn replay_protection(&self) -> ReplayProtection;

    /// 创建一个外部订单，不隐式支付确认、重试、拆单或推进 ERP 状态。
    ///
    /// # 参数
    /// `request` 为已验证并冻结的非空拆单组，所有动作身份须先持久化。
    /// # 返回
    /// 外部订单与下一步骤，创建成功不代表供应商接单。创建后发现超出批准金额也必须
    /// 返回已创建的订单及实际金额，由编排层阻断支付并登记异常，不得伪装为未创建。
    /// # 错误
    /// 创建前已确认价格/履约条件不符返回 BusinessRejected；可能已送达的超时返回 ResultUnknown。
    async fn create(&self, request: &CreateOrder) -> ConnectorResult<CreatedOrder>;

    /// 按原身份查询订单。Merchant 查询是创建结果未知时的恢复入口。
    ///
    /// # 参数
    /// `key` 为原我方订单号或已保存的外部订单号。
    /// # 返回
    /// 当前订单事实或 NotVisible；NotVisible 不能证明可安全重新下单。
    /// # 错误
    /// 不支持对应查询键时返回 CapabilityGap，其余查询失败返回分类错误。
    async fn order(&self, key: &OrderKey) -> ConnectorResult<Lookup<Snapshot<OrderSnapshot>>>;
}

/// 商城支付成功后，对已创建供应商订单确认渠道支付结果的独立动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmPayment {
    pub action: ActionKey,
    pub order: OrderReference,
    pub transaction_no: String,
    /// 供应商合同要求的人民币支付金额，不得由商城销售价无条件替代。
    pub amount: Amount,
}

#[async_trait]
pub trait PaymentConfirmation: Send + Sync {
    /// 声明支付确认动作的去重保证，与创建订单的保证分别配置。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 对此动作已核实的供应商幂等保证。
    /// # 错误
    /// 不返回错误。
    fn replay_protection(&self) -> ReplayProtection;

    /// 仅确认供应商侧支付结果；不得触发商城扣款。
    ///
    /// # 参数
    /// `request` 为已持久化的独立支付确认动作。
    /// # 返回
    /// 供应商支付轨道快照，不代表履约接单；未知结果通过 Orders::order 恢复。
    /// # 错误
    /// 可能产生副作用的超时返回 ResultUnknown，禁止适配器自行重发。
    async fn confirm(&self, request: &ConfirmPayment) -> ConnectorResult<Snapshot<PaymentState>>;
}

/// None 为整单取消；部分取消必须列出非空且不重复的明细及正数量。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelOrder {
    pub action: ActionKey,
    pub order: OrderReference,
    pub lines: Option<Vec<ReturnLine>>,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnLine {
    pub line_id: String,
    pub sku: SupplierSku,
    pub quantity: Quantity,
}

/// 请求受理身份；只表示受理，最终取消/退款必须另取结果事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionAccepted {
    pub action_id: String,
    pub external_request_id: Option<String>,
}

#[async_trait]
pub trait Cancellations: Send + Sync {
    /// 申请取消；即刻完成的供应商也通过订单查询确认最终事实。
    ///
    /// # 参数
    /// `request` 携带稳定售后动作身份、订单及取消范围。
    /// # 返回
    /// 受理身份；重复申请由编排层调查原订单，不自动重放。
    /// # 错误
    /// 不支持部分取消返回 CapabilityGap；业务拒绝或结果未知返回对应分类。
    async fn cancel(&self, request: &CancelOrder) -> ConnectorResult<ActionAccepted>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestRefund {
    pub action: ActionKey,
    pub order: OrderReference,
    pub lines: Vec<ReturnLine>,
    pub amount: Amount,
    pub reason_code: String,
}

/// 单笔退款事实；不得把订单累计退款额当作新一笔退款入账。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefundReceipt {
    pub action_id: String,
    pub external_refund_no: String,
    pub order: OrderReference,
    pub amount: Amount,
    pub completed_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefundProgress {
    Pending,
    Rejected { reason_code: String },
    Completed(RefundReceipt),
}

#[async_trait]
pub trait Refunds: Send + Sync {
    /// 发起一笔退款，不修改 ERP 消费、成本或应付事实。
    ///
    /// # 参数
    /// `request` 为非空正数量明细及正退款金额，动作身份对应原售后请求。
    /// # 返回
    /// 请求受理身份；不能解释为已退款。
    /// # 错误
    /// 能力不足、拒绝及未知结果分别返回分类错误，未知结果不自动重试。
    async fn request(&self, request: &RequestRefund) -> ConnectorResult<ActionAccepted>;

    /// 查询原退款动作的结果，不按订单累计金额推断某笔退款成功。
    ///
    /// # 参数
    /// `order` 为原订单；`action_id` 为申请时的稳定动作身份。
    /// # 返回
    /// 单笔进度及来源证据；当前查不到返回 NotVisible。
    /// # 错误
    /// 不能按原动作定位结果时返回 CapabilityGap，查询故障返回分类错误。
    async fn refund(
        &self,
        order: &OrderReference,
        action_id: &str,
    ) -> ConnectorResult<Lookup<Snapshot<RefundProgress>>>;
}
