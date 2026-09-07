//! 客户验收跨域工作台与登记响应，字段保持原 HTTP 合同。
use erp_core::money::Quantity;
use erp_fulfillment::dto::CustomerAcceptanceView;
use erp_fulfillment::entity::fulfillment::{DeliveryType, FulfillmentFactType};
use serde::Serialize;

/// 客户验收原子登记结果。
#[derive(Debug, Clone, Serialize)]
pub struct CommitCustomerAcceptanceView {
    /// 已过账客户验收单。
    pub acceptance: CustomerAcceptanceView,
    /// 过账后重新计算的可验收事实，供前端直接刷新结果区。
    pub remaining_eligibility: AcceptanceEligibilityView,
}

/// 可验收履约事实视图（W06：服务端扣除冲正后的净数量与守恒分配）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EligibleFulfillmentFactView {
    /// 履约事实行主键。
    pub fulfillment_line_id: String,
    /// 履约事实类型。
    pub fulfillment_fact_type: FulfillmentFactType,
    /// 发货类型（仅发货事实；仓发/直发展示区分用，其他事实为空）。
    pub delivery_type: Option<DeliveryType>,
    /// 履约单号（发货单号/履约记录号）。
    pub fulfillment_no: String,
    /// 销售稳定明细。
    pub sales_order_line_id: String,
    /// 行号。
    pub line_no: u32,
    /// 品名快照。
    pub item_snapshot: String,
    /// 单位快照。
    pub unit_code: Option<String>,
    /// 履约发生时间（秒级时间戳）。
    pub occurred_at: i64,
    /// 净成功履约数量（冲正后）。
    pub net_successful_quantity: Quantity,
    /// 已净验收分配数量（APPLY − REVERSE）。
    pub net_accepted_allocated_quantity: Quantity,
    /// 本次最多可验收数量（守恒）。
    pub eligible_quantity: Quantity,
    /// 物流承运方（发货事实）。
    pub carrier: Option<String>,
    /// 物流单号（发货事实）。
    pub tracking_no: Option<String>,
}

/// 验收销售明细分组视图（W06 销售行 + 可验收事实）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AcceptanceSalesLineGroupView {
    /// 销售稳定明细。
    pub sales_order_line_id: String,
    /// 行号。
    pub line_no: u32,
    /// 品名快照。
    pub item_snapshot: String,
    /// 单位快照。
    pub unit_code: Option<String>,
    /// 应履约数量。
    pub required_quantity: Quantity,
    /// 净已验收数量。
    pub net_accepted_quantity: Quantity,
    /// 可验收履约事实。
    pub fulfillment_facts: Vec<EligibleFulfillmentFactView>,
}

/// 客户验收工作台视图（W06：销售明细 + 可验收事实 + 验收历史）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AcceptanceEligibilityView {
    /// 销售单。
    pub sales_order_id: String,
    /// 销售行分组。
    pub sales_lines: Vec<AcceptanceSalesLineGroupView>,
    /// 验收历史（已过账与已冲正，按验收时间倒序）。
    pub history: Vec<CustomerAcceptanceView>,
}
