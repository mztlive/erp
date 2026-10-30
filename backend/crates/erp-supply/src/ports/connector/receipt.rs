//! 实际收到商品之后的供应商通知，独立于预占确认及供应商物流观察。

use async_trait::async_trait;
use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};

use super::common::{ActionKey, ConnectorResult, Lookup, Snapshot, SupplierSku};
use super::order::OrderReference;

/// 已由 ERP 实际验收或入库证明收到的冻结来源行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptLine {
    pub line_id: String,
    pub sku: SupplierSku,
    pub quantity: Quantity,
}

/// 原叶子、拓扑和 ERP 数量证据共同绑定的单次收货通知。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmReceipt {
    pub action: ActionKey,
    pub order: OrderReference,
    pub topology_version: u64,
    pub lines: Vec<ReceiptLine>,
    pub evidence_fingerprint: String,
}

/// 只表达供应商通知结果；不能回写为 ERP 收货或客户验收。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReceiptConfirmationState {
    Confirmed,
    NotRequired,
    /// 未取得已收到的明确来源事实；不构成重发依据。
    Unverified,
}

#[async_trait]
pub trait OrderReceiptConfirmation: Send + Sync {
    /// 通知原叶子商品已经实际收到，每个持久动作最多发送一次。
    /// # 参数
    /// `request` 已由服务端真实验收/入库、原采购及叶子数量守恒核验。
    /// # 返回
    /// 本次供应商明确结果；未知保持调查，不隐式重试。
    /// # 错误
    /// 协议、账号、目标或实际数量不能核验时拒绝；发送后错误不能证明未送达。
    async fn confirm_receipt(
        &self,
        request: &ConfirmReceipt,
    ) -> ConnectorResult<Snapshot<ReceiptConfirmationState>>;

    /// 查询原叶子收到状态及协议适用性，不能推断 ERP 实际验收。
    /// # 参数
    /// `order` 为服务端原执行绑定的第三方单号及当前真实叶子号。
    /// # 返回
    /// 当前明确状态、已证实无需通知，或仍未查明。
    /// # 错误
    /// 不支持的订单类型、账号或来源不可核实时拒绝，不能静默视为无需通知。
    async fn receipt_confirmation(
        &self,
        order: &OrderReference,
    ) -> ConnectorResult<Lookup<Snapshot<ReceiptConfirmationState>>>;
}
