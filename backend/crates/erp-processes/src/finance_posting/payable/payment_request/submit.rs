//! 提交冻结正式来源、占用应付额度并创建新的审批实例。
use application_core::CommandReceipt;
use bpm::ids::ApprovalNodeExecutionId;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_finance::dto::payable::payment_request::SubmitPurchasePaymentRequest;
use erp_finance::entity::payable::{PurchasePaymentRequestData, PurchasePaymentRequestSubmissionSnapshot};
use erp_finance::repository::PurchasePaymentRequestSnapshotExt;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_workflow::entity::approval_integration::{
    ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload, subject_ref_for,
};
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::entity::work_item::payment_due_at;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::binding::{
    BindPublishedDefinitionCommand, attach_published_binding, revalidate_binding_graph,
};
use erp_workflow::service::approval::business_adapter::{BindingRevalidationContext, adapter_spec_of};
use erp_workflow::service::approval::execution::{PlannedWrites, PreparedExecution, prepare_start};
use erp_workflow::service::approval::policy::require_process_required;
use erp_workflow::service::document_registry::{find_registered_document, new_registered_document};
use erp_workflow::{BpmExt, DocumentRegistryExt, WorkItemExt};
use persistence_core::NoTransaction;

use super::*;
use crate::audit::persist_log;
use crate::finance_posting::command_recovery::recovered_resource;
use crate::finance_posting::receivable::start_approval;

/// 在调用方事务内创建并正式提交；回执只在审批启动完成后保存。
/// # 参数
/// 数据库、授权与审批端口、申请表单、申请人、批次命令及同一事务执行器。
/// # 返回
/// 已冻结来源、占用余额并启动审批的申请。
/// # 错误
/// 创建或提交任一步失败时交由调用方回滚整个事务。
pub(super) async fn create_and_submit(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn ApprovalObjectReadPort,
    data: PurchasePaymentRequestData,
    actor: &AuditActor,
    command: &CommandReceipt,
    executor: &mut dyn Executor,
) -> Result<PurchasePaymentRequest> {
    let draft = PurchasePaymentRequest::new(id_generator::next_id(), data, actor.id())?;
    let draft = draft::create_draft(db, rbac, draft, actor, executor).await?;
    let req = SubmitPurchasePaymentRequest {
        expected_version: draft.base.version,
        idempotency_key: command.id().to_owned(),
    };
    let id = SubmitWrite { db, rbac, object_read, id: &draft.base.id, req: &req, actor, command }
        .persist(executor)
        .await?;
    load(db, &id, executor).await
}

impl PurchasePaymentRequestProcess {
    /// 原子提交申请并冻结逐采购付款额度。
    /// # 参数
    /// * `id` - 已保存的付款申请。
    /// * `req` - 本次版本及幂等键。
    /// * `actor` - 原申请人。
    /// # 返回
    /// 返回审批中申请；相同幂等键只恢复原结果。
    /// # 错误
    /// 来源、主体、账户、余额、审批人或版本不满足时整体拒绝。
    pub async fn submit(
        &self,
        id: &str,
        req: SubmitPurchasePaymentRequest,
        actor: &AuditActor,
    ) -> Result<PurchasePaymentRequest> {
        let command = CommandReceipt::from_resource_parts(
            "payment-request-submit-",
            actor.id(),
            "purchase_payment_request.submit",
            "purchase_payment_request",
            id,
            &req.idempotency_key,
            [id.to_owned(), req.expected_version.to_string()],
        )?;
        if let Some(id) = FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(&command, &mut NoTransaction)
            .await?
        {
            return self.authorized_result(&id, actor).await;
        }
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = self.object_read.clone();
        let response_actor = actor.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        let recovery = command.clone();
        let revision = rbac.current_policy_revision().await?;
        let result = rbac
            .clone()
            .run_authorized_policy_transaction(revision, move |executor| {
                Box::pin(async move {
                    SubmitWrite {
                        db: &db,
                        rbac: &rbac,
                        object_read: object_read.as_ref(),
                        id: &id,
                        req: &req,
                        actor: &actor,
                        command: &command,
                    }
                    .persist(executor)
                    .await
                })
            })
            .await;
        self.finish_command(result, &recovery, &response_actor).await
    }

