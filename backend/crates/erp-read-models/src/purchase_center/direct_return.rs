//! 来源销售已减量且已直发时的退回办理上下文。
use std::collections::HashMap;

use erp_core::money::{Amount, Quantity, line_amounts};
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_identity::AccessControlExt;
use erp_identity::repository::AccountCoreRepositoryExt;
use erp_procurement::entity::purchase_order::{
    ChangeQuantityBound, PurchaseOrder, PurchaseOrderRevisionLine, ReturnSettlementDecision,
};
use erp_procurement::repository::purchase_source_update::find_open_for_purchase;
use erp_returns::entity::returns::{PurchaseReturnStatus, SupplierRefundStatus, SupplierSettlementMethod};
use erp_returns::repository::direct_return::{confirmed_direct_returns, purchase_settlements};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::repository::work_item::WorkItemRepositoryExt;
use mongodb::Database;
use persistence_core::Executor;
use rust_decimal::Decimal;
use serde::Serialize;

use super::change_bounds::{load_change_quantity_facts, shipped_rows};
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DirectReturnOption {
    pub purchase_line_id: String,
    pub sales_line_id: String,
    pub item_name: String,
    pub unit: String,
    pub purchased_quantity: Quantity,
    pub target_quantity: Quantity,
    pub fulfilled_quantity: Quantity,
    pub returned_quantity: Quantity,
    pub required_return: Quantity,
    pub max_direct_return: Quantity,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DirectReturnPlan {
    pub sales_revision_id: String,
    pub lines: Vec<DirectReturnOption>,
    /// 原单价、费用不变的减量后预计含税额；提交改单时仍重新核算。
    pub target_gross: Amount,
    pub payable_account_id: Option<String>,
    pub return_numbers: Vec<String>,
    pub pending_settlement: bool,
    pub finance_tasks: Vec<ReturnFinanceTask>,
    pub settlement_decision: Option<ReturnSettlementDecision>,
    pub settlements: Vec<ReturnSettlementProgress>,
}

/// 已提交和已完成的财务处理，用于避免重复申请。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReturnSettlementProgress {
    pub id: String,
    pub number: String,
    pub method: SupplierSettlementMethod,
    pub amount: Amount,
    pub status: SupplierRefundStatus,
    pub decision_key: Option<String>,
}

async fn settlements(
    db: &Database,
    purchase_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<ReturnSettlementProgress>> {
    Ok(purchase_settlements(db, purchase_id, executor)
        .await?
        .into_iter()
        .map(|row| ReturnSettlementProgress {
            id: row.base.id,
            number: row.refund_no,
            method: row.settlement_method,
            amount: row.amount,
            status: row.status,
            decision_key: row.settlement_decision_key,
        })
        .collect())
}

/// 当前负责款项或红票处理的财务经办人。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReturnFinanceTask {
    pub kind: WorkItemType,
    pub owner_name: String,
    pub work_item_id: String,
}

