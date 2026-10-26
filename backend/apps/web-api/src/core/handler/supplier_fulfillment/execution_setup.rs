use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::supplier_api_orders::{ExecutionSetupRequest, ExecutionSetupView};
use persistence_core::NoTransaction;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "供应商订单",
    group_desc = "供应商履约订单、动作与退款事实管理",
    desc = "读取自动执行配送资料",
    resource = "supplier_fulfillment_order",
    action = "detail"
)]
/// 读取真实接口执行阶段和可补充资料。
/// # 参数
/// 当前账号和路径订单通过同一订单数据范围核验。
/// # 返回
/// 后台真实执行资料。
/// # 错误
/// 越权、非自动订单或未配置协议时拒绝。
pub async fn supplier_order_execution_setup(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<ExecutionSetupView> {
    state
        .supplier_fulfillment_process()
        .require_scoped_order(&id, &actor, "detail", &mut NoTransaction)
        .await?;
    let process = state
        .supplier_api_order_process()
        .ok_or_else(|| erp_processes::Error::NotFound("此订单没有已配置的 API 自动执行入口".into()))?;
    Ok(ApiResponse::ok_with_data(process.execution_setup(&id).await?))
}

#[permission_macros::permission(
    group = "供应商订单",
    group_desc = "供应商履约订单、动作与退款事实管理",
    desc = "补充自动执行配送资料",
    resource = "supplier_fulfillment_order",
    action = "submit"
)]
/// 保存经核验的配送补充资料，后台负责后续执行。
/// # 参数
/// 当前账号须具备订单 submit 权限与对象写范围。
/// # 返回
/// 保存后的执行资料，不能解释为已下单成功。
/// # 错误
/// 版本冲突、无效坐标或不允许编辑时拒绝。
pub async fn save_supplier_order_execution_setup(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<ExecutionSetupRequest>,
) -> Result<ExecutionSetupView> {
    state
        .supplier_fulfillment_process()
        .require_scoped_order(&id, &actor, "submit", &mut NoTransaction)
        .await?;
    let process = state
        .supplier_api_order_process()
        .ok_or_else(|| erp_processes::Error::BusinessLogicError("供应商协议尚未配置".into()))?;
    Ok(ApiResponse::ok_with_data(
        process
            .save_execution_setup(&id, request, &actor, &state.supplier_fulfillment_service().access())
            .await?,
    ))
}
