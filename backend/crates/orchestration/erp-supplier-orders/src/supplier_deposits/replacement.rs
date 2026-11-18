//! 原采购变更事务内核实本执行组资金，未发送占用受控释放，未知资金严格阻断。

use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::money::Amount;
use erp_finance::entity::supplier_deposit::{FundsEvidenceInboxState, SupplierDepositReservation};
use erp_finance::repository::funds_evidence::funds_evidence_by_order;
use erp_finance::repository::supplier_deposit_query::{reservation_for_execution, usages_for_reservation};
use erp_finance::repository::{PurchasePaymentRequestExt, SupplierDepositExt};
use erp_finance::service::supplier_deposit::SupplierDepositService;
use erp_supply::api_order::ApiOrderExecution;
use erp_supply::repository::SupplierFulfillmentExt;
use mongodb::Database;
use persistence_core::Executor;

use super::access::audit;
use crate::{Error, Result};

/// 为原采购执行替代核实资金；调用方必须复用采购变更根事务。
/// # 参数
/// * `db` / `execution` / `actor` / `executor` - 原执行、已获变更权限人员及根事务。
/// # 返回
/// 未发送占用已安全释放，或原已发送资金有完整清偿及无后续风险依据。
/// # 错误
/// 未决证据、未知发送、未返款/冲回或缺风险消解依据时阻断，不按取消状态猜测。
pub async fn prepare_execution_funds_replacement(
    db: &Database,
    execution: &ApiOrderExecution,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(mut reservation) = reservation_for_execution(db, &execution.base.id, executor).await? else {
        return Ok(());
    };
    if reservation.basis.purchase_order_id != execution.purchase_order_id
        || reservation.basis.purchase_revision_id != execution.purchase_order_revision_id
    {
        return Err(Error::ConflictError("原资金占用与采购执行版本不一致".into()));
    }

    let mut account = db
        .supplier_deposit_accounts()
        .find_by_id(&reservation.account_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原预存款账户不存在".into()))?;
    let usages = usages_for_reservation(db, &reservation.base.id, executor).await?;

    let order = db.supplier_fulfillment_orders().find_by_id(&execution.base.id, executor).await?;
    let has_write = execution.original_result_query_allowed()
        || order.as_ref().is_some_and(|order| order.external_order_no.is_some());
    let external_orders =
        order_ids(execution, order.as_ref().and_then(|order| order.external_order_no.as_deref()))?;
    ensure_evidence_clear(db, &reservation, &external_orders, executor).await?;

    let release = reservation.replacement_release_amount(has_write, &account, &usages)?;
    if release > Amount::zero() {
        SupplierDepositService::new(db.clone())
            .release_reservation(
                &reservation.base.id,
                "purchase-execution-replacement",
                release,
                None,
                actor.id(),
                executor,
            )
            .await?;
        audit(db, actor, "release", &reservation.account_id, executor).await?;
    } else {
        let mut request = db
            .purchase_payment_requests()
            .find_by_id(&reservation.basis.payment_request_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原付款申请不存在".into()))?;
        db.supplier_deposit_accounts().update(&mut account, executor).await?;
        db.supplier_deposit_reservations().update(&mut reservation, executor).await?;
        db.purchase_payment_requests().update(&mut request, executor).await?;
    }
    Ok(())
}

async fn ensure_evidence_clear(
    db: &Database,
    reservation: &SupplierDepositReservation,
    orders: &BTreeSet<String>,
    executor: &mut dyn Executor,
) -> Result<()> {
    for order in orders {
        let evidence = funds_evidence_by_order(db, &reservation.account_id, order, executor).await?;
        if evidence.iter().any(|record| record.state != FundsEvidenceInboxState::Applied) {
            return Err(Error::BusinessLogicError("原供应商订单仍有待核对资金证据，不能替代执行".into()));
        }
    }
    Ok(())
}

fn order_ids(execution: &ApiOrderExecution, root: Option<&str>) -> Result<BTreeSet<String>> {
    let mut orders = BTreeSet::new();
    if let Some(root) = root {
        orders.insert(root.into());
    }
    for tree in [&execution.order_tree, &execution.pending_order_tree].into_iter().flatten() {
        orders.insert(tree.root_order_no.clone());
        orders.extend(tree.nodes.iter().map(|node| node.external_order_no.clone()));
    }
    if orders.len() > 1000 {
        return Err(Error::BusinessLogicError("原供应商订单范围过大，需先完成人工资金核实".into()));
    }
    Ok(orders)
}
