//! 履约响应的唯一消费方 DTO；原字段和序列化保持。
use erp_supply::dto::supplier_fulfillment::*;
use serde::Serialize;
/// 调查动作关联的原开放任务投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderInvestigationWorkItemView {
    /// 原任务 ID。
    pub id: String,
    /// 调查后仍固定为开放。
    pub status: erp_workflow::entity::work_item::WorkItemStatus,
    /// 调查处理记录提交后的任务版本。
    pub task_version: u64,
}

/// W26 对象/任务调查统一响应。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderInvestigationResultView {
    /// 调查结果状态。
    pub result_status: SupplierOrderInvestigationResultStatus,
    /// 权限安全的业务说明。
    pub message: String,
    /// 客户端提交的操作身份。
    pub operation_id: String,
    /// 本次新增的不可变证据。
    pub evidence: SupplierOrderInvestigationEvidenceView,
    /// 返回时的订单事实。
    pub order: SupplierFulfillmentOrderView,
    /// 任务入口返回原开放任务；对象入口为空。
    pub work_item: Option<SupplierOrderInvestigationWorkItemView>,
    /// 本次证据后允许的固定下一动作。
    pub allowed_actions: Vec<String>,
    /// 本次证据后的动作阻断说明。
    pub action_blockers: Vec<SupplierOrderActionBlockerView>,
}

/// W26 强类型任务完成结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOrderTaskCompletionResultView {
    /// 服务端稳定操作结果身份。
    pub operation_id: String,
    /// 已完成的正式任务。
    pub work_item_id: String,
    /// 固定为 `COMPLETED`。
    pub work_item_status: erp_workflow::entity::work_item::WorkItemStatus,
    /// 完成后的任务版本。
    pub task_version: u64,
    /// 终态确认时的订单版本。
    pub order_lock_version: u64,
    /// 已固定的业务终态。
    pub resolution: SupplierOrderResolution,
}
