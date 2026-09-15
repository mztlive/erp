//! 回款冲正命令的原回款组织读取与发布定义绑定。

use application_core::AuditActor;
use erp_core::ids::{CustomerAccountId, CustomerReceiptId};
use erp_finance::repository::ReceivableExt;
use erp_identity::SharedRbacService;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::document_registry::BusinessDocument;
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use mongodb::Database;
use persistence_core::NoTransaction;

use super::super::adapter::{receipt_reversal_object_readable, receipt_reversal_responsible_org_id};
use crate::{Error, Result};

/// 查询原回款往来主体作为责任组织，并带回可选客户。
///
/// # 错误
/// 原回款不存在或往来主体为空时返回错误。
pub(super) async fn load_receipt_reversal_context(
    db: &Database,
    original_receipt_id: &CustomerReceiptId,
) -> Result<(String, Option<CustomerAccountId>)> {
    let receipt = db
        .customer_receipts()
        .find_by_id(original_receipt_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("原回款不存在".to_string()))?;
    let organization_id = receipt_reversal_responsible_org_id(receipt.counterparty_party_id.as_ref())?;
    Ok((organization_id, receipt.customer_id))
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
pub(super) async fn persist_bound_receipt_reversal_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ = receipt_reversal_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        session,
    )
    .await?;
    let binding = binding.ok_or_else(|| Error::Internal("回款冲正单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, session).await?;
    Ok(binding)
}
