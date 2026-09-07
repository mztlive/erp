//! 履约响应的唯一消费方 DTO；原字段和序列化保持。
use erp_supply::dto::supplier_fulfillment::*;
use serde::Serialize;
use serde_json::Value as WorkItemView;
/// 供应商履约订单详情视图（订单 + 明细 + 动作 + 状态历史 + 退款事实）。
#[derive(Debug, Clone, Serialize)]
pub struct SupplierFulfillmentOrderDetailView {
    /// 订单头。
    pub order: SupplierFulfillmentOrderView,
    /// 履约明细。
    pub items: Vec<SupplierFulfillmentItemView>,
    /// 状态历史（按发生时间升序）。
    pub status_history: Vec<SupplierOrderStatusHistoryView>,
    /// 对供应商动作（按创建时间降序）。
    pub actions: Vec<SupplierOrderActionView>,
    /// 退款事实（含分配行）。
    pub refund_facts: Vec<SupplierRefundFactView>,
    /// 权威供应商名称；基础资料缺失时为空，禁止回退显示 ID。
    pub supplier_name: Option<String>,
    /// 地址的服务端安全投影。
    pub address: SupplierOrderAddressView,
    /// 当前操作人可见的 W26 正式任务。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_item: Option<WorkItemView>,
    /// 当前任务/对象入口对应的权威原供应商动作。
    pub target_supplier_action_id: Option<String>,
    /// 该原动作的最新结构化调查证据。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_investigation: Option<SupplierOrderInvestigationEvidenceView>,
    /// W26 领域动作，不得从通用任务动作推导。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_actions: Vec<SupplierOrderAllowedAction>,
    /// W26 领域动作及展示事实阻断。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_blockers: Vec<SupplierOrderActionBlockerView>,
}
