//! 外部订单的可恢复步骤。方法成功只证明该步骤完成，不等于接单、退款或结算完成。

use std::fmt;

use async_trait::async_trait;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::money::{Amount, Quantity, UnitPrice};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::address::{AddressRequirement, ConfirmedSupplierAddress};
use super::common::{
    ActionKey, ConnectorError, ConnectorResult, Lookup, ReplayProtection, Snapshot, SupplierSku,
};
use super::offer::DeliveryContext;
use super::order_tree::OrderTree;
use crate::entity::failure::SupplierFailureClass;
use crate::entity::source_purchase::SourcePurchaseTerms;
use crate::entity::supplier_fulfillment::{CancelStatus, FulfillmentStatus, RefundStatus};
use crate::{Error, Result};

/// 履约个人信息；禁止 Debug 或把整个请求写入普通日志。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipient {
    pub name: String,
    pub phone: String,
    pub region: String,
    pub address: String,
    /// 地址准备所需的结构化定位；供应商无此要求时可缺省。
    pub geocoded_address: Option<GeocodedAddress>,
    /// 服务端确认且绑定原加密快照的四级地址；旧协议缺省不补造。
    #[serde(default)]
    pub supplier_address: Option<ConfirmedSupplierAddress>,
}

impl Recipient {
    /// 校验本协议必需的冻结收货资料和供应商地址绑定。
    /// # 参数
    /// 地址要求来自适配器；连接、执行身份及指纹来自服务端原执行。
    /// # 返回
    /// 完整资料可进入地址准备；不调用供应商或推测外部地址编码。
    /// # 错误
    /// 基础资料缺失、坐标系不符或四级地址尚未确认时阻断。
    pub fn ensure_address_ready(
        &self,
        requirement: AddressRequirement,
        connection: &str,
        execution: &str,
        fingerprint: &str,
    ) -> Result<()> {
        if self.name.trim().is_empty()
            || self.phone.trim().is_empty()
            || self.region.trim().is_empty()
            || self.address.trim().is_empty()
        {
            return Err(Error::ValidationError("请补齐冻结收件资料及联系电话".into()));
        }
        match requirement {
            AddressRequirement::Text => Ok(()),
            AddressRequirement::Geocoded(system) => {
                let address = self
                    .geocoded_address
                    .as_ref()
                    .ok_or_else(|| Error::ValidationError("请补齐已核验的收件坐标".into()))?;
                address.ensure_valid(system)
            },
            AddressRequirement::FourLevel => self
                .supplier_address
                .as_ref()
                .ok_or_else(|| Error::ValidationError("请逐级选择并确认供应商四级收货地址".into()))?
                .validate_binding(connection, execution, fingerprint),
        }
    }
}

/// 结构化地址与坐标基准；禁止从自由文本猜测区县或转换坐标。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeocodedAddress {
    pub city_name: String,
    pub district: String,
    pub latitude: String,
    pub longitude: String,
    pub coordinate_system: CoordinateSystem,
}

