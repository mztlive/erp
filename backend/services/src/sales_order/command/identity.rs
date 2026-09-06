use mongodb::ClientSession;

use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use application_core::AuditActor;
use database::DocumentRegistryExt;
use entities::document_registry::BusinessDocument;
use entities::sales_order::SalesOrder;
use sha2::{Digest, Sha256};

use super::super::adapter::{sales_order_object_readable, sales_order_responsible_org_id};
use super::super::dto::SubmitSalesOrderRequest;

/// 为销售提交幂等命令生成不泄露原始幂等键的稳定收据 ID。
pub(super) fn sales_submission_audit_id(
    actor_id: &str,
    sales_order_id: &str,
    idempotency_key: &str,
) -> String {
    format!(
        "sales-order-submit-{}",
        hex::encode(Sha256::digest(
            format!("{actor_id}|{sales_order_id}|{idempotency_key}").as_bytes()
        ))
    )
}

/// 锁定同一幂等键可重放的完整请求身份。
pub(super) fn sales_submission_fingerprint(
    actor_id: &str,
    sales_order_id: &str,
    request: &SubmitSalesOrderRequest,
) -> Result<String> {
    let payload = serde_json::to_vec(&(actor_id, sales_order_id, request))
        .map_err(|error| Error::Internal(format!("销售提交命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(payload)))
}

/// 按业务性质构造创建时绑定命令。
///
/// `GoodsService` 绑定 `SalesOrder`，`Voucher` 绑定 `VoucherSalesOrder`。
///
/// # 错误
/// 责任组织为空时返回校验错误。
pub(super) fn sales_create_bind_command(
    order: &SalesOrder,
    actor: &AuditActor,
) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: entities::approval_integration::document_type_of_sales_business(order.business_type),
        business_object_id: order.base.id.clone(),
        business_object_version: order.base.version,
        context: BindingRevalidationContext {
            organization_id: sales_order_responsible_org_id(order)?,
            creator_id: actor.id().to_string(),
        },
    })
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误，调用方必须回滚。
pub(super) async fn persist_bound_sales_document(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    document: &mut BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<entities::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ = sales_order_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("销售单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding.clone())?;
    db.business_documents().create(document, session).await?;
    Ok(binding)
}

/// 为销售建单命令生成不泄露原始幂等键的稳定收据 ID。
pub(super) fn sales_order_create_audit_id(actor_id: &str, idempotency_key: &str) -> String {
    let mut digest = Sha256::new();
    for part in [actor_id, idempotency_key.trim()] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("sales-order-create-{}", hex::encode(digest.finalize()))
}

/// 锁定销售建单命令的完整载荷与鉴权操作者。
pub(super) fn sales_order_create_fingerprint<T: serde::Serialize>(
    actor_id: &str,
    request: &T,
) -> Result<String> {
    let payload = serde_json::to_vec(&(actor_id, request))
        .map_err(|error| Error::Internal(format!("销售建单命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(payload)))
}
