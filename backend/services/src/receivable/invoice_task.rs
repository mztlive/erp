//! 应收子账与 W11 销项开票执行任务的原子生命周期编排。
//!
//! 任务身份、摘要与终态口径唯一来源为 `entities::work_item::finance_task`；
//! 本文件只解析责任人/组织、调用 factory 并持久化（FIN-E06）。

use database::{Executor, ReceivableExt, WorkItemExt};
use entities::common::time::Instant;
use entities::ids::{PartyId, ReceivableAccountId, WorkItemId};
use entities::receivable::ReceivableAccount;
use entities::work_item::{
    is_zero_amount, matches_sales_invoice_identity, new_sales_invoice_task, sales_invoice_impact_summary,
    FinanceResponsibilityOperation, SalesInvoiceTaskReason, SalesInvoiceTaskSpec, WorkItem, WorkItemStatus,
};
use id_generator::next_id;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::work_item::WorkItemService;

/// 触发应收可开票额度变化的正式业务事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SalesInvoiceTaskChange {
    /// 蓝票登记减少可开票额度。
    InvoicePosted,
    /// 红票登记恢复可开票额度。
    RedInvoiceIssued,
    /// 销售变更调整可开票总额。
    ReceivableChanged,
}

/// 为新形成且存在可开票额度的应收子账建立唯一开放销项开票任务。
///
/// # 错误
/// 任务重复、财务责任规则缺失、负责人无权限或仓储写入失败时返回错误。
pub(crate) async fn ensure_sales_invoice_task(
    db: &mongodb::Database,
    account: &ReceivableAccount,
    executor: &mut dyn Executor,
) -> Result<()> {
    if is_zero_amount(account.open_invoiceable_total) {
        return Ok(());
    }
    let tasks = sales_invoice_tasks(db, &account.base.id, executor).await?;
    let open = open_tasks(&tasks);
    match open.as_slice() {
        [] if tasks.is_empty() => {
            create_invoice_task(db, account, SalesInvoiceTaskReason::Initial, executor).await
        }
        [task] => ensure_task_identity(task, account),
        [] => Err(Error::BusinessLogicError(
            "应收子账已存在开票任务历史，不能重复建立初始任务".to_string(),
        )),
        _ => Err(duplicate_open_task_error()),
    }
}