impl GeocodedAddress {
    /// 核对明确坐标系、城市区县及定点经纬度范围。
    /// # 参数
    /// `required_system` 为适配器明确要求的坐标基准。
    /// # 返回
    /// 只验证原资料，不执行地理编码或坐标转换。
    /// # 错误
    /// 基准不符、资料缺失、格式或范围错误时拒绝。
    pub fn ensure_valid(&self, required_system: CoordinateSystem) -> Result<()> {
        let latitude: Decimal =
            self.latitude.parse().map_err(|_| Error::ValidationError("收件纬度尚未核验".into()))?;
        let longitude: Decimal =
            self.longitude.parse().map_err(|_| Error::ValidationError("收件经度尚未核验".into()))?;
        if self.coordinate_system != required_system
            || self.city_name.trim().is_empty()
            || self.district.trim().is_empty()
            || latitude < Decimal::from(-90)
            || latitude > Decimal::from(90)
            || longitude < Decimal::from(-180)
            || longitude > Decimal::from(180)
        {
            return Err(Error::ValidationError("收件坐标基准、范围或城市区县尚未核验".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoordinateSystem {
    Bd09,
    Gcj02,
    Wgs84,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderLine {
    #[serde(default)]
    pub source_purchase_terms: Option<SourcePurchaseTerms>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedAttribute {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryMethod {
    Courier,
    LocalDelivery { date: BusinessDate, slot: String },
    Pickup { store_id: String, date: BusinessDate, slot: String },
}

/// 查询取得的选项，必须与连接、地址、明细、数量及有效期绑定后才可提交。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryChoice {
    pub method: DeliveryMethod,
    pub shipping_fee: Amount,
    /// 供应商配送规则及所选时段的封装引用；不承载密钥或待执行 HTTP 请求。
    pub reference: String,
}

/// 一个已确定拆单组；适配器不得在 create 中再次静默拆单。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderReference {
    pub merchant_order_no: String,
    pub external_order_no: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentState {
    Pending,
    Confirmed,
}

/// 每条轨道独立；None 表示本次未取得该事实，禁止推导“无取消/无退款”。
#[derive(Clone, PartialEq, Eq)]
pub struct OrderSnapshot {
    /// 来源附件及协议原始事实以应用密钥加密；不把协议 JSON 暴露为领域字段。
    pub source_evidence: Option<EncryptedOrderEvidence>,
    pub reference: OrderReference,
    /// 来源未返回可核实金额时为 None；不得用批准金额伪造供应商实际金额。
    pub amounts: Option<OrderAmounts>,
    /// 只映射来源可证明的状态；HTTP 200 和创建成功均不是 Accepted。
    pub fulfillment: Option<FulfillmentStatus>,
    pub cancellation: Option<CancelStatus>,
    pub refund: Option<RefundStatus>,
    pub payment: Option<PaymentState>,
    /// 独立于支付轨道；缺失不能解释为未确认或需要支付通知。
    pub confirmation: Option<ConfirmationState>,
    /// 完整根单、历次父子关系和当前可操作叶子；未取得时保留 None。
    pub topology: Option<OrderTree>,
    /// 仅来源状态明确证明整个订单全部冻结行已发货时使用。
    pub complete_line_coverage: bool,
    /// None 表示未取得物流信息；Some(empty) 才表示来源明确返回当前无包裹。
    pub shipments: Option<Vec<Shipment>>,
}

/// 可持久化的加密来源证据，内容指纹用于忽略随机加密 nonce 的重复观察。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedOrderEvidence {
    pub encrypted_payload: String,
    pub content_fingerprint: String,
}

impl fmt::Debug for EncryptedOrderEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EncryptedOrderEvidence { [REDACTED] }")
    }
}

/// 供应商当前确认的金额，分别保存应支付额和采购成本，不能混用商城售价。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderAmounts {
    pub payable: Amount,
    /// 三项成本均为人民币含税、互不包含；无法拆分时保留 None。
    pub goods_cost: Option<Amount>,
    pub shipping_cost: Option<Amount>,
    pub service_cost: Option<Amount>,
}

/// 来源发货事实或物流观察；配送员手机号属于敏感资料，不派生 Debug。
#[derive(Clone, PartialEq, Eq)]
pub struct Shipment {
    /// 来源事实的稳定身份；允许适配器按已核验子单生成，不能展示为运单号。
    pub external_shipment_id: Option<String>,
    pub line_ids: Vec<String>,
    /// 供应商实际来源子单；缺失时不得从运单号猜测。
    pub source_order_no: Option<String>,
    /// 适配器核验的发货数量，可来自最终子单声明；None 表示只有物流观察。
    pub quantity_lines: Option<Vec<ShipmentLine>>,
    /// 仅旧协议明确证明包裹覆盖列出行全部数量时使用，缺省为 false。
    pub complete_line_coverage: bool,
    pub carrier: Option<String>,
    pub tracking_no: Option<String>,
    pub courier_phone: Option<String>,
    /// 无数量的物流事件只保存轨迹，不能作为全行发货依据。
    pub events: Vec<ShipmentEvent>,
}

/// 物流摘要可展示，原始描述可能含个人资料，须经应用密钥加密后保存。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipmentEvent {
    pub source_time_text: String,
    pub description: String,
    #[serde(default)]
    pub encrypted_description: Option<String>,
    #[serde(default)]
    pub content_fingerprint: Option<String>,
}

impl fmt::Debug for ShipmentEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ShipmentEvent { [REDACTED] }")
    }
}

/// 一条可追溯到冻结采购行的包裹数量，不按包裹数平均分摊。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipmentLine {
    pub line_id: String,
    pub quantity: Quantity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationNext {
    ObserveOrder,
    ConfirmPayment,
    ConfirmOrder,
}

