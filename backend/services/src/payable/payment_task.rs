//! 已确认采购应付与 W01 付款执行任务的原子生命周期编排。
//!
//! 任务对象固定为 `payable_account`，负责人由供应商精确责任规则或付款默认规则
//! 解析并冻结。采购审批最终通过已经提供付款授权；付款单不再创建第二套审批
//! 任务。付款部分核销只更新摘要，开放余额归零自动完成；冲正重新产生余额时按
//! 当前责任规则创建新任务身份。

use database::{Executor, PayableExt, SupplierExt, WorkItemExt};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{PayableAccountId, SupplierAccountId, WorkItemId};
use entities::payable::{PayableAccount, PayableEntry, PayableSourceType, PendingPaymentAllocation};
use entities::work_item::{
    is_purchase_payable, matches_supplier_payment_identity, new_supplier_payment_task, payment_due_at,
    supplier_payment_impact_summary, FinanceResponsibilityOperation, SupplierPaymentTaskReason,
    SupplierPaymentTaskSpec, WorkItem, WorkItemStatus,
};
use id_generator::next_id;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::work_item::WorkItemService;

/// 为采购最终通过形成的应付建立唯一开放付款执行任务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `account` - 已在当前事务形成的采购应付子账
/// * `entry` - 与子账一同形成的原始应付分录
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 新任务创建成功或已有任务满足冻结身份时返回成功。
///
/// # 错误
/// 对象关系、责任账号、付款权限、任务唯一性或仓储写入不满足时返回错误。
///
/// # 关键业务约束
/// 应付事实与付款执行责任必须在采购生效的同一事务中可见。
pub(crate) async fn ensure_purchase_payment_task(
    db: &mongodb::Database,
    account: &PayableAccount,
    entry: &PayableEntry,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !is_purchase_payable(account, entry) {
        return Err(Error::BusinessLogicError(
            "采购应付事实不完整，无法形成付款任务，请检查应付分录后重试".to_string(),
        ));
    }
    let tasks = payment_tasks(db, &account.base.id, executor).await?;
    let open = open_tasks(&tasks);
    match open.as_slice() {
        [] if tasks.is_empty() => {}
        [] => {
            return Err(Error::BusinessLogicError(
                "应付子账已存在付款任务历史，不能重复建立初始任务".to_string(),
            ));
        }
        [task] => {
            if !matches_supplier_payment_identity(task, &account.base.id) {
                return Err(Error::BusinessLogicError(
                    "付款任务责任身份与应付子账不一致，请联系管理员修复后重试".to_string(),
                ));
            }
            return Ok(());
        }
        _ => return Err(duplicate_open_task_error()),
    }
    let owner_organization_id = supplier_organization_id(db, account, executor).await?;
    let responsibility = resolve_payment_responsibility(db, account, executor).await?;
    let task = new_supplier_payment_task(
        WorkItemId::new(next_id()),
        SupplierPaymentTaskSpec {
            account_id: account.base.id.clone(),
            subject_version: account.base.version.to_string(),
            owner_organization_id,
            owner_user_id: responsibility.owner_user_id.clone(),
            reason: SupplierPaymentTaskReason::Initial,
            due_at: payment_due_at(entry.due_date).map_err(Error::Logic)?,
            open_total: account.open_total,
        },
        responsibility.responsibility_key.clone(),
    )
    .map_err(Error::Logic)?;
    db.work_items().create(&task, executor).await?;
    Ok(())
}

