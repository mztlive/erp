//! 审批提交资料的受限 HTTP 入口；不开放普通业务或文件目录访问。

use application_core::AuditActor;
use axum::Extension;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::HeaderValue;
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::response::Response;
use erp_processes::approval_materials;
use erp_workflow::service::approval::execution::runtime_service::ApprovalMaterialsView;

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::handler::file_asset::verify_content;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "读取本次提交的审批资料",
    resource = "approval_instance",
    action = "read"
)]
/// 返回授权实例的冻结资料与附件安全目录。
/// # 参数
/// 应用状态、认证账号和审批实例 ID。
/// # 返回
/// 不含当前草稿、存储键或内容指纹的提交资料。
/// # 错误
/// 无权或冻结资料缺失时拒绝。
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<ApprovalMaterialsView> {
    Ok(ApiResponse::ok_with_data(state.approval_runtime_service().materials(&actor, &id).await?))
}

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "预览本次提交的审批附件",
    resource = "approval_instance",
    action = "read"
)]
/// 预览实例允许清单中的同一版本图片或 PDF。
/// # 参数
/// 应用状态、认证账号、实例和附件 ID。
/// # 返回
/// 带禁止缓存和禁止类型嗅探响应头的文件内容。
/// # 错误
/// 未授权、资产变化、安全检查失败或类型不支持时拒绝。
pub async fn preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, file_id)): Path<(String, String)>,
) -> std::result::Result<Response, Error> {
    content(&state, &actor, &id, &file_id, false).await
}

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "下载本次提交的审批附件",
    resource = "approval_instance",
    action = "read"
)]
/// 下载允许清单中的同一版本文件，强制以附件响应避免执行活动内容。
/// # 参数
/// 应用状态、认证账号、实例和附件 ID。
/// # 返回
/// 禁止缓存的二进制附件响应。
/// # 错误
/// 未授权、资产变化、安全检查失败或存储读取失败时拒绝。
pub async fn download(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, file_id)): Path<(String, String)>,
) -> std::result::Result<Response, Error> {
    content(&state, &actor, &id, &file_id, true).await
}

/// 文件授权由组合层执行；对象存储读取在事务之外完成。
async fn content(
    state: &AppState,
    actor: &AuditActor,
    id: &str,
    file_id: &str,
    download: bool,
) -> std::result::Result<Response, Error> {
    let runtime = state.approval_runtime_service();
    let file =
        approval_materials::readable(&runtime, &state.file_asset_service(), actor, id, file_id).await?;
    if !download && !previewable(&file.content_type) {
        return Err(Error::Unprocessable("当前文件类型不支持在线预览，请下载查看".into()));
    }
    let bytes = state
        .storage()
        .read(&file.storage_object_key)
        .await
        .map_err(|_| Error::Internal("附件读取失败，请稍后重试".into()))?;
    verify_content(&file, &bytes, state.config_snapshot().app.secret.as_bytes())?;
    approval_materials::revalidate(&runtime, &state.file_asset_service(), actor, id, file_id).await?;
    let content_type = if download { "application/octet-stream" } else { &file.content_type };
    let mut response = Response::new(Body::from(bytes));
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(content_type).map_err(|_| Error::Unprocessable("附件类型无效".into()))?,
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static(if download { "attachment" } else { "inline" }),
    );
    Ok(response)
}

/// 在线预览仅允许不执行活动内容的图片及 PDF 类型。
fn previewable(content_type: &str) -> bool {
    matches!(content_type, "image/jpeg" | "image/png" | "image/webp" | "application/pdf")
}

#[cfg(test)]
mod tests {
    #[test]
    fn active_content_is_download_only() {
        assert!(super::previewable("application/pdf"));
        assert!(super::previewable("image/png"));
        assert!(!super::previewable("text/html"));
        assert!(!super::previewable("image/svg+xml"));
        assert!(!super::previewable("application/octet-stream"));
    }
}
