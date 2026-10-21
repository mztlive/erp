//! 供应商配送模板与供给指定 HTTP Handler。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_supply::dto::delivery_template::{
    AssignDeliveryTemplateRequest, AssignDeliveryTemplateResult, DeleteDeliveryTemplateRequest,
    DeliveryTemplateView, SaveDeliveryTemplateRequest,
};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "查询供应商配送模板",
    resource = "supplier_offering",
    action = "list"
)]
/// 查询供应商全部配送模板。
///
/// # 参数
/// * `state` - 应用状态
/// * `supplier_id` - 供应商
///
/// # 返回
/// 返回按名称排序的模板。
pub async fn list(
    State(state): State<AppState>,
    Path(supplier_id): Path<String>,
) -> Result<Vec<DeliveryTemplateView>> {
    let templates =
        erp_processes::supplier_delivery_template::supplier_delivery_templates(state.db(), supplier_id)
            .await?;
    Ok(ApiResponse::ok_with_data(templates))
}

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "新建供应商配送模板",
    resource = "supplier_offering",
    action = "update"
)]
/// 为供应商新建配送模板。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 审计操作人
/// * `supplier_id` - 供应商
/// * `req` - 模板内容
///
/// # 返回
/// 返回新模板。
pub async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(supplier_id): Path<String>,
    Json(req): Json<SaveDeliveryTemplateRequest>,
) -> Result<DeliveryTemplateView> {
    let view = erp_processes::supplier_delivery_template::create_delivery_template(
        state.db(),
        supplier_id,
        req,
        actor,
    )
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "保存供应商配送模板",
    resource = "supplier_offering",
    action = "update"
)]
/// 整体保存配送模板。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 审计操作人
/// * `id` - 模板主键
/// * `req` - 模板内容与期望版本
///
/// # 返回
/// 返回保存后的模板。
pub async fn update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SaveDeliveryTemplateRequest>,
) -> Result<DeliveryTemplateView> {
    let view =
        erp_processes::supplier_delivery_template::update_delivery_template(state.db(), id, req, actor)
            .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "删除供应商配送模板",
    resource = "supplier_offering",
    action = "update"
)]
/// 删除未被供给使用的配送模板。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 审计操作人
/// * `id` - 模板主键
/// * `req` - 期望版本
///
/// # 返回
/// 返回统一成功信封。
pub async fn delete(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<DeleteDeliveryTemplateRequest>,
) -> Result<()> {
    erp_processes::supplier_delivery_template::delete_delivery_template(state.db(), id, req, actor).await?;
    Ok(ApiResponse::ok())
}

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "为供给指定配送模板",
    resource = "supplier_offering",
    action = "update"
)]
/// 为同一供应商的供给批量指定配送模板或改回跟随默认模板。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 审计操作人
/// * `req` - 供给与目标模板
///
/// # 返回
/// 返回实际变化的供给数。
pub async fn assign(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<AssignDeliveryTemplateRequest>,
) -> Result<AssignDeliveryTemplateResult> {
    let result = erp_processes::supplier_delivery_template::assign_offering_delivery_template(
        state.db(),
        state.rbac(),
        req,
        actor,
    )
    .await?;
    Ok(ApiResponse::ok_with_data(result))
}
