//! 域 D32 `supplier_fulfillment` 的供应商网关抽象（外部 HTTP 调用的接缝）。
//!
//! P3 §7：供应商 API 调用必须在事务之外完成，先事务内落 `inbox_message`，事务外
//! 调用网关；结果经 `inbox_message` + `integration_error_task` 承接。本模块定义
//! 网关 trait 与结果分类，禁止在事务闭包内触发网关。
//!
//! 生产默认使用失败关闭网关；`SimulatedSupplierGateway` 只允许明确
//! `sim://` 地址在测试中产生模拟结果。任何普通 URL 都不得被伪造为供应商成功。

use serde::{Deserialize, Serialize};

use crate::entity::failure::SupplierFailureClass;
use crate::entity::supplier_api::SupplierApiConnection;
use crate::entity::supplier_fulfillment::{SupplierFulfillmentOrder, SupplierOrderAction};

/// 网关对一次供应商动作请求的处理结果分类（错误分类对齐 §6.21）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DispatchOutcome {
    /// 成功：返回供应商请求号与（下单动作的）外部订单号。
    Succeeded {
        /// 供应商请求号。
        external_request_id: String,
        /// 供应商订单号（下单动作返回，其余动作为 `None`）。
        external_order_no: Option<String>,
    },
    /// 业务明确拒绝（不自动重试）。
    Rejected {
        /// 脱敏拒绝原因摘要。
        summary: String,
    },
    /// 网络超时/查询能力不足导致结果未知（先查询原请求，不盲目重发）。
    ResultUnknown {
        /// 脱敏结果摘要。
        summary: String,
    },
    /// 其他失败分类（临时故障/鉴权签名/限流等）。
    Failed {
        /// 错误分类。
        error_class: SupplierFailureClass,
        /// 脱敏失败摘要。
        summary: String,
    },
}

/// 对原供应商动作进行结果调查后的可证明结论。
///
/// “查询请求本身成功”不属于业务终态；适配器只有在供应商明确证明原请求没有
/// 形成结果时才能返回 [`Self::VerifiedNoResult`]，其余情况一律失败关闭为
/// [`Self::ResultUnknown`]。已落库的业务终态由 Service 在调用网关前独立复验。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InvestigationOutcome {
    /// 供应商明确证明原请求没有形成结果，可沿原供应商幂等键重放。
    VerifiedNoResult {
        /// 权限安全的查询证据摘要。
        summary: String,
    },
    /// 供应商没有返回足以证明原结果的证据。
    ResultUnknown {
        /// 权限安全的查询证据摘要。
        summary: String,
    },
}

/// 供应商网关：向 API 供应商发起下单/取消/退款请求并返回分类结果。
///
/// 实现必须自带超时、重试上限与错误分类（AGENTS.md 外部依赖容错）；
/// 失败只以 [`DispatchOutcome`] 返回，不向调用方抛传输层错误。
pub trait SupplierGateway: Send + Sync {
    /// 向供应商发起一次动作请求。
    ///
    /// # 参数
    /// * `action` - 待发送的动作（含幂等键与摘要）
    /// * `order` - 所属供应商子订单
    /// * `connection` - 供应商 API 连接（地址/密钥引用等配置）
    ///
    /// # 返回
    /// 返回分类后的处理结果；实现内部完成超时与重试，不直接失败。
    fn dispatch<'a>(
        &'a self,
        action: &'a SupplierOrderAction,
        order: &'a SupplierFulfillmentOrder,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + 'a>>;

    /// 查询原供应商动作结果，不把传输成功误判成业务成功。
    ///
    /// 实现必须使用原动作身份查询，不得创建新订单或改用新供应商幂等键。只有
    /// 外部系统明确返回“原请求未形成结果”时才能开放安全重放。
    fn investigate<'a>(
        &'a self,
        target_action: &'a SupplierOrderAction,
        order: &'a SupplierFulfillmentOrder,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = InvestigationOutcome> + Send + 'a>>;
}
