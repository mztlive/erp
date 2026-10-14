//! 采购改单数量、交付与入库进度复核（第一期 §6.3“实物与服务采购改单执行规则”）。
//!
//! 提交时预检，最终审批生效事务内按当时事实复核：改后数量不得低于已发生数量，
//! 也不得超过销售剩余量；因销售归零或减量受影响的行必须调减到位。预计交付日
//! 复验改动的行和因履约期限提前受影响的行；收件分配全部复验。

use std::collections::HashSet;

use erp_core::ids::{PurchaseOrderId, PurchaseOrderRevisionId};
use erp_core::money::{Amount, Quantity};
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_fulfillment::repository::FulfillmentExt;
use erp_procurement::entity::purchase_order::{
    ExpenseLineState, FulfillmentResponsibility, ProgressStatus, PurchaseLineInput, PurchaseLineType,
    PurchaseOrder, PurchaseOrderRevisionLine, SourceImpactKind, receipt_progress_after_change,
    rekey_to_current_lines, validate_change_expenses, validate_change_quantities,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_procurement::repository::purchase_expense::occurrences_by_orders;
use erp_read_models::purchase_center::change_bounds::{ChangeQuantityFacts, load_change_quantity_facts};
use mongodb::Database;
use persistence_core::Executor;

use super::super::delivery_plan::{validate_change_input_delivery, validate_change_revision_delivery};
use super::super::expense_confirm::expense_progress;
use crate::{Error, Result};

/// 生效时来源事实已变化的提示前缀。
const EFFECT_STALE_PREFIX: &str = "改单审批期间来源销售或履约事实已变化：";
/// 生效时来源事实已变化的提示后缀。
const EFFECT_STALE_SUFFIX: &str = "。请撤回改单，按最新情况修改后重新提交";

/// 提交前预检改单目标的数量与交付安排。
///
/// # 参数
/// * `db` - 数据库
/// * `order` - 已证明可写的来源采购单
/// * `inputs` - 改单目标行
/// * `new_gross` - 改后含税总额
/// * `executor` - 调用方执行器
///
/// # 返回
/// 金额、数量和交付安排均合规时返回 `Ok(())`。
///
/// # 错误
/// 改后金额低于已付款或已收票、新增或遗漏原商品行、数量越界、交付日或收件分配不合规、
/// 事实读取失败时返回错误。
pub(super) async fn check_change_inputs(
    db: &Database,
    order: &PurchaseOrder,
    inputs: &[PurchaseLineInput],
    new_gross: Amount,
    executor: &mut dyn Executor,
) -> Result<()> {
    if let Some(account) =
        db.payable_accounts().find_by_purchase_order(&PurchaseOrderId::new(&order.base.id), executor).await?
    {
        account
            .ensure_purchase_change_amount(new_gross)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    }
    let facts = load_change_quantity_facts(db, order, executor).await?;
    validate_change_quantities(&facts.bounds, &input_quantities(inputs))
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    let occurred = occurred_expense_lines(db, order, executor).await?;
    validate_change_expenses(&revision_expenses(&facts.base_lines), &input_expenses(inputs), &occurred)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    let recheck = due_recheck_lines(&facts, order);
    validate_change_input_delivery(db, order, inputs, &facts.base_lines, &recheck, executor).await
}

/// 生效事务内按当时事实复核新版本行，并返回应写入的入库进度。
///
/// # 参数
/// * `db` - 数据库
/// * `order` - 尚未切换版本的来源采购单
/// * `lines` - 即将写入的新版本行
/// * `executor` - 生效事务执行器
///
/// # 返回
/// 仓库履约采购返回按新数量重算的入库进度；其他履约方式返回空。
///
/// # 错误
/// 来源销售或履约事实已变化导致目标不再合规，或事实读取失败时返回错误。
pub(super) async fn check_effective_lines(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[PurchaseOrderRevisionLine],
    executor: &mut dyn Executor,
) -> Result<Option<ProgressStatus>> {
    let facts = load_change_quantity_facts(db, order, executor).await?;
    validate_change_quantities(&facts.bounds, &revision_quantities(lines)).map_err(stale)?;
    let occurred = occurred_expense_lines(db, order, executor).await?;
    validate_change_expenses(&revision_expenses(&facts.base_lines), &revision_expenses(lines), &occurred)
        .map_err(stale)?;
    let recheck = due_recheck_lines(&facts, order);
    validate_change_revision_delivery(db, order, lines, &facts.base_lines, &recheck, executor)
        .await
        .map_err(stale_process)?;
    if order.fulfillment_responsibility == FulfillmentResponsibility::NotApplicable {
        return Ok(Some(expense_progress(lines, &occurred)));
    }
    if order.fulfillment_responsibility != FulfillmentResponsibility::Warehouse {
        return Ok(None);
    }
    receipt_progress(db, order, lines, executor).await.map(Some)
}

/// 按跨版本累计合格入库与新数量重算入库进度。
async fn receipt_progress(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[PurchaseOrderRevisionLine],
    executor: &mut dyn Executor,
) -> Result<ProgressStatus> {
    let order_id = PurchaseOrderId::new(&order.base.id);
    let revisions = db.purchase_order().list_revisions_by_order(&order_id, executor).await?;
    let ids = revisions.into_iter().map(|row| PurchaseOrderRevisionId::new(row.base.id)).collect::<Vec<_>>();
    let mut all_lines = db.purchase_order_revision_lines().find_lines_by_revision_ids(&ids, executor).await?;
    all_lines.extend_from_slice(lines);
    let received =
        db.fulfillment().qualified_received_totals_by_purchase_revision_line(&order_id, executor).await?;
    let rekeyed = rekey_to_current_lines(lines, &all_lines, &received)?;
    Ok(receipt_progress_after_change(lines, &rekeyed))
}

/// 本采购因履约期限提前受影响、须复验预计交付日的稳定销售行。
fn due_recheck_lines(facts: &ChangeQuantityFacts, order: &PurchaseOrder) -> HashSet<String> {
    facts
        .state
        .impact_of(&order.base.id)
        .into_iter()
        .flat_map(|impact| impact.reasons.iter())
        .filter(|reason| matches!(reason.kind, SourceImpactKind::DueEarlier { .. }))
        .map(|reason| reason.sales_order_line_id.clone())
        .collect()
}

/// 本采购单已确认发生的费用行身份。
async fn occurred_expense_lines(
    db: &Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<HashSet<String>> {
    Ok(occurrences_by_orders(db, std::slice::from_ref(&order.base.id), executor)
        .await?
        .into_iter()
        .map(|occurrence| occurrence.expense_line_id)
        .collect())
}

/// 版本行中的费用行比对内容。
fn revision_expenses(lines: &[PurchaseOrderRevisionLine]) -> Vec<ExpenseLineState<'_>> {
    lines
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::LogisticsFee)
        .map(|line| ExpenseLineState {
            expense: line.expense.as_ref(),
            gross_amount: line.gross_amount,
            input_tax_rate: line.input_tax_rate,
        })
        .collect()
}

