//! 供应商目录选品的冻结预览与逐项确认。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::supplier_catalog_adoption::{
    AdoptionBatchResult, AdoptionPreviewView, AdoptionTargetView,
};
use erp_supply::catalog_adoption::{ConfirmAdoptionRequest, PreviewAdoptionRequest};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "同步供应商商品目录",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 冻结本次采购明确勾选或完整筛选的来源规格。
/// # 参数
/// 路径为连接，载荷为选择模式，actor 为认证采购人员。
/// # 返回
/// 最多五百行的固定预览，可按每批五十行确认。
/// # 错误
/// 无权限、来源不存在、选择重复或超限时拒绝。
pub async fn preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<PreviewAdoptionRequest>,
) -> Result<AdoptionPreviewView> {
    Ok(ApiResponse::ok_with_data(
        state.supplier_catalog_adoption_process()?.preview(&id, request, &actor).await?,
    ))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "同步供应商商品目录",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 按明确映射及条款逐项建立正式公司商品、供给和来源关系。
/// # 参数
/// 路径为连接及预览；载荷为预览版本、原始操作号和确认行。
/// # 返回
/// 每行真实成功结果或失败原因，预览版本保持冻结。
/// # 错误
/// 越权、过期、异载荷或请求不属于冻结范围时整体拒绝。
pub async fn confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, preview)): Path<(String, String)>,
    Json(request): Json<ConfirmAdoptionRequest>,
) -> Result<AdoptionBatchResult> {
    Ok(ApiResponse::ok_with_data(
        state.supplier_catalog_adoption_process()?.confirm(&id, &preview, request, &actor).await?,
    ))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "同步供应商商品目录",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 获取明确匹配目标的 SKU 及公司商品并发版本。
/// # 参数
/// 路径为连接和目标 SKU，actor 为当前认证操作人。
/// # 返回
/// 已授权目标的版本、单位和规格身份。
/// # 错误
/// 无权限、越范围、不存在或已停用时拒绝。
pub async fn target(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, sku)): Path<(String, String)>,
) -> Result<AdoptionTargetView> {
    Ok(ApiResponse::ok_with_data(state.supplier_catalog_adoption_process()?.target(&id, &sku, &actor).await?))
}