    pub(super) async fn finish_command(
        &self,
        result: Result<String>,
        command: &CommandReceipt,
        actor: &AuditActor,
    ) -> Result<PurchasePaymentRequest> {
        let id = match result {
            Ok(id) => id,
            Err(error) => {
                let recovered = FinanceCommandReceiptService::new(self.db.clone())
                    .committed_resource_id(command, &mut NoTransaction)
                    .await;
                recovered_resource(error, recovered.map_err(Error::from))?.id
            },
        };
        self.authorized_result(&id, actor).await
    }
}

async fn bind(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn ApprovalObjectReadPort,
    request: &PurchasePaymentRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ApprovalDefinitionBinding> {
    if let Some(document) = find_registered_document(db, &request.base.id, executor).await? {
        return document
            .approval_binding
            .ok_or_else(|| Error::ConflictError("付款申请缺少发布的审批定义".into()));
    }
    let command = BindPublishedDefinitionCommand {
        document_type: DocumentType::PurchasePaymentRequest,
        business_object_id: request.base.id.clone(),
        business_object_version: request.base.version,
        context: BindingRevalidationContext::new(request.payer_party_id.clone(), actor.id().into())
            .with_business_org_unit_id(request.org_unit_id.clone()),
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
    .ok_or_else(|| Error::ConflictError("请先发布采购付款申请审批流程".into()))?;
    let mut document: BusinessDocument = new_registered_document(
        &request.base.id,
        DocumentType::PurchasePaymentRequest,
        request.request_no.clone(),
    )?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, executor).await?;
    Ok(binding)
}

impl SubmitWrite<'_> {
    async fn start(
        &self,
        request: &mut PurchasePaymentRequest,
        binding: &ApprovalDefinitionBinding,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let Self { db, actor, .. } = *self;
        let now = Instant::now();
        let writes = self.prepare_start_plan(request, binding, now, executor).await?;
        db.bpm_workflow().insert_command_receipt(&writes.receipt, executor).await?;
        if request.submission_version == 1
            && db
                .business_documents()
                .mark_approval_started(
                    &request.base.id,
                    DocumentType::PurchasePaymentRequest,
                    &binding.approval_process_definition_id,
                    binding.approval_definition_version,
                    now,
                    executor,
                )
                .await?
                .is_none()
        {
            return Err(Error::ConflictError("申请审批启动状态已变化".into()));
        }
        request.approval_instance_id = Some(writes.instance.base.id.clone());
        db.purchase_payment_requests().update(request, executor).await?;
        let submission = PurchasePaymentRequestSubmissionSnapshot::freeze(request)?;
        db.purchase_payment_request_snapshots().create(&submission, executor).await?;
        let snapshot = snapshot_payload(request, actor, now)?;
        let spec = adapter_spec_of(DocumentType::PurchasePaymentRequest)?;
        start_approval::persist_runtime_writes(
            db,
            &writes,
            start_approval::RuntimeSubject {
                document_type: DocumentType::PurchasePaymentRequest,
                snapshot_payload: &snapshot,
            },
            spec.owner_role.as_str(),
            &snapshot.responsible_org_id,
            now,
            executor,
        )
        .await?;
        assign_approval_due(db, request, &writes, executor).await
    }
}

async fn assign_approval_due(
    db: &Database,
    request: &PurchasePaymentRequest,
    writes: &PlannedWrites,
    executor: &mut dyn Executor,
) -> Result<()> {
    let execution =
        writes.created_executions.first().ok_or_else(|| Error::Internal("启动申请缺少审批节点".into()))?;
    let tasks = db
        .work_items()
        .open_approval_tasks_for_execution(&ApprovalNodeExecutionId::new(&execution.base.id), executor)
        .await?;
    if tasks.len() != 1 {
        return Err(Error::ConflictError("申请入口审批任务不唯一".into()));
    }
    for mut task in tasks {
        task.due_at = Some(payment_due_at(request.data.desired_payment_date)?);
        db.work_items().update(&mut task, executor).await?;
    }
    Ok(())
}

