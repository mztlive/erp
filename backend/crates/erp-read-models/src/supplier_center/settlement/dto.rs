//! 跨供应链与工作流的结算详情和决定响应，保持原 HTTP 字段。
use erp_core::money::Amount;
use erp_supply::dto::supplier_settlement::*;
use erp_workflow::entity::work_item::{WorkItemStatus, WorkItemType};
use serde::Serialize;
/// 结算详情嵌入的当前正式复核任务。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementReviewWorkItemView {
    /// 正式任务 ID。
    pub work_item_id: String,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 任务 CAS 版本。
    pub task_version: u64,
    /// 冻结的结算主题摘要。
    pub subject_version: String,
    /// 任务状态。
    pub status: WorkItemStatus,
    /// 当前正式处理状态。
    pub processing_state: SettlementReviewProcessingState,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人。
    pub owner_user_id: Option<String>,
    /// 当前 actor 的领域动作阻断。
    pub action_blockers: Vec<SettlementReviewActionBlockerView>,
}

/// 结算单详情视图（结算单 + 全部明细 + 全部差异 + actor-specific 复核责任）。
#[derive(Debug, Clone, Serialize)]
pub struct SupplierSettlementStatementDetailView {
    /// 结算单头。
    pub statement: SupplierSettlementStatementView,
    /// 结算明细。
    pub items: Vec<SupplierSettlementItemView>,
    /// 结算差异。
    pub differences: Vec<SupplierSettlementDifferenceView>,
    /// 服务端汇总统计，客户端不得自行猜测处理状态。
    pub stats: SettlementStatementStatsView,
    /// 结算对象当前处理态。
    pub processing_state: String,
    /// 当前正式复核任务；不存在时为空且动作保持关闭。
    pub review_work_item: Option<SettlementReviewWorkItemView>,
    /// 正式复核任务投影是否因缺失、重复或主题不一致而阻断。
    pub review_processing_state: SettlementReviewProcessingState,
    /// 任务缺失或责任事实异常时的 fail-closed 阻断。
    pub review_action_blockers: Vec<SettlementReviewActionBlockerView>,
    /// 当前 actor 可执行的结算对象动作。
    pub allowed_actions: Vec<String>,
    /// 当前 actor 的结算对象动作阻断。
    pub action_blockers: Vec<SettlementReviewActionBlockerView>,
}

/// 供应商结算正式复核结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SettlementReviewDecisionResult {
    /// 固定结果状态。
    pub result_status: SettlementReviewDecisionStatus,
    /// 面向用户的稳定说明。
    pub message: String,
    /// 原请求操作 ID。
    pub operation_id: String,
    /// 决定后的结算单投影。
    pub statement: SupplierSettlementStatementView,
    /// 已完成的正式任务。
    pub work_item_id: String,
    /// 固定终态。
    pub work_item_status: WorkItemStatus,
    /// 任务完成后的版本。
    pub task_version: u64,
    /// 确认形成的应付编号；驳回为空。
    pub payable_no: Option<String>,
    /// 确认形成的应付账户；驳回为空。
    pub payable_account_id: Option<String>,
    /// 结算确认追加的含税成本差额；驳回为空。
    pub cost_delta_gross: Option<Amount>,
}
