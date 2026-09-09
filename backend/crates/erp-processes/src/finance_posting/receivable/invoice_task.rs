//! 应收子账与 W11 销项开票执行任务的原子生命周期编排。
//!
//! 任务身份、摘要与终态口径唯一来源为 `erp_workflow::entity::work_item::finance_task`；
//! 本文件只解析责任人/组织、调用 factory 并持久化（FIN-E06）。

use erp_core::common::time::Instant;
use erp_core::ids::{PartyId, ReceivableAccountId, WorkItemId};
use erp_finance::entity::receivable::ReceivableAccount;
use erp_finance::repository::ReceivableExt;
use erp_workflow::entity::work_item::{matches_sales_invoice_identity, WorkItem};
use erp_workflow::WorkItemExt;
use persistence_core::Executor;

use crate::adapters::workflow::work_item_service;
use crate::{Error, Result};
use application_core::AuditActor;

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
    let _ = change;
    super::invoice_request::ensure_reserved_capacity(db, &account, executor).await?;
    super::invoice_request::sync_authorized_tasks(db, &account, executor).await
}

/// 同一次开票的任务身份、来源和额度，必须由外层正式提交命令提供。
pub(crate) struct InvoiceExecutionInput<'a> {
    pub work_item_id: &'a WorkItemId,
    pub expected_task_version: u64,
    pub party_id: &'a PartyId,
    pub account_ids: &'a [ReceivableAccountId],
    pub invoice_amount: erp_core::money::Amount,
}

/// 在销项发票正式提交事务内校验并记录当前开票执行任务活动。
///
/// # 错误
/// 任务版本、当前责任人、应收子账、往来主体或任一分配目标不属于同一任务时失败关闭。
pub(crate) async fn record_invoice_execution(
    db: &mongodb::Database,
    input: InvoiceExecutionInput<'_>,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<String> {
    let InvoiceExecutionInput {
        work_item_id,
        expected_task_version,
        party_id,
        account_ids,
        invoice_amount,
    } = input;
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
    work_item_service(
        db.clone(),
        crate::adapters::identity::shared_rbac_service(db.clone()),
    )
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
    let request_id = super::invoice_request::consume_authorization(
        db,
        work_item_id.as_ref(),
        &account.base.id,
        party_id,
        invoice_amount,
        executor,
    )
    .await?;
    task.record_activity(actor.id(), Instant::now())
        .map_err(Error::Logic)?;
    db.work_items().update(&mut task, executor).await?;
    Ok(request_id)
}

fn ensure_task_identity(task: &WorkItem, account: &ReceivableAccount) -> Result<()> {
    if matches_sales_invoice_identity(task, &account.base.id) {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "销项开票任务责任身份与应收子账不一致，请联系管理员修复后重试".to_string(),
    ))
}
