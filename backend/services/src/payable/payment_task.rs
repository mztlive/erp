//! 已确认采购应付与 W01 付款执行任务的原子生命周期编排。
//!
//! 任务对象固定为 `payable_account`，负责人由供应商精确责任规则或付款默认规则
//! 解析并冻结；付款单自身的审批任务仍由统一审批运行时维护，不与本任务合并。
//! 付款部分核销只更新摘要，开放余额归零自动完成；冲正重新产生余额时按当前
//! 责任规则创建新任务身份。

use chrono::{FixedOffset, TimeZone};
use database::{Executor, PayableExt, SupplierExt, WorkItemExt};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{PayableAccountId, SupplierAccountId, WorkItemId};
use entities::payable::{
    EntryDirection, PayableAccount, PayableEntry, PayableSourceType, PendingPaymentAllocation,
};
use entities::work_item::{
    AssignmentSource, FinanceResponsibilityOperation, WorkItem, WorkItemData, WorkItemPriority,
    WorkItemStatus, WorkItemType,
};
use id_generator::next_id;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::work_item::WorkItemService;

const PAYMENT_OWNER_ROLE: &str = "role-finance";
const PAYMENT_REASON: &str = "PAYABLE_PAYMENT_REQUIRED";
const PAYABLE_OBJECT_TYPE: &str = "payable_account";

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
    ensure_purchase_payable(account, entry)?;
    let tasks = payment_tasks(db, &account.base.id, executor).await?;
    let open = open_tasks(&tasks);
    match open.as_slice() {
        [] if tasks.is_empty() => {}
        [] => {
            return Err(Error::BusinessLogicError(
                "应付子账已存在付款任务历史，不能重复建立初始任务".to_string(),
            ));
        }
        [task] => return ensure_task_identity(task, account),
        _ => return Err(duplicate_open_task_error()),
    }
    let owner_organization_id = supplier_organization_id(db, account, executor).await?;
    let responsibility = resolve_payment_responsibility(db, account, executor).await?;
    let task = new_payment_task(
        account,
        entry.due_date,
        &responsibility.owner_user_id,
        owner_organization_id,
        &responsibility.responsibility_key,
        PAYMENT_REASON,
    )?;
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
    let mut task = db
        .work_items()
        .find_by_id(work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商付款执行任务不存在".to_string()))?;
    if task.base.version != expected_task_version {
        return Err(Error::ConflictError(
            "付款任务版本已变化，请刷新工作台任务后重试".to_string(),
        ));
    }
    let account = db
        .payable_accounts()
        .find_by_id(&task.business_object_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款任务关联的应付往来子账不存在".to_string()))?;
    ensure_task_identity(&task, &account)?;
    if !task.is_owned_by(actor.id()) {
        return Err(Error::Forbidden(
            "当前账号不是开放付款任务的当前责任人".to_string(),
        ));
    }
    WorkItemService::new(db.clone(), crate::iam::shared_rbac_service(db.clone()))
        .ensure_domain_decision_access(actor, &task, executor)
        .await?;
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

/// 校验采购应付与原始分录属于同一正式事实。
fn ensure_purchase_payable(account: &PayableAccount, entry: &PayableEntry) -> Result<()> {
    let matches = account.source_type == PayableSourceType::PurchaseOrder
        && entry.payable_account_id.as_ref() == account.base.id
        && entry.direction == EntryDirection::Increase
        && entry.source_document_id == account.source_document_id;
    if matches && !account.is_settled() {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "采购应付事实不完整，无法形成付款任务，请检查应付分录后重试".to_string(),
    ))
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
    ensure_task_identity(previous, account)?;
    let due_date = earliest_increase_due_date(db, account, executor).await?;
    let owner_organization_id = supplier_organization_id(db, account, executor).await?;
    let responsibility = resolve_payment_responsibility(db, account, executor).await?;
    let task = new_payment_task(
        account,
        due_date,
        &responsibility.owner_user_id,
        owner_organization_id,
        &responsibility.responsibility_key,
        "PAYABLE_REOPENED_BY_REVERSAL",
    )?;
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
    ensure_task_identity(task, account)?;
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
    ensure_task_identity(task, account)?;
    let impact = payment_impact_summary(account);
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

/// 构造带计划付款时限和具体财务责任人的付款任务。
fn new_payment_task(
    account: &PayableAccount,
    due_date: BusinessDate,
    owner_user_id: &str,
    owner_organization_id: String,
    responsibility_key: &str,
    reason_code: &str,
) -> Result<WorkItem> {
    WorkItem::new_with_responsibility_key(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: WorkItemType::SupplierPaymentExecution,
            business_object_type: PAYABLE_OBJECT_TYPE.to_string(),
            business_object_id: account.base.id.clone(),
            subject_version: account.base.version.to_string(),
            owner_role: PAYMENT_OWNER_ROLE.to_string(),
            owner_organization_id,
            owner_user_id: owner_user_id.to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: Some(payment_due_at(due_date)?),
            reason_code: Some(reason_code.to_string()),
            impact_summary: Some(payment_impact_summary(account)),
        },
        responsibility_key,
    )
    .map_err(Error::Logic)
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

/// 校验开放任务的对象、类型、责任键和原因码均属于当前应付。
fn ensure_task_identity(task: &WorkItem, account: &PayableAccount) -> Result<()> {
    let matches = task.work_item_type == WorkItemType::SupplierPaymentExecution
        && task.business_object_type == PAYABLE_OBJECT_TYPE
        && task.business_object_id == account.base.id
        && task.owner_role == PAYMENT_OWNER_ROLE
        && task
            .responsibility_key()
            .is_some_and(|key| key.starts_with("finance:SUPPLIER_PAYMENT:"))
        && matches!(
            task.reason_code.as_deref(),
            Some(PAYMENT_REASON | "PAYABLE_REOPENED_BY_REVERSAL")
        );
    if matches {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "付款任务责任身份与应付子账不一致，请联系管理员修复后重试".to_string(),
    ))
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

/// 读取子账最早的增加分录到期日，供冲正后继任务沿用付款时限。
async fn earliest_increase_due_date(
    db: &mongodb::Database,
    account: &PayableAccount,
    executor: &mut dyn Executor,
) -> Result<BusinessDate> {
    db.payable_entries()
        .find_entries_by_account(&PayableAccountId::new(account.base.id.clone()), executor)
        .await?
        .into_iter()
        .filter(|entry| entry.direction == EntryDirection::Increase)
        .map(|entry| entry.due_date)
        .min()
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

/// 把业务自然日转换为上海时区当日 23:59:59 的工作项时限。
fn payment_due_at(due_date: BusinessDate) -> Result<Instant> {
    let (year, month, day) = due_date.ymd();
    let timezone = FixedOffset::east_opt(8 * 3600).expect("上海固定时差必须合法");
    let due_at = timezone
        .with_ymd_and_hms(year, month, day, 23, 59, 59)
        .single()
        .ok_or_else(|| Error::Internal("计划付款日无法转换为工作项时限".to_string()))?;
    Ok(Instant::from_unix_secs(due_at.timestamp()))
}

/// 返回随开放余额变化的付款影响摘要。
fn payment_impact_summary(account: &PayableAccount) -> String {
    format!("未付金额 ¥{}，请按付款条件登记付款", account.open_total)
}

/// 返回开放任务重复的稳定业务错误。
fn duplicate_open_task_error() -> Error {
    Error::BusinessLogicError("同一应付子账存在多个开放付款任务，请联系管理员处理后重试".to_string())
}

#[cfg(test)]
mod tests {
    use super::payment_due_at;
    use entities::common::time::BusinessDate;

    #[test]
    fn payment_due_uses_shanghai_end_of_day() {
        let due = payment_due_at(BusinessDate::from_ymd(2026, 8, 26).unwrap()).unwrap();
        assert_eq!(due.unix_secs(), 1_787_759_999);
    }
}
