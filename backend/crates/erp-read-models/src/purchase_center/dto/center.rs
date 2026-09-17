//! 采购单对象中心与应付汇总视图。

use super::*;

/// 采购单对象中心视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseOrderCenterView {
    /// 实体主键。
    pub id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 主状态。
    pub status: PurchaseOrderStatus,
    /// 财务审核状态。
    pub review_status: PurchaseReviewStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 来源销售单。
    pub sales_order_id: String,
    /// 来源销售单业务单号。
    pub sales_order_no: String,
    /// 供应商。
    pub supplier_id: String,
    /// 供应商名称快照。
    pub supplier_name: String,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 付款条件。
    pub payment_term_code: String,
    /// 履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 当前采购单责任人账号 ID。
    pub owner_user_id: String,
    /// 当前采购单责任人展示名。
    pub owner_name: String,
    /// 仓库履约冻结的目标收货仓。
    pub target_warehouse_id: Option<String>,
    /// 付款进度。
    pub payment_progress: ProgressStatus,
    /// 收票进度。
    pub invoice_progress: ProgressStatus,
    /// 履约进度。
    pub fulfillment_progress: ProgressStatus,
    /// 当前待财务审核的不可变提交。
    pub current_submission_id: Option<String>,
    /// 当前生效版本。
    pub current_revision_id: Option<String>,
    /// 当前生效版本号。
    pub revision_no: Option<u32>,
    /// 当前内容来源（`DRAFT`/`SUBMISSION`/`REVISION`）。
    pub content_source: String,
    /// 当前内容行。
    pub lines: Vec<PurchaseOrderLineView>,
    /// 当前内容表头汇总。
    pub totals: TotalsView,
    /// 生效版本的销售分配。
    pub allocations: Vec<PurchaseSalesAllocationView>,
    /// 本采购单的变更单列表。
    pub changes: Vec<PurchaseChangeSummaryView>,
    /// 应付往来子账汇总（采购单生效形成应付后存在，否则为空）。
    pub payable_summary: Option<PurchaseOrderPayableSummaryView>,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 采购单应付往来子账汇总（按采购单维度，来自应付子账派生；未生效时为空）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseOrderPayableSummaryView {
    /// 应付未结（含税）。
    pub payable_open_amount: Amount,
    /// 已付并核销（含税）。
    pub paid_allocated_amount: Amount,
    /// 已收票并核销（含税）。
    pub purchase_invoice_allocated_amount: Amount,
}
