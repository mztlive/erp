//! 受控业务附件下载，调用方须在读取前后独立证明来源对象资格。
use application_core::AuditActor;
use axum::body::Body;
use axum::http::HeaderValue;
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::response::Response;
use erp_support::{FileAssetView, SecurityScanStatus, content_fingerprint};

use crate::app_state::AppState;
use crate::core::errors::Error;
use crate::core::upload::detect_image_mime;
/// 治理检查和敏感读取审计后在事务外读取存储字节。
pub(crate) async fn read_asset(
    state: &AppState,
    actor: &AuditActor,
    id: &str,
) -> std::result::Result<(FileAssetView, Vec<u8>), Error> {
    let view = state.file_asset_service().file_asset_preview(id, actor).await?;
    ensure_readable(&view)?;
    let bytes = state
        .storage()
        .read(&view.storage_object_key)
        .await
        .map_err(|_| Error::Internal("文件读取失败，请重试".into()))?;
    verify_content(&view, &bytes, state.config_snapshot().app.secret.as_bytes())?;
    Ok((view, bytes))
}

/// 存储真实字节必须匹配登记内容；可预览类型额外校验 MIME。
///
/// # 参数
/// * `view` - 已经通过业务来源授权的当前文件元数据。
/// * `bytes` / `key` - 存储读取结果及应用内容指纹密钥。
/// # 返回
/// 文件大小、内容指纹及支持预览类型的 MIME 均匹配时成功。
/// # 错误
/// 内容被替换或大小、类型不符时拒绝；其他类型仍须由调用方限制为下载。
pub(crate) fn verify_content(
    view: &FileAssetView,
    bytes: &[u8],
    key: &[u8],
) -> std::result::Result<(), Error> {
    let mime_matches = match view.content_type.as_str() {
        "application/pdf" => bytes.starts_with(b"%PDF-"),
        "image/jpeg" | "image/png" | "image/webp" => {
            detect_image_mime(bytes) == Some(view.content_type.as_str())
        },
        // 其他类型只在审批下载入口传输；不取消内容指纹校验，也不授予在线预览。
        _ => true,
    };
    let expected_size = u64::try_from(bytes.len()).map_err(|_| Error::Conflict("文件内容大小异常".into()))?;
    let actual_hmac = content_fingerprint(&super::sha256_hex(bytes), key);
    if !mime_matches || expected_size != view.byte_size || actual_hmac != view.content_hmac {
        return Err(Error::Conflict("文件内容已变化，请联系经办人核对".into()));
    }
    Ok(())
}

/// 存储读取后重验文件版本和当前治理状态。
pub(crate) async fn revalidate_asset(
    state: &AppState,
    original: &FileAssetView,
) -> std::result::Result<(), Error> {
    let latest = state.file_asset_service().file_asset_detail(&original.id).await?;
    ensure_readable(&latest)?;
    if latest.version != original.version || latest.content_hmac != original.content_hmac {
        return Err(Error::Conflict("文件已变化，请刷新后重新下载".into()));
    }
    Ok(())
}

/// 只允许未被销毁或安全检查拒绝的图片与 PDF。
fn ensure_readable(view: &FileAssetView) -> std::result::Result<(), Error> {
    if view.destroyed_at.is_some()
        || matches!(view.security_scan_status, SecurityScanStatus::Rejected | SecurityScanStatus::Quarantined)
        || !matches!(
            view.content_type.as_str(),
            "application/pdf" | "image/jpeg" | "image/png" | "image/webp"
        )
    {
        return Err(Error::Unprocessable("当前文件不可下载，请联系经办人核对".into()));
    }
    Ok(())
}

/// 下载保留内容类型并禁止缓存、嗅探及存储地址泄露。
pub(crate) fn asset_download_response(
    view: &FileAssetView,
    bytes: Vec<u8>,
) -> std::result::Result<Response, Error> {
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&view.content_type).map_err(|_| Error::Internal("文件类型无效".into()))?,
    );
    response.headers_mut().insert(CONTENT_DISPOSITION, HeaderValue::from_static("attachment"));
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response.headers_mut().insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

#[cfg(test)]
mod tests {
    use erp_core::ids::FileAssetId;
    use erp_support::{FileAsset, RegisterFileAssetRequest, RetentionClass, SensitivityClass};

    use super::*;

    #[test]
    fn registered_file_bytes_must_match_size_mime_and_real_content_fingerprint() {
        let key = b"test-secret";
        let bytes = b"%PDF-1.7\n";
        let request = RegisterFileAssetRequest {
            storage_object_key: "object".into(),
            file_name: "file.pdf".into(),
            content_type: "application/pdf".into(),
            byte_size: u64::try_from(bytes.len()).unwrap(),
            content_hmac: content_fingerprint(&super::super::sha256_hex(bytes), key),
            sensitivity_class: SensitivityClass::Sensitive,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
        };
        let asset = FileAsset::new(FileAssetId::new("file"), request.into_data("actor").unwrap()).unwrap();
        let mut view = FileAssetView::from(asset);
        assert!(verify_content(&view, bytes, key).is_ok());
        assert!(verify_content(&view, b"%PDF-1.8\n", key).is_err());
        assert!(verify_content(&view, b"%PDF-1.7\nextra", key).is_err());
        view.content_type = "image/png".into();
        assert!(verify_content(&view, bytes, key).is_err());
        view.content_type = "application/pdf".into();
        view.destroyed_at = Some(1);
        assert!(ensure_readable(&view).is_err());
    }

    #[test]
    fn download_only_files_still_require_exact_content_and_remain_unpreviewable() {
        let key = b"test-secret";
        let bytes = b"<html>submitted attachment</html>";
        let request = RegisterFileAssetRequest {
            storage_object_key: "object".into(),
            file_name: "attachment.html".into(),
            content_type: "text/html".into(),
            byte_size: u64::try_from(bytes.len()).unwrap(),
            content_hmac: content_fingerprint(&super::super::sha256_hex(bytes), key),
            sensitivity_class: SensitivityClass::Sensitive,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
        };
        let asset = FileAsset::new(FileAssetId::new("file"), request.into_data("actor").unwrap()).unwrap();
        let view = FileAssetView::from(asset);
        assert!(verify_content(&view, bytes, key).is_ok());
        assert!(verify_content(&view, b"<html>different attachment</html>", key).is_err());
        assert!(ensure_readable(&view).is_err());
    }
}
