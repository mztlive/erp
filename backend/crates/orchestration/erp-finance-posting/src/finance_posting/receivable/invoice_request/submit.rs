//! 开票申请原子创建、额度占用、定义绑定与审批启动。
use application_core::{AuditActor, CommandReceipt};
use erp_approval_runtime::approver_directory::resolve_approvers;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_core::ids::SalesInvoiceRequestId;
use erp_finance::dto::receivable::SubmitInvoiceRequest;
use erp_finance::entity::receivable::{InvoiceRequestData, InvoiceRequestStatus};
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_read_models::finance::receivable::invoice_request::InvoiceRequestView;
use erp_read_models::finance::receivable::invoice_request_source;
use erp_workflow::entity::approval_integration::{
    ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload, subject_ref_for,
};
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use erp_workflow::service::approval::business_adapter::{BindingRevalidationContext, adapter_spec_of};
use erp_workflow::service::approval::execution::{PreparedExecution, prepare_start};
use erp_workflow::service::document_registry::{find_registered_document, new_registered_document};
use erp_workflow::{BpmExt, DocumentRegistryExt};
use id_generator::next_id;
use persistence_core::Transactional;

use super::super::{ReceivableProcess, start_approval};
use super::authorization::{MongoAccess, RequestAction, authorize};
use super::*;
use crate::FinancePostingPorts;

impl ReceivableProcess {
    /// 首次提交开票申请，或按原申请版本重新提交时，在同一事务冻结资料、占用额度并启动已发布审批流程。
    /// 按原幂等键和载荷重放时返回原申请的当前视图，不重复占用额度或启动流程。
    /// 流程未发布、来源无效、重复申请超额、授权失败或版本冲突时整笔失败。
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
        if let Some(view) = self.replay_invoice_request_command(&command, actor).await? {
            return Ok(view);
        }

        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let response_actor = actor.clone();
        let actor = actor.clone();
        let recover = command.clone();

        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    SubmitWrite { db: &db, rbac: &rbac, req: &req, actor: &actor, command: &command }
                        .apply(executor)
                        .await
                })
            })
            .await;

        self.finish_invoice_request_command(result, &recover, &response_actor).await
    }
}
/// 本次提交的权威输入与共享事务端口。
struct SubmitWrite<'a> {
    db: &'a Database,
    rbac: &'a FinancePostingPorts,
    req: &'a SubmitInvoiceRequest,
    actor: &'a AuditActor,
    command: &'a CommandReceipt,
}
impl SubmitWrite<'_> {
    // 原事务内先授权或恢复回执，再冻结额度、审批与成功审计。
    async fn apply(&self, executor: &mut dyn Executor) -> Result<String> {
        let Self { db, rbac, req, actor, command } = *self;
        let receipts = FinanceCommandReceiptService::new(db.clone());
        let access = MongoAccess { db, rbac };
        if access.replay(actor, command, executor).await?.is_some() {
            return Err(super::command::replay_requires_readback());
        }

        if let Some(id) = req.request_id.as_deref() {
            authorize(&access, actor, id, RequestAction::Submit, executor).await?;
        }

        let account = lock_account(db, &req.receivable_account_id, executor).await?;
        let available =
            account.open_invoiceable_total.checked_sub(reserved(db, &account.base.id, executor).await?);
        let data = invoice_request_source::load(db, &account, executor)
            .await?
            .prepare_data(req.data.clone(), Some(available))?;

        let mut request = candidate(db, &account, req, data, actor, executor).await?;
        if req.request_id.is_none() {
            authorize(&access, actor, &request.base.id, RequestAction::Submit, executor).await?;
        }
        request.submit(available)?;

        let binding = bind(db, rbac, &request, actor, executor).await?;
        let id = request.base.id.clone();
        start(db, rbac, &mut request, &binding, req, actor, executor).await?;

        let audit = super::command::event(actor, &request, super::command::SUBMIT, Some(command))?;
        receipts.save_resource(command, id.clone(), audit.base.id.clone(), executor).await?;
        persist_log(db, &audit, executor).await?;
        Ok::<String, Error>(id)
    }
}