/// 外部订单创建已确认；返回后必须先持久化，才能执行 next。
#[derive(Clone, PartialEq, Eq)]
pub struct CreatedOrder {
    pub order: Snapshot<OrderSnapshot>,
    pub next: CreationNext,
}

#[async_trait]
pub trait Orders: Send + Sync {
    /// 声明当前协议实现及受信技术资料是否支持创建新订单。
    /// # 参数
    /// 无；业务连接启停和逐单授权由执行入口另行校验。
    /// # 返回
    /// 只保留原订单查询而尚不能创建时返回 false；已有完整实现默认支持创建。
    /// # 错误
    /// 不返回错误，不发起外部请求。
    fn supports_creation(&self) -> bool {
        true
    }

    /// 根据本协议恢复已创建订单的后续步骤。
    /// # 参数
    /// `snapshot` 为按原单号查得的可信订单事实。
    /// # 返回
    /// 具体协议明确给出后续步骤；缺省不推导支付通知或订单确认。
    /// # 错误
    /// 来源不能证明后续步骤时返回分类错误，禁止猜测确认方式。
    fn recovery_next(&self, _snapshot: &OrderSnapshot) -> ConnectorResult<CreationNext> {
        Err(ConnectorError {
            class: SupplierFailureClass::CapabilityGap,
            code: "ORDER_RECOVERY_NEXT_UNVERIFIED".into(),
            summary: "供应商原订单的后续执行规则尚未核实，请继续调查".into(),
            retry_after: None,
        })
    }

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

    /// 按原冻结明细查询拆单关系，避免把外部 SKU 猜配到采购行。
    /// # 参数
    /// `key` 为原单号，`frozen` 为该执行组的原批准行。
    /// # 返回
    /// 可验证的订单事实；旧协议默认沿用原单号查询。
    /// # 错误
    /// 不能证明数量归属、来源关系不完整或查询失败时拒绝。
    async fn order_with_lines(
        &self,
        key: &OrderKey,
        _frozen: &[OrderLine],
    ) -> ConnectorResult<Lookup<Snapshot<OrderSnapshot>>> {
        self.order(key).await
    }

    /// 调查原取消目标，保留异常实际价格但不放宽身份、拓扑和数量归属。
    /// # 参数
    /// `key` 和 `frozen` 为已持久化原单及冻结采购行。
    /// # 返回
    /// 原订单实际事实；旧供应商默认沿用原查询合同。
    /// # 错误
    /// 身份或数量不能核实时拒绝；确认专用的单价等式不能阻断异常单取消。
    async fn cancellation_order(
        &self,
        key: &OrderKey,
        frozen: &[OrderLine],
    ) -> ConnectorResult<Lookup<Snapshot<OrderSnapshot>>> {
        self.order_with_lines(key, frozen).await
    }
}

/// 预占订单的独立确认轨道；不与支付通知互相推导。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConfirmationState {
    Pending,
    Confirmed,
    Rejected,
}

/// 一个当前叶子订单的独立确认意图，必须先持久化再发送。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmOrder {
    pub action: ActionKey,
    pub order: OrderReference,
    pub topology_version: u64,
}

#[async_trait]
pub trait OrderConfirmation: Send + Sync {
    /// 返回本动作独立核实的远端重复请求保证。
    /// # 参数
    /// 无。
    /// # 返回
    /// 未核实的协议必须返回 Unverified。
    /// # 错误
    /// 不返回错误。
    fn replay_protection(&self) -> ReplayProtection;

    /// 确认预占订单，不隐式创建、支付通知、重试或确认其他叶子。
    /// # 参数
    /// `request` 绑定当前拓扑版本、原叶子单和已持久化动作。
    /// # 返回
    /// 本次确认事实；成功不代表履约或扣款完成。
    /// # 错误
    /// 可能已送达的失败返回 ResultUnknown；按原叶子查询恢复，不隐藏重试。
    async fn confirm_order(&self, request: &ConfirmOrder) -> ConnectorResult<Snapshot<ConfirmationState>>;
}

