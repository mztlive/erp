use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::http::HeaderValue;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use erp_data_import::data_transfer::{TransferKind, TransferRequest};
use erp_identity::PortalActor;
use erp_support::BackgroundJobListParams;
use serde_json::json;

use crate::http::errors::Error;
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::data_transfer::process;
use crate::wiring::errors::Error as ProcessError;

/// 登记已认证用户在当前业务权限内发起的固定类型导入导出任务，返回持久化任务 ID 供轮询；越权、输入非法或登记失败时拒绝。
pub async fn submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<TransferRequest>,
) -> Result<Response, Error> {
    Ok(private(
        ApiResponse::ok_with_data(process(state.application()).submit(request, &actor).await?)
            .into_response(),
    ))
}
/// 根据任务 ID 查询该操作人自己提交的任务进度；非本人或任务不存在时拒绝。
pub async fn status(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Response, Error> {
    Ok(private(
        ApiResponse::ok_with_data(process(state.application()).status(&id, &actor).await?).into_response(),
    ))
}
/// 为现有任务读取导出分类或导入续办信息，包括受控业务引用、续办状态与取消能力；源文件不可恢复或业务授权失效时拒绝，导入只向本人交付。
pub async fn context(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Response, Error> {
    Ok(private(
        ApiResponse::ok_with_data(process(state.application()).context(&id, &actor).await?).into_response(),
    ))
}

/// 出现批量提交结果未知时，凭原任务查回执并原子更新未确认项；不接受替换输入、命令号或重新提交。任务过期、无待确认项或回执核对失败时拒绝。
pub async fn recover(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Response, Error> {
    let result = process(state.application()).recover(&id, &actor).await?;
    Ok(private(ApiResponse::ok_with_data(json!({"file_name":result.file_name,"content_type":result.content_type,"content_base64":result.content_base64,"metadata":result.metadata})).into_response()))
}

/// 下载已完成且未过期的任务结果，返回文件名、类型、Base64 内容及元数据，不公开内部授权依据。导出可由当前仍有业务权限的非提交人读取；撤权或范围变化时拒绝。
pub async fn result(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Response, Error> {
    let result = process(state.application()).result(&id, &actor).await?;
    Ok(private(ApiResponse::ok_with_data(json!({"file_name":result.file_name,"content_type":result.content_type,"content_base64":result.content_base64,"metadata":result.metadata})).into_response()))
}
/// 仅受理供应商门户批量任务，从认证会话写入账号与绑定版本，供异步处理时重验；不接受后台任务类型。
pub async fn portal_submit(
    State(state): State<HttpState>,
    Extension(actor): Extension<PortalActor>,
    Json(mut request): Json<TransferRequest>,
) -> Result<Response, Error> {
    if request.kind != TransferKind::PortalBatch {
        return Err(ProcessError::Forbidden("门户仅支持本供应商批量任务".into()).into());
    }
    request.input = json!({"request":request.input,"session":{"account":actor.account,"account_version":actor.account_version,"binding_version":actor.binding_version}});
    submit(State(state), Extension(actor.audit_actor()), Json(request)).await
}
/// 供应商门户轮询时，用认证身份查本人任务进度；不交付其他人的任务。
pub async fn portal_status(
    State(state): State<HttpState>,
    Extension(actor): Extension<PortalActor>,
    Path(id): Path<String>,
) -> Result<Response, Error> {
    status(State(state), Extension(actor.audit_actor()), Path(id)).await
}
/// 供应商续办批量任务时，用原任务 ID 读取当前供应商范围内的上下文；身份、归属或权限失效时拒绝。
pub async fn portal_context(
    State(state): State<HttpState>,
    Extension(actor): Extension<PortalActor>,
    Path(id): Path<String>,
) -> Result<Response, Error> {
    context(State(state), Extension(actor.audit_actor()), Path(id)).await
}

/// 门户批量办理结果未知时，只核对原任务回执，不发起新的业务提交。身份、归属、有效期或回执核对不符时拒绝。
pub async fn portal_recover(
    State(state): State<HttpState>,
    Extension(actor): Extension<PortalActor>,
    Path(id): Path<String>,
) -> Result<Response, Error> {
    recover(State(state), Extension(actor.audit_actor()), Path(id)).await
}
/// 为供应商下载本人批量任务的未过期结果，交付前重验当前门户身份及供应商归属。
pub async fn portal_result(
    State(state): State<HttpState>,
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
/// 本人导入导出记录。
/// 非法分页或读取失败时拒绝。
pub async fn list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(mut query): Query<BackgroundJobListParams>,
) -> Result<Response, Error> {
    query.domain_job_type = Some("DATA_TRANSFER".into());
    Ok(private(
        ApiResponse::ok_with_data(
            state.application().bulk_job_service().own_background_job_list(&query, &actor).await?,
        )
        .into_response(),
    ))
}
/// 门户本人文件任务分页。
/// 仅本人提交记录。
/// 认证或分页无效时拒绝。
pub async fn portal_list(
    State(state): State<HttpState>,
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