async fn finance_tasks(
    db: &Database,
    purchase_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<ReturnFinanceTask>> {
    let tasks = db
        .work_items()
        .list_active_by_object("purchase_order", purchase_id, executor)
        .await?
        .into_iter()
        .filter(|task| {
            matches!(
                task.work_item_type,
                WorkItemType::PurchaseReturnRefund | WorkItemType::PurchaseReturnInvoice
            )
        })
        .collect::<Vec<_>>();
    let owners = tasks.iter().filter_map(|task| task.owner_user_id.clone()).collect::<Vec<_>>();
    let accounts = db.accounts().list_work_item_party_accounts(&owners, executor).await?;
    Ok(tasks
        .into_iter()
        .map(|task| ReturnFinanceTask {
            kind: task.work_item_type,
            owner_name: accounts
                .iter()
                .find(|account| Some(&account.base.id) == task.owner_user_id.as_ref())
                .map(|account| account.name.clone())
                .unwrap_or_else(|| "待重新指派".into()),
            work_item_id: task.base.id,
        })
        .collect())
}

/// 读取实际直发、已核实退回及销售目标，生成当前可办理范围。
/// # 参数
/// * `db` - 数据库。
/// * `order` - 已授权生效采购。
/// * `executor` - 同一事务或只读执行器。
/// # 返回
/// 返回逐行实际退回上限、后续采购目标及退货单号。
/// # 错误
/// 来源缺失、数量不一致或仓储失败时拒绝。
pub async fn direct_return_plan(
    db: &Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<DirectReturnPlan> {
    let facts = load_change_quantity_facts(db, order, executor).await?;
    let mut shipped: HashMap<String, Decimal> = HashMap::new();
    for (id, qty) in shipped_rows(db, order, executor).await? {
        *shipped.entry(id).or_default() += qty.to_decimal();
    }
    let returns = confirmed_direct_returns(db, &order.base.id, executor).await?;
    let mut returned: HashMap<String, Decimal> = HashMap::new();
    for record in &returns {
        if let Some(evidence) = &record.direct_return {
            for (id, qty) in &evidence.returned_lines {
                *returned.entry(id.clone()).or_default() += qty.to_decimal();
            }
        }
    }
    let mut lines = Vec::new();
    let mut target_gross = Amount::zero();
    for line in &facts.base_lines {
        let bound = facts.bounds.iter().find(|b| {
            line.sales_order_line_id.as_ref().is_some_and(|id| id.as_ref() == b.sales_order_line_id)
        });
        let Some(bound) = bound else {
            target_gross = target_gross.checked_add(line.gross_amount);
            continue;
        };
        let (option, gross) = option_for_line(line, bound, &shipped, &returned)?;
        target_gross = target_gross.checked_add(gross);
        if let Some(option) = option {
            lines.push(option);
        }
    }
    Ok(DirectReturnPlan {
        finance_tasks: finance_tasks(db, &order.base.id, executor).await?,
        settlements: settlements(db, &order.base.id, executor).await?,
        settlement_decision: find_open_for_purchase(db, &order.base.id, executor)
            .await?
            .and_then(|update| update.return_settlement),
        sales_revision_id: facts.state.coverage.revision.base.id,
        lines,
        target_gross,
        payable_account_id: db
            .payable_accounts()
            .find_by_purchase_order(&order.base.id.clone().into(), executor)
            .await?
            .map(|account| account.base.id),
        pending_settlement: returns.iter().any(|r| r.stable.status == PurchaseReturnStatus::Returned),
        return_numbers: returns.into_iter().map(|r| r.purchase_return_no).collect(),
    })
}

fn option_for_line(
    line: &PurchaseOrderRevisionLine,
    bound: &ChangeQuantityBound,
    shipped: &HashMap<String, Decimal>,
    returned: &HashMap<String, Decimal>,
) -> Result<(Option<DirectReturnOption>, Amount)> {
    let target = if bound.quantity_affected {
        (bound.sales_quantity.to_decimal() - bound.other_covered_quantity.to_decimal())
            .max(Decimal::ZERO)
            .min(bound.base_quantity.to_decimal())
    } else {
        bound.base_quantity.to_decimal()
    };
    let target = Quantity::try_from(target).map_err(Error::Logic)?;
    let cost = line.unit_cost_gross.ok_or_else(|| Error::BusinessLogicError("采购商品缺少单价".into()))?;
    let rate = line.input_tax_rate.ok_or_else(|| Error::BusinessLogicError("采购商品缺少税率".into()))?;
    let gross = line_amounts(cost, target, rate).0;
    let actual_return = returned.get(&bound.sales_order_line_id).copied().unwrap_or_default();
    if !bound.quantity_affected && actual_return.is_zero() {
        return Ok((None, gross));
    }
    let required = (bound.fulfilled_quantity.to_decimal() - target.to_decimal()).max(Decimal::ZERO);
    let remaining = (shipped.get(&bound.sales_order_line_id).copied().unwrap_or_default() - actual_return)
        .max(Decimal::ZERO);
    Ok((
        Some(DirectReturnOption {
            purchase_line_id: line.base.id.clone(),
            sales_line_id: bound.sales_order_line_id.clone(),
            item_name: bound.item_name.clone(),
            unit: line.base_unit_code.clone().unwrap_or_default(),
            purchased_quantity: bound.base_quantity,
            target_quantity: target,
            fulfilled_quantity: bound.fulfilled_quantity,
            returned_quantity: Quantity::try_from(actual_return).map_err(Error::Logic)?,
            required_return: Quantity::try_from(required).map_err(Error::Logic)?,
            max_direct_return: Quantity::try_from(required.min(remaining)).map_err(Error::Logic)?,
        }),
        gross,
    ))
}
