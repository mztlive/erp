//! 客户退款一次创建提交：准备、事务写入与命令结果恢复。

use application_core::{AuditActor, CommandReceipt};
use erp_audit::AuditLog;
use erp_core::common::time::Instant;
use erp_core::ids::CustomerReceiptId;
use erp_identity::SharedRbacService;
use erp_read_models::returns_center::dto::CustomerRefundView;
use erp_returns::dto::CommitCustomerRefundRequest;
use erp_returns::entity::returns::CustomerRefund;
use erp_returns::service::ReturnsService;
use erp_returns::service::approval::start_customer_refund_approval;
use erp_workflow::ApprovalObjectReadPort;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::service::approval::binding::BindPublishedDefinitionCommand;
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::{PreparedExecution, prepare_start};
use erp_workflow::service::document_registry::new_registered_document;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::super::ReturnsProcess;
use super::super::adapter::{
    build_customer_refund_snapshot, customer_refund_adapter, customer_refund_object_readable,
    customer_refund_subject_ref,
};
use super::super::start_approval::{
    CustomerRefundStartInput, build_customer_refund_start_input, load_bound_definition_graph_with_executor,
    persist_runtime_writes,
};
use super::{
    load_customer_responsible_org_id, persist_bound_customer_refund_document, replay_customer_commit,
};
use crate::adapters::workflow::approver_directory::resolve_approvers;
use crate::reverse_flow::command_recovery::{CommitApproval, commit_audits, recovered_resource, save_commit};
use crate::{Error, Result};

impl ReturnsProcess {
    /// 按原回款一次创建客户退款并启动审批。
    ///
    /// 单据注册、定义绑定、退款实体、审批快照、运行事实、入口任务和两类审计
    /// 全部在同一 MongoDB 事务内写入。
    ///
    /// # 参数
    /// * `req` - 一次创建并提交的请求。
    /// * `actor` - 已通过鉴权的审计操作人。
    ///
    /// # 返回
    /// 返回新建退款单视图；同载荷命令已提交时返回已有退款单视图。
    ///
    /// # 错误
    /// 请求校验、命令收据、原回款、发布定义、审批启动或仓储写入失败时返回对应错误。
    /// 提交结果未知时按同一命令回读，未确认则保留原错误。视图读取失败时返回对应错误。
    pub async fn commit_customer_refund(
        &self,
        req: CommitCustomerRefundRequest,
        actor: &AuditActor,
    ) -> Result<CustomerRefundView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_payload(
            "customer-refund-commit-",
            actor.id(),
            "customer_refund.commit",
            "customer_refund",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(id) = self.customer_refund_commit_replay(&command_receipt, actor).await? {
            return self.reads().customer_refund_detail(&id).await.map_err(Error::from);
        }
        let input = self.prepare_customer_refund_commit(&req, actor, &command_receipt).await?;
        let id = input.refund.base.id.clone();
        let command = command_receipt.clone();
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = self.object_read.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    apply_commit(&db, &rbac, object_read.as_ref(), input, &command, &actor_owned, executor)
                        .await
                })
            })
            .await;
        let detail_id = match transaction_result {
            Ok(Some(existing_id)) => existing_id,
            Ok(None) => id,
            Err(error) => {
                recovered_resource(error, self.customer_refund_commit_replay(&command_receipt, actor).await)?
            },
        };
        self.reads().customer_refund_detail(&detail_id).await.map_err(Error::from)
    }

    /// 按既有顺序准备原资金事实、冻结快照、单据绑定和两条业务审计。
    async fn prepare_customer_refund_commit(
        &self,
        req: &CommitCustomerRefundRequest,
        actor: &AuditActor,
        receipt: &CommandReceipt,
    ) -> Result<CommitPreparation> {
        let (source_fact_id, source_version, mut refund) =
            self.customer_refund_commit_source(req, actor).await?;
        let adapter = customer_refund_adapter()?;
        start_customer_refund_approval(&mut refund)?;
        let id = refund.base.id.clone();
        let subject = customer_refund_subject_ref(&id)?;
        let organization_id = load_customer_responsible_org_id(&self.db, &refund.customer_id).await?;
        let _ = customer_refund_object_readable(&organization_id, actor.id())?;
        let now = Instant::now();
        let snapshot = build_customer_refund_snapshot(&refund, &organization_id, actor.id(), now)?;
        let bind_command = BindPublishedDefinitionCommand {
            document_type: DocumentType::CustomerRefund,
            business_object_id: id.clone(),
            business_object_version: refund.base.version,
            context: BindingRevalidationContext::new(organization_id.clone(), actor.id().to_string()),
        };
        let document = new_registered_document(&id, DocumentType::CustomerRefund, refund.refund_no.clone())
            .map_err(Error::from)?;
        let audits = commit_audits(actor, receipt, &id)?;
        let approval = CommitApproval {
            subject,
            organization_id,
            snapshot,
            owner_role: adapter.owner_role,
            idempotency_key: req.idempotency_key.clone(),
            now,
        };
        Ok(CommitPreparation {
            source_fact_id,
            source_version,
            refund,
            bind_command,
            document,
            approval,
            audits,
        })
    }
}