/// 在付款核销或冲正事务内同步采购应付的付款执行任务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `account_id` - 本次核销进度发生变化的应付子账
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 摘要更新、自动完成或冲正后继任务建立成功时返回成功。
///
/// # 错误
/// 子账缺失、开放任务重复、历史责任损坏或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 部分付款不得完成任务；结清不得保留开放任务；冲正不得重开历史终态任务。
pub(crate) async fn sync_purchase_payment_task(
    db: &mongodb::Database,
    account_id: &PayableAccountId,
    executor: &mut dyn Executor,
) -> Result<()> {
    let account = db
        .payable_accounts()
        .find_by_id(account_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
    if account.source_type != PayableSourceType::PurchaseOrder {
        return Ok(());
    }
    let tasks = payment_tasks(db, &account.base.id, executor).await?;
    if tasks.is_empty() {
        return Err(Error::BusinessLogicError(
            "应付子账缺少付款执行任务，无法处理付款进度，请联系管理员修复数据".to_string(),
        ));
    }
    let open = open_tasks(&tasks);
    if open.len() > 1 {
        return Err(duplicate_open_task_error());
    }
    if account.is_settled() {
        return complete_open_task(db, open.into_iter().next(), &account, executor).await;
    }
    if let Some(task) = open.into_iter().next() {
        return update_open_task_summary(db, task, &account, executor).await;
    }
    create_reopened_task(db, &account, &tasks, executor).await
}

/// 在付款正式提交事务内校验并记录当前付款执行任务活动。
///
/// # 错误
/// 任务版本、当前责任人、应付子账、供应商或任一核销分录不属于同一任务时失败关闭。
pub(crate) async fn record_payment_execution(
    db: &mongodb::Database,
    work_item_id: &WorkItemId,
    expected_task_version: u64,
    supplier_id: &SupplierAccountId,
    allocations: &[PendingPaymentAllocation],
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let (mut task, account) =
        authorize_payment_execution(db, work_item_id, expected_task_version, None, actor, executor).await?;
    if &account.supplier_id != supplier_id {
        return Err(Error::BusinessLogicError(
            "付款供应商与当前任务的应付子账不一致".to_string(),
        ));
    }
    let account_entries = db
        .payable_entries()
        .find_entries_by_account(&PayableAccountId::new(account.base.id.as_str()), executor)
        .await?;
    let entry_ids: std::collections::HashSet<&str> = account_entries
        .iter()
        .map(|entry| entry.base.id.as_str())
        .collect();
    if allocations
        .iter()
        .any(|line| !entry_ids.contains(line.payable_entry_id.as_ref()))
    {
        return Err(Error::BusinessLogicError(
            "一次付款只能核销当前任务绑定应付子账中的分录".to_string(),
        ));
    }
    task.record_activity(actor.id(), Instant::now())
        .map_err(Error::Logic)?;
    db.work_items().update(&mut task, executor).await?;
    Ok(())
}

/// 校验付款执行任务的冻结身份、版本、当前责任人和领域权限。
///
/// 本方法不记录任务活动、不修改任务版本，可用于付款前查看敏感收款账号。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `work_item_id` - 当前付款执行任务
/// * `expected_task_version` - 页面读取的任务版本
/// * `expected_account_id` - 可选的页面应付子账身份
/// * `actor` - 当前操作人
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回已校验的任务与应付子账。
///
/// # 错误
/// 任务不存在、身份或版本漂移、非当前责任人或无处理权限时失败关闭。
pub(crate) async fn authorize_payment_execution(
    db: &mongodb::Database,
    work_item_id: &WorkItemId,
    expected_task_version: u64,
    expected_account_id: Option<&PayableAccountId>,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<(WorkItem, PayableAccount)> {
    let task = db
        .work_items()
        .find_by_id(work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商付款执行任务不存在".to_string()))?;
    if task.base.version != expected_task_version {
        return Err(Error::ConflictError(
            "付款任务版本已变化，请刷新工作台任务后重试".to_string(),
        ));
    }
    if expected_account_id.is_some_and(|id| task.business_object_id != id.as_ref()) {
        return Err(Error::BusinessLogicError(
            "付款任务与当前应付子账不一致，请刷新工作台任务后重试".to_string(),
        ));
    }
    let account = db
        .payable_accounts()
        .find_by_id(&task.business_object_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款任务关联的应付往来子账不存在".to_string()))?;
    if !matches_supplier_payment_identity(&task, &account.base.id) {
        return Err(Error::BusinessLogicError(
            "付款任务责任身份与应付子账不一致，请联系管理员修复后重试".to_string(),
        ));
    }
    if !task.is_owned_by(actor.id()) {
        return Err(Error::Forbidden(
            "当前账号不是开放付款任务的当前责任人".to_string(),
        ));
    }
    WorkItemService::new(db.clone(), crate::iam::shared_rbac_service(db.clone()))
        .ensure_domain_decision_access(actor, &task, executor)
        .await?;
    Ok((task, account))
}

/// 按当前责任规则创建冲正后继任务；没有历史任务属于损坏事实并失败关闭。
async fn create_reopened_task(
    db: &mongodb::Database,
    account: &PayableAccount,
    tasks: &[WorkItem],
    executor: &mut dyn Executor,
) -> Result<()> {
    let previous = tasks.first().ok_or_else(|| {
        Error::BusinessLogicError(
            "应付子账缺少付款执行任务，无法处理付款进度，请联系管理员修复数据".to_string(),
        )
    })?;
    if !matches_supplier_payment_identity(previous, &account.base.id) {
        return Err(Error::BusinessLogicError(
            "付款任务责任身份与应付子账不一致，请联系管理员修复后重试".to_string(),
        ));
    }
    let due_date = earliest_increase_due_date(db, account, executor).await?;
    let owner_organization_id = supplier_organization_id(db, account, executor).await?;
    let responsibility = resolve_payment_responsibility(db, account, executor).await?;
    let task = new_supplier_payment_task(
        WorkItemId::new(next_id()),
        SupplierPaymentTaskSpec {
            account_id: account.base.id.clone(),
            subject_version: account.base.version.to_string(),
            owner_organization_id,
            owner_user_id: responsibility.owner_user_id.clone(),
            reason: SupplierPaymentTaskReason::ReopenedByReversal,
            due_at: payment_due_at(due_date).map_err(Error::Logic)?,
            open_total: account.open_total,
        },
        responsibility.responsibility_key.clone(),
    )
    .map_err(Error::Logic)?;
    db.work_items().create(&task, executor).await?;
    Ok(())
}

/// 结清时系统完成唯一开放付款任务；没有开放任务视为幂等成功。
async fn complete_open_task(
    db: &mongodb::Database,
    task: Option<&WorkItem>,
    account: &PayableAccount,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(task) = task else {
        return Ok(());
    };
    if !matches_supplier_payment_identity(task, &account.base.id) {
        return Err(Error::BusinessLogicError(
            "付款任务责任身份与应付子账不一致，请联系管理员修复后重试".to_string(),
        ));
    }
    let mut task = task.clone();
    task.complete_when_payable_settled(Instant::now())
        .map_err(Error::Logic)?;
    db.work_items().update(&mut task, executor).await?;
    Ok(())
}

/// 部分付款后只更新剩余金额摘要，保持任务开放。
async fn update_open_task_summary(
    db: &mongodb::Database,
    task: &WorkItem,
    account: &PayableAccount,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !matches_supplier_payment_identity(task, &account.base.id) {
        return Err(Error::BusinessLogicError(
            "付款任务责任身份与应付子账不一致，请联系管理员修复后重试".to_string(),
        ));
    }
    let impact = supplier_payment_impact_summary(account.open_total);
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

/// 读取指定子账的全部付款任务历史。
async fn payment_tasks(
    db: &mongodb::Database,
    payable_account_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItem>> {
    db.work_items()
        .list_payment_execution_by_payable_newest_first(payable_account_id, executor)
        .await
        .map_err(Into::into)
}

/// 只保留开放任务引用。
fn open_tasks(tasks: &[WorkItem]) -> Vec<&WorkItem> {
    tasks
        .iter()
        .filter(|task| task.status == WorkItemStatus::Open)
        .collect()
}

/// 读取供应商往来主体作为付款任务责任组织。
async fn supplier_organization_id(
    db: &mongodb::Database,
    account: &PayableAccount,
    executor: &mut dyn Executor,
) -> Result<String> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(&account.supplier_id, executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("应付供应商不存在，无法形成付款任务".to_string()))?;
    Ok(supplier.party_id.to_string())
}

/// 读取子账最早的增加分录到期日，供冲正后继任务沿用付款时限（FIN-R07）。
///
/// 过滤 increase、只投影 due date、按 `(due_date, id)` 稳定排序取第一条由
/// Entry Repository 完成；无 increase 时保持原错误语义。
async fn earliest_increase_due_date(
    db: &mongodb::Database,
    account: &PayableAccount,
    executor: &mut dyn Executor,
) -> Result<BusinessDate> {
    db.payable_entries()
        .earliest_increase_due_date(&PayableAccountId::new(account.base.id.clone()), executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("应付子账缺少增加分录，无法形成付款任务".to_string()))
}

/// 按供应商精确规则、付款默认规则顺序解析当前具体负责人。
async fn resolve_payment_responsibility(
    db: &mongodb::Database,
    account: &PayableAccount,
    executor: &mut dyn Executor,
) -> Result<crate::work_item::ResolvedFinanceResponsibility> {
    WorkItemService::new(db.clone(), crate::iam::shared_rbac_service(db.clone()))
        .resolve_finance_responsibility(
            FinanceResponsibilityOperation::SupplierPayment,
            account.supplier_id.as_ref(),
            executor,
        )
        .await
}

/// 返回开放任务重复的稳定业务错误。
fn duplicate_open_task_error() -> Error {
    Error::BusinessLogicError("同一应付子账存在多个开放付款任务，请联系管理员处理后重试".to_string())
}

#[cfg(test)]
mod tests {
    use entities::common::time::BusinessDate;

    /// FIN-E06（应付部分）：时限仍为上海当日 23:59:59，由领域契约拥有。
    #[test]
    fn payment_due_uses_shanghai_end_of_day() {
        let due = super::payment_due_at(BusinessDate::from_ymd(2026, 8, 26).unwrap()).unwrap();
        assert_eq!(due.unix_secs(), 1_787_759_999);
    }

    /// FIN-E06（应付部分）：对象/类型/角色/原因/key/summary/due/identity 规则
    /// 下沉到 WorkItem 领域契约，原 Service 私有 helper 已删除。
    #[test]
    fn payment_task_rules_live_in_finance_task_contract() {
        let production = include_str!("payment_task.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        for rule in [
            "new_supplier_payment_task",
            "matches_supplier_payment_identity",
            "supplier_payment_impact_summary",
            "is_purchase_payable",
        ] {
            assert!(production.contains(rule), "缺少领域规则 {rule}");
        }
        for removed in [
            "fn ensure_purchase_payable",
            "fn new_payment_task",
            "fn ensure_task_identity",
            "fn payment_due_at",
            "fn payment_impact_summary",
            "PAYMENT_OWNER_ROLE",
            "PAYMENT_REASON",
            "PAYABLE_OBJECT_TYPE",
        ] {
            assert!(!production.contains(removed), "旧规则源未删除 {removed}");
        }
    }

    /// FIN-R07：最早增加到期日由 Entry Repository 按业务范围投影排序取得，
    /// Service 只解释无 increase 错误，不再全量过滤求最早。
    #[test]
    fn earliest_increase_due_date_uses_repository_projection() {
        let production = include_str!("payment_task.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let body = production
            .split("async fn earliest_increase_due_date")
            .nth(1)
            .expect("最早到期日函数")
            .split("\n}\n")
            .next()
            .expect("函数体");
        assert!(body.contains(".earliest_increase_due_date("));
        assert!(!body.contains(".filter("));
        assert!(!body.contains(".min()"));
        assert!(body.contains("缺少增加分录"));
    }

    /// 相同到期日按稳定次序取第一条，与 `(due_date, id)` 排序一致。
    #[test]
    fn earliest_due_date_tie_breaks_by_stable_id() {
        let first = BusinessDate::from_ymd(2026, 8, 26).unwrap();
        let later = BusinessDate::from_ymd(2026, 8, 27).unwrap();
        let mut rows = [("entry-2", first), ("entry-1", first), ("entry-3", later)];
        rows.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(right.0)));
        assert_eq!(rows[0], ("entry-1", first));
        assert_eq!(rows[1], ("entry-2", first));
    }
}
