//! 回款冲正一次创建提交：准备、事务写入与命令结果恢复。

use application_core::{AuditActor, CommandReceipt};
use erp_audit::AuditLog;
use erp_core::common::time::Instant;
use erp_core::ids::CustomerReceiptId;
use erp_finance::entity::receivable::CustomerReceiptStatus;
use erp_finance::repository::ReceivableExt;
use erp_identity::SharedRbacService;
use erp_read_models::returns_center::dto::ReceiptReversalView;
use erp_returns::ReturnsCommandReceiptService;
use erp_returns::dto::CommitReceiptReversalRequest;
use erp_returns::entity::returns::ReceiptReversal;
use erp_returns::service::ReturnsService;
use erp_returns::service::approval::start_receipt_reversal_approval;
use erp_returns::service::receipt_reversal::build_commit;
use erp_returns::service::shared::ensure_posted_source;
use erp_workflow::ApprovalObjectReadPort;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::service::approval::binding::BindPublishedDefinitionCommand;
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::{PreparedExecution, prepare_start};
use erp_workflow::service::document_registry::new_registered_document;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::super::ReturnsProcess;
use super::super::adapter::{
    build_receipt_reversal_snapshot, receipt_reversal_adapter, receipt_reversal_object_readable,
    receipt_reversal_subject_ref,
};
use super::super::start_approval::{
    ReceiptReversalStartInput, build_receipt_reversal_start_input, load_bound_definition_graph_with_executor,
    persist_receipt_reversal_runtime,
};
use super::context::{load_receipt_reversal_context, persist_bound_receipt_reversal_document};
use crate::adapters::workflow::approver_directory::resolve_approvers;
use crate::reverse_flow::command_recovery::{CommitApproval, commit_audits, recovered_resource, save_commit};
use crate::{Error, Result};

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
        if let Some(id) = ReturnsCommandReceiptService::new(self.db.clone())
            .committed_resource_id(&command_receipt, &mut NoTransaction)
            .await?
        {
            return self.reads().receipt_reversal_detail(&id).await.map_err(Error::from);
        }
        let input = self.prepare_receipt_reversal_commit(&req, actor, &command_receipt).await?;
        let id = input.reversal.base.id.clone();
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
            Err(error) => self.recover_receipt_commit(error, &command_receipt).await?,
        };
        self.reads().receipt_reversal_detail(&detail_id).await.map_err(Error::from)
    }

    /// 按既有顺序准备原资金事实、冻结快照、单据绑定和两条业务审计。
    async fn prepare_receipt_reversal_commit(
        &self,
        req: &CommitReceiptReversalRequest,
        actor: &AuditActor,
        receipt: &CommandReceipt,
    ) -> Result<CommitPreparation> {
        let (source_fact_id, source_version, mut reversal) =
            self.receipt_reversal_commit_source(req, actor).await?;
        let adapter = receipt_reversal_adapter()?;
        start_receipt_reversal_approval(&mut reversal)?;
        let id = reversal.base.id.clone();
        let subject = receipt_reversal_subject_ref(&id)?;
        let (organization_id, customer_id) =
            load_receipt_reversal_context(&self.db, &reversal.original_customer_receipt_id).await?;
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

    /// 只读查询冲正命令收据，未确认提交时保留原错误。
    async fn recover_receipt_commit(&self, error: Error, receipt: &CommandReceipt) -> Result<String> {
        recovered_resource(
            error,
            ReturnsCommandReceiptService::new(self.db.clone())
                .committed_resource_id(receipt, &mut NoTransaction)
                .await
                .map_err(Error::from),
        )
    }

    /// 按原顺序读取原回款及版本，准备一次创建提交所需的冲正事实。
    async fn receipt_reversal_commit_source(
        &self,
        req: &CommitReceiptReversalRequest,
        actor: &AuditActor,
    ) -> Result<(CustomerReceiptId, u64, ReceiptReversal)> {
        let receipt = self
            .db
            .customer_receipts()
            .find_by_id(&req.source_fact_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原客户回款不存在".to_string()))?;
        let source_fact_id = CustomerReceiptId::new(receipt.base.id.clone());
        let source_version = receipt.base.version;
        let reversal = build_commit(req, source_fact_id.clone(), receipt.amount, actor.id())?;
        Ok((source_fact_id, source_version, reversal))
    }
}

/// 事务前准备的数据；来源版本必须在写入事务内重验。
struct CommitPreparation {
    source_fact_id: CustomerReceiptId,
    source_version: u64,
    reversal: ReceiptReversal,
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
    validate_receipt_reversal_source(db, &input.source_fact_id, input.source_version, executor).await?;
    if let Some(id) =
        ReturnsCommandReceiptService::new(db.clone()).committed_resource_id(command, executor).await?
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
    let binding = persist_bound_receipt_reversal_document(
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
        input.reversal.approval_subject_version,
        actor,
        executor,
    )
    .await?;
    ReturnsService::persist_created_receipt_reversal(db, &input.reversal, executor).await?;
    if let PreparedExecution::Apply(writes) = prepared {
        persist_receipt_reversal_runtime(
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
    save_commit(db, command, &input.reversal.base.id, (&input.audits.0, &input.audits.1), executor).await
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
    let start_input = build_receipt_reversal_start_input(ReceiptReversalStartInput {
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
