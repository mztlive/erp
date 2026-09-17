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

mod approval;
mod basis;
mod center;
mod change;
mod list;

pub use approval::*;
pub use basis::*;
pub use center::*;
pub use change::*;
pub use list::*;

#[cfg(test)]
mod wire_tests {
    use std::str::FromStr;

    use super::*;

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

    #[test]
    fn history_page_default_is_closed_cursor() {
        let page = DocumentApprovalHistoryPageView::default();
        assert_eq!(page.next_cursor, None);
        assert!(!page.has_more);
    }

    #[test]
    fn definition_view_builder_keeps_identity_and_version() {
        let view = DocumentApprovalDefinitionView::new("def-1".to_string(), "采购审批".to_string())
            .with_version(3)
            .with_nodes(vec![DocumentApprovalNodeView {
                key: "node-1".to_string(),
                name: "节点一".to_string(),
            }]);
        assert_eq!(view.id, "def-1");
        assert_eq!(view.name, "采购审批");
        assert_eq!(view.version, 3);
        assert_eq!(view.nodes.len(), 1);
    }

    #[test]
    fn instance_and_history_builders_preserve_mandatory_fields() {
        let instance = DocumentApprovalInstanceView::new("inst-1".to_string(), "RUNNING".to_string())
            .with_current_round_no(2)
            .with_process_version(Some(4));
        assert_eq!(instance.id, "inst-1");
        assert_eq!(instance.current_round_no, 2);
        assert_eq!(instance.process_version, Some(4));
        let item = DocumentApprovalHistoryItemView::new(
            "exec-1".to_string(),
            "node-1".to_string(),
            "节点一".to_string(),
            "APPROVED".to_string(),
        )
        .with_round_no(2)
        .with_execution_no(3);
        assert_eq!(item.execution_id, "exec-1");
        assert_eq!((item.round_no, item.execution_no), (2, 3));
        assert_eq!(item.result, "APPROVED");
    }

    #[test]
    fn creation_basis_builders_preserve_mandatory_fields() {
        let line = CreationBasisLineView::new(
            "line-1".to_string(),
            "rev-line-1".to_string(),
            "supplier-1".to_string(),
            "10".to_string(),
            "4".to_string(),
            "6".to_string(),
            "6".to_string(),
            "6".to_string(),
            "11.30".to_string(),
            "0.13".to_string(),
            "2026-09-01".to_string(),
            "2026-09-05".to_string(),
            "67.80".to_string(),
        )
        .with_sales_line_no(1);
        assert_eq!(line.sales_order_line_id, "line-1");
        assert_eq!(line.max_create_quantity, "6");
        let basis = CreationBasisView::new(
            "wi-1".to_string(),
            "basis-1".to_string(),
            "so-1".to_string(),
            "SO-1".to_string(),
            "测试客户".to_string(),
            "rev-1".to_string(),
            "supplier-1".to_string(),
            "供应商".to_string(),
            "PHYSICAL".to_string(),
            "WAREHOUSE".to_string(),
            "NET-30".to_string(),
            "67.80".to_string(),
        )
        .with_lines(vec![line]);
        assert_eq!(basis.work_item_id, "wi-1");
        assert_eq!(basis.lines.len(), 1);
        assert_eq!(basis.estimated_gross, "67.80");
    }
}
