//! 销售变更详情与审批只读协议；字段保持原 HTTP 合同。

use erp_procurement::entity::purchase_order::{PurchaseSourceResolution, SourceImpactReason};
use erp_sales::entity::sales_review::SalesChangeType;
use serde::Serialize;

/// 销售变更单详情视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesChangeOrderDetailView {
    /// 实体主键。
    pub id: String,
    /// 原销售单。
    pub sales_order_id: String,
    /// 发起时当前版本。
    pub base_revision_id: String,
    /// 变更类型。
    pub change_type: SalesChangeType,
    /// 变更原因。
    pub reason: String,
    /// 变更状态。
    pub status: erp_sales::entity::sales_review::SalesChangeOrderStatus,
    /// 当前不可变目标提交。
    pub current_submission_id: Option<String>,
    /// 目标完整内容指纹。
    pub target_content_hash: Option<String>,
    /// 当前不可变提交来源工作副本的完整内容身份。
    pub submitted_working_copy_content_hash: Option<String>,
    /// 生效后生成的新销售版本。
    pub effective_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 统一只读审批结构。客户端不得据此选择定义或审批人。
    pub approval: DocumentApprovalView,
    /// 本变更生效后受影响采购的处理进度。
    pub procurement: SalesChangeProcurementView,
}

/// 销售变更触发的采购依据更新进度（第一期 §6.3“销售变更对采购的影响”第 11 条）。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct SalesChangeProcurementView {
    /// 受影响采购张数。
    pub total: usize,
    /// 尚未处理的张数。
    pub open: usize,
    /// 全部受影响采购已处理完成，由系统按事实判定。
    pub completed: bool,
    /// 变更尚在审批中，按提交内容预判；生效时按当时事实重算。
    pub predicted: bool,
    /// 逐张处理情况；无采购列表权限时为空。
    pub items: Vec<SalesChangeProcurementItemView>,
}

/// 一张受影响采购的处理情况。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SalesChangeProcurementItemView {
    /// 采购单 ID。
    pub purchase_order_id: String,
    /// 采购单号；采购不可读时为空。
    pub purchase_no: Option<String>,
    /// 供应商当前法定名称；缺失时为空。
    pub supplier_name: Option<String>,
    /// 采购负责人姓名；缺失时为空。
    pub owner_name: Option<String>,
    /// 是否仍待处理。
    pub open: bool,
    /// 草稿已按新销售版本修改，等待重新提交审批。
    pub awaiting_submission: bool,
    /// 处理方式代码。
    pub resolution: Option<PurchaseSourceResolution>,
    /// 处理方式中文名。
    pub resolution_label: Option<String>,
    /// 维持原采购的原因。
    pub maintain_note: Option<String>,
    /// 最近一次判定的影响原因。
    pub reasons: Vec<SourceImpactReason>,
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
            DocumentApprovalDefinitionView::new("def-1".to_string(), "复核审批".to_string()).with_version(2);
        assert_eq!(view.id, "def-1");
        assert_eq!(view.name, "复核审批");
        assert_eq!(view.version, 2);
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
