//! 从完整不可变采购修订投影预算输入；费用分配不依赖当前组的局部快照。

use erp_procurement::entity::purchase_order::{
    PurchaseExpenseCategory, PurchaseLineType, PurchaseOrderRevisionLine,
};
use erp_supply::api_order::{ApiOrderExecution, for_purchase};
use erp_supply::entity::execution_budget::{
    ApprovedExecutionBudget, ApprovedExpense, ApprovedGoods, BudgetGroup,
};
use persistence_core::NoTransaction;

use super::SupplierApiOrderProcess;
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    pub(super) async fn approved_budget(
        &self,
        execution: &ApiOrderExecution,
        lines: &[PurchaseOrderRevisionLine],
    ) -> Result<ApprovedExecutionBudget> {
        let executions = for_purchase(
            &self.db,
            &execution.purchase_order_id,
            Some(&execution.purchase_order_revision_id),
            &mut NoTransaction,
        )
        .await?;
        let groups = executions
            .into_iter()
            .filter(|group| group.superseded_by_purchase_revision_id.is_none())
            .map(|group| BudgetGroup {
                execution_id: group.base.id,
                connection_id: group.connection_id,
                lines: group
                    .source_lines
                    .into_iter()
                    .map(|line| (line.purchase_order_revision_line_id, line.quantity))
                    .collect(),
            })
            .collect::<Vec<_>>();

        let goods = lines
            .iter()
            .filter(|line| line.line_type == PurchaseLineType::ItemService)
            .filter(|line| line.quantity.is_some_and(|quantity| !quantity.to_decimal().is_zero()))
            .map(approved_goods)
            .collect::<Result<Vec<_>>>()?;
        let expenses = lines
            .iter()
            .filter(|line| line.line_type == PurchaseLineType::LogisticsFee)
            .map(approved_expense)
            .collect::<Result<Vec<_>>>()?;

        Ok(ApprovedExecutionBudget::freeze(
            &execution.purchase_order_revision_id,
            &execution.base.id,
            &goods,
            &expenses,
            &groups,
        )?)
    }
}

fn approved_goods(line: &PurchaseOrderRevisionLine) -> Result<ApprovedGoods> {
    Ok(ApprovedGoods {
        purchase_line_id: line.base.id.clone(),
        sales_line_id: line.sales_order_line_id.as_ref().map(ToString::to_string),
        quantity: line.quantity.ok_or_else(|| Error::BusinessLogicError("采购批准数量缺失".into()))?,
        amount: line.gross_amount,
    })
}

fn approved_expense(line: &PurchaseOrderRevisionLine) -> Result<ApprovedExpense> {
    let expense =
        line.expense.as_ref().ok_or_else(|| Error::BusinessLogicError("采购批准费用缺少来源明细".into()))?;
    Ok(ApprovedExpense {
        purchase_line_id: line.base.id.clone(),
        expense_line_id: expense.expense_line_id.clone(),
        sales_line_id: expense.sales_order_line_id.as_ref().map(ToString::to_string),
        is_freight: matches!(
            expense.category,
            PurchaseExpenseCategory::Logistics | PurchaseExpenseCategory::Delivery
        ),
        amount: line.gross_amount,
    })
}
