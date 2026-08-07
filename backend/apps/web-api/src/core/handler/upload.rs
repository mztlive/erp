use axum::extract::{Multipart, State};
use serde::Serialize;
use tracing::error;

use crate::{
    app_state::AppState,
    core::{
        errors::{Error, Result},
        response::ApiResponse,
        upload::extract_file,
    },
};

/// 上传成功响应。
#[derive(Debug, Serialize)]
pub struct UploadResponse {
    /// 上传文件的公开访问地址。
    pub url: String,
}

/// 处理已认证后台身份发起的图片上传。
///
/// # 参数
///
/// * `state` - 应用状态
/// * `multipart` - Multipart 表单数据
///
/// # 返回
///
/// 返回上传后的文件 URL，响应合同保持为 `{ url }`。
///
/// # 错误
///
/// Multipart 无文件、图片超过 5 MiB、文件类型不受支持或 S3 存储失败时返回错误。
pub(crate) async fn upload_file(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<UploadResponse> {
    let file = extract_file(&mut multipart)
        .await?
        .ok_or_else(|| Error::BadRequest("未上传文件".to_string()))?
        .validate_image()?;
    let unique_name = file.unique_name();

    state
        .storage()
        .save_with_content_type(&unique_name, file.content(), Some(file.content_type()))
        .await
        .map_err(|storage_error| {
            error!(error = %storage_error, object_key = %unique_name, "Failed to save upload to S3");
            Error::Internal("Object storage operation failed".to_string())
        })?;
    let url = state
        .storage()
        .public_url(&unique_name)
        .map_err(|storage_error| {
            error!(error = %storage_error, object_key = %unique_name, "Failed to build S3 public URL");
            Error::Internal("Object storage URL generation failed".to_string())
        })?;

    Ok(ApiResponse::ok_with_data(UploadResponse { url }))
}
