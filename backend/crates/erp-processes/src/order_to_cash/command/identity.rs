use application_core::AuditActor;
use erp_identity::SharedRbacService;
use erp_sales::entity::sales_order::SalesOrder;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::document_registry::BusinessDocument;
use erp_workflow::ports::OrderTaskSource;
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use mongodb::ClientSession;

use super::super::adapter::{sales_order_object_readable, sales_order_responsible_org_id};
use crate::{Error, Result};

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
        document_type: crate::order_to_cash::document_type_of_sales_business(order.business_type),
        business_object_id: order.base.id.clone(),
        business_object_version: order.base.version,
        context: BindingRevalidationContext {
            order_source: Some(OrderTaskSource::Sales(order.base.id.clone())),
            customer_id: Some(order.customer_id.to_string()),
            business_org_unit_id: Some(order.business_org_unit_id.clone()),
            scope_owner_user_id: Some(order.sales_owner_user_id.clone()),
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
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    document: &mut BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ =
        sales_order_object_readable(&bind_command.context.organization_id, &bind_command.context.creator_id)?;
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        session,
    )
    .await?;
    let binding = binding.ok_or_else(|| Error::Internal("销售单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding.clone())?;
    db.business_documents().create(document, session).await?;
    Ok(binding)
}