/// 在开票、红冲或应收金额变更事务内同步销项开票执行任务。
///
/// # 错误
/// 子账缺失、开放任务重复、规则或负责人失效、任务身份损坏时返回错误。
pub(crate) async fn sync_sales_invoice_task(
    db: &mongodb::Database,
    account_id: &ReceivableAccountId,
    change: SalesInvoiceTaskChange,
    executor: &mut dyn Executor,
) -> Result<()> {
    let account = db
        .receivable_accounts()
        .find_by_id(account_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
    let tasks = sales_invoice_tasks(db, &account.base.id, executor).await?;
    if tasks.is_empty() && change != SalesInvoiceTaskChange::ReceivableChanged {
        return Err(Error::BusinessLogicError(
            "应收子账缺少销项开票任务历史，请联系管理员修复后重试".to_string(),
        ));
    }
    let open = open_tasks(&tasks);
    if open.len() > 1 {
        return Err(duplicate_open_task_error());
    }
    if is_zero_amount(account.open_invoiceable_total) {
        return complete_open_task(db, open.into_iter().next(), &account, executor).await;
    }
    if let Some(task) = open.into_iter().next() {
        return update_open_task_summary(db, task, &account, executor).await;
    }
    let reason = match (tasks.is_empty(), change) {
        (true, SalesInvoiceTaskChange::ReceivableChanged) => SalesInvoiceTaskReason::Initial,
        (true, _) => {
            return Err(Error::BusinessLogicError(
                "应收子账缺少销项开票任务历史，请联系管理员修复后重试".to_string(),
            ));
        }
        (false, SalesInvoiceTaskChange::RedInvoiceIssued) => SalesInvoiceTaskReason::ReopenedByRedInvoice,
        (false, SalesInvoiceTaskChange::ReceivableChanged) => SalesInvoiceTaskReason::ReopenedBySalesChange,
        (false, SalesInvoiceTaskChange::InvoicePosted) => {
            return Err(Error::BusinessLogicError(
                "销项开票后仍有可开票额度但原任务已关闭，请联系管理员处理".to_string(),
            ));
        }
    };
    create_invoice_task(db, &account, reason, executor).await
}

/// 在销项发票正式提交事务内校验并记录当前开票执行任务活动。
///
/// # 错误
/// 任务版本、当前责任人、应收子账、往来主体或任一分配目标不属于同一任务时失败关闭。
pub(crate) async fn record_invoice_execution(
    db: &mongodb::Database,
    work_item_id: &WorkItemId,
    expected_task_version: u64,
    party_id: &PartyId,
    account_ids: &[ReceivableAccountId],
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut task = db
        .work_items()
        .find_by_id(work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销项开票执行任务不存在".to_string()))?;
    if task.base.version != expected_task_version {
        return Err(Error::ConflictError(
            "开票任务版本已变化，请刷新工作台任务后重试".to_string(),
        ));
    }
    let account = db
        .receivable_accounts()
        .find_by_id(&task.business_object_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("开票任务关联的应收往来子账不存在".to_string()))?;
    ensure_task_identity(&task, &account)?;
    if !task.is_owned_by(actor.id()) {
        return Err(Error::Forbidden(
            "当前账号不是开放开票任务的当前责任人".to_string(),
        ));
    }
    WorkItemService::new(db.clone(), crate::iam::shared_rbac_service(db.clone()))
        .ensure_domain_decision_access(actor, &task, executor)
        .await?;
    if &account.counterparty_party_id != party_id {
        return Err(Error::BusinessLogicError(
            "发票往来主体与当前任务的应收子账不一致".to_string(),
        ));
    }
    if account_ids
        .iter()
        .any(|account_id| account_id.as_ref() != task.business_object_id)
    {
        return Err(Error::BusinessLogicError(
            "一次销项开票只能分配到当前任务绑定的应收子账".to_string(),
        ));
    }
    task.record_activity(actor.id(), Instant::now())
        .map_err(Error::Logic)?;
    db.work_items().update(&mut task, executor).await?;
    Ok(())
}

async fn create_invoice_task(
    db: &mongodb::Database,
    account: &ReceivableAccount,
    reason: SalesInvoiceTaskReason,
    executor: &mut dyn Executor,
) -> Result<()> {
    let responsibility = WorkItemService::new(db.clone(), crate::iam::shared_rbac_service(db.clone()))
        .resolve_finance_responsibility(
            FinanceResponsibilityOperation::SalesInvoice,
            account.customer_id.as_ref(),
            executor,
        )
        .await?;
    let task = new_sales_invoice_task(
        WorkItemId::new(next_id()),
        SalesInvoiceTaskSpec {
            account_id: account.base.id.clone(),
            subject_version: account.base.version.to_string(),
            owner_organization_id: account.counterparty_party_id.to_string(),
            owner_user_id: responsibility.owner_user_id,
            reason,
            open_invoiceable_total: account.open_invoiceable_total,
        },
        responsibility.responsibility_key,
    )
    .map_err(Error::Logic)?;
    db.work_items().create(&task, executor).await?;
    Ok(())
}

async fn complete_open_task(
    db: &mongodb::Database,
    task: Option<&WorkItem>,
    account: &ReceivableAccount,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(task) = task else {
        return Ok(());
    };
    ensure_task_identity(task, account)?;
    let mut task = task.clone();
    task.complete_when_fully_invoiced(Instant::now())
        .map_err(Error::Logic)?;
    db.work_items().update(&mut task, executor).await?;
    Ok(())
}

async fn update_open_task_summary(
    db: &mongodb::Database,
    task: &WorkItem,
    account: &ReceivableAccount,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_task_identity(task, account)?;
    let impact = sales_invoice_impact_summary(account.open_invoiceable_total);
    let subject_version = account.base.version.to_string();
    if task.impact_summary.as_deref() == Some(impact.as_str()) && task.subject_version == subject_version {
        return Ok(());
    }
    let mut task = task.clone();
    task.subject_version = subject_version;
    task.update_impact_summary(Some(impact)).map_err(Error::Logic)?;
    db.work_items().update(&mut task, executor).await?;
    Ok(())
}

async fn sales_invoice_tasks(
    db: &mongodb::Database,
    receivable_account_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItem>> {
    db.work_items()
        .list_sales_invoice_execution_by_receivable_newest_first(receivable_account_id, executor)
        .await
        .map_err(Into::into)
}

fn open_tasks(tasks: &[WorkItem]) -> Vec<&WorkItem> {
    tasks
        .iter()
        .filter(|task| task.status == WorkItemStatus::Open)
        .collect()
}

fn ensure_task_identity(task: &WorkItem, account: &ReceivableAccount) -> Result<()> {
    if matches_sales_invoice_identity(task, &account.base.id) {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "销项开票任务责任身份与应收子账不一致，请联系管理员修复后重试".to_string(),
    ))
}

fn duplicate_open_task_error() -> Error {
    Error::BusinessLogicError("同一应收子账存在多个开放销项开票任务，请联系管理员处理".to_string())
}
