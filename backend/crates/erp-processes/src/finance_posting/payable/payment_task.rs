//! 已确认采购应付与 W01 付款执行任务的原子生命周期编排。
//!
//! 任务对象固定为 `payable_account`，负责人由供应商精确责任规则或付款默认规则
//! 解析并冻结。采购审批最终通过已经提供付款授权；付款单不再创建第二套审批
//! 任务。付款部分核销只更新摘要，开放余额归零自动完成；冲正重新产生余额时按
//! 当前责任规则创建新任务身份。

use application_core::AuditActor;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{PayableAccountId, SupplierAccountId, WorkItemId};
use erp_finance::entity::payable::{
    EntryDirection, PayableAccount, PayableEntry, PayableSourceType, PendingPaymentAllocation,
};
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_supplier::SupplierExt;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{
    FinanceResponsibilityOperation, PayablePurchaseAdmissionFact, PaymentExecutionMergeMember,
    PaymentExecutionMergeSet, SupplierPaymentTaskReason, SupplierPaymentTaskSpec, WorkItem, WorkItemStatus,
    is_purchase_payable, matches_supplier_payment_identity, new_supplier_payment_task, payment_due_at,
    supplier_payment_impact_summary,
};
use erp_workflow::repository::prelude::*;
use id_generator::next_id;
use persistence_core::Executor;

use crate::adapters::workflow::work_item_service;
use crate::{Error, Result};

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
    if !is_purchase_payable(&PayablePurchaseAdmissionFact {
        account_id: account.base.id.as_ref(),
        source_type_is_purchase_order: account.source_type == PayableSourceType::PurchaseOrder,
        source_document_id: account.source_document_id.as_ref(),
        is_settled: account.is_settled(),
        entry_payable_account_id: entry.payable_account_id.as_ref(),
        entry_direction_is_increase: entry.direction == EntryDirection::Increase,
        entry_source_document_id: entry.source_document_id.as_ref(),
    }) {
        return Err(Error::BusinessLogicError(
            "采购应付事实不完整，无法形成付款任务，请检查应付分录后重试".to_string(),
        ));
    }
    let tasks = payment_tasks(db, &account.base.id, executor).await?;
    let open = open_tasks(&tasks);
    match open.as_slice() {
        [] if tasks.is_empty() => {},
        [] => {
            return Err(Error::BusinessLogicError(
                "应付子账已存在付款任务历史，不能重复建立初始任务".to_string(),
            ));
        },
        [task] => {
            if !matches_supplier_payment_identity(task, &account.base.id) {
                return Err(Error::BusinessLogicError(
                    "付款任务责任身份与应付子账不一致，请联系管理员修复后重试".to_string(),
                ));
            }
            return Ok(());
        },
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

/// 一次付款执行要校验的任务集合与核销范围。
pub(crate) struct PaymentExecutionCommand<'a> {
    /// 当前工作台打开的付款执行任务。
    pub work_item_id: &'a WorkItemId,
    /// 页面读取的当前任务版本。
    pub expected_task_version: u64,
    /// 合并打款勾选的其它任务及其版本；单任务付款为空。
    pub additional_tasks: &'a [(WorkItemId, u64)],
    /// 本次付款单上的供应商。
    pub supplier_id: &'a SupplierAccountId,
    /// 待过账核销行。
    pub allocations: &'a [PendingPaymentAllocation],
}

/// 在付款正式提交事务内校验并记录当前付款执行任务活动。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `command` - 当前任务、附加任务、供应商与核销范围
/// * `actor` - 当前出纳
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 全部已勾选任务授权通过且核销范围合法时返回成功。
///
/// # 错误
/// 任务版本、当前责任人、应付子账、供应商或核销分录不属于已勾选任务时失败关闭。
///
/// # 关键业务约束
/// 一次打款必须覆盖每条已勾选任务，且不得核销未勾选应付。
pub(crate) async fn record_payment_execution(
    db: &mongodb::Database,
    command: PaymentExecutionCommand<'_>,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let merge = load_authorized_merge_set(
        db,
        command.work_item_id,
        command.expected_task_version,
        command.additional_tasks,
        actor,
        executor,
    )
    .await?;
    if merge.supplier_id() != command.supplier_id.as_ref() {
        return Err(Error::BusinessLogicError("付款供应商与当前任务的应付子账不一致".to_string()));
    }
    ensure_allocations_match_merge_set(db, &merge, command.allocations, executor).await?;
    record_merge_set_activity(db, &merge, actor, executor).await
}

