//! 采购变更编辑快照；恢复完整冻结目标，行键只用于稳定界面身份。

use serde::{Deserialize, Serialize};

use super::SavePurchaseOrderLine;
use crate::Result;
use crate::entity::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseChangeSubmissionLine, PurchaseLineType,
    PurchaseOrderRevision, PurchaseOrderRevisionLine,
};

/// 原采购变更单可编辑的完整目标快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseChangeDraftView {
    /// 原变更单当前乐观锁版本；提交时传为 `expected_lock_version`。
    pub version: u64,
    /// 原变更原因；提交时可一并修改。
    pub reason: String,
    /// 当前冻结目标或初始基准的付款条件。
    pub payment_term_code: String,
    /// 与 `lines` 同序的冻结行主键，仅用于界面稳定身份，不作为提交字段。
    pub line_keys: Vec<String>,
    /// 原目标全部行及冻结来源关联；金额、数量和税率均为字符串。
    pub lines: Vec<SavePurchaseOrderLine>,
    /// 当前生效版本的全部行，用于对照改动；撤回后恢复的目标与此不同。
    #[serde(default)]
    pub base_lines: Vec<SavePurchaseOrderLine>,
    /// 当前生效版本含税总额。
    #[serde(default)]
    pub base_gross_amount: String,
}

impl PurchaseChangeDraftView {
    /// 首次编辑采购变更时，从冻结的基准采购版本恢复付款条件及按稳定顺序保存的全部明细和行键。
    /// 草稿状态、基准身份或明细关联不一致时拒绝。
    pub fn from_base(
        change: &PurchaseChangeOrder,
        revision: &PurchaseOrderRevision,
        lines: &[PurchaseOrderRevisionLine],
    ) -> Result<Self> {
        change.ensure_draft_base(revision, lines)?;
        Ok(Self {
            version: change.base.version,
            reason: change.reason.clone(),
            payment_term_code: revision.payment_term_snapshot.payment_term_code.clone(),
            line_keys: lines.iter().map(|line| line.base.id.clone()).collect(),
            lines: lines.iter().map(SavePurchaseOrderLine::from).collect(),
            base_lines: Vec::new(),
            base_gross_amount: String::new(),
        })
    }

    /// 重新打开已有目标的采购变更时，从当前提交指针指向的编辑副本或冻结提交恢复完整明细和付款条件。
    /// 草稿资格、目标身份或明细不一致时拒绝，不回退到基准版本。
    pub fn from_submission(
        change: &PurchaseChangeOrder,
        submission: &PurchaseChangeSubmission,
        lines: &[PurchaseChangeSubmissionLine],
    ) -> Result<Self> {
        change.ensure_draft_target(submission, lines)?;
        Ok(Self {
            version: change.base.version,
            reason: change.reason.clone(),
            payment_term_code: submission.payment_term_snapshot.payment_term_code.clone(),
            line_keys: lines.iter().map(|line| line.base.id.clone()).collect(),
            lines: lines.iter().map(SavePurchaseOrderLine::from).collect(),
            base_lines: Vec::new(),
            base_gross_amount: String::new(),
        })
    }
}

impl PurchaseChangeDraftView {
    /// 将基准采购生效版本的明细和含税总额附到可编辑目标旁，供界面对照变更，不覆盖目标内容。
    pub fn with_base(
        mut self,
        revision: &PurchaseOrderRevision,
        lines: &[PurchaseOrderRevisionLine],
    ) -> Self {
        self.base_lines = lines.iter().map(SavePurchaseOrderLine::from).collect();
        self.base_gross_amount = revision.gross_amount.to_string();
        self
    }
}

impl From<&PurchaseOrderRevisionLine> for SavePurchaseOrderLine {
    // 由采购版本行投影可再次编辑的行载荷。
    //
    // 历史销售提交行固定为空；仅物流费用行携带含税金额。
    //
    // 返回字段已转为字符串的 `SavePurchaseOrderLine`。
    fn from(line: &PurchaseOrderRevisionLine) -> Self {
        Self {
            line_type: line.line_type,
            delivery_allocations: (!line.delivery_allocations.is_empty())
                .then(|| line.delivery_allocations.clone()),
            procurement_confirmation_line_id: line
                .procurement_confirmation_line_id
                .as_ref()
                .map(ToString::to_string),
            sku_id: line.sku_id.as_ref().map(ToString::to_string),
            sku_revision_id: line.sku_revision_id.as_ref().map(ToString::to_string),
            product_name: line.product_name_snapshot.clone(),
            specification: line.specification_snapshot.clone(),
            quantity: line.quantity.map(|value| value.to_string()),
            base_unit_code: line.base_unit_code.clone(),
            unit_cost_gross: line.unit_cost_gross.map(|value| value.to_string()),
            input_tax_rate: line.input_tax_rate.map(|value| value.to_string()),
            expected_delivery_date: line.expected_delivery_date.map(|value| value.to_string()),
            sales_order_line_id: line.sales_order_line_id.as_ref().map(ToString::to_string),
            sales_order_revision_line_id: line.sales_order_revision_line_id.as_ref().map(ToString::to_string),
            sales_order_submission_line_id: None,
            allocated_quantity: line.allocated_quantity.map(|value| value.to_string()),
            gross_amount: (line.line_type == PurchaseLineType::LogisticsFee)
                .then(|| line.gross_amount.to_string()),
            expense: line.expense.clone(),
        }
    }
}

impl From<&PurchaseChangeSubmissionLine> for SavePurchaseOrderLine {
    // 由采购变更提交行投影可再次编辑的行载荷。
    //
    // 保留历史销售提交行；仅物流费用行携带含税金额。
    //
    // 返回字段已转为字符串的 `SavePurchaseOrderLine`。
    fn from(line: &PurchaseChangeSubmissionLine) -> Self {
        Self {
            line_type: line.line_type,
            delivery_allocations: (!line.delivery_allocations.is_empty())
                .then(|| line.delivery_allocations.clone()),
            procurement_confirmation_line_id: line
                .procurement_confirmation_line_id
                .as_ref()
                .map(ToString::to_string),
            sku_id: line.sku_id.as_ref().map(ToString::to_string),
            sku_revision_id: line.sku_revision_id.as_ref().map(ToString::to_string),
            product_name: line.product_name_snapshot.clone(),
            specification: line.specification_snapshot.clone(),
            quantity: line.quantity.map(|value| value.to_string()),
            base_unit_code: line.base_unit_code.clone(),
            unit_cost_gross: line.unit_cost_gross.map(|value| value.to_string()),
            input_tax_rate: line.input_tax_rate.map(|value| value.to_string()),
            expected_delivery_date: line.expected_delivery_date.map(|value| value.to_string()),
            sales_order_line_id: line.sales_order_line_id.as_ref().map(ToString::to_string),
            sales_order_revision_line_id: line.sales_order_revision_line_id.as_ref().map(ToString::to_string),
            sales_order_submission_line_id: line
                .sales_order_submission_line_id
                .as_ref()
                .map(ToString::to_string),
            allocated_quantity: line.allocated_quantity.map(|value| value.to_string()),
            gross_amount: (line.line_type == PurchaseLineType::LogisticsFee)
                .then(|| line.gross_amount.to_string()),
            expense: line.expense.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
