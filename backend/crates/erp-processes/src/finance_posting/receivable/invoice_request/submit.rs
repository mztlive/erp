//! 开票申请原子创建、额度占用、定义绑定与审批启动。
use application_core::{AuditActor, CommandReceipt};
use erp_audit::{AuditExt, CommandReceiptServiceExt};
use erp_core::common::time::Instant;
use erp_core::ids::SalesInvoiceRequestId;
use erp_finance::dto::receivable::SubmitInvoiceRequest;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_read_models::finance::receivable::invoice_request::InvoiceRequestView;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::entity::approval_integration::{
    ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload,
};
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use erp_workflow::service::approval::business_adapter::{BindingRevalidationContext, adapter_spec_of};
use erp_workflow::service::approval::execution::{PreparedExecution, prepare_start};
use erp_workflow::service::document_registry::{find_registered_document, new_registered_document};
use erp_workflow::{BpmExt, DocumentRegistryExt};
use id_generator::next_id;

use super::super::{ReceivableProcess, start_approval};
use super::*;

impl ReceivableProcess {
    /// 原子创建或重新提交开票申请，冻结资料及额度后启动已发布流程。
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
        if let Some(id) = command.committed_resource_id(&self.db).await? {
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
            .run_authorized_policy_transaction(revision, move |session| {
                Box::pin(async move {
                    let account = lock_account(&db, &req.receivable_account_id, session).await?;
                    ensure_effective_source(&db, &account, session).await?;
                    let mut request = candidate(&db, &account, &req, &actor, session).await?;
                    let available = account
                        .open_invoiceable_total
                        .checked_sub(reserved(&db, &account.base.id, session).await?);
                    request.submit(available)?;
                    let binding = bind(&db, &rbac, object_read.as_ref(), &request, &actor, session).await?;
                    let id = request.base.id.clone();
                    start(&db, &mut request, &binding, &req.idempotency_key, &actor, session).await?;
                    db.audit_logs().create(&command.audit(actor, id.clone())?, session).await?;
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
/// * `session` - 当前业务事务。
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
    session: &mut mongodb::ClientSession,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    if let Some(document) = find_registered_document(db, &request.base.id, session).await? {
        return document
            .approval_binding
            .ok_or_else(|| Error::ConflictError("请先发布开票申请审批流程".into()));
    }
    let command = BindPublishedDefinitionCommand {
        document_type: DocumentType::SalesInvoiceRequest,
        business_object_id: request.base.id.clone(),
        business_object_version: request.base.version,
        context: BindingRevalidationContext {
            order_source: None,
            customer_id: None,
            business_org_unit_id: None,
            scope_owner_user_id: None,
            organization_id: request.counterparty_party_id.to_string(),
            creator_id: actor.id().into(),
        },
    };
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        &command,
        actor,
        session,
    )
    .await?
    .ok_or_else(|| Error::ConflictError("请先发布开票申请审批流程".into()))?;
    let mut document: BusinessDocument = new_registered_document(
        &request.base.id,
        DocumentType::SalesInvoiceRequest,
        request.request_no.clone(),
    )?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, session).await?;
    Ok(binding)
}
/// 同一事务写入启动收据、单据守卫、申请快照和审批任务。
async fn start(
    db: &Database,
    request: &mut SalesInvoiceRequest,
    binding: &erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding,
    key: &str,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let now = Instant::now();
    let graph = start_approval::load_bound_definition_graph_with_executor(db, binding, session).await?;
    let subject = erp_workflow::entity::approval_integration::subject_ref_for(
        DocumentType::SalesInvoiceRequest,
        &request.base.id,
    )?;
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
    db.bpm_workflow().insert_command_receipt(&writes.receipt, session).await?;
    let guarded = db
        .business_documents()
        .mark_approval_started(
            &request.base.id,
            DocumentType::SalesInvoiceRequest,
            &binding.approval_process_definition_id,
            binding.approval_definition_version,
            now,
            session,
        )
        .await?;
    if guarded.is_none() {
        return Err(Error::ConflictError("申请审批状态已变化，请刷新后重试".into()));
    }
    db.sales_invoice_requests().update(request, session).await?;
    let snapshot = ApprovalSubjectSnapshotPayload {
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
    };
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
        session,
    )
    .await
}

/// 申请只能来自已生效销售单形成的当前应收来源。
async fn ensure_effective_source(
    db: &Database,
    account: &ReceivableAccount,
    executor: &mut dyn Executor,
) -> Result<()> {
    let order = db
        .sales_orders()
        .find_by_id(&account.sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;
    if order.commercial_status != erp_sales::entity::sales_order::CommercialStatus::Effective
        || order.current_revision_id().is_none()
        || order.customer_id != account.customer_id
    {
        return Err(Error::ConflictError("仅可为已生效销售单提交开票申请".into()));
    }
    Ok(())
}

#[cfg(test)]
mod first_submit_binding_tests {
    /// 首次提交必须先绑定再注册，不能把未注册单据当成 NotFound。
    #[test]
    fn first_submit_does_not_treat_missing_document_as_not_found() {
        let production = include_str!("submit.rs").split("#[cfg(test)]").next().expect("生产代码");
        let bind_fn = production
            .split("async fn bind(")
            .nth(1)
            .and_then(|rest| rest.split("async fn start(").next())
            .expect("bind 生产片段");
        assert!(bind_fn.contains("find_registered_document"));
        assert!(bind_fn.contains("bind_published_definition_on_document_create"));
        assert!(bind_fn.contains("new_registered_document"));
        assert!(
            !bind_fn.contains("find_approval_binding"),
            "find_approval_binding 会把 DocumentMissing 映射成 NotFound，首次提交不得调用"
        );
    }
}
