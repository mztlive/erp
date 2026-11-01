//! 采购单列表行与动作阻断视图。

use super::*;

/// 采购单列表行视图（契约形状：`purchaseOrderId`/`purchaseNo`/`status` 等）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseOrderListItemView {
    /// 实体主键。
    pub id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 来源实物及服务销售单。
    pub sales_order_id: String,
    /// 来源销售单业务单号。
    pub sales_order_no: String,
    /// 唯一供应商。
    pub supplier_id: String,
    /// 供应商名称（D07 主体修订快照）。
    pub supplier_name: String,
    /// 采购类型。
    pub purchase_type: PurchaseType,
    /// 冻结的履约责任。
    pub fulfillment_responsibility: FulfillmentResponsibility,
    /// 付款条件代码（实体 `payment_term_code`）。
    pub payment_term_code: String,
    /// 当前采购单负责人账号 ID。
    pub owner_user_id: Option<String>,
    /// 当前采购单负责人展示名（账号不存在时回落账号 ID）。
    pub owner_name: String,
    /// 主状态。
    pub status: PurchaseOrderStatus,
    /// 财务审核状态。
    pub review_status: PurchaseReviewStatus,
    /// 含税行汇总（字符串，未生效时为零值）。
    pub gross_amount: String,
    /// 不含税行汇总。
    pub net_amount: String,
    /// 税额行汇总。
    pub tax_amount: String,
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
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 采购工作面可安全展示的动作阻断摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseActionBlockerView {
    /// 被阻断的动作代码。
    pub action: String,
    /// 结构化阻断码。
    pub code: String,
    /// 面向用户的安全说明。
    pub message: String,
}
