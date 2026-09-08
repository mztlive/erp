//! Receivable detail projections combining finance facts with workflow and sales context.

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_finance::dto::receivable::{
    ReceiptAllocationView, ReceivableEntryView, ReceivableInvoiceFactView, ReceivableReceiptFactView,
};
use erp_finance::entity::receivable::{CustomerReceiptStatus, ReceivableAccountStatus};
use serde::Serialize;

/// 应收往来子账响应视图（W11 应收台账行 + 详情）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceivableAccountView {
    /// 实体主键。
    pub id: String,
    /// 来源销售单。
    pub sales_order_id: String,
    /// 销售单业务单号。
    pub sales_order_no: String,
    /// 当前不可变销售版本号。
    pub sales_order_revision_no: u32,
    /// 当前销售版本生效时间（秒级时间戳）。
    pub sales_order_snapshot_at: u64,
    /// 往来子账序号。
    pub account_seq: u32,
    /// 本子账开始适用的销售版本。
    pub source_sales_order_revision_id: String,
    /// 当前销售版本。
    pub current_sales_order_revision_id: String,
    /// 企业客户经营归属。
    pub customer_id: String,
    /// 当前销售版本冻结的客户名称。
    pub customer_name: String,
    /// 收款和开票往来主体。
    pub counterparty_party_id: String,
    /// 当前销售版本冻结的收款/开票往来主体名称；缺失时为空并阻断登记动作。
    pub counterparty_party_name: Option<String>,
    /// 含税应收总额。
    pub gross_total: Amount,
    /// 已核销含税总额。
    pub settled_total: Amount,
    /// 剩余开放含税余额。
    pub open_total: Amount,
    /// 可开票含税总额。
    pub invoiceable_total: Amount,
    /// 净已开含税总额。
    pub invoiced_total: Amount,
    /// 剩余可开票含税额度。
    pub open_invoiceable_total: Amount,
    /// 子账状态。
    pub status: ReceivableAccountStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 账户领域版本；客户端不得自行递增或与其它版本互换。
    pub account_domain_version: String,
    /// 当前账户关联的正式回款事实。
    pub receipt_facts: Vec<ReceivableReceiptFactView>,
    /// 当前账户关联的正式销项发票事实。
    pub invoice_facts: Vec<ReceivableInvoiceFactView>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 应收分录（含抵销合计）。
    pub entries: Vec<ReceivableEntryView>,
}

/// 客户回款单响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerReceiptView {
    /// 实体主键。
    pub id: String,
    /// 回款单号。
    pub receipt_no: String,
    /// 回款单状态。
    pub status: CustomerReceiptStatus,
    /// 实际付款往来主体。
    pub counterparty_party_id: String,
    /// 可选经营归属提示。
    pub customer_id: Option<String>,
    /// 实际到账时间（秒级时间戳）。
    pub received_at: Instant,
    /// 含税到账金额。
    pub amount: Amount,
    /// 银行流水或凭证引用。
    pub bank_reference: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 已核销合计（净）。
    pub allocated_total: Amount,
    /// 未分配余额。
    pub unallocated_amount: Amount,
    /// 核销分配行。
    pub allocations: Vec<ReceiptAllocationView>,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
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
    /// 当前审批人。
    pub current_assignee: Option<String>,
    /// 最近驳回原因。
    pub latest_rejection: Option<String>,
}

/// 有界历史项。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryItemView {
    /// 执行主键。
    pub execution_id: String,
    /// 轮次。
    pub round_no: u32,
    /// 节点键。
    pub node_key: String,
    /// 结束结果。
    pub result: String,
}

/// 完整历史分页。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentApprovalHistoryPageView {
    /// 下一页游标。
    pub next_cursor: Option<String>,
    /// 是否还有更多。
    pub has_more: bool,
}
