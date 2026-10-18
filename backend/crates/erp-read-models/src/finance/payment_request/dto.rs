//! 付款申请的脱敏整单与当前事实投影。

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_finance::entity::payable::supplier_credit::SupplierCreditUse;
use erp_finance::entity::payable::{PaymentRequestAllocation, PurchasePaymentRequest};
use erp_party::PartyBankAccountView;
use erp_procurement::entity::purchase_order::{PurchaseLineType, PurchaseOrderRevisionLine};
use serde::Serialize;
use serde_json::Value;

use crate::finance::dto::DocumentApprovalView;

/// 当前余额与履约事实独立于提交时快照返回。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentRequestCurrentLine {
    pub purchase_order_id: String,
    pub payable_account_id: String,
    pub purchase_revision_id: Option<String>,
    pub status: String,
    pub receiving_progress: String,
    pub current_gross_total: Amount,
    pub current_settled_total: Amount,
    pub current_open_total: Amount,
    pub current_reserved_total: Amount,
    pub current_available_total: Amount,
    pub source_changed: bool,
    pub payable_entries: Vec<PaymentRequestPayableEntryView>,
}

/// 当前来源账户的增加分录及有效可核销余额。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentRequestPayableEntryView {
    pub id: String,
    pub amount: Amount,
    pub settled_total: Amount,
    pub offset_total: Amount,
    pub open_total: Amount,
}

/// 全部采购来源获准或正式任务获准后交付的整单视图。
#[derive(Debug, Clone, Serialize)]
pub struct PurchasePaymentRequestView {
    #[serde(flatten)]
    pub request: PurchasePaymentRequest,
    pub supplier_name: String,
    pub created_by_name: Option<String>,
    pub purchase_previews: Vec<Value>,
    pub sales_previews: Vec<Value>,
    pub current_lines: Vec<PaymentRequestCurrentLine>,
    pub approval: DocumentApprovalView,
    pub allowed_actions: Vec<String>,
    pub attachments: Vec<PaymentRequestAttachmentView>,
    pub payments: Vec<PaymentRequestPaymentView>,
    pub credit_uses: Vec<SupplierCreditUse>,
}

/// 本申请形成的实际付款与有效冲正金额，回单沿本申请资格读取。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentRequestPaymentView {
    pub payment_id: String,
    pub payment_no: String,
    pub paid_at: Instant,
    pub amount: Amount,
    pub reversed_amount: Amount,
    pub effective_amount: Amount,
    pub allocations: Vec<PaymentRequestAllocation>,
    pub receipt: Option<PaymentRequestAttachmentView>,
}

/// 精确历史提交的原审核材料，不包含当前操作资格或后续付款进度。
#[derive(Debug, Clone, Serialize)]
pub struct PurchasePaymentRequestSubmissionView {
    #[serde(flatten)]
    pub request: PurchasePaymentRequest,
    pub purchase_previews: Vec<Value>,
    pub sales_previews: Vec<Value>,
    pub attachments: Vec<PaymentRequestAttachmentView>,
}

/// 本申请允许目录的安全文件元数据。
#[derive(Debug, Clone, Serialize)]
pub struct PaymentRequestAttachmentView {
    pub file_asset_id: String,
    pub file_name: String,
    pub content_type: String,
    pub byte_size: u64,
}

/// 已证明申请动作和采购正式来源的候选；收款账户仅为候选且全部脱敏。
#[derive(Debug, Clone, Serialize)]
pub struct PurchasePaymentRequestCandidate {
    pub purchase_summary: String,
    pub prepayment_gap: Option<Amount>,
    pub purchase_order_id: String,
    pub purchase_no: String,
    pub purchase_revision_id: String,
    pub supplier_id: String,
    pub supplier_name: String,
    pub payer_party_id: String,
    pub payer_party_name: String,
    pub payable_account_id: String,
    pub gross_total: Amount,
    pub settled_total: Amount,
    pub open_total: Amount,
    pub reserved_total: Amount,
    pub available_total: Amount,
    pub payment_terms: Value,
    pub bank_accounts: Vec<PartyBankAccountView>,
}

