//! 个人抄送收件箱；有效内部账号凭本人精确收件记录读取。
use application_core::AuditActor;
use axum::extract::{Path, Query, State};
use axum::http::HeaderValue;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use erp_processes::approval_materials::ApprovalMaterialTarget;
use erp_read_models::sales_center::approval_materials::copy_detail;
use erp_workflow::repository::approval_copy::CopyListQuery;
use erp_workflow::service::approval::copies::CopyReadRequest;
use serde::Deserialize;

use crate::app_state::AppState;
use crate::core::errors::Error;
use crate::core::response::ApiResponse;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryQuery {
    after: Option<u32>,
}

/// 返回本人收件箱。
/// # 参数
/// 应用状态、认证账号和筛选。
/// # 返回
/// 分页及未读统计。
/// # 错误
/// 内部账号失效或查询失败。
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<CopyListQuery>,
) -> Result<Response, Error> {
    let page = state.approval_runtime_service().copies(&actor, query).await?;
    Ok(no_store(ApiResponse::ok_with_data(page).into_response()))
}
/// 返回冻结资料与审批历史。
/// # 参数
/// 当前身份、收件 ID 和历史游标。
/// # 返回
/// 只读详情。
/// # 错误
/// 越界或资料不匹配。
pub async fn detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Response, Error> {
    let result =
        copy_detail(&state.db(), &state.approval_runtime_service(), &actor, &id, query.after).await?;
    Ok(no_store(ApiResponse::ok_with_data(result).into_response()))
}
/// 标记明确选中的本人记录已读。
/// # 参数
/// 当前身份和记录集合。
/// # 返回
/// 实际更新条数。
/// # 错误
/// 越界或写入失败时整批拒绝。
pub async fn mark_read(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<CopyReadRequest>,
) -> Result<Response, Error> {
    let changed = state.approval_runtime_service().read_copies(&actor, request).await?;
    Ok(no_store(ApiResponse::ok_with_data(changed).into_response()))
}
/// 预览本次抄送冻结附件。
/// # 参数
/// 当前身份、收件和文件 ID。
/// # 返回
/// 安全内容响应。
/// # 错误
/// 越界、隔离、版本变化或内容校验失败。
pub async fn preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, file)): Path<(String, String)>,
) -> Result<Response, Error> {
    super::materials::content(
        &state,
        &actor,
        ApprovalMaterialTarget {
            instance_id: "",
            file_id: &file,
            source_revision_id: None,
            copy_receipt_id: Some(&id),
        },
        false,
    )
    .await
}
/// 下载本次抄送冻结附件。
/// # 参数
/// 当前身份、收件和文件 ID。
/// # 返回
/// 强制下载响应。
/// # 错误
/// 越界、隔离、版本变化或内容校验失败。
pub async fn download(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, file)): Path<(String, String)>,
) -> Result<Response, Error> {
    super::materials::content(
        &state,
        &actor,
        ApprovalMaterialTarget {
            instance_id: "",
            file_id: &file,
            source_revision_id: None,
            copy_receipt_id: Some(&id),
        },
        true,
    )
    .await
}
fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyCursorQuery {
    after: Option<String>,
}
#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "查看实例抄送记录",
    resource = "approval_instance",
    action = "read"
)]
/// 返回已授权实例的抄送历史。
/// # 参数
/// 当前身份、实例 ID 和游标。
/// # 返回
/// 抄送记录页。
/// # 错误
/// 非实例读者拒绝。
pub async fn history(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<CopyCursorQuery>,
) -> Result<Response, Error> {
    let page = state.approval_runtime_service().copy_history(&actor, &id, query.after.as_deref()).await?;
    Ok(no_store(ApiResponse::ok_with_data(page).into_response()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_copy_responses_are_private_and_never_cached() {
        let response = no_store(ApiResponse::ok_with_data(0_u64).into_response());
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "private, no-store");
    }

    #[test]
    fn detail_query_cannot_override_recipient_identity() {
        let valid: HistoryQuery = serde_json::from_str(r#"{"after":2}"#).unwrap();
        assert_eq!(valid.after, Some(2));
        assert!(serde_json::from_str::<HistoryQuery>(r#"{"recipient_user_id":"other"}"#).is_err());
        assert!(serde_json::from_str::<HistoryQuery>(r#"{"after":-1}"#).is_err());
    }
}
