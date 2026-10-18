//! 直退后的财务责任交接；任务以实际账款和票额为完成依据。
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_core::money::Amount;
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_procurement::entity::purchase_order::{PurchaseOrder, ReturnSettlementDecision};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::purchase_source_update::find_open_for_purchase;
use erp_read_models::purchase_center::direct_return::{DirectReturnPlan, direct_return_plan};
use erp_returns::repository::direct_return::confirmed_direct_returns;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};
use erp_workflow::repository::work_item::WorkItemRepositoryExt;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::adapters::identity::shared_rbac_service;
use crate::adapters::workflow::work_item_service;
use crate::{Error, Result};

/// 在原业务事务内按退货、净付款及净票额交接或完成财务任务。
/// # 参数
/// * `db` - 数据库。
/// * `purchase_id` - 已由入口授权或财务事实关联的采购。
/// * `executor` - 原命令事务。
/// # 返回
/// 无重复开放任务；未完成的任务保留原责任人。
/// # 错误
/// 缺失责任配置、关联记录或事务冲突时拒绝整个命令。
pub(crate) async fn sync_return_finance(
    db: &Database,
    purchase_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(order) = db.purchase_orders().find_by_id(purchase_id, executor).await? else {
        return Ok(());
    };
    if confirmed_direct_returns(db, purchase_id, executor).await?.is_empty() {
        return Ok(());
    }
    let active = db.work_items().list_active_by_object("purchase_order", purchase_id, executor).await?;
    let open = find_open_for_purchase(db, purchase_id, executor).await?.is_some();
    let plan = if open && order.stable.current_revision_id.is_some() {
        Some(direct_return_plan(db, &order, executor).await?)
    } else {
        None
    };
    let account =
        db.payable_accounts().find_by_purchase_order(&purchase_id.to_owned().into(), executor).await?;
    let amounts = plan
        .as_ref()
        .zip(account.as_ref())
        .map(|(plan, account)| {
            (
                ReturnSettlementDecision::excess(account.settled_total, plan.target_gross),
                ReturnSettlementDecision::excess(account.invoiced_total, plan.target_gross),
            )
        })
        .unwrap_or((Amount::zero(), Amount::zero()));
    for (kind, amount) in
        [(WorkItemType::PurchaseReturnRefund, amounts.0), (WorkItemType::PurchaseReturnInvoice, amounts.1)]
    {
        let existing = active.iter().find(|item| item.work_item_type == kind).cloned();
        if let Some(mut item) = existing {
            if amount == Amount::zero() {
                item.complete_when_return_settled(Instant::now()).map_err(Error::Logic)?;
            } else {
                item.update_impact_summary(Some(summary(&order, amount))).map_err(Error::Logic)?;
            }
            db.work_items().update(&mut item, executor).await?;
        } else if let Some(plan) = plan.as_ref().filter(|plan| {
            amount > Amount::zero()
                && ReturnSettlementDecision::ready_for_finance(
                    plan.settlement_decision.as_ref(),
                    &plan.sales_revision_id,
                    plan.target_gross,
                    plan.pending_settlement,
                    plan.lines.iter().all(|line| line.required_return.to_decimal().is_zero()),
                )
        }) {
            create_task(db, &order, plan, kind, amount, executor).await?;
        }
    }
    Ok(())
}

fn summary(order: &PurchaseOrder, amount: Amount) -> String {
    format!("{} · 供应商已确认退货 · 尚需处理 ¥{}", order.purchase_no, amount)
}

async fn create_task(
    db: &Database,
    order: &PurchaseOrder,
    plan: &DirectReturnPlan,
    kind: WorkItemType,
    amount: Amount,
    executor: &mut dyn Executor,
) -> Result<()> {
    let sales = db
        .sales_orders()
        .find_by_id(order.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;
    let counterparty = if kind == WorkItemType::PurchaseReturnRefund {
        order.supplier_id.as_ref()
    } else {
        sales.customer_id.as_ref()
    };
    let owner = work_item_service(db.clone(), shared_rbac_service(db.clone()))
        .resolve_return_finance_responsibility(kind, counterparty, executor)
        .await?;
    let item = WorkItem::new_with_responsibility_key(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: kind,
            business_object_type: "purchase_order".into(),
            business_object_id: order.base.id.clone(),
            subject_version: plan.sales_revision_id.clone(),
            owner_role: if kind == WorkItemType::PurchaseReturnRefund {
                "role-cashier"
            } else {
                "role-invoice-clerk"
            }
            .into(),
            owner_organization_id: order.business_org_unit_id.clone(),
            owner_user_id: owner.owner_user_id,
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::High,
            due_at: None,
            reason_code: Some("PURCHASE_RETURN_FINANCE_REQUIRED".into()),
            impact_summary: Some(summary(order, amount)),
        },
        owner.responsibility_key,
    )
    .map_err(Error::Logic)?;
    db.work_items().create(&item, executor).await?;
    Ok(())
}

/// 按财务分配涉及的应付子账同步对应采购；供应商结算子账不派采购任务。
/// # 参数
/// * `db` - 数据库。
/// * `account_ids` - 发生退款或红冲的应付子账。
/// * `executor` - 原过账事务。
/// # 返回
/// 全部关联采购完成同步。
/// # 错误
/// 查询或任务写入失败时整体回滚。
pub(crate) async fn sync_return_accounts(
    db: &Database,
    account_ids: Vec<String>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut ids = account_ids;
    ids.sort();
    ids.dedup();
    for id in ids {
        if let Some(account) = db.payable_accounts().find_by_id(&id, executor).await? {
            sync_return_finance(db, &account.source_document_id, executor).await?;
        }
    }
    Ok(())
}