/// 授权当前任务与附加任务并构造合并集合。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `work_item_id` - 当前付款执行任务
/// * `expected_task_version` - 当前任务版本
/// * `additional_tasks` - 附加任务身份与版本
/// * `actor` - 当前出纳
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回已通过身份校验的合并集合。
///
/// # 错误
/// 任一任务未授权、版本冲突或成员集合不合法时失败关闭。
async fn load_authorized_merge_set(
    db: &mongodb::Database,
    work_item_id: &WorkItemId,
    expected_task_version: u64,
    additional_tasks: &[(WorkItemId, u64)],
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<PaymentExecutionMergeSet> {
    let mut members = Vec::with_capacity(additional_tasks.len() + 1);
    let (_, account) =
        authorize_payment_execution(db, work_item_id, expected_task_version, None, actor, executor).await?;
    members.push(merge_member(work_item_id, &account));
    for (task_id, task_version) in additional_tasks {
        let (_, account) =
            authorize_payment_execution(db, task_id, *task_version, None, actor, executor).await?;
        members.push(merge_member(task_id, &account));
    }
    PaymentExecutionMergeSet::try_new(members).map_err(|error| Error::BusinessLogicError(error.to_string()))
}

/// 由已授权应付构造合并成员事实。
///
/// # 参数
/// * `work_item_id` - 付款执行任务
/// * `account` - 任务绑定的应付子账
///
/// # 返回
/// 返回供应商与应付身份已冻结的成员。
///
/// # 错误
/// 无。
fn merge_member(work_item_id: &WorkItemId, account: &PayableAccount) -> PaymentExecutionMergeMember {
    PaymentExecutionMergeMember {
        work_item_id: work_item_id.clone(),
        payable_account_id: account.base.id.clone(),
        supplier_id: account.supplier_id.to_string(),
    }
}

/// 校验核销分录全部属于已勾选任务，且每条任务都有分配。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `merge` - 已授权合并集合
/// * `allocations` - 待过账核销行
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 核销范围合法时返回成功。
///
/// # 错误
/// 分录不存在或不属于已勾选应付时失败关闭。
async fn ensure_allocations_match_merge_set(
    db: &mongodb::Database,
    merge: &PaymentExecutionMergeSet,
    allocations: &[PendingPaymentAllocation],
    executor: &mut dyn Executor,
) -> Result<()> {
    let account_ids: Vec<PayableAccountId> =
        merge.payable_account_ids().into_iter().map(PayableAccountId::new).collect();
    let entries = db.payable_entries().find_entries_by_accounts(&account_ids, executor).await?;
    let account_by_entry: std::collections::HashMap<&str, &str> =
        entries.iter().map(|entry| (entry.base.id.as_str(), entry.payable_account_id.as_ref())).collect();
    let mut allocation_accounts = Vec::with_capacity(allocations.len());
    for line in allocations {
        let account_id = account_by_entry.get(line.payable_entry_id.as_ref()).ok_or_else(|| {
            Error::BusinessLogicError(if merge.is_merged() {
                "一次付款只能核销已勾选付款任务对应应付中的分录".to_string()
            } else {
                "一次付款只能核销当前任务绑定应付子账中的分录".to_string()
            })
        })?;
        allocation_accounts.push((*account_id).to_string());
    }
    merge
        .ensure_allocations_in_scope(&allocation_accounts)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))
}

/// 为合并集合内全部开放任务记录本次付款活动。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `merge` - 已授权合并集合
/// * `actor` - 当前出纳
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 全部任务活动写入成功时返回成功。
///
/// # 错误
/// 任务读取、活动规则或仓储更新失败时返回错误。
async fn record_merge_set_activity(
    db: &mongodb::Database,
    merge: &PaymentExecutionMergeSet,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let occurred_at = Instant::now();
    for member in merge.members() {
        let mut task = db
            .work_items()
            .find_by_id(&member.work_item_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商付款执行任务不存在".to_string()))?;
        task.record_activity(actor.id(), occurred_at).map_err(Error::Logic)?;
        db.work_items().update(&mut task, executor).await?;
    }
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
        return Err(Error::ConflictError("付款任务版本已变化，请刷新工作台任务后重试".to_string()));
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
        return Err(Error::Forbidden("当前账号不是开放付款任务的当前责任人".to_string()));
    }
    work_item_service(db.clone(), crate::adapters::identity::shared_rbac_service(db.clone()))
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
    task.complete_when_payable_settled(Instant::now()).map_err(Error::Logic)?;
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
    tasks.iter().filter(|task| task.status == WorkItemStatus::Open).collect()
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
) -> Result<erp_workflow::service::work_item::ResolvedFinanceResponsibility> {
    work_item_service(db.clone(), crate::adapters::identity::shared_rbac_service(db.clone()))
        .resolve_finance_responsibility(
            FinanceResponsibilityOperation::SupplierPayment,
            account.supplier_id.as_ref(),
            executor,
        )
        .await
        .map_err(Error::from)
}

/// 返回开放任务重复的稳定业务错误。
fn duplicate_open_task_error() -> Error {
    Error::BusinessLogicError("同一应付子账存在多个开放付款任务，请联系管理员处理后重试".to_string())
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;

    /// FIN-E06（应付部分）：时限仍为上海当日 23:59:59，由领域契约拥有。
    #[test]
    fn payment_due_uses_shanghai_end_of_day() {
        let due = super::payment_due_at(BusinessDate::from_ymd(2026, 8, 26).unwrap()).unwrap();
        assert_eq!(due.unix_secs(), 1_787_759_999);
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
