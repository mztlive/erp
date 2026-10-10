//! 只有已批准申请专属任务可执行付款，逐应付授权不可移用。
use std::collections::BTreeMap;

use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_core::ids::{PayableAccountId, WorkItemId};
use erp_finance::entity::payable::{
    PayableAccount, PayableEntry, PaymentRequestAllocation, PendingPaymentAllocation, SupplierPayment,
};
use erp_finance::repository::PurchasePaymentRequestSnapshotExt;
use erp_identity::{AccessControlExt, Permission, RoleRepositoryExt};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{
    FinanceResponsibilityOperation, SupplierPaymentTaskReason, SupplierPaymentTaskSpec, WorkItem,
    WorkItemStatus, matches_purchase_payment_request_identity, new_approved_purchase_payment_request_task,
    payment_due_at,
};
use id_generator::next_id;

use super::*;
use crate::adapters::workflow::work_item_service;
use crate::audit::persist_log;

/// 最终审批通过后授予原逐行额度并派发指定出纳任务。
/// # 参数
/// * `db` - 数据库。
/// * `id` - 申请身份。
/// * `actor` - 已授权财务审批人。
/// * `executor` - 审批运行时同一事务。
/// # 返回
/// 返回批准及任务创建成功结果。
/// # 错误
/// 来源、账户、余额、状态或指定责任人不合法时拒绝。
pub(crate) async fn approve(
    db: &Database,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut request = load(db, id, executor).await?;
    if request.created_by == actor.id() {
        return Err(Error::Forbidden("付款申请人不得审批本人申请".into()));
    }
    source::revalidate(db, &request, executor).await?;
    request.approve()?;
    let rbac = shared_rbac_service(db.clone());
    let responsibility = work_item_service(db.clone(), rbac)
        .resolve_finance_responsibility(
            FinanceResponsibilityOperation::SupplierPayment,
            &request.supplier_id,
            executor,
        )
        .await?;
    let task = new_approved_purchase_payment_request_task(
        WorkItemId::new(next_id()),
        SupplierPaymentTaskSpec {
            account_id: request.base.id.clone(),
            subject_version: request.submission_version.to_string(),
            owner_organization_id: business_org(&request)?.to_owned(),
            owner_user_id: responsibility.owner_user_id,
            reason: SupplierPaymentTaskReason::Initial,
            due_at: payment_due_at(request.data.desired_payment_date)?,
            open_total: request.remaining(),
        },
        &responsibility.responsibility_key,
        &request.base.id,
        &request.request_no,
    )?;
    request.work_item_id = Some(task.base.id.clone());
    db.work_items().create(&task, executor).await?;
    db.purchase_payment_requests().update(&mut request, executor).await?;
    let audit = actor.clone().resource_log(
        "purchase_payment_request.approve",
        "purchase_payment_request",
        id.to_owned(),
    )?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

pub(crate) struct PaymentRequestExecutionFacts {
    pub accounts: Vec<PayableAccount>,
    pub entries: Vec<PayableEntry>,
}

/// 同一付款命令的精确申请、任务、实际付款与分配事实。
pub(crate) struct PaymentRequestExecutionInput<'a> {
    pub request_id: &'a str,
    pub version: u64,
    pub task_id: &'a WorkItemId,
    pub task_version: u64,
    pub payment: &'a mut SupplierPayment,
    pub allocations: &'a [PendingPaymentAllocation],
    pub actor: &'a AuditActor,
}

