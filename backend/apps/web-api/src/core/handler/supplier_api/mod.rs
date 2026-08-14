//! 域 D25 `supplier_api` 的 HTTP handler。
//!
//! Handler 只做协议适配；连接治理动作全部进入固定强命令，列表/详情只返回
//! Service 按当前操作人投影的权威动作与阻塞原因。

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    supplier_api::{
        ConfirmBusinessCapabilityRequirementCommand, ConfirmBusinessCapabilityRequirementResult,
        CreateSupplierApiConnectionRequest, PageView, SupplierApiCapabilityListParams,
        SupplierApiCapabilityView, SupplierApiConnectionDetailView, SupplierApiConnectionListParams,
        SupplierApiConnectionView, SupplierApiService, SupplierConnectionCommand,
        SupplierConnectionCommandResult, SupplierConnectionJobView, UnavailableSupplierApiGateway,
        UpdateSupplierCapabilitiesCommand, UpdateSupplierCapabilitiesResult,
    },
};

use crate::{
    app_state::AppState,
    core::{errors::Result, response::ApiResponse},
};

fn service(state: &AppState) -> SupplierApiService {
    SupplierApiService::new(state.db(), Arc::new(UnavailableSupplierApiGateway)).with_rbac(state.rbac())
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查询供应商 API 连接列表",
    resource = "supplier_api_connection",
    action = "list"
)]
/// 查询供应商 API 连接列表。
pub async fn supplier_api_connection_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SupplierApiConnectionListParams>,
) -> Result<PageView<SupplierApiConnectionView>> {
    let page = service(&state).connection_list_for_actor(&params, &actor).await?;
    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查询供应商 API 连接详情",
    resource = "supplier_api_connection",
    action = "detail"
)]
/// 查询供应商 API 连接详情。
pub async fn supplier_api_connection_detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SupplierApiConnectionDetailView> {
    let view = service(&state).connection_detail_for_actor(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "创建供应商 API 连接",
    resource = "supplier_api_connection",
    action = "create"
)]
/// 创建停用状态的供应商 API 连接身份。
pub async fn supplier_api_connection_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSupplierApiConnectionRequest>,
) -> Result<SupplierApiConnectionView> {
    let view = service(&state).create_connection(req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "执行供应商 API 连接治理强命令",
    resource = "supplier_api_connection",
    action = "command"
)]
/// 执行固定动作注册表中的连接治理命令。
pub async fn supplier_api_connection_command(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<SupplierConnectionCommand>,
) -> Result<SupplierConnectionCommandResult> {
    let result = service(&state)
        .execute_connection_command(&id, command, &actor)
        .await?;
    if let Some(job_id) = result.job_id.clone() {
        let state = state.clone();
        let actor = actor.clone();
        let connection_id = id.clone();
        tokio::spawn(async move {
            if let Err(error) = service(&state).process_connection_job(&job_id, &actor).await {
                tracing::error!(
                    connection_id,
                    job_id,
                    error = %error,
                    "supplier connection background job processor failed"
                );
            }
        });
    }
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "确认供应商 API 业务能力需求",
    resource = "supplier_api_capability",
    action = "confirm_requirement"
)]
/// 追加采购业务能力确认事实。
pub async fn supplier_api_business_capability_confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<ConfirmBusinessCapabilityRequirementCommand>,
) -> Result<ConfirmBusinessCapabilityRequirementResult> {
    let result = service(&state)
        .confirm_business_capability_requirement(&id, command, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "配置供应商 API 能力",
    resource = "supplier_api_capability",
    action = "update"
)]
/// 使用连接版本与逐能力版本原子配置能力。
pub async fn supplier_api_capabilities_update(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<UpdateSupplierCapabilitiesCommand>,
) -> Result<UpdateSupplierCapabilitiesResult> {
    let result = service(&state).update_capabilities(&id, command, &actor).await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查询供应商 API 连接后台任务",
    resource = "supplier_api_connection",
    action = "detail"
)]
/// 查询健康检查或目录同步任务进度与终态。
pub async fn supplier_api_connection_job_detail(
    State(state): State<AppState>,
    Path((id, job_id)): Path<(String, String)>,
) -> Result<SupplierConnectionJobView> {
    let result = service(&state).connection_job(&id, &job_id).await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查询连接能力列表",
    resource = "supplier_api_capability",
    action = "list"
)]
/// 查询连接能力声明列表。
pub async fn supplier_api_capability_list(
    State(state): State<AppState>,
    Query(params): Query<SupplierApiCapabilityListParams>,
) -> Result<PageView<SupplierApiCapabilityView>> {
    let page = service(&state).capability_list(&params).await?;
    Ok(ApiResponse::ok_with_data(page))
}
