//! 回款冲正一次创建提交：准备、事务写入与命令结果恢复。

use application_core::{AuditActor, CommandReceipt};
use erp_approval_runtime::approval_start::StartTaskContext;
use erp_approval_runtime::approver_directory::resolve_approvers;
use erp_audit::AuditLog;
use erp_core::common::time::Instant;
use erp_core::ids::CustomerReceiptId;
use erp_finance::entity::receivable::CustomerReceiptStatus;
use erp_finance::repository::ReceivableExt;
use erp_identity::AccessTarget;
use erp_permissions::returns;
use erp_read_models::returns_center::dto::ReceiptReversalView;
use erp_returns::dto::CommitReceiptReversalRequest;
use erp_returns::entity::returns::ReceiptReversal;
use erp_returns::service::ReturnsService;
use erp_returns::service::approval::start_receipt_reversal_approval;
use erp_returns::service::receipt_reversal::build_commit;
use erp_returns::service::shared::ensure_posted_source;
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
    build_receipt_reversal_snapshot, receipt_reversal_adapter, receipt_reversal_object_readable,
    receipt_reversal_subject_ref,
};
use super::super::start_approval::{
    ReverseStartInput, build_receipt_reversal_start_input, load_bound_definition_graph_with_executor,
    persist_receipt_reversal_runtime,
};
use super::context::{load_receipt_reversal_context, persist_bound_receipt_reversal_document};
use crate::reverse_flow::command_recovery::{CommitApproval, commit_audits, save_commit};
use crate::{Error, Result, ReverseFlowPorts};

