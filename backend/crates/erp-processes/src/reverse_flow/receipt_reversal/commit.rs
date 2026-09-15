//! 回款冲正一次创建并提交：根命令回执优先，原资金事实在事务内重验。

use super::super::adapter::{
    build_receipt_reversal_snapshot, receipt_reversal_adapter, receipt_reversal_object_readable,
    receipt_reversal_subject_ref,
};
use super::super::start_approval::{
    build_receipt_reversal_start_input, load_bound_definition_graph_with_executor,
    persist_receipt_reversal_runtime, ReceiptReversalStartInput,
};
use super::super::ReturnsProcess;
use super::context::{load_receipt_reversal_context, persist_bound_receipt_reversal_document};
use crate::{Error, Result};
use application_core::{AuditActor, CommandReceipt};
use erp_audit::{AuditActorLogs, AuditExt, CommandReceiptServiceExt as _};
use erp_core::common::time::Instant;
use erp_core::ids::CustomerReceiptId;
use erp_finance::entity::receivable::CustomerReceiptStatus;
use erp_finance::repository::ReceivableExt;
use erp_read_models::returns_center::dto::ReceiptReversalView;
use erp_returns::dto::CommitReceiptReversalRequest;
use erp_returns::service::approval::start_receipt_reversal_approval;
use erp_returns::service::receipt_reversal::build_commit;
use erp_returns::service::shared::ensure_posted_source;
use erp_returns::service::ReturnsService;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::binding::BindPublishedDefinitionCommand;
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::approval::execution::prepare_start;
use erp_workflow::service::document_registry::new_registered_document;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

impl ReturnsProcess {
    /// 按原回款一次创建回款冲正并启动审批。
    ///
    /// 单据注册、定义绑定、冲正实体、审批快照、运行事实、入口任务和审计在同一
    /// MongoDB 事务内完成。
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
        if let Some(reversal_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self
                .reads()
                .receipt_reversal_detail(&reversal_id)
                .await
                .map_err(crate::Error::from);
        }
        let receipt = self
            .db
            .customer_receipts()
            .find_by_id(&req.source_fact_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原客户回款不存在".to_string()))?;
        let source_fact_id = CustomerReceiptId::new(receipt.base.id.clone());
        let source_version = receipt.base.version;
        let mut reversal = build_commit(&req, source_fact_id.clone(), receipt.amount, actor.id())?;
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
            context: BindingRevalidationContext {
                order_source: None,
                customer_id: None,
                business_org_unit_id: None,
                scope_owner_user_id: None,
                organization_id: organization_id.clone(),
                creator_id: actor.id().to_string(),
            },
        };
        let document =
            new_registered_document(&id, DocumentType::ReceiptReversal, reversal.reversal_no.clone())
                .map_err(crate::Error::from)?;
        let create_audit =
            actor
                .clone()
                .resource_log("receipt_reversal.create", "receipt_reversal", id.clone())?;
        let submit_audit =
            actor
                .clone()
                .resource_log("receipt_reversal.submit", "receipt_reversal", id.clone())?;
        let command_audit = command_receipt.audit(actor.clone(), id.clone())?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let idempotency_key = req.idempotency_key;
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    validate_receipt_reversal_source(&db, &source_fact_id, source_version, session).await?;
                    let binding = persist_bound_receipt_reversal_document(
                        &db,
                        &rbac,
                        object_read.as_ref(),
                        document,
                        &bind_command,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    let graph = load_bound_definition_graph_with_executor(&db, &binding, session).await?;
                    let start_input = build_receipt_reversal_start_input(ReceiptReversalStartInput {
                        graph,
                        binding: &binding,
                        subject,
                        subject_version: reversal.approval_subject_version,
                        actor_id: actor_owned.id(),
                        organization_id: &organization_id,
                        idempotency_key: &idempotency_key,
                        receipt: None,
                        now,
                    })?;
                    let prepared = prepare_start(start_input)?;
                    ReturnsService::persist_created_receipt_reversal(&db, &reversal, session).await?;
                    if let erp_workflow::service::approval::execution::PreparedExecution::Apply(writes) =
                        prepared
                    {
                        persist_receipt_reversal_runtime(
                            &db,
                            &writes,
                            &snapshot,
                            adapter.owner_role,
                            &organization_id,
                            now,
                            session,
                        )
                        .await?;
                    }
                    db.audit_logs().create(&create_audit, session).await?;
                    db.audit_logs().create(&submit_audit, session).await?;
                    db.audit_logs().create(&command_audit, session).await?;
                    Ok::<(), crate::Error>(())
                })
            })
            .await;
        let detail_id = match transaction_result {
            Ok(()) => id,
            Err(error) => match command_receipt.committed_resource_id(&self.db).await? {
                Some(reversal_id) => reversal_id,
                None => return Err(error),
            },
        };
        self.reads()
            .receipt_reversal_detail(&detail_id)
            .await
            .map_err(crate::Error::from)
    }
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
