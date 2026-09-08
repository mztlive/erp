//! 采购对象中心与依据的 HTTP 只读视图。

use erp_core::money::Amount;
use erp_procurement::dto::purchase_order::{
    PurchaseChangeSummaryView, PurchaseOrderLineView, PurchaseSalesAllocationView, TotalsView,
};
use erp_procurement::entity::purchase_order::{
    FulfillmentResponsibility, ProgressStatus, PurchaseOrderStatus, PurchaseReviewStatus, PurchaseType,
    SupplySourceType,
};
use serde::{Deserialize, Serialize};

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

/// 单据详情返回的统一只读审批结构。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalView {
    /// `PROCESS_REQUIRED` 或 `NO_APPROVAL`。
    pub requirement: String,
    /// 创建时冻结的定义摘要；未绑定为空。
    pub definition: Option<DocumentApprovalDefinitionView>,
    /// 已启动后的实例摘要；未提交为空。
    pub instance: Option<DocumentApprovalInstanceView>,
    /// 有界最近历史。
    pub recent_history: Vec<DocumentApprovalHistoryItemView>,
    /// 完整历史分页游标。
    pub history_page: DocumentApprovalHistoryPageView,
    /// 服务端允许的动作；不含选择定义或审批人。
    pub allowed_actions: Vec<String>,
}

/// 绑定定义只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalDefinitionView {
    /// 定义主键。
    pub id: String,
    /// 定义名称。
    pub name: String,
    /// 定义业务版本。
    pub version: u32,
    /// 节点摘要。单据详情不展开审批人。
    pub nodes: Vec<DocumentApprovalNodeView>,
}

/// 定义节点只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalNodeView {
    /// 节点键。
    pub key: String,
    /// 节点名称。
    pub name: String,
}

/// 运行实例只读摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalInstanceView {
    /// 实例主键。
    pub id: String,
    /// 实例状态。
    pub status: String,
    /// 当前轮次。
    pub current_round_no: u32,
    /// 当前节点键。
    pub current_node: Option<String>,
    /// 当前节点名称。
    pub current_node_name: Option<String>,
    /// 当前审批人。
    pub current_assignee: Option<String>,
    /// 当前审批人显示名。
    pub current_assignee_name: Option<String>,
    /// 最近驳回原因。
    pub latest_rejection: Option<String>,
    /// 绑定定义业务版本。
    pub process_version: Option<u32>,
    /// 受阻代码；非 BLOCKED 为空。
    pub blocker_code: Option<String>,
}

/// 有界历史项。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryItemView {
    /// 执行主键。
    pub execution_id: String,
    /// 轮次。
    pub round_no: u32,
    /// 实例内执行序号。
    pub execution_no: u32,
    /// 节点键。
    pub node_key: String,
    /// 节点名称。
    pub node_name: String,
    /// 结束结果。
    pub result: String,
    /// 审批人显示名。
    pub assignee_name: Option<String>,
    /// 决定人。
    pub decided_by: Option<String>,
    /// 决定原因。
    pub decision_reason: Option<String>,
    /// 决定时间（unix 秒）。
    pub decided_at: Option<i64>,
}

/// 完整历史分页。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryPageView {
    /// 下一页游标。
    pub next_cursor: Option<String>,
    /// 是否还有更多。
    pub has_more: bool,
}

/// 采购创建依据查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreationBasisListParams {
    /// 可选来源销售单；从销售详情或工作台进入时用于收窄范围。
    pub sales_order_id: Option<String>,
    /// 可选供给分配任务；提供时必须是当前账号拥有的开放任务。
    pub work_item_id: Option<String>,
}