/// 事务前准备的数据；来源版本必须在写入事务内重验。
struct CommitPreparation {
    source_fact_id: CustomerReceiptId,
    source_version: u64,
    refund: CustomerRefund,
    bind_command: BindPublishedDefinitionCommand,
    document: BusinessDocument,
    approval: CommitApproval,
    audits: (AuditLog, AuditLog),
}

/// 保留先重验来源、再查命令收据、最后执行写入的顺序。
async fn apply_commit(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn ApprovalObjectReadPort,
    input: CommitPreparation,
    command: &CommandReceipt,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    input.refund.ensure_submitter(actor.id())?;
    if let Some(id) = replay_customer_commit(
        db,
        rbac,
        actor,
        (&input.source_fact_id, input.source_version),
        command,
        executor,
    )
    .await?
    {
        return Ok(Some(id));
    }
    write_commit(db, rbac, object_read, input, command, actor, executor).await?;
    Ok(None)
}

/// 绑定、审批准备、实体、运行事实、审计及回执严格按原顺序写入。
async fn write_commit(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn ApprovalObjectReadPort,
    input: CommitPreparation,
    command: &CommandReceipt,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let binding = persist_bound_customer_refund_document(
        db,
        rbac,
        object_read,
        input.document,
        &input.bind_command,
        actor,
        executor,
    )
    .await?;
    let prepared = prepare_commit_start(
        db,
        &input.approval,
        &binding,
        input.refund.approval_subject_version,
        actor,
        executor,
    )
    .await?;
    ReturnsService::new(db.clone()).create_customer_refund(&input.refund, executor).await?;
    if let PreparedExecution::Apply(writes) = prepared {
        persist_runtime_writes(
            db,
            &writes,
            &input.approval.snapshot,
            input.approval.owner_role,
            &input.approval.organization_id,
            input.approval.now,
            executor,
        )
        .await?;
    }
    save_commit(db, command, &input.refund.base.id, (&input.audits.0, &input.audits.1), executor).await
}

/// 在调用方事务中读取绑定图、重验审批人并准备启动写集合。
async fn prepare_commit_start(
    db: &Database,
    approval: &CommitApproval,
    binding: &ApprovalDefinitionBinding,
    subject_version: u32,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<PreparedExecution> {
    let graph = load_bound_definition_graph_with_executor(db, binding, executor).await?;
    let graph = resolve_approvers(db, graph, actor.id(), executor).await?;
    let start_input = build_customer_refund_start_input(CustomerRefundStartInput {
        graph,
        binding,
        subject: approval.subject.clone(),
        subject_version,
        actor_id: actor.id(),
        organization_id: &approval.organization_id,
        idempotency_key: &approval.idempotency_key,
        receipt: None,
        now: approval.now,
    })?;
    Ok(prepare_start(start_input)?)
}
