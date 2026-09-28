//! 批量供给协议入口；权限与对应单条动作保持一致。
use application_core::AuditActor;
use axum::extract::State;
use axum::{Extension, Json};
use erp_processes::adapters::scoped_offering_process;
use erp_processes::supply_governance::offering::batch::{BatchRequest, BatchResult, Targeted};
use erp_supply::dto::supplier_offering::{
    CreateSupplierOfferingRequest, ReviseSupplierOfferingRequest, UpdateSupplierOfferingAvailabilityRequest,
};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "新增供应商供给",
    resource = "supplier_offering",
    action = "create"
)]
/// 批量校验或新增供给。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前主体
/// * `request` - 有界批量新增参数
///
/// # 返回
/// 返回逐行结果。
///
/// # 错误
/// 无权执行或批容器非法时返回错误。
pub async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<BatchRequest<CreateSupplierOfferingRequest>>,
) -> Result<BatchResult> {
    Ok(ApiResponse::ok_with_data(
        scoped_offering_process(state.db(), state.rbac()).batch(request, &actor).await?,
    ))
}

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "保存供应商供给条款",
    resource = "supplier_offering",
    action = "update"
)]
/// 批量校验或追加商业条款修订。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前主体
/// * `request` - 目标供给与修订参数
///
/// # 返回
/// 返回逐行结果。
///
/// # 错误
/// 无权执行或批容器非法时返回错误。
pub async fn revise(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<BatchRequest<Targeted<ReviseSupplierOfferingRequest>>>,
) -> Result<BatchResult> {
    Ok(ApiResponse::ok_with_data(
        scoped_offering_process(state.db(), state.rbac()).batch(request, &actor).await?,
    ))
}

#[permission_macros::permission(
    group = "供应商供给",
    group_desc = "维护公司 SKU 的供应商供给、价格条款和可供状态",
    desc = "更新供应商可供状态",
    resource = "supplier_offering_availability",
    action = "update"
)]
/// 批量校验或更新可供状态与数量。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 当前主体
/// * `request` - 目标供给与可供参数
///
/// # 返回
/// 返回逐行结果。
///
/// # 错误
/// 无权执行或批容器非法时返回错误。
pub async fn availability(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<BatchRequest<Targeted<UpdateSupplierOfferingAvailabilityRequest>>>,
) -> Result<BatchResult> {
    Ok(ApiResponse::ok_with_data(
        scoped_offering_process(state.db(), state.rbac()).batch(request, &actor).await?,
    ))
}
