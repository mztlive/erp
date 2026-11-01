//! 运费另计跟进投影：采购行运费另计后，是否已追加物流费用采购或标记供应商不收运费。
//!
//! 同一销售单存在该供应商未作废的费用采购单，且其中有金额为正的物流类费用行归属该
//! 销售明细或不归属具体明细时视为已处理；采购单标记"供应商不收运费"同样视为已处理。

use std::collections::{HashMap, HashSet};

use erp_core::ids::{PurchaseOrderRevisionId, PurchaseOrderSubmissionId, SupplierAccountId};
use erp_procurement::entity::purchase_order::{
    PurchaseExpenseCategory, PurchaseExpenseDetail, PurchaseOrder, PurchaseType,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_procurement::repository::purchase_expense::active_orders_by_sales_orders;
use mongodb::Database;
use persistence_core::Executor;
use serde::Serialize;

use super::repository::supplier_names::current_legal_names_by_account_ids;
use crate::Result;

/// 一条待追加运费的采购商品。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FreightFollowUpView {
    /// 来源销售单。
    pub sales_order_id: String,
    /// 采购单。
    pub purchase_order_id: String,
    /// 采购单号；草稿为空。
    pub purchase_no: String,
    /// 供应商。
    pub supplier_id: String,
    /// 供应商名称。
    pub supplier_name: Option<String>,
    /// 稳定销售行。
    pub sales_order_line_id: String,
    /// 商品名称。
    pub product_name: Option<String>,
}

/// 一条运费另计的采购商品事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreightLineFact {
    /// 来源销售单。
    pub sales_order_id: String,
    /// 采购单。
    pub purchase_order_id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 供应商。
    pub supplier_id: String,
    /// 稳定销售行。
    pub sales_order_line_id: String,
    /// 商品名称。
    pub product_name: Option<String>,
}

/// 一条物流类费用行事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogisticsExpenseFact {
    /// 来源销售单。
    pub sales_order_id: String,
    /// 收款供应商。
    pub supplier_id: String,
    /// 归属销售明细；空表示整张销售单。
    pub sales_order_line_id: Option<String>,
}

/// 从运费另计采购商品中剔除已有物流费用采购覆盖的条目。
///
/// # 参数
/// * `freight_lines` - 未标记供应商不收运费的运费另计采购商品
/// * `expenses` - 未作废费用采购单中金额为正的物流类费用行
///
/// # 返回
/// 返回仍待追加运费的采购商品，保持输入顺序。
///
/// # 错误
/// 无。
pub fn pending_freight_lines(
    freight_lines: Vec<FreightLineFact>,
    expenses: &[LogisticsExpenseFact],
) -> Vec<FreightLineFact> {
    freight_lines
        .into_iter()
        .filter(|line| {
            !expenses.iter().any(|expense| {
                expense.sales_order_id == line.sales_order_id
                    && expense.supplier_id == line.supplier_id
                    && expense.sales_order_line_id.as_ref().is_none_or(|id| *id == line.sales_order_line_id)
            })
        })
        .collect()
}

/// 批量读取一组销售单仍待追加运费的采购商品。
///
/// # 参数
/// * `db` - 数据库
/// * `sales_order_ids` - 销售单
/// * `executor` - 执行器
///
/// # 返回
/// 以销售单主键索引的待跟进条目；没有待跟进的销售单不出现。
///
/// # 错误
/// 仓储读取失败时返回错误。
pub async fn freight_follow_ups(
    db: &Database,
    sales_order_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, Vec<FreightFollowUpView>>> {
    if sales_order_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let orders = active_orders_by_sales_orders(db, sales_order_ids, executor).await?;
    let (goods, expenses): (Vec<_>, Vec<_>) =
        orders.into_iter().partition(|order| order.purchase_type != PurchaseType::Expense);
    let goods =
        goods.into_iter().filter(|order| !order.unwaived_freight_lines().is_empty()).collect::<Vec<_>>();
    if goods.is_empty() {
        return Ok(HashMap::new());
    }
    let lines = current_lines(db, goods.iter().chain(expenses.iter()), executor).await?;
    let pending = pending_freight_lines(freight_facts(&goods, &lines), &logistics_facts(&expenses, &lines));
    let supplier_ids = pending
        .iter()
        .map(|line| SupplierAccountId::new(line.supplier_id.clone()))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let names = current_legal_names_by_account_ids(db, &supplier_ids, executor).await?;
    let mut grouped: HashMap<String, Vec<FreightFollowUpView>> = HashMap::new();
    for line in pending {
        grouped.entry(line.sales_order_id.clone()).or_default().push(FreightFollowUpView {
            supplier_name: names.get(&line.supplier_id).cloned(),
            sales_order_id: line.sales_order_id,
            purchase_order_id: line.purchase_order_id,
            purchase_no: line.purchase_no,
            supplier_id: line.supplier_id,
            sales_order_line_id: line.sales_order_line_id,
            product_name: line.product_name,
        });
    }
    Ok(grouped)
}

/// 采购单当前内容中的一行：商品名称或费用内容。
struct CurrentLine {
    sales_order_line_id: Option<String>,
    product_name: Option<String>,
    expense: Option<PurchaseExpenseDetail>,
    positive: bool,
}

/// 读取采购单当前内容：已生效取当前版本，否则取当前提交。
async fn current_lines<'a>(
    db: &Database,
    orders: impl Iterator<Item = &'a PurchaseOrder>,
    executor: &mut dyn Executor,
) -> Result<HashMap<String, Vec<CurrentLine>>> {
    let mut revision_owner = HashMap::new();
    let mut submission_owner = HashMap::new();
    for order in orders {
        if let Some(revision_id) = order.stable.current_revision_id.clone() {
            revision_owner.insert(revision_id, order.base.id.clone());
        } else if let Some(submission_id) = order.current_submission_id.clone() {
            submission_owner.insert(submission_id, order.base.id.clone());
        }
    }
    let revision_ids = revision_owner.keys().cloned().map(PurchaseOrderRevisionId::new).collect::<Vec<_>>();
    let submission_ids =
        submission_owner.keys().cloned().map(PurchaseOrderSubmissionId::new).collect::<Vec<_>>();
    let mut lines: HashMap<String, Vec<CurrentLine>> = HashMap::new();
    for line in db.purchase_order_revision_lines().find_lines_by_revision_ids(&revision_ids, executor).await?
    {
        if let Some(order_id) = revision_owner.get(line.purchase_order_revision_id.as_ref()) {
            lines.entry(order_id.clone()).or_default().push(CurrentLine {
                sales_order_line_id: line.sales_order_line_id.map(|id| id.to_string()),
                product_name: line.product_name_snapshot,
                positive: line.gross_amount.to_decimal() > rust_decimal::Decimal::ZERO,
                expense: line.expense,
            });
        }
    }
    for line in
        db.purchase_order_submission_lines().find_lines_by_submission_ids(&submission_ids, executor).await?
    {
        if let Some(order_id) = submission_owner.get(line.purchase_order_submission_id.as_ref()) {
            lines.entry(order_id.clone()).or_default().push(CurrentLine {
                sales_order_line_id: line.sales_order_line_id.map(|id| id.to_string()),
                product_name: line.product_name_snapshot,
                positive: line.gross_amount.to_decimal() > rust_decimal::Decimal::ZERO,
                expense: line.expense,
            });
        }
    }
    Ok(lines)
}

