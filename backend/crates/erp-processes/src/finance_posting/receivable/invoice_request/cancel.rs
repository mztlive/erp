//! 撤回申请与受阻取消共用领域转换，释放占用与关闭审批任务同事务。

use application_core::{AuditActor, CommandReceipt};
use erp_core::common::time::Instant;
use erp_finance::dto::receivable::CancelInvoiceRequest;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_read_models::finance::receivable::invoice_request::InvoiceRequestView;
use erp_workflow::entity::approval_integration::subject_ref_for;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{
    PlannedWrites, PreparedExecution, claim_and_persist_document_cancel_runtime, prepare_cancel,
};
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::{NoTransaction, Transactional};

use super::super::{ReceivableProcess, cancel_approval};
use super::*;
use crate::audit::persist_log;

impl ReceivableProcess {
    /// 原申请人撤回审批；独立财务回执命中时只回读原申请。
    ///
    /// # 参数
    /// * `id` - 当前开票申请身份。
    /// * `req` - 精确版本、撤回原因与幂等键。
    /// * `actor` - 已通过入口鉴权的当前申请人。
    /// # 返回
    /// 返回首次撤回申请的当前视图。
    /// # 错误
    /// 非原申请人、版本冲突、审批已完成或任务状态变化时拒绝；查证失败保留未知结果。
    pub async fn cancel_invoice_request(
        &self,
        id: &str,
        req: CancelInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<InvoiceRequestView> {
        let command = cancel_receipt(id, &req, actor)?;
        if let Some(id) = FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(&command, &mut NoTransaction)
            .await?
        {
            return Ok(self.read.invoice_request_detail(&id).await?);
        }
        let Some(prepared) = self.prepare_invoice_request_cancel(id, &req, actor).await? else {
            return Ok(self.read.invoice_request_detail(id).await?);
        };
        let db = self.db.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let recover = command.clone();
        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let receipts = FinanceCommandReceiptService::new(db.clone());
                    if let Some(id) = receipts.committed_resource_id(&command, executor).await? {
                        return Ok::<String, Error>(id);
                    }
                    claim_and_persist_document_cancel_runtime(
                        &db,
                        &prepared.writes,
                        &prepared.closed,
                        executor,
                    )
                    .await?;
                    let current = load(&db, &id, executor).await?;
                    ensure_expected_version(current.base.version, req.expected_version)?;
                    let event_id = cancel_and_record(&db, &id, &actor, Some(&command), executor).await?;
                    receipts.save_resource(&command, id.clone(), event_id, executor).await?;
                    Ok::<String, Error>(id)
                })
            })
            .await;
        self.finish_invoice_request_command(result, &recover).await
    }

    /// 保持申请人、版本、审批绑定与运行时的既有首错顺序。
    async fn prepare_invoice_request_cancel(
        &self,
        id: &str,
        req: &CancelInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<Option<PreparedInvoiceRequestCancel>> {
        let request = load(&self.db, id, &mut NoTransaction).await?;
        if request.created_by != actor.id() {
            return Err(Error::Forbidden("只能撤回本人提交的开票申请".into()));
        }
        ensure_expected_version(request.base.version, req.expected_version)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("申请缺少审批流程".into()))?;
        let subject = subject_ref_for(DocumentType::SalesInvoiceRequest, id)?;
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
            return Ok(None);
        };
        let closed =
            WorkItem::close_all_for_approval_cancellation(runtime.open_tasks, actor.id(), &req.reason, now)?;
        Ok(Some(PreparedInvoiceRequestCancel { writes, closed }))
    }
}

struct PreparedInvoiceRequestCancel {
    writes: Box<PlannedWrites>,
    closed: Vec<WorkItem>,
}

/// 将对象 ID 固定在命令作用域中；不同申请允许分别使用同一个调用方幂等键。
fn cancel_receipt(id: &str, req: &CancelInvoiceRequest, actor: &AuditActor) -> Result<CommandReceipt> {
    Ok(CommandReceipt::from_resource_parts(
        "invoice-request-cancel-",
        actor.id(),
        "sales_invoice_request.cancel",
        "sales_invoice_request",
        id,
        &req.idempotency_key,
        [id.to_string(), req.expected_version.to_string(), req.reason.clone(), req.idempotency_key.clone()],
    )?)
}

/// 撤回及管理员受阻取消的唯一业务动作；必须在审批运行时事务内调用。
/// # 参数
/// * `db` - 当前用例数据库。
/// * `id` - 开票申请 ID。
/// * `actor` - 已授权的取消操作人。
/// * `executor` - 审批运行时使用的同一事务执行器。
/// # 返回
/// 申请撤回及安全业务事件保存成功时返回空结果。
/// # 错误
/// 申请、应收锁、状态迁移或持久化失败时停止执行。
pub(crate) async fn cancel(
    db: &Database,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    cancel_and_record(db, id, actor, None, executor).await.map(|_| ())
}

async fn cancel_and_record(
    db: &Database,
    id: &str,
    actor: &AuditActor,
    command: Option<&CommandReceipt>,
    executor: &mut dyn Executor,
) -> Result<String> {
    let mut request = load(db, id, executor).await?;
    lock_account(db, request.receivable_account_id.as_ref(), executor).await?;
    request.cancel_approval()?;
    db.sales_invoice_requests().update(&mut request, executor).await?;
    let audit = super::command::event(actor, &request, super::command::CANCEL, command)?;
    persist_log(db, &audit, executor).await?;
    Ok(audit.base.id)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_finance::FinanceCommandReceipt;

    use super::*;

    #[test]
    fn cancel_identity_has_fixed_resource_type_binds_target_and_preserves_exact_version_payload() {
        let actor = AuditActor::new("actor".into(), "sales".into(), AccountKind::Admin);
        let req = CancelInvoiceRequest {
            expected_version: 7,
            reason: "更正资料".into(),
            idempotency_key: "key".into(),
        };
        let first = cancel_receipt("request-1", &req, &actor).unwrap();
        let other = cancel_receipt("request-2", &req, &actor).unwrap();
        assert_ne!(first.id(), other.id());
        assert_eq!(first.resource_type(), "sales_invoice_request");
        let saved = FinanceCommandReceipt::resource(&first, "request-1".into(), "event-1".into()).unwrap();
        assert_eq!(
            saved.resource_id(&cancel_receipt("request-1", &req, &actor).unwrap()).unwrap(),
            "request-1"
        );
        let mut changed = req;
        changed.expected_version = 8;
        let changed = cancel_receipt("request-1", &changed, &actor).unwrap();
        assert!(matches!(saved.resource_id(&changed), Err(erp_finance::Error::ConflictError(_))));
    }
}
