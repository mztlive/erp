//! 域 D25 `supplier_api` 的 HTTP handler。
//!
//! Handler 只做协议适配；连接治理动作全部进入固定强命令，列表/详情只返回
//! Service 按当前操作人投影的权威动作与阻塞原因。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_read_models::supplier_center::supplier_api::dto::SupplierConnectionJobView;
use erp_supply::dto::supplier_api::{
    CreateSupplierApiConnectionRequest, PageView, SupplierApiConnectionDetailView,
    SupplierApiConnectionListItemView, SupplierApiConnectionListParams, SupplierApiConnectionView,
    SupplierConnectionCommand, SupplierConnectionCommandResult,
};
use erp_supply::entity::supplier_api::SupplierConnectionAction;

use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
pub mod adoption_batches;
pub mod adoptions;
pub mod catalog;
pub mod connector_types;
pub mod dangaoshushu;
pub mod jd_vop;
pub mod references;

/// 查询供应商 API 连接列表。
pub async fn supplier_api_connection_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<SupplierApiConnectionListParams>,
) -> Result<PageView<SupplierApiConnectionListItemView>> {
    let page =
        state.application().supplier_api_read_service().connection_list_for_actor(&params, &actor).await?;
    Ok(ApiResponse::ok_with_data(page))
}

/// 查询供应商 API 连接详情。
pub async fn supplier_api_connection_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SupplierApiConnectionDetailView> {
    let view =
        state.application().supplier_api_read_service().connection_detail_for_actor(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 创建停用状态的供应商 API 连接身份。
pub async fn supplier_api_connection_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateSupplierApiConnectionRequest>,
) -> Result<SupplierApiConnectionView> {
    let view = state.application().supplier_api_governance_process().create_connection(req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 执行固定动作注册表中的连接治理命令。
pub async fn supplier_api_connection_command(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(command): Json<SupplierConnectionCommand>,
) -> Result<SupplierConnectionCommandResult> {
    let source_media = if command.action == SupplierConnectionAction::StartCatalogSync {
        state.application().supplier_source_media_preparer()?
    } else {
        None
    };
    let result = state
        .application()
        .supplier_api_governance_process()
        .execute_connection_command(&id, command, &actor)
        .await?;
    if let Some(job_id) = result.job_id.clone() {
        let application = state.application().clone();
        let actor = actor.clone();
        let connection_id = id.clone();
        state.tasks().spawn(async move {
            if let Err(error) = application
                .supplier_connection_execution_process()
                .with_source_media(source_media)
                .process_connection_job(&job_id, &actor)
                .await
            {
                tracing::error!(
                    connection_id,
                    job_id,
                    error = %error,
                    "supplier connection background job processor failed"
                );
            }
        })?;
    }
    Ok(ApiResponse::ok_with_data(result))
}

/// 查询健康检查或目录同步任务进度与终态。
pub async fn supplier_api_connection_job_detail(
    State(state): State<HttpState>,
    Path((id, job_id)): Path<(String, String)>,
) -> Result<SupplierConnectionJobView> {
    let result = state.application().supplier_api_read_service().connection_job(&id, &job_id).await?;
    Ok(ApiResponse::ok_with_data(result))
}
