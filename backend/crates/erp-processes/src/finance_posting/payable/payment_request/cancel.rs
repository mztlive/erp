//! 撤回、终态驳回及关闭未付部分均原子释放占用。
use application_core::CommandReceipt;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_finance::dto::payable::payment_request::CancelPurchasePaymentRequest;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_identity::{AccessControlExt, Permission, RoleRepositoryExt};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::approval_integration::subject_ref_for;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::{WorkItem, WorkItemCloseData, WorkItemStatus};
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{
    PlannedWrites, PreparedExecution, claim_and_persist_document_cancel_runtime, prepare_cancel,
};
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::{NoTransaction, Transactional};

use super::*;
use crate::adapters::persist_cancel_copies;
use crate::audit::persist_log;
use crate::finance_posting::receivable::cancel_approval as runtime_cancel;

impl PurchasePaymentRequestProcess {
    /// 原申请人撤回审批，取消当前任务并释放本次占用。
    /// # 参数
    /// * `id` - 付款申请身份。
    /// * `req` - 精确版本、原因和幂等键。
    /// * `actor` - 原申请人。
    /// # 返回
    /// 返回已撤回申请；重提须重新冻结与占用。
    /// # 错误
    /// 已批准、非本人、版本变化或任务终止冲突时拒绝。
    pub async fn withdraw(
        &self,
        id: &str,
        req: CancelPurchasePaymentRequest,
        actor: &AuditActor,
    ) -> Result<PurchasePaymentRequest> {
        let command = receipt(id, &req, actor, "withdraw")?;
        if let Some(id) = FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(&command, &mut NoTransaction)
            .await?
        {
            return self.authorized_result(&id, actor).await;
        }
        let Some((writes, closed)) = prepare_withdraw(&self.db, id, &req, actor).await? else {
            return self.authorized_result(id, actor).await;
        };
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let response_actor = actor.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        let recovery = command.clone();
        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let receipts = FinanceCommandReceiptService::new(db.clone());
                    if let Some(id) = receipts.committed_resource_id(&command, executor).await? {
                        return Ok(id);
                    }
                    let mut request = load(&db, &id, executor).await?;
                    ensure_version(&request, req.expected_version)?;
                    ensure_creator(&request, &actor)?;
                    require_sources(&db, &rbac, &request, &actor, "withdraw", executor).await?;
                    claim_and_persist_document_cancel_runtime(&db, &writes, &closed, executor).await?;
                    persist_cancel_copies(&db, &writes, executor).await?;
                    release_remaining(&db, &request, actor.id(), executor).await?;
                    request.withdraw(&req.reason)?;
                    db.purchase_payment_requests().update(&mut request, executor).await?;
                    let audit = actor.resource_log(
                        "purchase_payment_request.withdraw",
                        "purchase_payment_request",
                        id.clone(),
                    )?;
                    receipts.save_resource(&command, id.clone(), audit.base.id.clone(), executor).await?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<String, Error>(id)
                })
            })
            .await;
        self.finish_command(result, &recovery, &response_actor).await
    }

    /// 显式终止原批准付款授权，释放尚未支付部分并保留实际付款。
    /// # 参数
    /// * `id` - 付款申请身份。
    /// * `req` - 精确版本、原因和幂等键。
    /// * `actor` - 有关闭权限的原申请人或财务处理人。
    /// # 返回
    /// 返回已关闭申请；未付金额释放、开放任务终止，已完成任务保留。
    /// # 错误
    /// 状态、权限、版本、占用或并发付款发生变化时拒绝。
    pub async fn close(
        &self,
        id: &str,
        req: CancelPurchasePaymentRequest,
        actor: &AuditActor,
    ) -> Result<PurchasePaymentRequest> {
        let command = receipt(id, &req, actor, "close")?;
        if let Some(id) = FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(&command, &mut NoTransaction)
            .await?
        {
            return self.authorized_result(&id, actor).await;
        }
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let response_actor = actor.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        let recovery = command.clone();
        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let receipts = FinanceCommandReceiptService::new(db.clone());
                    if let Some(id) = receipts.committed_resource_id(&command, executor).await? {
                        return Ok(id);
                    }
                    let mut request = load(&db, &id, executor).await?;
                    ensure_version(&request, req.expected_version)?;
                    require_sources(&db, &rbac, &request, &actor, "close", executor).await?;
                    require_finance_or_creator(&db, &rbac, &request, &actor, executor).await?;
                    release_remaining(&db, &request, actor.id(), executor).await?;
                    request.close(&req.reason)?;
                    close_execution_task(&db, &request, &actor, &req.reason, executor).await?;
                    db.purchase_payment_requests().update(&mut request, executor).await?;
                    let audit = actor.resource_log(
                        "purchase_payment_request.close",
                        "purchase_payment_request",
                        id.clone(),
                    )?;
                    receipts.save_resource(&command, id.clone(), audit.base.id.clone(), executor).await?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<String, Error>(id)
                })
            })
            .await;
        self.finish_command(result, &recovery, &response_actor).await
    }
}
fn receipt(
    id: &str,
    req: &CancelPurchasePaymentRequest,
    actor: &AuditActor,
    action: &str,
) -> Result<CommandReceipt> {
    Ok(CommandReceipt::from_resource_parts(
        "payment-request-stop-",
        actor.id(),
        &format!("purchase_payment_request.{action}"),
        "purchase_payment_request",
        id,
        &req.idempotency_key,
        [id.to_owned(), req.expected_version.to_string(), req.reason.clone()],
    )?)
}

