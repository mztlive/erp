//! 申请批准后派工，以及每次开票的授权消耗。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_core::ids::{PartyId, WorkItemId};
use erp_finance::entity::receivable::SalesInvoiceRequest;
use erp_finance::repository::prelude::*;
use erp_identity::entity::resource_access::event::{GrantEventEvidence, GrantEventPayload};
use erp_identity::service::resource_access::append_event;
use erp_read_models::work_item_links;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{
    FinanceResponsibilityOperation, SalesInvoiceTaskReason, SalesInvoiceTaskSpec, WorkItem, WorkItemStatus,
    new_approved_sales_invoice_task,
};
use erp_workflow::repository::prelude::*;
use id_generator::next_id;

use super::*;
use crate::FinancePostingPorts;

/// 最终审批通过时原子授予额度并生成申请专属财务执行任务。
///
/// # 参数
/// * `db` - 数据库。
/// * `rbac` - 财务办理与任务权限端口。
/// * `id` - 开票申请 ID。
/// * `approval_instance_id` - 本次最终通过的正式审批实例 ID。
/// * `actor` - 已认证审批人。
/// * `executor` - 审批事务执行器。
///
/// # 返回
/// 申请已批准、执行任务与审计写入成功时无返回值。
///
/// # 错误
/// 申请或应收不存在、占用额度超过可开票金额、批准状态不允许、财务负责人缺失、任务构造或写入失败时返回错误。
pub async fn approve(
    db: &Database,
    rbac: &FinancePostingPorts,
    id: &str,
    approval_instance_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut request = load(db, id, executor).await?;
    let account = lock_account(db, request.receivable_account_id.as_ref(), executor).await?;
    ensure_reserved_capacity(db, &account, executor).await?;
    request.approve()?;

    let service = rbac.tasks.as_ref();
    let responsibility = service
        .resolve_finance_responsibility(
            FinanceResponsibilityOperation::SalesInvoice,
            account.customer_id.as_ref(),
            executor,
        )
        .await?;
    let mut task = new_approved_sales_invoice_task(
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
    task.set_links(work_item_links::capture(db, "sales_invoice_request", &request.base.id, executor).await?)?;
    db.work_items().create(&task, executor).await?;
    db.sales_invoice_requests().update(&mut request, executor).await?;

    append_invoice_grant(db, &request, &task, approval_instance_id, executor).await?;
    let audit = actor.clone().resource_log(
        "sales_invoice_request.approve",
        "sales_invoice_request",
        id.to_owned(),
    )?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}
/// 消耗与任务一一关联的已批准申请；旧任务及错配任务一律拒绝。
///
/// # 参数
/// * `db` - 数据库。
/// * `task_id` - 开票执行任务 ID。
/// * `account_id` - 当前应收子账 ID。
/// * `party_id` - 发票往来主体。
/// * `amount` - 本次登记的开票金额。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 返回被消耗的开票申请 ID。
///
/// # 错误
/// 没有已批准申请、主体或应收不符、金额无法入账、应收锁或申请更新失败时返回错误。
pub(crate) async fn consume_authorization(
    db: &Database,
    task_id: &str,
    account_id: &str,
    party_id: &PartyId,
    amount: Amount,
    executor: &mut dyn Executor,
) -> Result<String> {
    let mut request =
        db.sales_invoice_requests().find_for_task(task_id, executor).await?.ok_or_else(|| {
            Error::ConflictError("此开票任务没有已批准的申请，请从销售单发起开票申请".into())
        })?;
    if request.receivable_account_id.as_ref() != account_id || &request.counterparty_party_id != party_id {
        return Err(Error::ConflictError("开票申请与当前销售应收或客户不一致".into()));
    }

    lock_account(db, account_id, executor).await?;
    request.record_invoice(amount)?;
    db.sales_invoice_requests().update(&mut request, executor).await?;
    Ok(request.base.id)
}
/// 开票登记后按每张申请剩余授权更新任务，不按销售单全部余额关闭任务。
///
/// # 参数
/// * `db` - 数据库。
/// * `account` - 当前应收子账。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 开放任务已按剩余授权完成或更新摘要时无返回值。没有对应申请的开放任务跳过。
///
/// # 错误
/// 任务查询、完成、摘要更新或任务写回失败时返回错误。
pub(crate) async fn sync_authorized_tasks(
    db: &Database,
    account: &ReceivableAccount,
    executor: &mut dyn Executor,
) -> Result<()> {
    let tasks = db
        .work_items()
        .list_sales_invoice_execution_by_receivable_newest_first(&account.base.id, executor)
        .await?;

    for mut task in tasks.into_iter().filter(|task| task.status == WorkItemStatus::Open) {
        let Some(request) =
            db.sales_invoice_requests().find_for_task(task.base.id.as_str(), executor).await?
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

/// 使用已保存的申请版本和实际任务责任人记录授权来源。
async fn append_invoice_grant(
    db: &Database,
    request: &SalesInvoiceRequest,
    task: &WorkItem,
    approval_instance_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    append_event(
        db,
        GrantEventEvidence {
            payload: GrantEventPayload::InvoiceApproved {
                invoice_request_id: request.base.id.clone(),
                approval_instance_id: approval_instance_id.into(),
                work_item_id: task.base.id.clone(),
            },
            recipient: task
                .owner_user_id
                .clone()
                .ok_or_else(|| Error::ConflictError("开票任务缺少实际分配人".into()))?,
            fact_version: request.base.version,
            occurred_at: request.base.updated_at,
        },
        executor,
    )
    .await?;
    Ok(())
}
