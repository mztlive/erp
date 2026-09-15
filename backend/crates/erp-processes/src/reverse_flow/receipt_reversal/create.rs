//! 回款冲正草稿创建的注册、绑定和审计根事务。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_identity::SharedRbacService;
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
use persistence_core::Transactional;
use validator::Validate;

use super::super::ReturnsProcess;
use super::context::{load_receipt_reversal_context, persist_bound_receipt_reversal_document};
use crate::Result;

impl ReturnsProcess {
    /// 登记回款冲正草稿，并在同一事务绑定已发布审批定义。
    ///
    /// 冲正单号全局唯一（唯一索引）构成幂等去重。经办人与复核人必须不同。
    /// 绑定失败必须回滚业务实体，不得把绑定推迟到提交。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建冲正单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 冲正单号重复或流程未配置
    /// * `NotFound` - 原回款不存在
    pub async fn create_receipt_reversal(
        &self,
        req: CreateReceiptReversalRequest,
        actor: &AuditActor,
    ) -> Result<ReceiptReversalView> {
        req.validate()?;
        let reversal = build_create(req, actor.id())?;
        persist_created_receipt_reversal(
            &self.db,
            &self.rbac,
            std::sync::Arc::clone(&self.object_read),
            reversal.clone(),
            actor.clone(),
        )
        .await?;
        self.reads().receipt_reversal_detail(&reversal.base.id).await.map_err(crate::Error::from)
    }
}

/// 在创建事务内写入冲正单、绑定发布定义并登记单据。
///
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误。
async fn persist_created_receipt_reversal(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    reversal: ReceiptReversal,
    actor: AuditActor,
) -> Result<()> {
    let (organization_id, _) =
        load_receipt_reversal_context(db, &reversal.original_customer_receipt_id).await?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::ReceiptReversal,
        business_object_id: reversal.base.id.clone(),
        business_object_version: reversal.base.version,
        context: BindingRevalidationContext {
            order_source: None,
            customer_id: None,
            business_org_unit_id: None,
            scope_owner_user_id: None,
            organization_id,
            creator_id: actor.id().to_string(),
        },
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
    let db = db.clone();
    let rbac = rbac.clone();
    let object_read = object_read.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_receipt_reversal_document(
                    &db,
                    &rbac,
                    object_read.as_ref(),
                    document,
                    &bind_command,
                    &actor,
                    session,
                )
                .await?;
                ReturnsService::persist_created_receipt_reversal(&db, &reversal, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::Error>(())
            })
        })
        .await
}
