//! 应收详情投影：组合财务事实与审批、销售上下文。

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_finance::dto::receivable::{
    ReceiptAllocationView, ReceivableEntryView, ReceivableInvoiceFactView, ReceivableReceiptFactView,
};
use erp_finance::entity::receivable::{
    CustomerReceiptStatus, PendingReceiptAllocation, ReceivableAccountStatus,
};
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
    /// 审批提交时冻结的拟核销分配，独立于已过账金额。
    pub pending_allocations: Vec<PendingReceiptAllocation>,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
}

/// 单据审批展示合同；实例和历史沿用共用运行态字段。
pub use crate::approval_runtime::{
    DocumentApprovalDefinitionView, DocumentApprovalHistoryItemView, DocumentApprovalHistoryPageView,
    DocumentApprovalInstanceView, DocumentApprovalNodeView, DocumentApprovalView,
};

#[cfg(test)]
mod wire_tests {
    use super::*;

    #[test]
    fn history_page_default_is_closed_cursor() {
        let page = DocumentApprovalHistoryPageView::default();
        assert!(page.next_cursor.is_none());
        assert!(!page.has_more);
    }

    #[test]
    fn definition_view_builder_keeps_identity_and_version() {
        let view =
            DocumentApprovalDefinitionView::new("def-1".to_string(), "费用审批".to_string()).with_version(3);
        assert_eq!(view.id, "def-1");
        assert_eq!(view.name, "费用审批");
        assert_eq!(view.version, 3);
        assert!(view.nodes.is_empty());
    }

    #[test]
    fn instance_and_history_builders_preserve_mandatory_fields() {
        let instance = DocumentApprovalInstanceView::new("inst-1".to_string(), "RUNNING".to_string());
        assert_eq!(instance.id, "inst-1");
        assert_eq!(instance.status, "RUNNING");
        let item = DocumentApprovalHistoryItemView::new(
            "exec-1".to_string(),
            "node-1".to_string(),
            "APPROVED".to_string(),
        )
        .with_round_no(2);
        assert_eq!(item.execution_id, "exec-1");
        assert_eq!(item.round_no, 2);
    }
}
