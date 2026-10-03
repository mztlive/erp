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
    /// 原变更原因；提交不会变更该事实。
    pub reason: String,
    /// 当前冻结目标或初始基准的付款条件。
    pub payment_term_code: String,
    /// 与 `lines` 同序的冻结行主键，仅用于界面稳定身份，不作为提交字段。
    pub line_keys: Vec<String>,
    /// 原目标全部行及冻结来源关联；金额、数量和税率均为字符串。
    pub lines: Vec<SavePurchaseOrderLine>,
}

impl PurchaseChangeDraftView {
    /// 从初始基准版本恢复采购变更编辑快照。
    ///
    /// # 参数
    /// * `change` - 原变更草稿。
    /// * `revision` - 原基准版本。
    /// * `lines` - 完整且按稳定顺序排列的原版本行。
    /// # 返回
    /// 返回包含全部冻结关联和稳定行键的快照。
    /// # 错误
    /// 状态或固定来源不一致、缺少明细时拒绝。
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
        })
    }

    /// 从撤回前最后一次冻结提交恢复完整采购变更目标。
    ///
    /// # 参数
    /// * `change` - 保留当前提交指针的原变更草稿。
    /// * `submission` - 当前冻结目标提交。
    /// * `lines` - 完整且按稳定顺序排列的目标提交行。
    /// # 返回
    /// 返回最后提交的付款条件和全部目标行，不以基准覆盖已修改内容。
    /// # 错误
    /// 状态、提交指针、固定来源或明细不一致时拒绝。
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
        })
    }
}

impl From<&PurchaseOrderRevisionLine> for SavePurchaseOrderLine {
    fn from(line: &PurchaseOrderRevisionLine) -> Self {
        Self {
            line_type: line.line_type,
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
        }
    }
}

impl From<&PurchaseChangeSubmissionLine> for SavePurchaseOrderLine {
    fn from(line: &PurchaseChangeSubmissionLine) -> Self {
        Self {
            line_type: line.line_type,
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
        }
    }
}

#[cfg(test)]
mod tests;
