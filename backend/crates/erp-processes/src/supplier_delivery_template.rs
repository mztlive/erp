//! 供应商配送模板维护与供给指定：供应商存在性、供给写访问与审计同事务提交。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::ids::{SupplierAccountId, SupplierDeliveryTemplateId};
use erp_identity::SharedRbacService;
use erp_supplier::SupplierExt;
use erp_supply::dto::delivery_template::{
    AssignDeliveryTemplateRequest, AssignDeliveryTemplateResult, DeleteDeliveryTemplateRequest,
    DeliveryTemplateView, SaveDeliveryTemplateRequest,
};
use erp_supply::service::delivery_template::DeliveryTemplateService;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::audit::{persist_logs, run_audited};
use crate::{Error, Result};

const TEMPLATE_RESOURCE: &str = "supplier_delivery_template";

/// 查询供应商全部配送模板。
///
/// # 参数
/// * `db` - 数据库
/// * `supplier_id` - 供应商
///
/// # 返回
/// 按名称排序的模板视图。
///
/// # 错误
/// 查询失败时返回错误。
pub async fn supplier_delivery_templates(
    db: Database,
    supplier_id: String,
) -> Result<Vec<DeliveryTemplateView>> {
    let templates = DeliveryTemplateService::new(db)
        .supplier_templates(&SupplierAccountId::new(supplier_id), &mut NoTransaction)
        .await?;
    Ok(templates.iter().map(DeliveryTemplateView::from_template).collect())
}

/// 为供应商新建配送模板。
///
/// # 参数
/// * `db` - 数据库
/// * `supplier_id` - 供应商
/// * `req` - 模板内容
/// * `actor` - 操作人
///
/// # 返回
/// 返回新模板视图。
///
/// # 错误
/// 供应商不存在、内容不合法、名称重复或写入失败时返回错误。
pub async fn create_delivery_template(
    db: Database,
    supplier_id: String,
    req: SaveDeliveryTemplateRequest,
    actor: AuditActor,
) -> Result<DeliveryTemplateView> {
    if db.supplier_accounts().find_by_id(supplier_id.trim(), &mut NoTransaction).await?.is_none() {
        return Err(Error::NotFound("供应商不存在".into()));
    }
    let content = req.into_content()?;
    let id = SupplierDeliveryTemplateId::new(next_id());
    let audit = actor.clone().resource_log_with_message(
        "supplier_delivery_template.create",
        TEMPLATE_RESOURCE,
        id.to_string(),
        Some(format!("配送模板 {}", content.name.trim())),
    )?;
    let supplier = SupplierAccountId::new(supplier_id.trim());
    let template = run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            let service = DeliveryTemplateService::new(db.clone());
            Ok(service.create(id, supplier, content, actor.id(), executor).await?)
        })
    })
    .await?;
    Ok(DeliveryTemplateView::from_template(&template))
}

/// 整体保存配送模板。
///
/// # 参数
/// * `db` - 数据库
/// * `id` - 模板主键
/// * `req` - 模板内容与期望版本
/// * `actor` - 操作人
///
/// # 返回
/// 返回保存后的模板视图。
///
/// # 错误
/// 缺少期望版本、版本冲突、内容不合法、名称重复或写入失败时返回错误。
pub async fn update_delivery_template(
    db: Database,
    id: String,
    req: SaveDeliveryTemplateRequest,
    actor: AuditActor,
) -> Result<DeliveryTemplateView> {
    let expected_version =
        req.expected_version.ok_or_else(|| Error::ValidationError("修改模板须提供当前版本".into()))?;
    let content = req.into_content()?;
    let audit = actor.clone().resource_log_with_message(
        "supplier_delivery_template.update",
        TEMPLATE_RESOURCE,
        id.clone(),
        Some(format!("配送模板 {}", content.name.trim())),
    )?;
    let template = run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            let service = DeliveryTemplateService::new(db.clone());
            Ok(service.update(&id, expected_version, content, actor.id(), executor).await?)
        })
    })
    .await?;
    Ok(DeliveryTemplateView::from_template(&template))
}

/// 删除未被供给使用的配送模板。
///
/// # 参数
/// * `db` - 数据库
/// * `id` - 模板主键
/// * `req` - 期望版本
/// * `actor` - 操作人
///
/// # 返回
/// 删除成功返回 `Ok(())`。
///
/// # 错误
/// 版本冲突、仍被供给使用或写入失败时返回错误。
pub async fn delete_delivery_template(
    db: Database,
    id: String,
    req: DeleteDeliveryTemplateRequest,
    actor: AuditActor,
) -> Result<()> {
    let audit =
        actor.clone().resource_log("supplier_delivery_template.delete", TEMPLATE_RESOURCE, id.clone())?;
    run_audited(&db, audit, move |db, executor| {
        Box::pin(async move {
            DeliveryTemplateService::new(db.clone()).delete(&id, req.expected_version, executor).await?;
            Ok(())
        })
    })
    .await
}

/// 为同一供应商的供给批量指定模板或改回跟随默认模板。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 权限快照
/// * `req` - 供给与目标模板
/// * `actor` - 操作人
///
/// # 返回
/// 返回实际变化的供给数。
///
/// # 错误
/// 任一供给不可见或不可维护、模板不属于供给的供应商或写入失败时整体拒绝。
pub async fn assign_offering_delivery_template(
    db: Database,
    rbac: SharedRbacService,
    req: AssignDeliveryTemplateRequest,
    actor: AuditActor,
) -> Result<AssignDeliveryTemplateResult> {
    let offering_ids = req.normalized_offering_ids()?;
    let mut logs = offering_ids
        .iter()
        .map(|id| {
            actor.clone().resource_log(
                "supplier_offering.delivery_template.assign",
                "supplier_offering",
                id.clone(),
            )
        })
        .collect::<erp_audit::Result<Vec<_>>>()?;
    let first = logs.remove(0);
    let template_id = req.template_id;
    run_audited(&db, first, move |db, executor| {
        Box::pin(async move {
            let access = crate::adapters::offering_access(db.clone(), rbac);
            let mut offerings = Vec::with_capacity(offering_ids.len());
            for id in &offering_ids {
                offerings.push(access.require_offering(&actor, "update", id, executor).await?);
            }
            let updated = DeliveryTemplateService::new(db.clone())
                .assign(offerings, template_id.as_deref(), actor.id(), executor)
                .await?;
            persist_logs(db, &logs, executor).await?;
            Ok(AssignDeliveryTemplateResult { updated_count: updated })
        })
    })
    .await
}
