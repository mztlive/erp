//! 撤回申请与受阻取消共用领域转换，释放占用与关闭审批任务同事务。
use application_core::{AuditActor, CommandReceipt};
use erp_audit::{AuditActorLogs, AuditExt, CommandReceiptServiceExt};
use erp_core::common::time::Instant;
use erp_finance::dto::receivable::CancelInvoiceRequest;
use erp_read_models::finance::receivable::invoice_request::InvoiceRequestView;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{
    PreparedExecution, claim_and_persist_document_cancel_runtime, prepare_cancel,
};
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::{NoTransaction, Transactional};

use super::super::{ReceivableProcess, cancel_approval};
use super::*;
impl ReceivableProcess {
    /// 原申请人撤回审批；幂等重试返回已提交结果。
    /// # 错误
    /// 非原申请人、版本冲突、审批已完成或任务状态变化时拒绝。
    pub async fn cancel_invoice_request(
        &self,
        id: &str,
        req: CancelInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<InvoiceRequestView> {
        let command = CommandReceipt::from_payload(
            "invoice-request-cancel-",
            actor.id(),
            "sales_invoice_request.cancel",
            id,
            &req.idempotency_key,
            &req,
        )?;
        if let Some(id) = command.committed_resource_id(&self.db).await? {
            return Ok(self.read.invoice_request_detail(&id).await?);
        }
        let request = load(&self.db, id, &mut NoTransaction).await?;
        if request.created_by != actor.id() {
            return Err(Error::Forbidden("只能撤回本人提交的开票申请".into()));
        }
        erp_finance::service::receivable::mapping::ensure_expected_version(
            request.base.version,
            req.expected_version,
        )?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("申请缺少审批流程".into()))?;
        let subject = erp_workflow::entity::approval_integration::subject_ref_for(
            DocumentType::SalesInvoiceRequest,
            id,
        )?;
        let runtime = cancel_approval::load_cancel_runtime(
            &self.db,
            &binding,
            &subject,
            request.approval_subject_version,
        )
        .await?;
        let now = Instant::now();
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let prepared = prepare_cancel(cancel_approval::build_customer_receipt_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &key,
            None,
            now,
        )?)?;
        let PreparedExecution::Apply(writes) = prepared else {
            return Ok(self.read.invoice_request_detail(id).await?);
        };
        let closed =
            WorkItem::close_all_for_approval_cancellation(runtime.open_tasks, actor.id(), &req.reason, now)?;
        let db = self.db.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let recover = command.clone();
        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    claim_and_persist_document_cancel_runtime(&db, &writes, &closed, executor).await?;
                    let current = load(&db, &id, executor).await?;
                    erp_finance::service::receivable::mapping::ensure_expected_version(
                        current.base.version,
                        req.expected_version,
                    )?;
                    cancel(&db, &id, &actor, executor).await?;
                    db.audit_logs().create(&command.audit(actor, id.clone())?, executor).await?;
                    Ok::<String, Error>(id)
                })
            })
            .await;
        match result {
            Ok(id) => Ok(self.read.invoice_request_detail(&id).await?),
            Err(error) => match recover.committed_resource_id(&self.db).await? {
                Some(id) => Ok(self.read.invoice_request_detail(&id).await?),
                None => Err(error),
            },
        }
    }
}
/// 撤回及管理员受阻取消的唯一业务动作；必须在审批运行时事务内调用。
pub(crate) async fn cancel(
    db: &Database,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut request = load(db, id, executor).await?;
    lock_account(db, request.receivable_account_id.as_ref(), executor).await?;
    request.cancel_approval()?;
    db.sales_invoice_requests().update(&mut request, executor).await?;
    let audit = actor.clone().resource_log(
        "sales_invoice_request.cancel",
        "sales_invoice_request",
        id.to_string(),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}