impl ReturnsProcess {
    /// 按原回款一次创建回款冲正并启动审批。
    ///
    /// 单据注册、定义绑定、冲正实体、审批快照、运行事实、入口任务和审计在同一
    /// MongoDB 事务内完成。
    ///
    /// # 参数
    /// * `req` - 一次创建并提交的请求。
    /// * `actor` - 已通过鉴权的审计操作人。
    ///
    /// # 返回
    /// 返回新建冲正单视图；同载荷命令已提交时返回已有冲正单视图。
    ///
    /// # 错误
    /// 请求校验、命令收据、原回款、发布定义、审批启动或仓储写入失败时返回对应错误。
    /// 提交结果未知时按同一命令回读，未确认则保留原错误。视图读取失败时返回对应错误。
    pub async fn commit_receipt_reversal(
        &self,
        req: CommitReceiptReversalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_payload(
            "receipt-reversal-commit-",
            actor.id(),
            "receipt_reversal.commit",
            "receipt_reversal",
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
                    this.commit_receipt_reversal_snapshot(&req, &command, &actor_owned, executor).await
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
                    DocumentType::ReceiptReversal,
                    &source_id,
                )
                .await?
            },
        };

        self.receipt_reversal_result(
            &detail_id,
            actor,
            &returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT,
            AccessTarget::record(&detail_id)?,
        )
        .await
    }

    /// 原事务先授权服务端候选，再读取收据、准备来源并完成全部写入。
    async fn commit_receipt_reversal_snapshot(
        &self,
        req: &CommitReceiptReversalRequest,
        command: &CommandReceipt,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        self.require_commit_source(actor, DocumentType::ReceiptReversal, &req.source_fact_id, executor)
            .await?;
        if let Some(id) =
            self.committed_return_command(command, actor, DocumentType::ReceiptReversal, executor).await?
        {
            return Ok(id);
        }

        let input = self.prepare_receipt_reversal_commit(req, actor, command, executor).await?;
        let id = input.reversal.base.id.clone();
        apply_commit(&self.db, &self.rbac, input, command, actor, executor).await?;
        Ok(id)
    }

    /// 按既有顺序准备原资金事实、冻结快照、单据绑定和两条业务审计。
    async fn prepare_receipt_reversal_commit(
        &self,
        req: &CommitReceiptReversalRequest,
        actor: &AuditActor,
        receipt: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<CommitPreparation> {
        let (source_fact_id, source_version, mut reversal) =
            self.receipt_reversal_commit_source(req, actor, executor).await?;

        let adapter = receipt_reversal_adapter()?;
        start_receipt_reversal_approval(&mut reversal)?;
        let id = reversal.base.id.clone();
        let subject = receipt_reversal_subject_ref(&id)?;
        let (organization_id, customer_id) =
            load_receipt_reversal_context(&self.db, &reversal.original_customer_receipt_id, executor).await?;
        let _ = receipt_reversal_object_readable(&organization_id, actor.id())?;
        let now = Instant::now();
        let snapshot = build_receipt_reversal_snapshot(
            &reversal,
            &organization_id,
            customer_id.as_ref(),
            actor.id(),
            now,
        )?;

        let bind_command = BindPublishedDefinitionCommand {
            document_type: DocumentType::ReceiptReversal,
            business_object_id: id.clone(),
            business_object_version: reversal.base.version,
            context: BindingRevalidationContext::new(organization_id.clone(), actor.id().to_string()),
        };
        let document =
            new_registered_document(&id, DocumentType::ReceiptReversal, reversal.reversal_no.clone())
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
            reversal,
            bind_command,
            document,
            approval,
            audits,
        })
    }

    /// 按原顺序读取原回款及版本，准备一次创建提交所需的冲正事实。
    async fn receipt_reversal_commit_source(
        &self,
        req: &CommitReceiptReversalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(CustomerReceiptId, u64, ReceiptReversal)> {
        let receipt = self
            .db
            .customer_receipts()
            .find_by_id(&req.source_fact_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原客户回款不存在".to_string()))?;
        let source_fact_id = CustomerReceiptId::new(receipt.base.id.clone());
        let source_version = receipt.base.version;
        let reversal = build_commit(req, source_fact_id.clone(), receipt.amount, actor.id())?;
        Ok((source_fact_id, source_version, reversal))
    }
}

/// 原事务内准备的来源版本、待写实体和审批上下文。
struct CommitPreparation {
    source_fact_id: CustomerReceiptId,
    source_version: u64,
    reversal: ReceiptReversal,
    bind_command: BindPublishedDefinitionCommand,
    document: BusinessDocument,
    approval: CommitApproval,
    audits: (AuditLog, AuditLog),
}

/// 已授权且未命中收据后，在原事务校验来源业务状态并执行写入。
async fn apply_commit(
    db: &Database,
    rbac: &ReverseFlowPorts,
    input: CommitPreparation,
    command: &CommandReceipt,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    validate_receipt_reversal_source(db, &input.source_fact_id, input.source_version, executor).await?;
    write_commit(db, rbac, input, command, actor, executor).await
}

/// 绑定、审批准备、实体、运行事实、审计及回执严格按原顺序写入。
async fn write_commit(
    db: &Database,
    rbac: &ReverseFlowPorts,
    input: CommitPreparation,
    command: &CommandReceipt,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let binding = persist_bound_receipt_reversal_document(
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
        input.reversal.approval_subject_version,
        actor,
        executor,
    )
    .await?;

    ReturnsService::persist_created_receipt_reversal(db, &input.reversal, executor).await?;
    if let PreparedExecution::Apply(writes) = prepared {
        persist_receipt_reversal_runtime(
            None,
            db,
            rbac,
            &writes,
            &input.approval.snapshot,
            StartTaskContext {
                document_type: DocumentType::ReceiptReversal,
                owner_role: input.approval.owner_role,
                organization_id: &input.approval.organization_id,
                now: input.approval.now,
            },
            executor,
        )
        .await?;
    }

    save_commit(db, command, &input.reversal.base.id, (&input.audits.0, &input.audits.1), executor).await
}

/// 在调用方事务中读取绑定图、重验审批人并准备启动写集合。
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
    let start_input = build_receipt_reversal_start_input(ReverseStartInput {
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
/// 在创建冲正单的同一事务中重读并校验原客户回款事实。
///
/// 原回款必须仍为调用前读取的版本且已经过账，避免为非正式事实创建审批任务。
async fn validate_receipt_reversal_source(
    db: &Database,
    source_fact_id: &CustomerReceiptId,
    expected_version: u64,
    executor: &mut dyn Executor,
) -> Result<()> {
    let receipt = db
        .customer_receipts()
        .find_by_id(source_fact_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原客户回款不存在".to_string()))?;
    Ok(ensure_posted_source(
        receipt.base.version,
        expected_version,
        receipt.status == CustomerReceiptStatus::Posted,
        "只有已过账的客户回款才能发起冲正",
    )?)
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
        let request: CommitReceiptReversalRequest = serde_json::from_value(serde_json::json!({
            "source_fact_id":"unread-source", "reason":"correct", "idempotency_key":"command"
        }))
        .unwrap();
        let receipt = CommandReceipt::from_payload(
            "receipt-reversal-commit-",
            actor.id(),
            "receipt_reversal.commit",
            "receipt_reversal",
            &request.idempotency_key,
            &request,
        )
        .unwrap();

        denied(
            process
                .commit_receipt_reversal_snapshot(
                    &request,
                    &receipt,
                    &actor,
                    &mut SnapshotExecutor::default(),
                )
                .await,
        );
    }
}
