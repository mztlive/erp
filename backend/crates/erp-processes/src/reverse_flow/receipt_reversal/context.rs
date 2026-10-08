//! 回款冲正命令的原回款组织读取与发布定义绑定。

use application_core::AuditActor;
use erp_core::ids::{CustomerAccountId, CustomerReceiptId};
use erp_finance::repository::ReceivableExt;
use erp_identity::SharedRbacService;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::document_registry::BusinessDocument;
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::super::adapter::{receipt_reversal_object_readable, receipt_reversal_responsible_org_id};
use crate::{Error, Result};

/// 查询原回款往来主体作为责任组织，并带回可选客户。
///
/// # 参数
/// * `db` - 数据库。
/// * `original_receipt_id` - 原客户回款主键。
///
/// # 返回
/// 返回责任组织 ID，以及原回款上的可选客户。
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
/// # 参数
/// * `db` - 数据库。
/// * `rbac` - 绑定重验使用的共享 RBAC。
/// * `object_read` - 对象读取授权端口。
/// * `document` - 待登记单据；成功时写入绑定后持久化。消耗该值。
/// * `bind_command` - 发布定义绑定命令。
/// * `actor` - 审计操作人。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 返回已附加到注册行的发布定义绑定。
///
/// # 错误
/// 对象读取校验失败、无发布定义、绑定失败或注册写入失败时返回错误。
pub(super) async fn persist_bound_receipt_reversal_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
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
        executor,
    )
    .await?;
    let binding = binding.ok_or_else(|| Error::Internal("回款冲正单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, executor).await?;
    Ok(binding)
}
