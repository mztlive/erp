//! 供应商退款一次创建提交：准备、事务写入与命令结果恢复。

use application_core::{AuditActor, CommandReceipt};
use erp_approval_runtime::approval_start::StartTaskContext;
use erp_approval_runtime::approver_directory::resolve_approvers;
use erp_audit::AuditLog;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierPaymentId;
use erp_identity::AccessTarget;
use erp_permissions::returns;
use erp_read_models::returns_center::dto::SupplierRefundView;
use erp_returns::dto::CommitSupplierRefundRequest;
use erp_returns::entity::returns::SupplierRefund;
use erp_returns::service::ReturnsService;
use erp_returns::service::approval::start_supplier_refund_approval;
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
    build_supplier_refund_snapshot, supplier_refund_adapter, supplier_refund_object_readable,
    supplier_refund_subject_ref,
};
use super::super::return_settlement::{freeze_confirmation, validate_settlement};
use super::super::start_approval::{
    ReverseStartInput, build_supplier_refund_start_input, load_bound_definition_graph_with_executor,
    persist_supplier_refund_runtime,
};
use super::{
    load_supplier_refund_org_id, persist_bound_supplier_refund_document, validate_supplier_refund_source,
};
use crate::reverse_flow::command_recovery::{CommitApproval, commit_audits, save_commit};
use crate::{Error, Result, ReverseFlowPorts};

