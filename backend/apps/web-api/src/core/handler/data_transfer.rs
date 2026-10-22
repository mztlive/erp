use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::http::HeaderValue;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use erp_identity::PortalActor;
use erp_processes::Error as ProcessError;
use erp_processes::data_transfer::{TransferKind, TransferRequest};
use erp_support::BackgroundJobListParams;
use serde_json::json;

use crate::app_state::AppState;
use crate::core::data_transfer::process;
use crate::core::errors::Error;
use crate::core::extractor::{Json, Query};
use crate::core::response::ApiResponse;

/// 登记当前业务权限允许的私有任务。
/// # 参数
/// 当前认证人及固定类型任务请求。
/// # 返回
/// 持久化任务身份。
/// # 错误
/// 越权、非法输入或登记失败时拒绝。
pub async fn submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<TransferRequest>,
) -> Result<Response, Error> {
    Ok(private(ApiResponse::ok_with_data(process(&state).submit(request, &actor).await?).into_response()))
}
/// 查询本人任务状态。
/// # 参数
/// 当前认证人与任务 ID。
/// # 返回
/// 服务端真实任务进度。
/// # 错误
/// 非本人或任务不存在时拒绝。
pub async fn status(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Response, Error> {
    Ok(private(ApiResponse::ok_with_data(process(&state).status(&id, &actor).await?).into_response()))
}
/// 读取本人任务的未过期结果。
/// # 参数
/// 当前认证人与任务 ID。
/// # 返回
/// 文件名、类型及 Base64 字节，不返回内部授权依据。
/// # 错误
/// 撤权、范围变化、未完成或过期时拒绝。
pub async fn result(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Response, Error> {
    let result = process(&state).result(&id, &actor).await?;
    Ok(private(ApiResponse::ok_with_data(json!({"file_name":result.file_name,"content_type":result.content_type,"content_base64":result.content_base64,"metadata":result.metadata})).into_response()))
}
/// 绑定门户认证版本后登记批量任务。
/// # 参数
/// 服务端认证门户身份及批量请求。
/// # 返回
/// 本人任务。
/// # 错误
/// 任务类型、身份或业务校验失败时拒绝。
pub async fn portal_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Json(mut request): Json<TransferRequest>,
) -> Result<Response, Error> {
    if request.kind != TransferKind::PortalBatch {
        return Err(ProcessError::Forbidden("门户仅支持本供应商批量任务".into()).into());
    }
    request.input = json!({"request":request.input,"session":{"account":actor.account,"account_version":actor.account_version,"binding_version":actor.binding_version}});
    submit(State(state), Extension(actor.audit_actor()), Json(request)).await
}
/// 查询门户本人任务。
/// # 参数
/// 门户身份及任务 ID。
/// # 返回
/// 任务状态。
/// # 错误
/// 身份或归属无效时拒绝。
pub async fn portal_status(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Path(id): Path<String>,
) -> Result<Response, Error> {
    status(State(state), Extension(actor.audit_actor()), Path(id)).await
}
/// 读取门户本人任务结果。
/// # 参数
/// 门户身份及任务 ID。
/// # 返回
/// 经当前供应商归属重验的结果。
/// # 错误
/// 身份、归属或有效期无效时拒绝。
pub async fn portal_result(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Path(id): Path<String>,
) -> Result<Response, Error> {
    result(State(state), Extension(actor.audit_actor()), Path(id)).await
}
fn private(mut response: Response) -> Response {
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

/// 分页查询本人文件任务，不要求全局任务管理权限。
/// # 参数
/// 当前身份与分页条件；服务端固定任务类型并覆盖所有者。
/// # 返回
/// 本人导入导出记录。
/// # 错误
/// 非法分页或读取失败时拒绝。
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(mut query): Query<BackgroundJobListParams>,
) -> Result<Response, Error> {
    query.domain_job_type = Some("DATA_TRANSFER".into());
    Ok(private(
        ApiResponse::ok_with_data(state.bulk_job_service().background_job_list(&query, &actor, false).await?)
            .into_response(),
    ))
}
/// 门户本人文件任务分页。
/// # 参数
/// 当前门户身份与分页条件。
/// # 返回
/// 仅本人提交记录。
/// # 错误
/// 认证或分页无效时拒绝。
pub async fn portal_list(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    query: Query<BackgroundJobListParams>,
) -> Result<Response, Error> {
    list(State(state), Extension(actor.audit_actor()), query).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_responses_forbid_shared_or_browser_storage() {
        let response = private(ApiResponse::ok_with_data(json!({"id":"task"})).into_response());
        assert_eq!(response.headers()[CACHE_CONTROL], "private, no-store");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }
}