/// 命令恢复沿原付款与冻结任务授权；终止任务仍可证明已经完成的原命令。
/// # 参数
/// * `db` / `rbac` / `executor` - 当前数据库、授权服务与查询事务。
/// * `request_id` / `task_id` / `payment` - 原命令的申请、任务和实际付款。
/// * `actor` - 原命令操作人，仍须具有当前有效结果读取权限。
/// # 返回
/// 当前身份和不可变来源关系均成立时允许返回原付款结果。
/// # 错误
/// 账号或角色失效、任务责任改变、冻结依据缺失或付款关系不一致时拒绝。
pub(crate) async fn authorize_payment_result(
    db: &Database,
    rbac: &SharedRbacService,
    request_id: &str,
    task_id: &WorkItemId,
    payment: &SupplierPayment,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    require_result_permissions(db, rbac, actor, executor).await?;
    if payment.created_by != actor.id() || payment.payment_request_id.as_deref() != Some(request_id) {
        return Err(Error::Forbidden("付款命令结果不属于当前操作人或申请".into()));
    }
    let task = db
        .work_items()
        .find_by_id(task_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原付款申请任务不存在".into()))?;
    if !can_read_payment_result_task(&task, request_id, actor.id()) {
        return Err(Error::Forbidden("原付款任务责任资格已变化".into()));
    }
    let version = task
        .subject_version
        .parse::<u32>()
        .map_err(|_| Error::ConflictError("原付款任务提交版本无效".into()))?;
    let snapshot = db
        .purchase_payment_request_snapshots()
        .find_for_version(request_id, version, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("原付款缺少冻结来源依据".into()))?;
    basis::validate_snapshot(payment, &snapshot)
}

fn can_read_payment_result_task(task: &WorkItem, request_id: &str, actor_id: &str) -> bool {
    matches_purchase_payment_request_identity(task, request_id)
        && task.owner_user_id.as_deref() == Some(actor_id)
}

async fn require_result_permissions(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.accounts()
        .find_by_id(actor.id(), executor)
        .await?
        .filter(|account| account.is_active_backoffice() && account.kind == actor.kind())
        .ok_or_else(|| Error::Forbidden("付款结果读取账号已失效".into()))?;
    let required = [
        Permission::parse("purchase_payment_request:detail")?,
        Permission::parse("purchase_payment_request:pay")?,
    ];
    let snapshot = rbac.role_permission_snapshot(actor.kind(), actor.id(), &required).await?;
    for permission in required {
        if db.roles().enabled_roles(&snapshot.granting_role_ids(&permission), executor).await?.is_empty() {
            return Err(Error::Forbidden("付款结果读取权限已撤销".into()));
        }
    }
    rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
    Ok(())
}

/// 授权当前指定出纳并消耗该申请各采购行的实际付款金额。
/// # 参数
/// * `db` / `rbac` - 数据库与权限服务。
/// * `input` - 页面所见申请和任务版本、实际付款、分配及指定出纳。
/// * `executor` - 当前付款事务。
/// # 返回
/// 返回已在同一事务重验的应付和分录。
/// # 错误
/// 非批准任务、责任人、冻结账户或金额范围变化时拒绝。
pub(crate) async fn record_payment_execution(
    db: &Database,
    rbac: &SharedRbacService,
    input: PaymentRequestExecutionInput<'_>,
    executor: &mut dyn Executor,
) -> Result<PaymentRequestExecutionFacts> {
    let PaymentRequestExecutionInput {
        request_id,
        version,
        task_id,
        task_version,
        payment,
        allocations,
        actor,
    } = input;
    let mut request = load(db, request_id, executor).await?;
    ensure_version(&request, version)?;
    let mut task = authorize_task(db, rbac, &request, task_id, task_version, actor, executor).await?;
    source::revalidate(db, &request, executor).await?;
    let snapshot =
        request.payee_account.as_ref().ok_or_else(|| Error::ConflictError("批准申请缺少账户".into()))?;
    if payment.supplier_id.as_ref() != request.supplier_id
        || payment.payee_bank_account_id.as_ref().map(AsRef::as_ref) != Some(snapshot.account_id.as_str())
    {
        return Err(Error::ConflictError("实际付款供应商或收款账户与申请不一致".into()));
    }
    let ids = request.lines.iter().map(|l| PayableAccountId::new(&l.payable_account_id)).collect::<Vec<_>>();
    let entries = db.payable_entries().find_entries_by_accounts(&ids, executor).await?;
    let request_allocations = allocations_by_account(&entries, allocations)?;
    request.record_payment(&request_allocations)?;
    for allocation in &request_allocations {
        if !db
            .payable_accounts()
            .consume_payment_request(&allocation.payable_account_id, &allocation.amount, actor.id(), executor)
            .await?
        {
            return Err(Error::ConflictError("批准占用金额已变化".into()));
        }
    }
    let accounts = db.payable_accounts().find_accounts_by_ids(&ids, executor).await?;
    payment.payment_request_id = Some(request.base.id.clone());
    payment.payer_party_id = Some(request.payer_party_id.clone());
    payment.payer_party_name = Some(request.payer_party_name.clone());
    payment.request_allocations = request_allocations;
    task.record_activity(actor.id(), Instant::now())?;
    db.work_items().update(&mut task, executor).await?;
    db.purchase_payment_requests().update(&mut request, executor).await?;
    audit_payment(db, &request.base.id, actor, executor).await?;
    Ok(PaymentRequestExecutionFacts { accounts, entries })
}

async fn audit_payment(
    db: &Database,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let audit = actor.clone().resource_log(
        "purchase_payment_request.pay",
        "purchase_payment_request",
        id.to_owned(),
    )?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

pub(super) async fn authorize_task(
    db: &Database,
    rbac: &SharedRbacService,
    request: &PurchasePaymentRequest,
    task_id: &WorkItemId,
    version: u64,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<erp_workflow::entity::work_item::WorkItem> {
    let task = db
        .work_items()
        .find_by_id(task_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("申请付款任务不存在".into()))?;
    if task.base.version != version {
        return Err(Error::ConflictError("付款任务版本已变化，请刷新后重试".into()));
    }
    if !matches_purchase_payment_request_identity(&task, &request.base.id)
        || request.work_item_id.as_deref() != Some(task.base.id.as_str())
        || task.subject_version != request.submission_version.to_string()
        || task.status != WorkItemStatus::Open
        || !matches!(
            request.status,
            PurchasePaymentRequestStatus::Approved | PurchasePaymentRequestStatus::PartiallyPaid
        )
    {
        return Err(Error::ConflictError("此任务没有当前有效的已批准付款申请".into()));
    }
    if !task.is_owned_by(actor.id()) {
        return Err(Error::Forbidden("只有该申请指定出纳可以登记付款".into()));
    }
    work_item_service(db.clone(), rbac.clone()).ensure_domain_decision_access(actor, &task, executor).await?;
    Ok(task)
}

fn allocations_by_account(
    entries: &[PayableEntry],
    allocations: &[PendingPaymentAllocation],
) -> Result<Vec<PaymentRequestAllocation>> {
    let mut amounts = BTreeMap::<String, Amount>::new();
    for allocation in allocations {
        let entry = entries
            .iter()
            .find(|e| e.base.id == allocation.payable_entry_id.as_ref())
            .ok_or_else(|| Error::ConflictError("付款分录不在批准申请采购范围内".into()))?;
        let total = amounts.entry(entry.payable_account_id.to_string()).or_insert(Amount::zero());
        *total = Amount::try_from(
            total
                .to_decimal()
                .checked_add(allocation.allocated_amount.to_decimal())
                .ok_or_else(|| Error::Logic("付款分配汇总溢出".into()))?,
        )?;
    }
    Ok(amounts
        .into_iter()
        .map(|(payable_account_id, amount)| PaymentRequestAllocation { payable_account_id, amount })
        .collect())
}

pub(crate) async fn sync_task(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<()> {
    let request = load(db, id, executor).await?;
    let task_id =
        request.work_item_id.as_ref().ok_or_else(|| Error::ConflictError("申请付款任务不存在".into()))?;
    let mut task = db
        .work_items()
        .find_by_id(task_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("申请付款任务不存在".into()))?;
    if task.status != WorkItemStatus::Open {
        return Ok(());
    }
    if request.remaining() == Amount::zero() {
        task.complete_when_payment_request_paid(Instant::now())?;
    } else {
        task.update_impact_summary(Some(format!(
            "申请 {} · 待付款 {} 元",
            request.request_no,
            request.remaining()
        )))?;
    }
    task.subject_version = request.submission_version.to_string();
    db.work_items().update(&mut task, executor).await?;
    Ok(())
}

/// 付款冲正后仅在原批准申请尚未关闭时恢复占用与执行任务。
/// # 参数
/// * `db` - 数据库。
/// * `payment` - 原付款及逐申请采购分配。
/// * `allocations` - 本次冲正按应付汇总的金额。
/// * `actor` - 已授权冲正操作人。
/// * `executor` - 冲正事务。
/// # 返回
/// 原申请净付款、占用与任务保持一致。
/// # 错误
/// 超原付款、来源或账户变化、责任规则失效时拒绝恢复。
pub(crate) async fn restore_reversed_payment(
    db: &Database,
    payment: &SupplierPayment,
    allocations: &[PaymentRequestAllocation],
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let id =
        payment.require_payment_request_basis().map_err(|error| Error::ConflictError(error.to_string()))?;
    let mut request = load(db, id, executor).await?;
    basis::revalidate(db, payment, &request, allocations, executor).await?;
    let was_closed = request.status == PurchasePaymentRequestStatus::Closed;
    request.reverse_payment(allocations)?;
    if !was_closed {
        source::revalidate(db, &request, executor).await?;
        for allocation in allocations {
            if !db
                .payable_accounts()
                .restore_payment_request(
                    &allocation.payable_account_id,
                    &allocation.amount,
                    actor.id(),
                    executor,
                )
                .await?
            {
                return Err(Error::ConflictError("冲正后无法恢复原批准占用，请处理申请".into()));
            }
        }
    }
    db.purchase_payment_requests().update(&mut request, executor).await?;
    if !was_closed {
        reopen_task(db, &mut request, executor).await?;
    }
    Ok(())
}

async fn reopen_task(
    db: &Database,
    request: &mut PurchasePaymentRequest,
    executor: &mut dyn Executor,
) -> Result<()> {
    if let Some(id) = &request.work_item_id
        && let Some(task) = db.work_items().find_by_id(id, executor).await?
        && task.status == WorkItemStatus::Open
    {
        if !restored_task_matches(&task, &request.base.id, request.submission_version) {
            return Err(Error::ConflictError("原付款申请执行责任已变化，请先处理任务资格后冲正".into()));
        }
        require_current_task_owner(db, &task, executor).await?;
        return sync_task(db, &request.base.id, executor).await;
    }
    create_reopened_task(db, request, executor).await
}

async fn require_current_task_owner(
    db: &Database,
    task: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<()> {
    let owner_id = task
        .owner_user_id
        .as_deref()
        .ok_or_else(|| Error::ConflictError("原付款申请任务缺少指定出纳".into()))?;
    let owner = db
        .accounts()
        .find_by_id(owner_id, executor)
        .await?
        .filter(|account| account.is_active_backoffice())
        .ok_or_else(|| Error::Forbidden("原付款申请任务的当前出纳账号已失效".into()))?;
    // 仅作为当前任务责任人的资格事实；冲正审计仍使用真实冲正操作人。
    let actor = AuditActor::new(owner.base.id, owner.secret.account().to_string(), owner.kind);
    work_item_service(db.clone(), shared_rbac_service(db.clone()))
        .ensure_domain_decision_access(&actor, task, executor)
        .await?;
    Ok(())
}

async fn create_reopened_task(
    db: &Database,
    request: &mut PurchasePaymentRequest,
    executor: &mut dyn Executor,
) -> Result<()> {
    let responsibility = work_item_service(db.clone(), shared_rbac_service(db.clone()))
        .resolve_finance_responsibility(
            FinanceResponsibilityOperation::SupplierPayment,
            &request.supplier_id,
            executor,
        )
        .await?;
    let task = new_approved_purchase_payment_request_task(
        WorkItemId::new(next_id()),
        SupplierPaymentTaskSpec {
            account_id: request.base.id.clone(),
            subject_version: request.submission_version.to_string(),
            owner_organization_id: business_org(request)?.to_owned(),
            owner_user_id: responsibility.owner_user_id,
            reason: SupplierPaymentTaskReason::ReopenedByReversal,
            due_at: payment_due_at(request.data.desired_payment_date)?,
            open_total: request.remaining(),
        },
        &responsibility.responsibility_key,
        &request.base.id,
        &request.request_no,
    )?;
    request.work_item_id = Some(task.base.id.clone());
    db.work_items().create(&task, executor).await?;
    db.purchase_payment_requests().update(request, executor).await?;
    Ok(())
}

fn restored_task_matches(task: &WorkItem, request_id: &str, submission_version: u32) -> bool {
    task.status == WorkItemStatus::Open
        && task.owner_user_id.as_deref().is_some_and(|id| !id.trim().is_empty())
        && matches_purchase_payment_request_identity(task, request_id)
        && task.subject_version == submission_version.to_string()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{PayableAccountId, PayableEntryId};
    use erp_finance::entity::payable::{EntryDirection, PayableEntryData, PayableEntryType};
    use erp_workflow::entity::work_item::WorkItemCloseData;

    use super::*;

    fn request_task() -> WorkItem {
        new_approved_purchase_payment_request_task(
            WorkItemId::new("task-1"),
            SupplierPaymentTaskSpec {
                account_id: "request-1".into(),
                subject_version: "2".into(),
                owner_organization_id: "finance-org".into(),
                owner_user_id: "cashier-1".into(),
                reason: SupplierPaymentTaskReason::Initial,
                due_at: Instant::from_unix_secs(100),
                open_total: "10".parse().unwrap(),
            },
            "finance:SUPPLIER_PAYMENT:rule-1",
            "request-1",
            "FKSQ-1",
        )
        .unwrap()
    }

    #[test]
    fn completed_or_closed_original_task_allows_result_recovery_only_for_its_owner() {
        let mut completed = request_task();
        assert!(completed.is_owned_by("cashier-1"));
        completed.complete_when_payment_request_paid(Instant::from_unix_secs(200)).unwrap();
        assert!(!completed.is_owned_by("cashier-1"));
        assert!(can_read_payment_result_task(&completed, "request-1", "cashier-1"));
        assert!(!can_read_payment_result_task(&completed, "request-1", "cashier-2"));
        assert!(!can_read_payment_result_task(&completed, "request-2", "cashier-1"));
        let mut closed = request_task();
        closed
            .close(
                "buyer-1",
                WorkItemCloseData { close_reason: "关闭未付申请".into() },
                Instant::from_unix_secs(200),
            )
            .unwrap();
        assert!(can_read_payment_result_task(&closed, "request-1", "cashier-1"));
        closed.reason_code = Some("INITIAL".into());
        assert!(!can_read_payment_result_task(&closed, "request-1", "cashier-1"));
    }

    #[test]
    fn restoring_open_task_keeps_legitimate_reassignment_and_original_request_key() {
        let mut task = request_task();
        assert_eq!(task.responsibility_key(), Some("finance:SUPPLIER_PAYMENT:rule-1:request:request-1"));
        assert!(restored_task_matches(&task, "request-1", 2));
        task.reassign("cashier-2", Instant::from_unix_secs(150)).unwrap();
        assert!(restored_task_matches(&task, "request-1", 2));
        assert_eq!(task.owner_user_id.as_deref(), Some("cashier-2"));
        assert_eq!(task.responsibility_key(), Some("finance:SUPPLIER_PAYMENT:rule-1:request:request-1"));
        for (request, version) in [("request-2", 2), ("request-1", 3)] {
            assert!(!restored_task_matches(&task, request, version));
        }
        let mut completed = task;
        completed.complete_when_payment_request_paid(Instant::from_unix_secs(200)).unwrap();
        assert!(!restored_task_matches(&completed, "request-1", 2));
    }

    #[test]
    fn restoring_open_task_rejects_missing_owner_and_nonrequest_execution_identity() {
        let original = request_task();
        for mutation in 0..5 {
            let mut task = original.clone();
            match mutation {
                0 => task.owner_user_id = None,
                1 => task.owner_user_id = Some(" ".into()),
                2 => task.reason_code = Some("INITIAL".into()),
                3 => task.business_object_type = "PAYABLE_ACCOUNT".into(),
                _ => task.owner_role = "sales".into(),
            }
            assert!(!restored_task_matches(&task, "request-1", 2), "mutation {mutation}");
        }
    }

    fn entry(id: &str, account: &str) -> PayableEntry {
        PayableEntry::new(
            PayableEntryId::new(id),
            PayableEntryData {
                payable_account_id: PayableAccountId::new(account),
                entry_type: PayableEntryType::Original,
                direction: EntryDirection::Increase,
                amount: Amount::from_str("100.00").unwrap(),
                due_date: BusinessDate::from_ymd(2026, 10, 10).unwrap(),
                source_fact_type: "purchase_order".into(),
                source_document_id: "purchase-1".into(),
                source_revision_id: "revision-1".into(),
                source_sequence: 1,
                posted_at: Instant::from_unix_secs(1),
            },
        )
        .unwrap()
    }
    fn allocation(id: &str, amount: &str) -> PendingPaymentAllocation {
        PendingPaymentAllocation::new(PayableEntryId::new(id), Amount::from_str(amount).unwrap()).unwrap()
    }
    #[test]
    fn execution_aggregates_real_entry_amounts_without_transferring_purchase_authorization() {
        let entries =
            vec![entry("entry-1", "account-1"), entry("entry-2", "account-1"), entry("entry-3", "account-2")];
        let result = allocations_by_account(
            &entries,
            &[allocation("entry-1", "10.01"), allocation("entry-2", "20.02"), allocation("entry-3", "0.07")],
        )
        .unwrap();
        assert_eq!(
            result,
            vec![
                PaymentRequestAllocation {
                    payable_account_id: "account-1".into(),
                    amount: Amount::from_str("30.03").unwrap()
                },
                PaymentRequestAllocation {
                    payable_account_id: "account-2".into(),
                    amount: Amount::from_str("0.07").unwrap()
                }
            ]
        );
    }
    #[test]
    fn execution_rejects_an_entry_outside_the_approved_purchase_scope() {
        let error =
            allocations_by_account(&[entry("authorized", "account-1")], &[allocation("unrelated", "1.00")])
                .unwrap_err();
        assert!(matches!(error,Error::ConflictError(message) if message.contains("不在批准申请采购范围")));
    }
}
