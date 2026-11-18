//! 供应商配送模板与供给指定 HTTP Handler。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_supply::dto::delivery_template::{
    AssignDeliveryTemplateRequest, AssignDeliveryTemplateResult, DeleteDeliveryTemplateRequest,
    DeliveryTemplateView, SaveDeliveryTemplateRequest,
};

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 查询供应商全部配送模板。
/// 返回按名称排序的模板。
pub async fn list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(supplier_id): Path<String>,
) -> Result<Vec<DeliveryTemplateView>> {
    let templates = erp_supplier_catalog::supplier_delivery_template::supplier_delivery_templates(
        state.application().db(),
        state.application().authorization(),
        supplier_id,
        actor,
    )
    .await?;
    Ok(ApiResponse::ok_with_data(templates))
}

/// 为供应商新建配送模板。
/// 返回新模板。
pub async fn create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(supplier_id): Path<String>,
    Json(req): Json<SaveDeliveryTemplateRequest>,
) -> Result<DeliveryTemplateView> {
    let view = erp_supplier_catalog::supplier_delivery_template::create_delivery_template(
        state.application().db(),
        state.application().authorization(),
        supplier_id,
        req,
        actor,
    )
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 整体保存配送模板。
/// `req` 为模板内容与期望版本。
/// 返回保存后的模板。
pub async fn update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SaveDeliveryTemplateRequest>,
) -> Result<DeliveryTemplateView> {
    let view = erp_supplier_catalog::supplier_delivery_template::update_delivery_template(
        state.application().db(),
        state.application().authorization(),
        id,
        req,
        actor,
    )
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 删除未被供给使用的配送模板。
pub async fn delete(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<DeleteDeliveryTemplateRequest>,
) -> Result<()> {
    erp_supplier_catalog::supplier_delivery_template::delete_delivery_template(
        state.application().db(),
        state.application().authorization(),
        id,
        req,
        actor,
    )
    .await?;
    Ok(ApiResponse::ok())
}

/// 为同一供应商的供给批量指定配送模板或改回跟随默认模板。
/// 返回实际变化的供给数。
pub async fn assign(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<AssignDeliveryTemplateRequest>,
) -> Result<AssignDeliveryTemplateResult> {
    let result = erp_supplier_catalog::supplier_delivery_template::assign_offering_delivery_template(
        state.application().db(),
        crate::wiring::offering_access(state.application().db(), state.application().authorization()),
        req,
        actor,
    )
    .await?;
    Ok(ApiResponse::ok_with_data(result))
}