/// 商城支付成功后，对已创建供应商订单确认渠道支付结果的独立动作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// 在保存可能发送意图前核验原目标是否允许申请取消。
    /// # 参数
    /// `request` 为服务器已冻结的真实原目标及取消范围。
    /// # 返回
    /// 可申请时成功；不构成最终取消保证，旧协议沿用原发送合同。
    /// # 错误
    /// 不可取消、资格未知或读取失败时返回错误，不得发送取消命令。
    async fn validate(&self, _request: &CancelOrder) -> ConnectorResult<()> {
        Ok(())
    }

    /// 查询原取消申请的当前进度；受理与完成分别表达。
    /// # 参数
    /// `order` 为原叶子订单身份。
    /// # 返回
    /// 来源明确的取消进度，不携带退款完成推断。
    /// # 错误
    /// 缺少独立查询能力时返回能力缺口，调用方继续查询订单事实。
    async fn progress(&self, _order: &OrderReference) -> ConnectorResult<Snapshot<CancelStatus>> {
        Err(ConnectorError {
            class: SupplierFailureClass::CapabilityGap,
            code: "CANCEL_PROGRESS_UNAVAILABLE".into(),
            summary: "该供应商未提供独立取消进度查询".into(),
            retry_after: None,
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    struct UndeclaredProtocol;

    fn unsupported() -> ConnectorError {
        ConnectorError {
            class: SupplierFailureClass::CapabilityGap,
            code: "UNSUPPORTED".into(),
            summary: "未实现".into(),
            retry_after: None,
        }
    }

    #[async_trait]
    impl Orders for UndeclaredProtocol {
        fn replay_protection(&self) -> ReplayProtection {
            ReplayProtection::Unverified
        }
        async fn create(&self, _request: &CreateOrder) -> ConnectorResult<CreatedOrder> {
            Err(unsupported())
        }
        async fn order(&self, _key: &OrderKey) -> ConnectorResult<Lookup<Snapshot<OrderSnapshot>>> {
            Err(unsupported())
        }
    }

    #[test]
    fn unspecified_recovery_never_infers_confirmation_from_payment() {
        let protocol = UndeclaredProtocol;
        for payment in [None, Some(PaymentState::Pending), Some(PaymentState::Confirmed)] {
            let snapshot = OrderSnapshot {
                source_evidence: None,
                reference: OrderReference {
                    merchant_order_no: "original".into(),
                    external_order_no: "root".into(),
                },
                amounts: None,
                fulfillment: None,
                cancellation: None,
                refund: None,
                payment,
                confirmation: Some(ConfirmationState::Pending),
                topology: None,
                complete_line_coverage: false,
                shipments: None,
            };
            let error = protocol.recovery_next(&snapshot).unwrap_err();
            assert_eq!(error.class, SupplierFailureClass::CapabilityGap);
            assert_eq!(error.code, "ORDER_RECOVERY_NEXT_UNVERIFIED");
        }
    }

    #[test]
    fn old_recipient_has_no_invented_supplier_address() {
        let value: Recipient = serde_json::from_str(
            r#"{
            "name":"收件人","phone":"phone","region":"region","address":"address",
            "geocoded_address":null
        }"#,
        )
        .unwrap();
        assert!(value.supplier_address.is_none());
    }

    #[test]
    fn address_readiness_uses_protocol_requirement_and_rejects_invalid_coordinates() {
        let mut recipient = Recipient {
            name: "收件人".into(),
            phone: "phone".into(),
            region: "region".into(),
            address: "address".into(),
            geocoded_address: None,
            supplier_address: None,
        };
        recipient.ensure_address_ready(AddressRequirement::Text, "c", "e", "fingerprint").unwrap();
        assert!(
            recipient.ensure_address_ready(AddressRequirement::FourLevel, "c", "e", "fingerprint").is_err()
        );
        recipient.geocoded_address = Some(GeocodedAddress {
            city_name: "城市".into(),
            district: "区县".into(),
            latitude: "39.9".into(),
            longitude: "116.4".into(),
            coordinate_system: CoordinateSystem::Bd09,
        });
        recipient
            .ensure_address_ready(
                AddressRequirement::Geocoded(CoordinateSystem::Bd09),
                "c",
                "e",
                "fingerprint",
            )
            .unwrap();
        assert!(
            recipient
                .ensure_address_ready(
                    AddressRequirement::Geocoded(CoordinateSystem::Gcj02),
                    "c",
                    "e",
                    "fingerprint"
                )
                .is_err()
        );
        recipient.geocoded_address.as_mut().unwrap().latitude = "NaN".into();
        assert!(
            recipient
                .ensure_address_ready(
                    AddressRequirement::Geocoded(CoordinateSystem::Bd09),
                    "c",
                    "e",
                    "fingerprint"
                )
                .is_err()
        );
    }
}