/// 管理员取消审批时释放额度。
/// # 参数
/// * `db` - 数据库。
/// * `id` - 当前申请。
/// * `actor` - 已授权处理人。
/// * `executor` - 审批运行时事务。
/// # 返回
/// 返回撤回与额度释放成功结果。
/// # 错误
/// 状态、占用或仓储失败时返回错误。
pub(crate) async fn cancel_approval(
    db: &Database,
    id: &str,
    actor: &AuditActor,
    reason: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    stop_approval(db, id, actor, reason, false, executor).await
}
/// 终态驳回时释放额度且不重启。
/// # 参数
/// * `db` - 数据库。
/// * `id` - 当前申请。
/// * `actor` - 财务审批人。
/// * `executor` - 审批运行时事务。
/// # 返回
/// 返回驳回与释放成功结果。
/// # 错误
/// 状态、占用或仓储失败时返回错误。
pub(crate) async fn reject(
    db: &Database,
    id: &str,
    actor: &AuditActor,
    reason: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    stop_approval(db, id, actor, reason, true, executor).await
}
async fn stop_approval(
    db: &Database,
    id: &str,
    actor: &AuditActor,
    reason: &str,
    rejected: bool,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut request = load(db, id, executor).await?;
    release_remaining(db, &request, actor.id(), executor).await?;
    if rejected {
        request.reject(reason)?;
    } else {
        request.withdraw(reason)?;
    }
    db.purchase_payment_requests().update(&mut request, executor).await?;
    let code = if rejected { "purchase_payment_request.reject" } else { "purchase_payment_request.withdraw" };
    let audit = actor.clone().resource_log(code, "purchase_payment_request", id.to_owned())?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

async fn require_finance_or_creator(
    db: &Database,
    rbac: &SharedRbacService,
    request: &PurchasePaymentRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if request.created_by == actor.id() {
        return Ok(());
    }
    let close = Permission::parse("purchase_payment_request:close")?;
    let decide = Permission::parse("approval_instance:decide")?;
    let snapshot =
        rbac.role_permission_snapshot(actor.kind(), actor.id(), &[close.clone(), decide.clone()]).await?;
    let roles = snapshot.granting_role_ids_for_all(&[close, decide]);
    if db.roles().enabled_roles(&roles, executor).await?.is_empty() {
        return Err(Error::Forbidden("只有原申请人或获授权财务处理人可以关闭原付款授权".into()));
    }
    rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
    Ok(())
}

async fn prepare_withdraw(
    db: &Database,
    id: &str,
    req: &CancelPurchasePaymentRequest,
    actor: &AuditActor,
) -> Result<Option<(PlannedWrites, Vec<WorkItem>)>> {
    let request = load(db, id, &mut NoTransaction).await?;
    ensure_creator(&request, actor)?;
    ensure_version(&request, req.expected_version)?;
    let binding = find_approval_binding(db, id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::ConflictError("申请缺少审批定义".into()))?;
    let runtime = runtime_cancel::load_cancel_runtime(
        db,
        &binding,
        &subject_ref_for(DocumentType::PurchasePaymentRequest, id)?,
        request.approval_subject_version,
    )
    .await?;
    let now = Instant::now();
    let key = normalize_idempotency_key(&req.idempotency_key)?;
    let PreparedExecution::Apply(writes) =
        prepare_cancel(runtime_cancel::build_customer_receipt_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &key,
            None,
            now,
        )?)?
    else {
        return Ok(None);
    };
    let closed =
        WorkItem::close_all_for_approval_cancellation(runtime.open_tasks, actor.id(), &req.reason, now)?;
    Ok(Some((*writes, closed)))
}

async fn close_execution_task(
    db: &Database,
    request: &PurchasePaymentRequest,
    actor: &AuditActor,
    reason: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if let Some(task_id) = &request.work_item_id {
        let mut task = db
            .work_items()
            .find_by_id(task_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("申请执行任务不存在".into()))?;
        if task.status == WorkItemStatus::Open {
            task.close(actor.id(), WorkItemCloseData { close_reason: reason.to_owned() }, Instant::now())?;
            db.work_items().update(&mut task, executor).await?;
        }
    }
    Ok(())
}
