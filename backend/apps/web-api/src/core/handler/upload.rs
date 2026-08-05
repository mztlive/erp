use axum::extract::{Extension, Multipart, State};
use serde::Serialize;
use storage::LocalStorage;

use crate::{
    app_state::AppState,
    core::{
        errors::{Error, Result},
        response::ApiResponse,
        upload::{ensure_storage_headroom, extract_file, WriteLock},
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
/// * `write_lock` - 串行覆盖磁盘水位检查与保存的进程内锁
/// * `multipart` - Multipart 表单数据
///
/// # 返回
///
/// 返回上传后的文件 URL，响应合同保持为 `{ url }`。
///
/// # 错误
///
/// Multipart 无文件、图片超过 5 MiB、文件类型不受支持、保存后低于磁盘安全水位或
/// 存储失败时返回错误。
pub(crate) async fn upload_file(
    State(state): State<AppState>,
    Extension(write_lock): Extension<WriteLock>,
    mut multipart: Multipart,
) -> Result<UploadResponse> {
    let config = state.config_snapshot();
    let storage = LocalStorage::new(state.upload_path())
        .await
        .map_err(|error| Error::Internal(format!("Failed to init storage: {error}")))?;

    let file = extract_file(&mut multipart)
        .await?
        .ok_or_else(|| Error::BadRequest("未上传文件".to_string()))?
        .validate_image()?;
    let unique_name = file.unique_name();

    let _write_guard = write_lock.lock().await;
    ensure_storage_headroom(
        state.upload_path(),
        file.content().len(),
        config.app.upload_min_free_bytes,
    )?;
    storage
        .save(&unique_name, file.content())
        .await
        .map_err(|error| Error::Internal(format!("Failed to save file: {error}")))?;

    Ok(ApiResponse::ok_with_data(UploadResponse {
        url: config.file_url(&unique_name),
    }))
}
