//! 回款冲正命令的原回款组织读取与发布定义绑定。

use application_core::AuditActor;
use erp_core::ids::{CustomerAccountId, CustomerReceiptId};
use erp_finance::repository::ReceivableExt;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::document_registry::BusinessDocument;
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use mongodb::Database;
use persistence_core::Executor;

use super::super::adapter::{receipt_reversal_object_readable, receipt_reversal_responsible_org_id};
use crate::{Error, Result, ReverseFlowPorts};

// 查询原回款往来主体作为责任组织，并带回可选客户。
//
// `original_receipt_id` 为原客户回款主键。返回责任组织 ID，以及原回款上的可选客户。原回款不存在或往来主体为空时返回错误。
pub(super) async fn load_receipt_reversal_context(
    db: &Database,
    original_receipt_id: &CustomerReceiptId,
    executor: &mut dyn Executor,
) -> Result<(String, Option<CustomerAccountId>)> {
    let receipt = db
        .customer_receipts()
        .find_by_id(original_receipt_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原回款不存在".to_string()))?;
    let organization_id = receipt_reversal_responsible_org_id(receipt.counterparty_party_id.as_ref())?;
    Ok((organization_id, receipt.customer_id))
}

// 查询发布定义、写入绑定并持久化注册行。
//
// `rbac` 为绑定重验使用的共享 RBAC；`document` 为待登记单据；成功时写入绑定后持久化。消耗该值；`bind_command` 为发布定义绑定命令；沿用原事务的执行器。
//
// 返回已附加到注册行的发布定义绑定。对象读取校验失败、无发布定义、绑定失败或注册写入失败时返回错误。
pub(super) async fn persist_bound_receipt_reversal_document(
    db: &Database,
    rbac: &ReverseFlowPorts,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ = receipt_reversal_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding = rbac.runtime.bind_document(db, bind_command, actor, executor).await?;
    let binding = binding.ok_or_else(|| Error::Internal("回款冲正单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, executor).await?;
    Ok(binding)
}
