use application_core::AuditActor;
use erp_identity::SharedRbacService;
use erp_sales::entity::sales_order::SalesOrder;
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::document_registry::BusinessDocument;
use erp_workflow::ports::OrderTaskSource;
use erp_workflow::service::approval::binding::{BindPublishedDefinitionCommand, attach_published_binding};
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use persistence_core::Executor;

use super::super::adapter::{sales_order_object_readable, sales_order_responsible_org_id};
use crate::{Error, Result};

/// 按业务性质构造创建时绑定命令。
///
/// `GoodsService` 绑定 `SalesOrder`，`Voucher` 绑定 `VoucherSalesOrder`。
///
/// # 参数
/// * `order` - 待绑定销售单。
/// * `actor` - 创建人。
///
/// # 返回
/// 返回按业务性质分派的创建绑定命令。
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
        context: BindingRevalidationContext::new(
            sales_order_responsible_org_id(order)?,
            actor.id().to_string(),
        )
        .with_order_source(Some(OrderTaskSource::Sales(order.base.id.clone())))
        .with_customer_id(Some(order.customer_id.to_string()))
        .with_business_org_unit_id(Some(order.business_org_unit_id.clone()))
        .with_scope_owner_user_id(Some(order.sales_owner_user_id.clone())),
    })
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 参数
/// * `db` - 业务数据库。
/// * `rbac` - 授权源。
/// * `object_read` - 审批对象读取端口。
/// * `document` - 待写入的单据注册行。
/// * `bind_command` - 创建时绑定命令。
/// * `actor` - 创建人。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 返回已写入注册行的发布定义绑定。
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
    executor: &mut dyn Executor,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ =
        sales_order_object_readable(&bind_command.context.organization_id, &bind_command.context.creator_id)?;
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        executor,
    )
    .await?;
    let binding = binding.ok_or_else(|| Error::Internal("销售单必须绑定已发布定义".to_string()))?;
    attach_published_binding(document, binding.clone())?;
    db.business_documents().create(document, executor).await?;
    Ok(binding)
}
