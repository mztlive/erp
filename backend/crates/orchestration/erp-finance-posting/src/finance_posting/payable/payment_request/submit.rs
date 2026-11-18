//! 提交冻结正式来源、占用应付额度并创建新的审批实例。
use application_core::CommandReceipt;
use bpm::ids::ApprovalNodeExecutionId;
use erp_approval_runtime::approver_directory::resolve_approvers;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
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
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use erp_workflow::service::approval::business_adapter::{BindingRevalidationContext, adapter_spec_of};
use erp_workflow::service::approval::execution::{PlannedWrites, PreparedExecution, prepare_start};
use erp_workflow::service::approval::policy::require_process_required;
use erp_workflow::service::document_registry::{find_registered_document, new_registered_document};
use erp_workflow::{BpmExt, DocumentRegistryExt, WorkItemExt};
use persistence_core::Transactional;

use super::*;
use crate::FinancePostingPorts;
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
    rbac: &FinancePostingPorts,
    data: PurchasePaymentRequestData,
    actor: &AuditActor,
    command: &CommandReceipt,
    executor: &mut dyn Executor,
) -> Result<PurchasePaymentRequest> {
    let draft = PurchasePaymentRequest::new(id_generator::next_id(), data, actor.id())?;
    let draft = draft::create_draft(db, rbac, draft, actor, executor).await?;

    let req = SubmitPurchasePaymentRequest {
        resubmission_reason: None,
        expected_version: draft.base.version,
        idempotency_key: command.id().to_owned(),
    };
    let id =
        SubmitWrite { db, rbac, id: &draft.base.id, req: &req, actor, command }.persist(executor).await?;
    load(db, &id, executor).await
}

impl PurchasePaymentRequestProcess {
    /// 原子提交申请并冻结逐采购付款额度。
    /// # 参数
    /// * `id` - 已保存的付款申请。
    /// * `req` - 本次版本及幂等键。
    /// * `actor` - 具备提交和完整来源资格的当前经办人。
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
        let mut payload = vec![id.to_owned(), req.expected_version.to_string()];
        if let Some(reason) = &req.resubmission_reason {
            payload.push(reason.clone());
        }
        let command = CommandReceipt::from_resource_parts(
            "payment-request-submit-",
            actor.id(),
            "purchase_payment_request.submit",
            "purchase_payment_request",
            id,
            &req.idempotency_key,
            payload,
        )?;

        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let response_actor = actor.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        let recovery = command.clone();
        let response_id = id.clone();
        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let result = SubmitWrite {
                        db: &db,
                        rbac: &rbac,
                        id: &id,
                        req: &req,
                        actor: &actor,
                        command: &command,
                    }
                    .persist(executor)
                    .await?;
                    authorized_request(&db, &rbac.authorization, &result, &actor, "submit", executor).await
                })
            })
            .await;

        self.finish_command(result, &recovery, &response_actor, "submit", &response_id).await
    }

    /// 首次事务直接返回同快照结果；失败只查证原命令，不重放业务写入。
    /// # 参数
    /// result 为首次事务结果，command 为原命令回执，actor 为当前认证操作人，
    /// action 为原命令动作，id 为原付款申请主键。
    /// # 返回
    /// 返回同事务授权的付款申请，或已提交命令的当前授权结果。
    /// # 错误
    /// 查证缺失保留原错；未知提交下授权或读取失败保留原未知结果。
    pub(super) async fn finish_command(
        &self,
        result: Result<PurchasePaymentRequest>,
        command: &CommandReceipt,
        actor: &AuditActor,
        action: &'static str,
        id: &str,
    ) -> Result<PurchasePaymentRequest> {
        let original = match result {
            Ok(request) => return Ok(request),
            Err(error) if error.command_may_have_committed() => error,
            Err(error) => return Err(error),
        };
        let unknown = matches!(original, Error::OutcomeUnknown(_));

        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let command = command.clone();
        let id = id.to_owned();
        let recovered = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let request =
                        authorized_request(&db, &rbac.authorization, &id, &actor, action, executor).await?;
                    let committed = FinanceCommandReceiptService::new(db.clone())
                        .committed_resource_id(&command, executor)
                        .await?;
                    match committed {
                        Some(committed_id) if committed_id == id => Ok(Some(request)),
                        Some(_) => Err(Error::ConflictError("付款申请命令回执目标不一致".into())),
                        None => Ok(None),
                    }
                })
            })
            .await;

        match recovered {
            Ok(Some(request)) => Ok(request),
            Ok(None) => Err(original),
            Err(_) if unknown => Err(original),
            Err(error) => Err(error),
        }
    }
}

async fn bind(
    db: &Database,
    rbac: &FinancePostingPorts,
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
    let binding = rbac
        .runtime
        .bind_document(db, &command, actor, executor)
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
        let Self { db, rbac, actor, .. } = *self;
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
            rbac,
            &writes,
            start_approval::RuntimeStartInput {
                resubmission_reason: self.req.resubmission_reason.as_deref(),
                document_type: DocumentType::PurchasePaymentRequest,
                snapshot_payload: &snapshot,
                owner_role: spec.owner_role.as_str(),
                organization_id: &snapshot.responsible_org_id,
                now,
            },
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
    if tasks.is_empty() || tasks.len() > 1 + execution.additional_assignees.len() {
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
    rbac: &'a FinancePostingPorts,
    id: &'a str,
    req: &'a SubmitPurchasePaymentRequest,
    actor: &'a AuditActor,
    command: &'a CommandReceipt,
}

impl SubmitWrite<'_> {
    async fn persist(&self, executor: &mut dyn Executor) -> Result<String> {
        let Self { db, rbac, id, req, actor, command } = *self;
        let mut request = authorized_request(db, &rbac.authorization, id, actor, "submit", executor).await?;
        let receipts = FinanceCommandReceiptService::new(db.clone());
        if let Some(id) = receipts.committed_resource_id(command, executor).await? {
            return Ok(id);
        }

        let sources = require_sources(db, rbac, &request, actor, "submit", executor).await?;
        ensure_version(&request, req.expected_version)?;
        let submission =
            source::prepare_submission(db, &rbac.authorization, &sources, &request, actor, true, executor)
                .await?;
        request.submit(submission)?;
        db.purchase_payment_requests().update(&mut request, executor).await?;

        let binding = bind(db, rbac, &request, actor, executor).await?;
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
        let Self { db, rbac, req, actor, .. } = *self;
        let graph = start_approval::load_bound_definition_graph_with_executor(db, binding, executor).await?;
        let graph = resolve_approvers(db, &rbac.authorization, graph, actor.id(), executor).await?;

        let context =
            BindingRevalidationContext::new(request.payer_party_id.clone(), request.created_by.clone())
                .with_business_org_unit_id(request.org_unit_id.clone());
        rbac.runtime
            .revalidate_binding(
                &require_process_required(DocumentType::PurchasePaymentRequest)?,
                &context,
                &graph,
                executor,
            )
            .await?;
        if graph.nodes.iter().any(|node| node.has_participant(&request.created_by)) {
            return Err(Error::ConflictError("付款申请人不得审批本人申请，请配置合格财务审批人".into()));
        }

        let input = start_approval::build_document_start_input(start_approval::DocumentStartInput {
            resubmission_reason: req.resubmission_reason.as_deref(),
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
