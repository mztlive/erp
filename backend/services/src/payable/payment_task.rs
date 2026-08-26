//! 已确认采购应付与 W01 付款执行任务的原子生命周期编排。
//!
//! 任务对象固定为 `payable_account`，责任人固定为形成应付的最终审批人；付款单
//! 自身的审批任务仍由统一审批运行时维护，不与本任务合并。付款部分核销只更新
//! 摘要，开放余额归零自动完成；冲正重新产生余额时创建新任务身份。

use chrono::{FixedOffset, TimeZone};
use database::{AccessControlExt, Executor, PayableExt, SupplierExt, WorkItemExt};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{PayableAccountId, WorkItemId};
use entities::payable::{EntryDirection, PayableAccount, PayableEntry, PayableSourceType};
use entities::work_item::{
    AssignmentSource, AvailableWorkItemAccount, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus,
    WorkItemType,
};
use entities::{Permission, PermissionSet};
use id_generator::next_id;

use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

const PAYMENT_OWNER_ROLE: &str = "role-finance";
const PAYMENT_REASON: &str = "PAYABLE_PAYMENT_REQUIRED";
const PAYABLE_OBJECT_TYPE: &str = "payable_account";

/// 为采购最终通过形成的应付建立唯一开放付款执行任务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `account` - 已在当前事务形成的采购应付子账
/// * `entry` - 与子账一同形成的原始应付分录
/// * `owner_user_id` - 最终通过采购单的具体财务账号
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
    owner_user_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    ensure_purchase_payable(account, entry)?;
    let tasks = payment_tasks(db, &account.base.id, executor).await?;
    let open = open_tasks(&tasks);
    match open.as_slice() {
        [] => {}
        [task] => return ensure_task_identity(task, account),
        _ => return Err(duplicate_open_task_error()),
    }
    let owner_organization_id = supplier_organization_id(db, account, executor).await?;
    let rbac = crate::iam::shared_rbac_service(db.clone());
    ensure_payment_owner_eligible(db, &rbac, owner_user_id, executor).await?;
    let task = new_payment_task(account, entry.due_date, owner_user_id, owner_organization_id)?;
    db.work_items().create(&task, executor).await?;
    Ok(())
}

/// 在付款核销或冲正事务内同步采购应付的付款执行任务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `account_id` - 本次核销进度发生变化的应付子账
/// * `fallback_owner_user_id` - 旧数据没有任务历史时使用的当前财务操作人
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
    fallback_owner_user_id: &str,
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
    create_reopened_or_legacy_task(db, &account, &tasks, fallback_owner_user_id, executor).await
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

/// 创建冲正后继任务；旧数据没有历史时按当前财务操作人补建。
async fn create_reopened_or_legacy_task(
    db: &mongodb::Database,
    account: &PayableAccount,
    tasks: &[WorkItem],
    fallback_owner_user_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let impact = Some(payment_impact_summary(account));
    if let Some(previous) = tasks.first() {
        ensure_task_identity(previous, account)?;
        let task = previous
            .successor_for_reopened_payable(
                WorkItemId::new(next_id()),
                account.base.version.to_string(),
                impact,
            )
            .map_err(Error::Logic)?;
        db.work_items().create(&task, executor).await?;
        return Ok(());
    }
    let due_date = earliest_increase_due_date(db, account, executor).await?;
    let owner_organization_id = supplier_organization_id(db, account, executor).await?;
    let rbac = crate::iam::shared_rbac_service(db.clone());
    ensure_payment_owner_eligible(db, &rbac, fallback_owner_user_id, executor).await?;
    let task = new_payment_task(account, due_date, fallback_owner_user_id, owner_organization_id)?;
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
    if task.impact_summary.as_deref() == Some(impact.as_str()) {
        return Ok(());
    }
    let mut task = task.clone();
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
            reason_code: Some(PAYMENT_REASON.to_string()),
            impact_summary: Some(payment_impact_summary(account)),
        },
        payment_responsibility_key(&account.base.id),
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
        && task.responsibility_key() == Some(payment_responsibility_key(&account.base.id).as_str())
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

/// 读取子账最早的增加分录到期日，供旧数据补建任务。
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

/// 校验付款任务具体负责人可登录且具备完整 W12 登记、查看和提交权限。
async fn ensure_payment_owner_eligible(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    owner_user_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let required_codes = WorkItemType::SupplierPaymentExecution
        .supplier_payment_execution_permissions(PAYABLE_OBJECT_TYPE)
        .ok_or_else(|| Error::Internal("付款执行权限合同未注册".to_string()))?;
    let account = db
        .accounts()
        .find_work_item_account(owner_user_id, executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("付款责任人账号不存在，请调整审批责任后重试".to_string()))?;
    AvailableWorkItemAccount::from_account(&account)
        .map_err(|_| Error::BusinessLogicError("付款责任人账号不可用，请调整审批责任后重试".to_string()))?;
    let granted = PermissionSet::new(rbac.permissions(account.kind, owner_user_id).await?);
    let required = PermissionSet::new(
        required_codes
            .iter()
            .map(|code| Permission::parse(code).expect("固定付款执行权限必须合法")),
    );
    if granted.covers(&required) {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "付款责任人缺少登记或提交付款所需权限，请先调整财务角色后重试".to_string(),
    ))
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

/// 返回稳定的一子账一责任键。
fn payment_responsibility_key(payable_account_id: &str) -> String {
    format!("payable_account:{payable_account_id}")
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