impl ReturnsProcess {
    /// 需从原供应商付款发起退款审批时，一次创建并提交退款单。原事务校验付款来源，并一并写入单据注册、定义绑定、
    /// 退款实体、冻结快照、审批运行事实、入口任务和审计。请求、回执、原付款、发布定义、启动计划或仓储失败返回原分类。
    ///
    /// 同一幂等键同载荷已提交时只回读原退款视图；未知提交查证不成功时保留原错误，不重复写入。结果视图读取失败返回对应错误。
    pub async fn commit_supplier_refund(
        &self,
        req: CommitSupplierRefundRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_payload(
            "supplier-refund-commit-",
            actor.id(),
            "supplier_refund.commit",
            "supplier_refund",
            &req.idempotency_key,
            &req,
        )?;

        let this = self.clone();
        let actor_owned = actor.clone();
        let command = command_receipt.clone();
        let source_id = req.source_fact_id.clone();
        let transaction_result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.commit_supplier_refund_snapshot(&req, &command, &actor_owned, executor).await
                })
            })
            .await;

        let detail_id = match transaction_result {
            Ok(id) => id,
            Err(error) => {
                self.recover_return_commit(
                    error,
                    &command_receipt,
                    actor,
                    DocumentType::SupplierRefund,
                    &source_id,
                )
                .await?
            },
        };

        self.supplier_refund_result(
            &detail_id,
            actor,
            &returns::SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT,
            AccessTarget::record(&detail_id)?,
        )
        .await
    }

    // 供应商退款的创建并提交事务先授权服务端候选，再查原回执；只有未命中才准备来源并执行新写入。
    async fn commit_supplier_refund_snapshot(
        &self,
        req: &CommitSupplierRefundRequest,
        command: &CommandReceipt,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        self.require_commit_source(actor, DocumentType::SupplierRefund, &req.source_fact_id, executor)
            .await?;
        if let Some(id) =
            self.committed_return_command(command, actor, DocumentType::SupplierRefund, executor).await?
        {
            return Ok(id);
        }

        let input = self.prepare_supplier_refund_commit(req, actor, command, executor).await?;
        let id = input.refund.base.id.clone();
        apply_commit(&self.db, &self.rbac, input, command, actor, executor).await?;
        Ok(id)
    }

    // 供应商退款首次提交时依次准备原资金事实、冻结快照、登记及定义绑定所需信息，再生成两条业务审计。
    async fn prepare_supplier_refund_commit(
        &self,
        req: &CommitSupplierRefundRequest,
        actor: &AuditActor,
        receipt: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<CommitPreparation> {
        let (source_fact_id, source_version, mut refund) =
            self.supplier_refund_commit_source(req, actor, executor).await?;

        let adapter = supplier_refund_adapter()?;
        start_supplier_refund_approval(&mut refund)?;
        let id = refund.base.id.clone();
        let subject = supplier_refund_subject_ref(&id)?;
        let organization_id = load_supplier_refund_org_id(&self.db, &refund.supplier_id, executor).await?;
        let _ = supplier_refund_object_readable(&organization_id, actor.id())?;
        let now = Instant::now();
        let snapshot = build_supplier_refund_snapshot(&refund, &organization_id, actor.id(), now)?;

        let bind_command = BindPublishedDefinitionCommand {
            document_type: DocumentType::SupplierRefund,
            business_object_id: id.clone(),
            business_object_version: refund.base.version,
            context: BindingRevalidationContext::new(organization_id.clone(), actor.id().to_string()),
        };
        let document = new_registered_document(&id, DocumentType::SupplierRefund, refund.refund_no.clone())
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

/// 原事务内准备的来源版本、待写实体和审批上下文。
struct CommitPreparation {
    source_fact_id: SupplierPaymentId,
    source_version: u64,
    refund: SupplierRefund,
    bind_command: BindPublishedDefinitionCommand,
    document: BusinessDocument,
    approval: CommitApproval,
    audits: (AuditLog, AuditLog),
}

// 供应商退款候选已授权且没有原回执时，在原事务核验来源状态后进入创建提交写入。
async fn apply_commit(
    db: &Database,
    rbac: &ReverseFlowPorts,
    mut input: CommitPreparation,
    command: &CommandReceipt,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    validate_supplier_refund_source(db, &input.source_fact_id, input.source_version, executor).await?;
    validate_settlement(db, &input.refund, executor).await?;
    freeze_confirmation(db, &mut input.refund, executor).await?;
    write_commit(db, rbac, input, command, actor, executor).await
}

// 首次创建并提交供应商退款时，依次写入绑定、审批准备后的实体、运行事实、审计和回执。
async fn write_commit(
    db: &Database,
    rbac: &ReverseFlowPorts,
    input: CommitPreparation,
    command: &CommandReceipt,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let binding = persist_bound_supplier_refund_document(
        db,
        rbac,
        input.document,
        &input.bind_command,
        actor,
        executor,
    )
    .await?;
    let prepared = prepare_commit_start(
        db,
        rbac,
        &input.approval,
        &binding,
        input.refund.approval_subject_version,
        actor,
        executor,
    )
    .await?;

    ReturnsService::new(db.clone()).create_supplier_refund(&input.refund, executor).await?;
    if let PreparedExecution::Apply(writes) = prepared {
        persist_supplier_refund_runtime(
            None,
            db,
            rbac,
            &writes,
            &input.approval.snapshot,
            StartTaskContext {
                document_type: DocumentType::SupplierRefund,
                owner_role: input.approval.owner_role,
                organization_id: &input.approval.organization_id,
                now: input.approval.now,
            },
            executor,
        )
        .await?;
    }

    save_commit(db, command, &input.refund.base.id, (&input.audits.0, &input.audits.1), executor).await
}

// 生成供应商退款启动计划前，在调用方事务读取冻结绑定的定义图并重验审批人资格。
async fn prepare_commit_start(
    db: &Database,
    rbac: &ReverseFlowPorts,
    approval: &CommitApproval,
    binding: &ApprovalDefinitionBinding,
    subject_version: u32,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<PreparedExecution> {
    let graph = load_bound_definition_graph_with_executor(db, binding, executor).await?;
    let graph = resolve_approvers(db, &rbac.identity, graph, actor.id(), executor).await?;
    let start_input = build_supplier_refund_start_input(ReverseStartInput {
        resubmission_reason: None,
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

#[cfg(test)]
mod authorization_tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::super::super::testing::{actor, denied, process};
    use super::*;

    #[tokio::test]
    async fn creation_and_submission_authorize_before_receipt_source_or_definition_reads() {
        let process = process().await;
        let actor = actor();
        let request: CommitSupplierRefundRequest = serde_json::from_value(serde_json::json!({
            "source_fact_id":"unread-source", "reason":"correct", "idempotency_key":"command"
        }))
        .unwrap();
        let receipt = CommandReceipt::from_payload(
            "supplier-refund-commit-",
            actor.id(),
            "supplier_refund.commit",
            "supplier_refund",
            &request.idempotency_key,
            &request,
        )
        .unwrap();

        denied(
            process
                .commit_supplier_refund_snapshot(&request, &receipt, &actor, &mut SnapshotExecutor::default())
                .await,
        );
    }
}
