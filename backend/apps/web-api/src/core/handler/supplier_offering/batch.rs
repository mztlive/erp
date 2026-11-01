//! 批量供给协议入口；权限与对应单条动作保持一致。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::State;
use erp_supplier_catalog::supply_governance::offering::batch::{BatchRequest, BatchResult, Targeted};
use erp_supply::dto::supplier_offering::{
    ReviseSupplierOfferingRequest, UpdateSupplierOfferingAvailabilityRequest,
};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;
use crate::wiring::scoped_offering_process;

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
