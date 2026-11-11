//! 撤回、终态驳回及关闭未付部分均原子释放占用。
use application_core::CommandReceipt;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_finance::dto::payable::payment_request::CancelPurchasePaymentRequest;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
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
use crate::finance_posting::receivable::cancel_approval as runtime_cancel;

impl PurchasePaymentRequestProcess {
    /// 当前经办人按统一授权撤回审批，取消当前任务并释放本次占用。
    /// # 参数
    /// * `id` - 付款申请身份。
    /// * `req` - 精确版本、原因和幂等键。
    /// * `actor` - 具备撤回和完整来源资格的当前经办人。
    /// # 返回
    /// 返回已撤回申请；重提须重新冻结与占用。
    /// # 错误
    /// 已批准、操作无权、版本变化或任务终止冲突时拒绝。
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
        let operation = PaymentWithdrawal {
            db: self.db.clone(),
            rbac: self.rbac.clone(),
            id: id.to_owned(),
            request: req,
            actor: actor.clone(),
            command,
        };
        let read = operation.clone();
        let authorized = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { read.authorized_request(executor).await })
            })
            .await?;
        let Some((writes, closed)) =
            prepare_withdraw(&self.db, &authorized, &operation.request, actor).await?
        else {
            return self.authorized_result(id, actor).await;
        };
        let recovery = operation.command.clone();
        let client = operation.db.client().clone();
        let result = client
            .with_transaction(move |executor| Box::pin(operation.persist(writes, closed, executor)))
            .await;
        self.finish_command(result, &recovery, actor).await
    }

    /// 显式终止原批准付款授权，释放尚未支付部分并保留实际付款。
    /// # 参数
    /// * `id` - 付款申请身份。
    /// * `req` - 精确版本、原因和幂等键。
    /// * `actor` - 统一关闭权限及全部真实采购来源授权均通过的操作人。
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

/// 撤回前读取和实际写入复用原幂等命令，写入阶段重新证明来源。
#[derive(Clone)]
struct PaymentWithdrawal {
    db: Database,
    rbac: FinancePostingPorts,
    id: String,
    request: CancelPurchasePaymentRequest,
    actor: AuditActor,
    command: CommandReceipt,
}

impl PaymentWithdrawal {
    async fn authorized_request(&self, executor: &mut dyn Executor) -> Result<PurchasePaymentRequest> {
        let request = load(&self.db, &self.id, executor).await?;
        require_sources(&self.db, &self.rbac, &request, &self.actor, "withdraw", executor).await?;
        Ok(request)
    }

    async fn persist(
        self,
        writes: PlannedWrites,
        closed: Vec<WorkItem>,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let receipts = FinanceCommandReceiptService::new(self.db.clone());
        if let Some(id) = receipts.committed_resource_id(&self.command, executor).await? {
            return Ok(id);
        }
        let mut request = self.authorized_request(executor).await?;
        ensure_version(&request, self.request.expected_version)?;
        claim_and_persist_document_cancel_runtime(&self.db, &writes, &closed, self.actor.id(), executor)
            .await?;
        self.rbac.runtime.persist_cancel_copies(&self.db, &writes, executor).await?;
        release_remaining(&self.db, &request, self.actor.id(), executor).await?;
        request.withdraw(&self.request.reason)?;
        self.db.purchase_payment_requests().update(&mut request, executor).await?;
        let audit = self.actor.clone().resource_log(
            "purchase_payment_request.withdraw",
            "purchase_payment_request",
            self.id.clone(),
        )?;
        receipts.save_resource(&self.command, self.id.clone(), audit.base.id.clone(), executor).await?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(self.id)
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
pub async fn cancel_approval(
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
pub async fn reject(
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

async fn prepare_withdraw(
    db: &Database,
    request: &PurchasePaymentRequest,
    req: &CancelPurchasePaymentRequest,
    actor: &AuditActor,
) -> Result<Option<(PlannedWrites, Vec<WorkItem>)>> {
    let id = request.base.id.as_str();
    ensure_version(request, req.expected_version)?;
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