/// 采购创建依据行视图（销售当前版本行 + 当前采购剩余量）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreationBasisLineView {
    /// 销售稳定行身份。
    pub sales_order_line_id: String,
    /// 销售当前版本行身份。
    pub sales_order_revision_line_id: String,
    /// 销售当前版本内的业务行号。
    pub sales_line_no: u32,
    /// 确认供应商。
    pub supplier_id: String,
    /// 销售当前版本目标数量。
    pub sales_quantity: String,
    /// 当前采购覆盖数量。
    pub covered_quantity: String,
    /// 当前采购剩余数量。
    pub remaining_quantity: String,
    /// 本供应商当前最大可创建数量，等于 `min(remaining, available)`。
    pub max_create_quantity: String,
    /// 兼容展示字段，值等于 `max_create_quantity`。
    pub confirmed_quantity: String,
    /// 最新含税成本。
    pub latest_cost_gross: String,
    /// 进项税率。
    pub input_tax_rate: String,
    /// 采购预计交付日预填值（`YYYY-MM-DD`）。
    pub expected_delivery_date: String,
    /// 销售对客户承诺的最晚交付日（`YYYY-MM-DD`）。
    pub sales_delivery_deadline: String,
    /// 商品名称快照（销售提交行侧联查，缺失时为空）。
    pub product_name: Option<String>,
    /// 规格快照。
    pub specification: Option<String>,
    /// 销售单位快照。
    pub unit: Option<String>,
    /// 含税行金额（按确认数量与成本逐行舍入）。
    pub gross_amount: String,
}

/// 采购创建依据视图（已生效销售单 × 合格供给供应商，§7.4 选源建单入口）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreationBasisView {
    /// 当前账号拥有且冻结本依据销售行范围的开放供给分配任务。
    pub work_item_id: String,
    /// 精确创建依据（任务、销售当前版本、供应商、采购类型、付款条件、履约责任及剩余量指纹）。
    pub basis_id: String,
    /// 供给来源。
    pub source_type: SupplySourceType,
    /// 被确认的销售单。
    pub sales_order_id: String,
    /// 销售单号。
    pub sales_order_no: String,
    /// 销售当前版本冻结的客户名称。
    pub customer_name: String,
    /// 销售当前版本冻结的合同编号；无合同时为空。
    pub contract_no: Option<String>,
    /// 销售单负责人展示名；账号档案缺失时为空。
    pub sales_owner_name: Option<String>,
    /// 目标销售当前版本。
    pub sales_order_revision_id: String,
    /// 供应商。
    pub supplier_id: String,
    /// 供应商名称。
    pub supplier_name: String,
    /// 现有库存来源的余额主键；采购来源为空。
    pub stock_balance_id: Option<String>,
    /// 现有库存来源的仓库主键；采购来源为空。
    pub warehouse_id: Option<String>,
    /// 现有库存来源的仓库名称；采购来源为空。
    pub warehouse_name: Option<String>,
    /// 来源当前总可供量；库存为余额可用量，未声明上限的采购来源为空。
    pub source_available_quantity: Option<String>,
    /// 采购类型（由商品稳定业务类型确定）。
    pub purchase_type: String,
    /// 履约责任（由采购在商品类型允许范围内选择）。
    pub fulfillment_responsibility: String,
    /// 付款条件（供应商商业资料快照，缺省 `NET-30`；不含经营类目）。
    pub payment_term_code: String,
    /// 供应商经营类目；未登记时为空。
    pub business_category: Option<String>,
    /// 可拆入本单的已确认分行。
    pub lines: Vec<CreationBasisLineView>,
    /// 含税行汇总（只汇总已舍入行金额）。
    pub estimated_gross: String,
}

/// 采购变更单视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseChangeOrderView {
    /// 变更单主键。
    pub id: String,
    /// 原采购单。
    pub purchase_order_id: String,
    /// 基准版本。
    pub base_revision_id: String,
    /// 变更原因。
    pub reason: String,
    /// 状态。
    pub status: String,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<String>,
    /// 生效后形成的新采购版本。
    pub effective_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    use std::str::FromStr;

    /// 财务余额跨域投影不得经过浮点数；保留超过 JS 安全整数范围的分位和字段名。
    #[test]
    fn payable_summary_preserves_exact_decimal_strings() {
        let value = PurchaseOrderPayableSummaryView {
            payable_open_amount: Amount::from_str("9007199254740993.01").unwrap(),
            paid_allocated_amount: Amount::from_str("0.02").unwrap(),
            purchase_invoice_allocated_amount: Amount::from_str("7.30").unwrap(),
        };
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            serde_json::json!({
                "payable_open_amount": "9007199254740993.01",
                "paid_allocated_amount": "0.02",
                "purchase_invoice_allocated_amount": "7.30"
            })
        );
    }
}