/// 改单目标中的费用行比对内容；缺少金额的行按 0 比对，由行校验另行拒绝。
fn input_expenses(inputs: &[PurchaseLineInput]) -> Vec<ExpenseLineState<'_>> {
    inputs
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::LogisticsFee)
        .map(|line| ExpenseLineState {
            expense: line.expense.as_ref(),
            gross_amount: line.gross_amount.unwrap_or_else(Amount::zero),
            input_tax_rate: line.input_tax_rate,
        })
        .collect()
}

/// 改单目标中的商品行（稳定销售行，数量）。
fn input_quantities(inputs: &[PurchaseLineInput]) -> Vec<(String, Quantity)> {
    inputs
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
        .filter_map(|line| Some((line.sales_order_line_id.as_ref()?.to_string(), line.quantity?)))
        .collect()
}

/// 新版本商品行（稳定销售行，数量）。
fn revision_quantities(lines: &[PurchaseOrderRevisionLine]) -> Vec<(String, Quantity)> {
    lines
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
        .filter_map(|line| Some((line.sales_order_line_id.as_ref()?.to_string(), line.quantity?)))
        .collect()
}

/// 把生效时的领域校验失败改写为撤回重提提示。
fn stale(error: erp_core::Error) -> Error {
    Error::ConflictError(format!("{EFFECT_STALE_PREFIX}{error}{EFFECT_STALE_SUFFIX}"))
}

/// 把生效时的交付复验失败改写为撤回重提提示；读取失败保持原错误。
fn stale_process(error: Error) -> Error {
    match error {
        Error::Logic(error) => stale(error),
        Error::BusinessLogicError(message) | Error::ConflictError(message) => {
            Error::ConflictError(format!("{EFFECT_STALE_PREFIX}{message}{EFFECT_STALE_SUFFIX}"))
        },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生效时数量或交付不再合规必须提示撤回重提，读取失败保持原分类。
    #[test]
    fn effect_failures_ask_to_withdraw_and_resubmit() {
        let quantity = stale(erp_core::Error::from("第 1 行 礼盒 最多可采购 3"));
        assert!(
            matches!(quantity, Error::ConflictError(message) if message.contains("最多可采购 3") && message.contains("撤回改单"))
        );

        let delivery = stale_process(Error::ConflictError("预计交付日晚于销售履约期限".into()));
        assert!(
            matches!(delivery, Error::ConflictError(message) if message.starts_with(EFFECT_STALE_PREFIX))
        );

        let missing = stale_process(Error::NotFound("来源销售单不存在".into()));
        assert!(matches!(missing, Error::NotFound(_)));
    }
}
