//! 兼容旧客户端的履约状态类型；任务接口不再组装业务付款资料。
use serde::Serialize;

use super::FulfillmentQueueGateState;

/// 采购整单实际付款进度，与付款申请审批状态分离。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FulfillmentPaymentState {
    Unpaid,
    Partial,
    Paid,
    NotRequired,
}

/// 列表行只返回状态，不展开付款申请、金额、回单或来源单据。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemFulfillmentStatus {
    /// 缺少应付子账或不适用采购付款时为空，不视为未付款。
    pub payment_state: Option<FulfillmentPaymentState>,
    pub gate_state: FulfillmentQueueGateState,
}
