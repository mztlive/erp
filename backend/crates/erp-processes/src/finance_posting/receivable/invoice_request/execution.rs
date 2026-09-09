//! 申请批准后派工，以及每次开票的授权消耗。
use super::*;
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_core::ids::{PartyId, WorkItemId};
use erp_workflow::entity::work_item::{
    new_approved_sales_invoice_task, FinanceResponsibilityOperation, SalesInvoiceTaskReason,
    SalesInvoiceTaskSpec, WorkItemStatus,
};
use erp_workflow::WorkItemExt;
use id_generator::next_id;

/// 最终审批通过时原子授予额度并生成申请专属财务执行任务。
/// # 错误
/// 应收额度不足、财务负责人缺失、状态或并发冲突时审批事务回滚。
pub(crate) async fn approve_in_transaction(
    db: &Database,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut request = load(db, id, executor).await?;
    let account = lock_account(db, request.receivable_account_id.as_ref(), executor).await?;
    ensure_reserved_capacity(db, &account, executor).await?;
    request.approve()?;
    let service = crate::adapters::workflow::work_item_service(
        db.clone(),
        crate::adapters::identity::shared_rbac_service(db.clone()),
    );
    let responsibility = service
        .resolve_finance_responsibility(
            FinanceResponsibilityOperation::SalesInvoice,
            account.customer_id.as_ref(),
            executor,
        )
        .await?;
    let task = new_approved_sales_invoice_task(
        WorkItemId::new(next_id()),
        SalesInvoiceTaskSpec {
            account_id: account.base.id.clone(),
            subject_version: account.base.version.to_string(),
            owner_organization_id: account.counterparty_party_id.to_string(),
            owner_user_id: responsibility.owner_user_id,
            reason: SalesInvoiceTaskReason::Initial,
            open_invoiceable_total: request.remaining(),
        },
        &responsibility.responsibility_key,
        &request.base.id,
        &request.request_no,
    )?;
    request.work_item_id = Some(task.base.id.clone());
    db.work_items().create(&task, executor).await?;
    db.sales_invoice_requests().update(&mut request, executor).await?;
    let audit = actor.clone().resource_log(
        "sales_invoice_request.approve",
        "sales_invoice_request",
        id.to_owned(),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}
/// 消耗与任务一一关联的已批准申请；旧任务及错配任务一律拒绝。
/// # 错误
/// 申请缺失、主体或应收不符、金额超额时不允许登记发票。
pub(crate) async fn consume_authorization(
    db: &Database,
    task_id: &str,
    account_id: &str,
    party_id: &PartyId,
    amount: Amount,
    executor: &mut dyn Executor,
) -> Result<String> {
    let mut request = db
        .sales_invoice_requests()
        .find_for_task(task_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("此开票任务没有已批准的申请，请从销售单发起开票申请".into()))?;
    if request.receivable_account_id.as_ref() != account_id || &request.counterparty_party_id != party_id {
        return Err(Error::ConflictError("开票申请与当前销售应收或客户不一致".into()));
    }
    lock_account(db, account_id, executor).await?;
    request.record_invoice(amount)?;
    db.sales_invoice_requests().update(&mut request, executor).await?;
    Ok(request.base.id)
}
/// 开票登记后按每张申请剩余授权更新任务，不按销售单全部余额关闭任务。
pub(crate) async fn sync_authorized_tasks(
    db: &Database,
    account: &ReceivableAccount,
    executor: &mut dyn Executor,
) -> Result<()> {
    let tasks = db
        .work_items()
        .list_sales_invoice_execution_by_receivable_newest_first(&account.base.id, executor)
        .await?;
    for mut task in tasks
        .into_iter()
        .filter(|task| task.status == WorkItemStatus::Open)
    {
        let Some(request) = db
            .sales_invoice_requests()
            .find_for_task(task.base.id.as_str(), executor)
            .await?
        else {
            continue;
        };
        if request.remaining() == Amount::zero() {
            task.complete_when_fully_invoiced(Instant::now())?;
        } else {
            task.update_impact_summary(Some(format!(
                "申请 {} · 待开票 {} 元",
                request.request_no,
                request.remaining()
            )))?;
        }
        task.subject_version = account.base.version.to_string();
        db.work_items().update(&mut task, executor).await?;
    }
    Ok(())
}