fn freight_facts(goods: &[PurchaseOrder], lines: &HashMap<String, Vec<CurrentLine>>) -> Vec<FreightLineFact> {
    let mut facts = Vec::new();
    for order in goods {
        for sales_line in order.unwaived_freight_lines() {
            let product_name = lines.get(&order.base.id).and_then(|rows| {
                rows.iter()
                    .find(|row| row.sales_order_line_id.as_deref() == Some(sales_line.as_ref()))
                    .and_then(|row| row.product_name.clone())
            });
            facts.push(FreightLineFact {
                sales_order_id: order.sales_order_id.to_string(),
                purchase_order_id: order.base.id.clone(),
                purchase_no: order.purchase_no.clone(),
                supplier_id: order.supplier_id.to_string(),
                sales_order_line_id: sales_line.to_string(),
                product_name,
            });
        }
    }
    facts
}

fn logistics_facts(
    expenses: &[PurchaseOrder],
    lines: &HashMap<String, Vec<CurrentLine>>,
) -> Vec<LogisticsExpenseFact> {
    expenses
        .iter()
        .flat_map(|order| {
            lines.get(&order.base.id).into_iter().flatten().filter_map(move |row| {
                let expense = row.expense.as_ref()?;
                (row.positive && expense.category == PurchaseExpenseCategory::Logistics).then(|| {
                    LogisticsExpenseFact {
                        sales_order_id: order.sales_order_id.to_string(),
                        supplier_id: order.supplier_id.to_string(),
                        sales_order_line_id: expense.sales_order_line_id.as_ref().map(ToString::to_string),
                    }
                })
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{FreightLineFact, LogisticsExpenseFact, pending_freight_lines};

    fn line(supplier: &str, sales_line: &str) -> FreightLineFact {
        FreightLineFact {
            sales_order_id: "so-1".into(),
            purchase_order_id: "po-1".into(),
            purchase_no: "PO-1".into(),
            supplier_id: supplier.into(),
            sales_order_line_id: sales_line.into(),
            product_name: Some("礼盒".into()),
        }
    }

    fn expense(sales_order: &str, supplier: &str, sales_line: Option<&str>) -> LogisticsExpenseFact {
        LogisticsExpenseFact {
            sales_order_id: sales_order.into(),
            supplier_id: supplier.into(),
            sales_order_line_id: sales_line.map(Into::into),
        }
    }

    #[test]
    fn line_specific_or_whole_order_logistics_expense_resolves_freight() {
        let pending = pending_freight_lines(
            vec![line("sup-1", "sol-1"), line("sup-1", "sol-2")],
            &[expense("so-1", "sup-1", Some("sol-1"))],
        );
        assert_eq!(pending, vec![line("sup-1", "sol-2")]);
        let pending = pending_freight_lines(vec![line("sup-1", "sol-2")], &[expense("so-1", "sup-1", None)]);
        assert!(pending.is_empty());
    }

    #[test]
    fn other_supplier_or_sales_order_does_not_resolve_freight() {
        let pending = pending_freight_lines(
            vec![line("sup-1", "sol-1")],
            &[expense("so-1", "sup-2", None), expense("so-2", "sup-1", None)],
        );
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn no_expense_keeps_all_freight_lines_pending() {
        assert_eq!(pending_freight_lines(vec![line("sup-1", "sol-1")], &[]).len(), 1);
    }
}