struct SubmitWrite<'a> {
    db: &'a Database,
    rbac: &'a SharedRbacService,
    object_read: &'a dyn ApprovalObjectReadPort,
    id: &'a str,
    req: &'a SubmitPurchasePaymentRequest,
    actor: &'a AuditActor,
    command: &'a CommandReceipt,
}

impl SubmitWrite<'_> {
    async fn persist(&self, executor: &mut dyn Executor) -> Result<String> {
        let Self { db, rbac, object_read, id, req, actor, command } = *self;
        let receipts = FinanceCommandReceiptService::new(db.clone());
        if let Some(id) = receipts.committed_resource_id(command, executor).await? {
            return Ok(id);
        }
        let mut request = load(db, id, executor).await?;
        ensure_creator(&request, actor)?;
        ensure_version(&request, req.expected_version)?;
        require_sources(db, rbac, &request, actor, "submit", executor).await?;
        let submission = source::prepare_submission(db, rbac, &request, actor, true, executor).await?;
        request.submit(submission)?;
        db.purchase_payment_requests().update(&mut request, executor).await?;
        let binding = bind(db, rbac, object_read, &request, actor, executor).await?;
        self.start(&mut request, &binding, executor).await?;
        let audit = actor.clone().resource_log(
            "purchase_payment_request.submit",
            "purchase_payment_request",
            id.to_owned(),
        )?;
        receipts.save_resource(command, id.to_owned(), audit.base.id.to_owned(), executor).await?;
        persist_log(db, &audit, executor).await?;
        Ok::<String, Error>(id.to_owned())
    }
}

impl SubmitWrite<'_> {
    async fn prepare_start_plan(
        &self,
        request: &PurchasePaymentRequest,
        binding: &ApprovalDefinitionBinding,
        now: Instant,
        executor: &mut dyn Executor,
    ) -> Result<PlannedWrites> {
        let Self { db, rbac, object_read, req, actor, .. } = *self;
        let graph = start_approval::load_bound_definition_graph_with_executor(db, binding, executor).await?;
        let auth = crate::adapters::workflow::workflow_auth(db.clone(), rbac.clone());
        let context =
            BindingRevalidationContext::new(request.payer_party_id.clone(), request.created_by.clone())
                .with_business_org_unit_id(request.org_unit_id.clone());
        revalidate_binding_graph(
            db,
            &auth,
            object_read,
            &require_process_required(DocumentType::PurchasePaymentRequest)?,
            &context,
            &graph,
            executor,
        )
        .await?;
        if graph.nodes.iter().any(|node| node.assignee_participant_id.as_str() == request.created_by) {
            return Err(Error::ConflictError("付款申请人不得审批本人申请，请配置合格财务审批人".into()));
        }
        let input = start_approval::build_document_start_input(start_approval::DocumentStartInput {
            document_type: DocumentType::PurchasePaymentRequest,
            graph,
            binding,
            subject: subject_ref_for(DocumentType::PurchasePaymentRequest, &request.base.id)?,
            subject_version: request.approval_subject_version,
            actor_id: actor.id(),
            organization_id: business_org(request)?,
            idempotency_key: &req.idempotency_key,
            receipt: None,
            now,
        })?;
        let PreparedExecution::Apply(writes) = prepare_start(input)? else {
            return Err(Error::ConflictError("付款申请已提交".into()));
        };
        Ok(*writes)
    }
}

fn snapshot_payload(
    request: &PurchasePaymentRequest,
    actor: &AuditActor,
    now: Instant,
) -> Result<ApprovalSubjectSnapshotPayload> {
    Ok(ApprovalSubjectSnapshotPayload {
        document_no: request.request_no.clone(),
        responsible_org_id: business_org(request)?.to_owned(),
        submitted_by: actor.id().into(),
        submitted_at: now,
        counterparty: Some(ApprovalSubjectCounterparty::Supplier {
            supplier_id: request.supplier_id.clone().into(),
        }),
        total_amount: Some(request.total_amount),
        total_quantity: None,
        line_count: u32::try_from(request.lines.len()).map_err(|_| Error::Internal("申请来源超限".into()))?,
    })
}
