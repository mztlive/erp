//! 审批提交资料的受限 HTTP 入口；不开放普通业务或文件目录访问。

use application_core::AuditActor;
use axum::Extension;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::HeaderValue;
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::response::{IntoResponse, Response};
use erp_processes::approval_materials::{self, ApprovalMaterialTarget};
use erp_read_models::sales_center::approval_materials::{ApprovalDocumentMaterials, materials};
use erp_read_models::workbench::{InvoiceApprovalReview, invoice_approval_review};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::core::errors::Error;
use crate::core::handler::file_asset::verify_content;
use crate::core::response::ApiResponse;

/// 当前销售来源资料必须携带页面读取的精确版本，缺省仍只读取提交附件。
#[derive(Default, Deserialize)]
pub struct MaterialQuery {
    source_revision_id: Option<String>,
}

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "读取开票审批当前核对依据",
    resource = "approval_instance",
    action = "read"
)]
/// 返回实例授权内唯一应收的当前票款核对依据。
/// # 参数
/// 应用状态、认证账号和开票审批实例 ID。
/// # 返回
/// 当前额度、回款与销售依据；不改写提交快照。
/// # 错误
/// 未授权、非开票审批、申请版本或来源不匹配时拒绝。
pub async fn invoice_review(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> std::result::Result<Response, Error> {
    let review: InvoiceApprovalReview =
        invoice_approval_review(&state.db(), &state.approval_runtime_service(), &actor, &id).await?;
    let mut response = ApiResponse::ok_with_data(review).into_response();
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(response)
}

#[permission_macros::permission(
    group = "审批实例",
    group_desc = "审批运行、决定、恢复与取消",
    desc = "读取本次提交的审批资料",
    resource = "approval_instance",
    action = "read"
)]
/// 返回授权实例的冻结资料；开票审批另列当前销售来源资料。
/// # 参数
/// 应用状态、认证账号和审批实例 ID。
/// # 返回
/// 提交资料与独立的当前销售依据；不含草稿、存储键或内容指纹。
/// # 错误
/// 无权或冻结资料缺失时拒绝。
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> std::result::Result<Response, Error> {
    let result: ApprovalDocumentMaterials =
        materials(&state.db(), &state.approval_runtime_service(), &actor, &id).await?;
    let mut response = ApiResponse::ok_with_data(result).into_response();
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(response)
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
    Query(query): Query<MaterialQuery>,
) -> std::result::Result<Response, Error> {
    content(
        &state,
        &actor,
        ApprovalMaterialTarget {
            copy_receipt_id: None,
            instance_id: &id,
            file_id: &file_id,
            source_revision_id: query.source_revision_id.as_deref(),
        },
        false,
    )
    .await
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
    Query(query): Query<MaterialQuery>,
) -> std::result::Result<Response, Error> {
    content(
        &state,
        &actor,
        ApprovalMaterialTarget {
            copy_receipt_id: None,
            instance_id: &id,
            file_id: &file_id,
            source_revision_id: query.source_revision_id.as_deref(),
        },
        true,
    )
    .await
}

/// 文件授权由组合层执行；对象存储读取在事务之外完成。
pub(super) async fn content(
    state: &AppState,
    actor: &AuditActor,
    target: ApprovalMaterialTarget<'_>,
    download: bool,
) -> std::result::Result<Response, Error> {
    let runtime = state.approval_runtime_service();
    let file =
        approval_materials::readable(&state.db(), &runtime, &state.file_asset_service(), actor, target)
            .await?;
    if !download && !previewable(&file.content_type) {
        return Err(Error::Unprocessable("当前文件类型不支持在线预览，请下载查看".into()));
    }
    let bytes = state
        .storage()
        .read(&file.storage_object_key)
        .await
        .map_err(|_| Error::Internal("附件读取失败，请稍后重试".into()))?;
    verify_content(&file, &bytes, state.config_snapshot().app.secret.as_bytes())?;
    approval_materials::revalidate(&state.db(), &runtime, &state.file_asset_service(), actor, target, &file)
        .await?;
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
    fn source_query_is_explicit_and_legacy_requests_stay_frozen() {
        let frozen: super::MaterialQuery = serde_json::from_str("{}").unwrap();
        assert!(frozen.source_revision_id.is_none());
        let source: super::MaterialQuery =
            serde_json::from_str(r#"{"source_revision_id":"revision-1"}"#).unwrap();
        assert_eq!(source.source_revision_id.as_deref(), Some("revision-1"));
    }

    #[test]
    fn active_content_is_download_only() {
        assert!(super::previewable("application/pdf"));
        assert!(super::previewable("image/png"));
        assert!(!super::previewable("text/html"));
        assert!(!super::previewable("image/svg+xml"));
        assert!(!super::previewable("application/octet-stream"));
    }
}