impl PurchasePaymentRequestCandidate {
    /// 从当前生效采购版本的前三行生成商品或费用摘要。
    ///
    /// # 参数
    /// * `lines` - 当前生效版本的采购明细。
    /// # 返回
    /// 商品行显示名称，费用行显示类别和说明；旧费用行缺少内容时显示“费用”。
    /// # 错误
    /// 不返回错误；无明细时返回空摘要。
    pub(super) fn summary_from_lines(lines: &[PurchaseOrderRevisionLine]) -> String {
        lines
            .iter()
            .take(3)
            .filter_map(|line| match &line.expense {
                Some(expense) => Some(format!("{}：{}", expense.category.label(), expense.description)),
                None if line.line_type == PurchaseLineType::LogisticsFee => Some("费用".into()),
                None => line.product_name_snapshot.clone(),
            })
            .collect::<Vec<_>>()
            .join("、")
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::ids::PurchaseOrderRevisionId;
    use erp_procurement::entity::purchase_order::{PurchaseExpenseCategory, PurchaseExpenseDetail};

    use super::*;

    fn line() -> PurchaseOrderRevisionLine {
        PurchaseOrderRevisionLine {
            base: BaseModel::fake(),
            purchase_order_revision_id: PurchaseOrderRevisionId::new("revision"),
            line_no: 1,
            line_type: PurchaseLineType::LogisticsFee,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            supplier_offering_source: None,
            delivery_allocations: vec![],
            product_name_snapshot: None,
            specification_snapshot: None,
            quantity: None,
            base_unit_code: None,
            unit_cost_gross: None,
            gross_amount: "300".parse().unwrap(),
            net_amount: "300".parse().unwrap(),
            tax_amount: Amount::zero(),
            input_tax_rate: None,
            expected_delivery_date: None,
            sales_order_line_id: None,
            sales_order_revision_line_id: None,
            allocated_quantity: None,
            expense: None,
        }
    }

    #[test]
    fn candidate_summary_includes_expense_category_and_description_without_product_name() {
        let mut logistics = line();
        logistics.expense = Some(
            PurchaseExpenseDetail::new(
                "expense-1".into(),
                PurchaseExpenseCategory::Logistics,
                "货拉拉".into(),
                None,
            )
            .unwrap(),
        );
        let mut printing = line();
        printing.expense = Some(
            PurchaseExpenseDetail::new(
                "expense-2".into(),
                PurchaseExpenseCategory::Printing,
                "礼盒印刷".into(),
                None,
            )
            .unwrap(),
        );
        assert_eq!(
            PurchasePaymentRequestCandidate::summary_from_lines(&[logistics, printing]),
            "物流：货拉拉、印刷：礼盒印刷"
        );
    }

    #[test]
    fn candidate_summary_preserves_product_names_and_three_line_limit() {
        let lines = ["茶叶", "礼盒", "瓜子", "食用油"]
            .into_iter()
            .map(|name| PurchaseOrderRevisionLine {
                line_type: PurchaseLineType::ItemService,
                product_name_snapshot: Some(name.into()),
                ..line()
            })
            .collect::<Vec<_>>();
        assert_eq!(PurchasePaymentRequestCandidate::summary_from_lines(&lines), "茶叶、礼盒、瓜子");
    }

    #[test]
    fn candidate_summary_handles_empty_and_legacy_expense_lines() {
        assert_eq!(PurchasePaymentRequestCandidate::summary_from_lines(&[]), "");
        assert_eq!(PurchasePaymentRequestCandidate::summary_from_lines(&[line()]), "费用");
        let unnamed = PurchaseOrderRevisionLine { line_type: PurchaseLineType::ItemService, ..line() };
        assert_eq!(PurchasePaymentRequestCandidate::summary_from_lines(&[unnamed]), "");
    }
}
