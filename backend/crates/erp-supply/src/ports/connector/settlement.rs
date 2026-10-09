//! 结算来源只读合同；供应商退款、商城退款和余额恢复保持独立。

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::money::Amount;

use super::common::{ConnectorResult, Page, Scan, Snapshot};
use super::order::OrderReference;

/// 对账来源明细，不直接生成应付或资金流水。所有金额为人民币且非负。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementEntry {
    /// 必须是供应商稳定账单行/流水 ID，不能用页码或当前时间生成。
    pub external_entry_id: String,
    pub order: OrderReference,
    pub occurred_at: Instant,
    pub movement: SettlementMovement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettlementMovement {
    Charge { goods: Amount, shipping: Amount, service: Amount },
    Refund { external_refund_no: String, amount: Amount },
}

#[async_trait]
pub trait Settlements: Send + Sync {
    /// 分页读取订单扣费及退款来源，保留原流水身份和修订证据。
    ///
    /// # 参数
    /// `scan` 为固定时间窗口或全量扫描及恢复游标。
    /// # 返回
    /// 独立来源明细；各费用不得重复相加，退款不能抵消商城消费事实。
    /// # 错误
    /// 缺乏可核对的稳定身份或费用口径时返回 MappingError，不伪造结算证据。
    async fn entries(&self, scan: &Scan) -> ConnectorResult<Page<Snapshot<SettlementEntry>>>;
}