// 固定销售应收来源；草稿修改必须提交版本，操作者资格由统一授权入口检查。
async fn candidate(
    db: &Database,
    account: &ReceivableAccount,
    req: &SubmitInvoiceRequest,
    data: InvoiceRequestData,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<SalesInvoiceRequest> {
    if let Some(id) = &req.request_id {
        let mut request = load(db, id, executor).await?;
        ensure_expected_version(
            request.base.version,
            req.expected_version.ok_or_else(|| Error::ValidationError("请刷新申请后重试".into()))?,
        )?;
        if request.receivable_account_id.as_ref() != account.base.id {
            return Err(Error::Forbidden("不能更换申请来源销售单".into()));
        }
        if !matches!(request.status, InvoiceRequestStatus::Draft | InvoiceRequestStatus::Rejected) {
            return Err(Error::ConflictError("申请已提交，请刷新后重试".into()));
        }
        request.update(data)?;
        return Ok(request);
    }

    if req.expected_version.is_some() {
        return Err(Error::ValidationError("新建申请不能指定已有版本".into()));
    }
    let request = SalesInvoiceRequest::new(SalesInvoiceRequestId::new(next_id()), account, data, actor.id())?;
    db.sales_invoice_requests().create(&request, executor).await?;
    Ok(request)
}
// 开票申请首次提交时先用当前提交人和已写入申请计算发布定义绑定，再写入注册行；重提须沿用原绑定。
// 待注册时不调用 `find_approval_binding`，该方法会把 `DocumentMissing` 映射为 `NotFound`。
// 沿用提交事务执行器；未发布流程、绑定校验失败或重提缺少原绑定时返回错误。
async fn bind(
    db: &Database,
    rbac: &FinancePostingPorts,
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
    let binding = rbac
        .runtime
        .bind_document(db, &command, actor, executor)
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
// 同一事务写入启动收据、单据守卫、申请快照和审批任务。
async fn start(
    db: &Database,
    rbac: &FinancePostingPorts,
    request: &mut SalesInvoiceRequest,
    binding: &erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding,
    req: &SubmitInvoiceRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let now = Instant::now();
    let graph = start_approval::load_bound_definition_graph_with_executor(db, binding, executor).await?;
    let graph = resolve_approvers(db, &rbac.authorization, graph, actor.id(), executor).await?;
    let subject = subject_ref_for(DocumentType::SalesInvoiceRequest, &request.base.id)?;

    let input = start_approval::build_document_start_input(start_approval::DocumentStartInput {
        resubmission_reason: req.resubmission_reason.as_deref(),
        document_type: DocumentType::SalesInvoiceRequest,
        graph,
        binding,
        subject,
        subject_version: request.approval_subject_version,
        actor_id: actor.id(),
        organization_id: request.counterparty_party_id.as_ref(),
        idempotency_key: &req.idempotency_key,
        receipt: None,
        now,
    })?;
    let PreparedExecution::Apply(writes) = prepare_start(input)? else {
        return Err(Error::ConflictError("申请已提交，请刷新查看结果".into()));
    };

    db.bpm_workflow().insert_command_receipt(&writes.receipt, executor).await?;
    mark_request_started(db, request, binding, now, executor).await?;
    db.sales_invoice_requests().update(request, executor).await?;

    let snapshot = request_start_snapshot(request, actor, now);
    let spec = adapter_spec_of(DocumentType::SalesInvoiceRequest)?;
    start_approval::persist_runtime_writes(
        db,
        rbac,
        &writes,
        start_approval::RuntimeStartInput {
            resubmission_reason: req.resubmission_reason.as_deref(),
            document_type: DocumentType::SalesInvoiceRequest,
            snapshot_payload: &snapshot,
            owner_role: spec.owner_role.as_str(),
            organization_id: request.counterparty_party_id.as_ref(),
            now,
        },
        executor,
    )
    .await
}

// 在启动收据之后认领原业务单据的精确审批绑定。
async fn mark_request_started(
    db: &Database,
    request: &SalesInvoiceRequest,
    binding: &ApprovalDefinitionBinding,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
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
    Ok(())
}

// 启动快照固定一行，金额取申请金额，责任组织与客户取往来主体。
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
        line_count: request.data.lines.len().max(1) as u32,
    }
}
