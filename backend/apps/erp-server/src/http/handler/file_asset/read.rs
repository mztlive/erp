//! 受控业务附件下载，调用方须在读取前后独立证明来源对象资格。
use application_core::AuditActor;
use axum::body::Body;
use axum::http::HeaderValue;
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::response::Response;
use erp_support::{FileAssetView, SecurityScanStatus, content_fingerprint};
use persistence_core::ReadExecutor;

use crate::http::errors::Error;
use crate::http::state::HttpState;
use crate::http::upload::detect_image_mime;
// 业务来源资格由调用方证明；在治理检查与敏感读取审计后出事务读取对象，并核对字节与已登记内容一致。
pub(crate) async fn read_asset(
    state: &HttpState,
    actor: &AuditActor,
    id: &str,
) -> std::result::Result<(FileAssetView, Vec<u8>), Error> {
    let view = state
        .application()
        .file_asset_service()
        .preview_asset(id, actor, &mut ReadExecutor::default())
        .await?;
    ensure_readable(&view)?;
    let bytes = state
        .application()
        .storage()
        .read(&view.storage_object_key)
        .await
        .map_err(|_| Error::Internal("文件读取失败，请重试".into()))?;
    verify_content(&view, &bytes, state.application().config_snapshot().app.secret.as_bytes())?;
    Ok((view, bytes))
}

// 拿到对象存储字节后，核对登记大小、内容指纹以及支持预览类型的真实 MIME；发现内容替换或记录不匹配时拒绝。其他类型仍须由调用方限制为仅下载。
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

// 对象读取之后、响应发出之前重新读取文件治理状态并比对版本和指纹；中途变更时拒绝交付。
pub(crate) async fn revalidate_asset(
    state: &HttpState,
    original: &FileAssetView,
) -> std::result::Result<(), Error> {
    let latest = state
        .application()
        .file_asset_service()
        .asset_view(&original.id, &mut ReadExecutor::default())
        .await?;
    ensure_readable(&latest)?;
    if latest.version != original.version || latest.content_hmac != original.content_hmac {
        return Err(Error::Conflict("文件已变化，请刷新后重新下载".into()));
    }
    Ok(())
}

// 为业务附件读取挡住已销毁、已拒绝或隔离的文件，并只允许 PDF、JPEG、PNG 和 WebP 类型。
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

// 以登记 MIME 发送已经重验的附件字节，强制下载、不缓存也不嗅探；不交付底层存储地址。
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
