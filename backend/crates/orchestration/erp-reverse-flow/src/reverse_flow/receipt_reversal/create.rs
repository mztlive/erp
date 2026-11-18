//! 回款冲正草稿创建的注册、绑定和审计根事务。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_identity::AccessTarget;
use erp_permissions::returns;
use erp_read_models::returns_center::dto::ReceiptReversalView;
use erp_returns::dto::CreateReceiptReversalRequest;
use erp_returns::entity::returns::ReceiptReversal;
use erp_returns::service::ReturnsService;
use erp_returns::service::receipt_reversal::build_create;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::binding::BindPublishedDefinitionCommand;
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::document_registry::new_registered_document;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::super::ReturnsProcess;
use super::context::{load_receipt_reversal_context, persist_bound_receipt_reversal_document};
use crate::{Result, ReverseFlowPorts};

impl ReturnsProcess {
    /// 登记回款冲正草稿，并在同一事务绑定已发布审批定义。
    ///
    /// 冲正单号全局唯一（唯一索引）构成幂等去重。经办人与复核人必须不同。绑定失败必须回滚业务实体，不得把绑定推迟到提交。返回新建冲正单视图。
    ///
    /// `ConflictError`：冲正单号重复或流程未配置。`NotFound`：原回款不存在。
    pub async fn create_receipt_reversal(
        &self,
        req: CreateReceiptReversalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        let process = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.create_receipt_reversal_stage(req, &actor, executor).await })
            })
            .await
    }

    // 原创建事务内先授权，再读取真实往来主体、绑定定义并原子登记。
    async fn create_receipt_reversal_stage(
        &self,
        req: CreateReceiptReversalRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ReceiptReversalView> {
        self.rbac
            .identity
            .require(
                actor,
                &returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_CREATE,
                AccessTarget::Operation,
                executor,
            )
            .await?;

        req.validate()?;
        let reversal = build_create(req, actor.id())?;
        persist_created_receipt_reversal(&self.db, &self.rbac, reversal.clone(), actor.clone(), executor)
            .await?;

        self.receipt_reversal_result_read(
            &reversal.base.id,
            actor,
            &returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_CREATE,
            AccessTarget::Operation,
            executor,
        )
        .await
    }
}

// 在创建事务内写入冲正单、绑定发布定义并登记单据。
//
// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。无发布定义、人员重验失败或写入失败时返回错误。
async fn persist_created_receipt_reversal(
    db: &Database,
    rbac: &ReverseFlowPorts,
    reversal: ReceiptReversal,
    actor: AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    rbac.identity
        .require(
            &actor,
            &returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_CREATE,
            AccessTarget::Operation,
            executor,
        )
        .await?;

    let (organization_id, _) =
        load_receipt_reversal_context(db, &reversal.original_customer_receipt_id, executor).await?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::ReceiptReversal,
        business_object_id: reversal.base.id.clone(),
        business_object_version: reversal.base.version,
        context: BindingRevalidationContext::new(organization_id, actor.id().to_string()),
    };
    let document = new_registered_document(
        &reversal.base.id,
        DocumentType::ReceiptReversal,
        reversal.reversal_no.clone(),
    )
    .map_err(crate::Error::from)?;
    let audit = actor.clone().resource_log(
        "receipt_reversal.create",
        "receipt_reversal",
        reversal.base.id.clone(),
    )?;

    persist_bound_receipt_reversal_document(db, rbac, document, &bind_command, &actor, executor).await?;
    ReturnsService::persist_created_receipt_reversal(db, &reversal, executor).await?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

#[cfg(test)]
mod creation_authorization_tests {
    use erp_core::common::time::Instant;
    use persistence_core::testing::SnapshotExecutor;

    use super::super::super::testing::{actor, denied, process};

    #[tokio::test]
    async fn creation_authorizes_before_counterparty_source_or_definition_reads() {
        let process = process().await;
        let request=serde_json::from_value(serde_json::json!({
"refund_no":"RF-1","reversal_no":"RV-1","customer_id":"unread-customer","supplier_id":"unread-supplier",
"original_customer_receipt_id":"unread-receipt","original_supplier_payment_id":"unread-payment",
"reason_text":"reason","amount":"1","handled_by":"actor","reviewed_by":"reviewer","occurred_at":Instant::from_unix_secs(100)
})).unwrap();
        denied(
            process.create_receipt_reversal_stage(request, &actor(), &mut SnapshotExecutor::default()).await,
        );
    }
}
