//! 开票申请原子创建、额度占用、定义绑定与审批启动。
use application_core::{AuditActor, CommandReceipt};
use erp_core::common::time::Instant;
use erp_core::ids::SalesInvoiceRequestId;
use erp_finance::dto::receivable::SubmitInvoiceRequest;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_read_models::finance::receivable::invoice_request::InvoiceRequestView;
use erp_read_models::finance::receivable::invoice_request_source;
use erp_workflow::entity::approval_integration::{
    ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload, subject_ref_for,
};
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use erp_workflow::service::approval::business_adapter::{BindingRevalidationContext, adapter_spec_of};
use erp_workflow::service::approval::execution::{PreparedExecution, prepare_start};
use erp_workflow::service::document_registry::{find_registered_document, new_registered_document};
use erp_workflow::{BpmExt, DocumentRegistryExt};
use id_generator::next_id;
use persistence_core::NoTransaction;

use super::super::{ReceivableProcess, start_approval};
use super::*;
use crate::audit::persist_log;

impl ReceivableProcess {
    /// 原子创建或重新提交开票申请，冻结资料及额度后启动已发布流程。
    /// # 参数
    /// * `req` - 新申请或带版本的原申请、金额资料与幂等键。
    /// * `actor` - 当前已认证申请人。
    /// # 返回
    /// 返回首次提交申请的当前视图；重放不重复占用额度或启动审批。
    /// # 错误
    /// 未发布流程、来源非法、重复申请超额、权限和版本冲突时整体回滚。
    pub async fn submit_invoice_request(
        &self,
        req: SubmitInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<InvoiceRequestView> {
        let command = CommandReceipt::from_payload(
            "invoice-request-submit-",
            actor.id(),
            "sales_invoice_request.submit",
            "sales_invoice_request",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(id) = FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(&command, &mut NoTransaction)
            .await?
        {
            return Ok(self.read.invoice_request_detail(&id).await?);
        }
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = self.object_read.clone();
        let actor = actor.clone();
        let recover = command.clone();
        let revision = rbac.current_policy_revision().await?;
        let result = rbac
            .clone()
            .run_authorized_policy_transaction(revision, move |executor| {
                Box::pin(async move {
                    let receipts = FinanceCommandReceiptService::new(db.clone());
                    if let Some(id) = receipts.committed_resource_id(&command, executor).await? {
                        return Ok::<String, Error>(id);
                    }
                    let account = lock_account(&db, &req.receivable_account_id, executor).await?;
                    let available = account
                        .open_invoiceable_total
                        .checked_sub(reserved(&db, &account.base.id, executor).await?);
                    invoice_request_source::load(&db, &account, executor)
                        .await?
                        .validate(&req.data, available)?;
                    let mut request = candidate(&db, &account, &req, &actor, executor).await?;
                    request.submit(available)?;
                    let binding = bind(&db, &rbac, object_read.as_ref(), &request, &actor, executor).await?;
                    let id = request.base.id.clone();
                    start(&db, &mut request, &binding, &req.idempotency_key, &actor, executor).await?;
                    let audit =
                        super::command::event(&actor, &request, super::command::SUBMIT, Some(&command))?;
                    receipts.save_resource(&command, id.clone(), audit.base.id.clone(), executor).await?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<String, Error>(id)
                })
            })
            .await;
        self.finish_invoice_request_command(result, &recover).await
    }
}
/// 固定销售应收来源；草稿修改仅限原申请人且必须提交版本。
async fn candidate(
    db: &Database,
    account: &ReceivableAccount,
    req: &SubmitInvoiceRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<SalesInvoiceRequest> {
    if let Some(id) = &req.request_id {
        let mut request = load(db, id, executor).await?;
        ensure_expected_version(
            request.base.version,
            req.expected_version.ok_or_else(|| Error::ValidationError("请刷新申请后重试".into()))?,
        )?;
        if request.created_by != actor.id() || request.receivable_account_id.as_ref() != account.base.id {
            return Err(Error::Forbidden("只能修改本人申请且不能更换来源销售单".into()));
        }
        if request.status != erp_finance::entity::receivable::InvoiceRequestStatus::Draft {
            return Err(Error::ConflictError("申请已提交，请刷新后重试".into()));
        }
        request.data = req.data.clone().normalize()?;
        return Ok(request);
    }
    if req.expected_version.is_some() {
        return Err(Error::ValidationError("新建申请不能指定已有版本".into()));
    }
    let request = SalesInvoiceRequest::new(
        SalesInvoiceRequestId::new(next_id()),
        account,
        req.data.clone(),
        actor.id(),
    )?;
    db.sales_invoice_requests().create(&request, executor).await?;
    Ok(request)
}
/// 首次提交时注册单据并绑定已发布流程；重提沿用原绑定。
///
/// # 参数
/// * `db` - 数据库。
/// * `rbac` - 共享 RBAC。
/// * `object_read` - 审批对象读取端口。
/// * `request` - 已写入的开票申请。
/// * `actor` - 提交人。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 已绑定的发布定义。
///
/// # 错误
/// 未发布流程、绑定校验失败，或重提时注册行没有绑定时返回错误。
///
/// # 约束
/// 未注册表示首次提交。`find_approval_binding` 会把 `DocumentMissing` 映射成
/// NotFound，首次提交不得调用它；与回款单一样先算绑定再写入注册行。
async fn bind(
    db: &Database,
    rbac: &erp_identity::SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    request: &SalesInvoiceRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    if let Some(document) = find_registered_document(db, &request.base.id, executor).await? {
        return document
            .approval_binding
            .ok_or_else(|| Error::ConflictError("请先发布开票申请审批流程".into()));
    }
    let command = BindPublishedDefinitionCommand {
        document_type: DocumentType::SalesInvoiceRequest,
        business_object_id: request.base.id.clone(),
        business_object_version: request.base.version,
        context: BindingRevalidationContext::new(
            request.counterparty_party_id.to_string(),
            actor.id().into(),
        ),
    };
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        &command,
        actor,
        executor,
    )
    .await?
    .ok_or_else(|| Error::ConflictError("请先发布开票申请审批流程".into()))?;
    let mut document: BusinessDocument = new_registered_document(
        &request.base.id,
        DocumentType::SalesInvoiceRequest,
        request.request_no.clone(),
    )?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, executor).await?;
    Ok(binding)
}
/// 同一事务写入启动收据、单据守卫、申请快照和审批任务。
async fn start(
    db: &Database,
    request: &mut SalesInvoiceRequest,
    binding: &erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding,
    key: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let now = Instant::now();
    let graph = start_approval::load_bound_definition_graph_with_executor(db, binding, executor).await?;
    let subject = subject_ref_for(DocumentType::SalesInvoiceRequest, &request.base.id)?;
    let input = start_approval::build_document_start_input(start_approval::DocumentStartInput {
        document_type: DocumentType::SalesInvoiceRequest,
        graph,
        binding,
        subject,
        subject_version: request.approval_subject_version,
        actor_id: actor.id(),
        organization_id: request.counterparty_party_id.as_ref(),
        idempotency_key: key,
        receipt: None,
        now,
    })?;
    let PreparedExecution::Apply(writes) = prepare_start(input)? else {
        return Err(Error::ConflictError("申请已提交，请刷新查看结果".into()));
    };
    db.bpm_workflow().insert_command_receipt(&writes.receipt, executor).await?;
    let guarded = db
        .business_documents()
        .mark_approval_started(
            &request.base.id,
            DocumentType::SalesInvoiceRequest,
            &binding.approval_process_definition_id,
            binding.approval_definition_version,
            now,
            executor,
        )
        .await?;
    if guarded.is_none() {
        return Err(Error::ConflictError("申请审批状态已变化，请刷新后重试".into()));
    }
    db.sales_invoice_requests().update(request, executor).await?;
    let snapshot = request_start_snapshot(request, actor, now);
    let spec = adapter_spec_of(DocumentType::SalesInvoiceRequest)?;
    start_approval::persist_runtime_writes(
        db,
        &writes,
        start_approval::RuntimeSubject {
            document_type: DocumentType::SalesInvoiceRequest,
            snapshot_payload: &snapshot,
        },
        spec.owner_role.as_str(),
        request.counterparty_party_id.as_ref(),
        now,
        executor,
    )
    .await
}

/// 启动快照固定一行，金额取申请金额，责任组织与客户取往来主体。
fn request_start_snapshot(
    request: &SalesInvoiceRequest,
    actor: &AuditActor,
    now: Instant,
) -> ApprovalSubjectSnapshotPayload {
    ApprovalSubjectSnapshotPayload {
        document_no: request.request_no.clone(),
        responsible_org_id: request.counterparty_party_id.to_string(),
        submitted_by: actor.id().into(),
        submitted_at: now,
        counterparty: Some(ApprovalSubjectCounterparty::Customer {
            customer_id: request.customer_id.clone(),
        }),
        total_amount: Some(request.data.amount),
        total_quantity: None,
        line_count: 1,
    }
}
