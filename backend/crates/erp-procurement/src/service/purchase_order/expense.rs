//! 费用采购单草稿行构造与费用行发生事实查找（§7.5.1）。

use erp_core::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId};
use erp_core::money::{Amount, fee_amounts};
use id_generator::next_id;

use crate::dto::purchase_order::{CreateExpenseLineRequest, EXPENSE_LINE_LIMIT};
use crate::entity::purchase_order::{
    PurchaseExpenseDetail, PurchaseLineType, PurchaseOrderRevisionLine, PurchaseOrderSubmissionLine,
    PurchaseOrderSubmissionLineData, zero_amount,
};
use crate::{Error, Result};

/// 已构造的费用草稿行及表头汇总。
pub struct ExpenseDraftLines {
    /// 草稿行。
    pub lines: Vec<PurchaseOrderSubmissionLine>,
    /// 含税、不含税、税额汇总。
    pub totals: (Amount, Amount, Amount),
}

/// 由请求构造费用采购单草稿行；每行生成新的稳定费用行身份。
///
/// # 参数
/// * `submission_id` - 所属草稿提交
/// * `requested` - 请求费用行
///
/// # 返回
/// 返回草稿行与汇总金额；金额按含税金额与税率拆分。
///
/// # 错误
/// 行数为 0 或超限、金额不为正、税率越界或费用内容非法时返回 `ValidationError`。
pub fn build_expense_draft_lines(
    submission_id: &PurchaseOrderSubmissionId,
    requested: &[CreateExpenseLineRequest],
) -> Result<ExpenseDraftLines> {
    if requested.is_empty() || requested.len() > EXPENSE_LINE_LIMIT {
        return Err(Error::ValidationError("费用行须为 1–50 行".into()));
    }
    let (mut gross_total, mut net_total, mut tax_total) = (zero_amount(), zero_amount(), zero_amount());
    let mut lines = Vec::with_capacity(requested.len());
    for (index, line) in requested.iter().enumerate() {
        let built = expense_line(submission_id, index, line)?;
        gross_total = gross_total.checked_add(built.gross_amount);
        net_total = net_total.checked_add(built.net_amount);
        tax_total = tax_total.checked_add(built.tax_amount);
        lines.push(built);
    }
    Ok(ExpenseDraftLines { lines, totals: (gross_total, net_total, tax_total) })
}

fn expense_line(
    submission_id: &PurchaseOrderSubmissionId,
    index: usize,
    line: &CreateExpenseLineRequest,
) -> Result<PurchaseOrderSubmissionLine> {
    if line.gross_amount.to_decimal() <= rust_decimal::Decimal::ZERO {
        return Err(Error::ValidationError(format!("第 {} 行费用金额必须大于 0", index + 1)));
    }
    let rate = line.input_tax_rate.to_decimal();
    if rate.is_sign_negative() || rate >= rust_decimal::Decimal::ONE {
        return Err(Error::ValidationError(format!("第 {} 行税率必须在 0 到 1 之间", index + 1)));
    }
    let expense = PurchaseExpenseDetail::new(
        next_id(),
        line.category,
        line.description.clone(),
        line.sales_order_line_id.clone(),
    )
    .map_err(|error| Error::ValidationError(format!("第 {} 行{error}", index + 1)))?;
    let (gross, net, tax) = fee_amounts(line.gross_amount, line.input_tax_rate);
    let line_no = u32::try_from(index + 1).map_err(|_| Error::ValidationError("费用行数溢出".into()))?;
    PurchaseOrderSubmissionLine::new(
        PurchaseOrderSubmissionLineId::new(next_id()),
        PurchaseOrderSubmissionLineData {
            purchase_order_submission_id: submission_id.clone(),
            line_no,
            line_type: PurchaseLineType::LogisticsFee,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            supplier_offering_source: None,
            delivery_allocations: Vec::new(),
            product_name_snapshot: None,
            specification_snapshot: None,
            quantity: None,
            base_unit_code: None,
            unit_cost_gross: None,
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(line.input_tax_rate),
            expected_delivery_date: None,
            sales_order_line_id: None,
            sales_order_revision_line_id: None,
            sales_order_submission_line_id: None,
            allocated_quantity: None,
            expense: Some(expense),
        },
    )
    .map_err(Into::into)
}

/// 在采购当前版本中查找费用行。
///
/// # 参数
/// * `lines` - 采购当前生效版本行
/// * `expense_line_id` - 稳定费用行身份
///
/// # 返回
/// 返回该费用行。
///
/// # 错误
/// 当前版本不存在该费用行时返回 `NotFound`。
pub fn find_expense_line<'a>(
    lines: &'a [PurchaseOrderRevisionLine],
    expense_line_id: &str,
) -> Result<&'a PurchaseOrderRevisionLine> {
    lines
        .iter()
        .find(|line| line.expense.as_ref().is_some_and(|expense| expense.expense_line_id == expense_line_id))
        .ok_or_else(|| Error::NotFound("费用行不存在或已不在当前生效版本".into()))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::SalesOrderLineId;
    use erp_core::money::Rate;

    use super::*;
    use crate::entity::purchase_order::PurchaseExpenseCategory;

    fn request(gross: &str, rate: &str) -> CreateExpenseLineRequest {
        CreateExpenseLineRequest {
            category: PurchaseExpenseCategory::Logistics,
            description: "第三方物流".into(),
            gross_amount: Amount::from_str(gross).unwrap(),
            input_tax_rate: Rate::from_str(rate).unwrap(),
            sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
        }
    }

    #[test]
    fn builds_fee_lines_with_stable_ids_and_totals() {
        let id = PurchaseOrderSubmissionId::new("sub-1");
        let built =
            build_expense_draft_lines(&id, &[request("106.00", "0.06"), request("20.00", "0")]).unwrap();
        assert_eq!(built.lines.len(), 2);
        let first = &built.lines[0];
        assert_eq!(first.line_type, PurchaseLineType::LogisticsFee);
        assert_eq!(first.line_no, 1);
        assert_eq!(first.net_amount.to_string(), "99.64");
        let expense = first.expense.as_ref().unwrap();
        assert_eq!(expense.sales_order_line_id, Some(SalesOrderLineId::new("sol-1")));
        assert_ne!(expense.expense_line_id, built.lines[1].expense.as_ref().unwrap().expense_line_id);
        assert_eq!(built.totals.0, Amount::from_str("126.00").unwrap());
        assert_eq!(built.totals.1.checked_add(built.totals.2), built.totals.0);
    }

    #[test]
    fn rejects_empty_zero_or_invalid_rate_lines() {
        let id = PurchaseOrderSubmissionId::new("sub-1");
        assert!(build_expense_draft_lines(&id, &[]).is_err());
        assert!(build_expense_draft_lines(&id, &[request("0", "0")]).is_err());
        assert!(build_expense_draft_lines(&id, &[request("1.00", "1")]).is_err());
        let mut blank = request("1.00", "0");
        blank.description = " ".into();
        assert!(build_expense_draft_lines(&id, &[blank]).is_err());
        let many = vec![request("1.00", "0"); EXPENSE_LINE_LIMIT + 1];
        assert!(build_expense_draft_lines(&id, &many).is_err());
    }
}
