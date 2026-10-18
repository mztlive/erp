//! 供应商现金退款或留抵确认的原子过账。
use erp_finance::repository::prelude::*;
use erp_returns::entity::returns::SupplierSettlementMethod;
use erp_returns::repository::{ReturnsExt, SupplierRefundPostedTotalsExt};

use super::*;
use crate::reverse_flow::return_finance::sync_return_accounts;
use crate::reverse_flow::return_settlement::{create_credit, validate_settlement};
/// 在最终通过事务内执行过账副作用并写回退款单。
///
/// # 参数
/// * `db` - 数据库。
/// * `refund_id` - 退款单主键。
/// * `actor_id` - 过账操作人 ID。
/// * `actor` - 审计操作人。
/// * `executor` - 最终通过事务的执行器；本函数不再开启事务。
///
/// # 返回
/// 成功时无返回值。退款单已过账并写回。
///
/// # 错误
/// 非审批中、原付款不存在、累计超额或仓储失败时返回错误。
pub(in crate::reverse_flow) async fn apply_supplier_refund_final_post(
    db: &Database,
    refund_id: &str,
    actor_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut refund =
        ReturnsService::new(db.clone()).prepare_supplier_refund_post(refund_id, executor).await?;
    execute_supplier_refund_domain_action(
        &mut refund,
        erp_workflow::service::approval::policy::ApprovalDomainAction::SupplierRefundPost,
    )?;
    apply_supplier_refund_posting(db, &refund, actor_id, executor).await?;
    ReturnsService::new(db.clone()).persist_supplier_refund_post(&mut refund, executor).await?;
    let audit =
        actor.clone().resource_log("supplier_refund.post", "supplier_refund", refund.base.id.clone())?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

/// 在调用方事务内写入退款入账副作用。
///
/// # 错误
/// 原付款不存在、累计超额或仓储失败时返回错误。
async fn apply_supplier_refund_posting(
    db: &Database,
    refund: &SupplierRefund,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let account = validate_settlement(db, refund, executor).await?;
    let original_payment_id = erp_returns::service::supplier_refund::original_payment_id(refund)?;
    let mut payment = erp_finance::service::payable::supplier_refund::load_posted_payment_for_refund(
        db,
        &original_payment_id,
        executor,
    )
    .await?;
    ReturnsService::new(db.clone()).validate_supplier_refund_amount(refund, payment.amount, executor).await?;
    payment.cash_refunded_amount = db
        .supplier_refunds()
        .posted_refund_total_by_payment(&original_payment_id, &refund.base.id, executor)
        .await?;
    erp_finance::service::payable::supplier_refund::persist_refund_offsets_and_reversals(
        db,
        &erp_finance::service::payable::supplier_refund::SupplierRefundPostingFact {
            refund_id: refund.base.id.clone(),
            target_account_id: account.as_ref().map(|row| row.base.id.clone()),
            amount: refund.amount,
            occurred_at: refund.occurred_at,
        },
        &payment,
        actor_id,
        executor,
    )
    .await?;
    if refund.settlement_method == SupplierSettlementMethod::SupplierCredit {
        let account = account.as_ref().ok_or_else(|| Error::ConflictError("留抵缺少来源应付".into()))?;
        create_credit(db, refund, &payment, account, executor).await?;
    } else {
        payment.record_cash_refund(refund.amount)?;
    }
    db.supplier_payments().update(&mut payment, executor).await?;
    let allocations = db
        .payment_allocations()
        .find_allocations_by_payments(&[original_payment_id], executor)
        .await?;
    let entry_ids = allocations.into_iter().map(|line| line.payable_entry_id).collect::<Vec<_>>();
    let entries = db.payable_entries().find_entries_by_ids(&entry_ids, executor).await?;
    sync_return_accounts(
        db,
        entries.into_iter().map(|entry| entry.payable_account_id.to_string()).collect(),
        executor,
    )
    .await?;
    Ok(())
}
