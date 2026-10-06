//! 门户命令只进入跨域过程，响应再次采用外部字段允许列表。
use axum::extract::{Path, State};
use axum::{Extension, Json};
use erp_identity::PortalActor;
use erp_processes::supplier_portal::{
    CooperationSave, NewProductSave, PortalBatchInput, PortalBatchResult, PortalTransition,
};
use erp_read_models::supplier_portal::SupplierPortalReadService;
use erp_supply::portal::{PortalAvailabilityInput, PortalQuoteInput};
use serde_json::{Value, to_value};

use super::process;
use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::response::ApiResponse;

/// 直接维护自己的人工供给可供事实。
/// # 参数
/// 当前门户身份、精确供给与原版本命令。
/// # 返回
/// 本次可供结果及版本。
/// # 错误
/// 只读、越界、API来源、版本失效或数量非法时拒绝。
pub async fn availability(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Path(id): Path<String>,
    Json(input): Json<PortalAvailabilityInput>,
) -> Result<Value> {
    let result = process(&state).availability_update(&id, input, &actor).await?;
    Ok(ApiResponse::ok_with_data(to_value(result).map_err(|error| Error::Internal(error.to_string()))?))
}

/// 保存已有公司SKU报价或自己供给条款草稿。
/// # 参数
/// 当前身份与原始报价内容。
/// # 返回
/// 本供应商申请的外部投影。
/// # 错误
/// 定向开放、条款、版本或身份不符时拒绝。
pub async fn save_application(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Json(input): Json<PortalQuoteInput>,
) -> Result<Value> {
    let result = process(&state).application_save(None, input, &actor).await?;
    external_application(&state, &actor, &result.base.id).await
}

/// 按服务器认定的申请领域保存原稿修改。
/// # 参数
/// 精确申请、原版本及对应类型的原始内容。
/// # 返回
/// 保存后的本供应商申请。
/// # 错误
/// 归属、状态、版本或输入不符时拒绝。
pub async fn update_application(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Path(id): Path<String>,
    Json(input): Json<Value>,
) -> Result<Value> {
    process(&state).application_update(&id, input, &actor).await?;
    external_application(&state, &actor, &id).await
}

/// 冻结当前原稿并建立采购确认任务。
/// # 参数
/// 精确申请、当前版本与保留的操作号。
/// # 返回
/// 实际提交状态与本供应商原稿历史。
/// # 错误
/// 身份、申请版本或负责人资格失效时拒绝。
pub async fn submit(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Path(id): Path<String>,
    Json(input): Json<PortalTransition>,
) -> Result<Value> {
    process(&state).submit(&id, &input, &actor).await?;
    external_application(&state, &actor, &id).await
}

/// 撤回未完成的申请及当前专项任务。
/// # 参数
/// 精确申请及原版本命令。
/// # 返回
/// 保留提交历史的撤回状态。
/// # 错误
/// 已完成、越界或并发修改时拒绝。
pub async fn withdraw(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Path(id): Path<String>,
    Json(input): Json<PortalTransition>,
) -> Result<Value> {
    process(&state).withdraw(&id, &input, &actor).await?;
    external_application(&state, &actor, &id).await
}

/// 保存供应商新品原稿，不创建公司商品或SKU。
/// # 参数
/// 一份商品及完整规格、原始字典与供应条款。
/// # 返回
/// 新品草稿的外部投影。
/// # 错误
/// 输入、材料归属或身份无效时拒绝。
pub async fn save_new_product(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Json(input): Json<NewProductSave>,
) -> Result<Value> {
    let result = process(&state).new_product_save(None, input, &actor).await?;
    external_application(&state, &actor, &result.base.id).await
}

/// 保存供应商合作条款申请。
/// # 参数
/// 当前供应商档案版本与拟变更条款。
/// # 返回
/// 尚未影响正式档案和采购快照的申请。
/// # 错误
/// 档案版本、输入或身份无效时拒绝。
pub async fn save_cooperation(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Json(input): Json<CooperationSave>,
) -> Result<Value> {
    let result = process(&state).cooperation_save(None, input, &actor).await?;
    external_application(&state, &actor, &result.base.id).await
}

/// 整批预检后按独立业务单元提交或恢复原结果。
/// # 参数
/// 保留行号、原内容和原操作号的批量命令。
/// # 返回
/// 每行校验、成功、失败或待核实结果。
/// # 错误
/// 身份、批次数量或批量结构不符时拒绝。
pub async fn batch(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Json(input): Json<PortalBatchInput>,
) -> Result<PortalBatchResult> {
    Ok(ApiResponse::ok_with_data(process(&state).batch(&actor, input).await?))
}

async fn external_application(state: &AppState, actor: &PortalActor, id: &str) -> Result<Value> {
    let result = SupplierPortalReadService::new(state.db()).application(actor, id).await?;
    Ok(ApiResponse::ok_with_data(to_value(result).map_err(|error| Error::Internal(error.to_string()))?))
}
