//! Multipart 图片上传的 HTTP 协议适配与输入校验。

use std::{path::Path, sync::OnceLock, time::Duration};

use axum::{
    extract::{multipart::Field, DefaultBodyLimit, Multipart, Request, State},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::MethodRouter,
};
use tracing::{error, warn};

use crate::{
    app_state::AppState,
    core::{errors, middleware::RbacSubject, rate_limit::RateLimiter, response::ApiResponse},
};

/// 单个上传文件内容允许的最大字节数。
pub(crate) const MAX_UPLOAD_FILE_BYTES: usize = 5 * 1024 * 1024;

/// Multipart 请求允许的最大字节数，额外空间用于边界、字段头等协议开销。
pub(crate) const MAX_MULTIPART_REQUEST_BYTES: usize = MAX_UPLOAD_FILE_BYTES + 1024 * 1024;

/// 合同 PDF 允许的最大字节数，与前端提交合同保持一致。
pub(crate) const MAX_CONTRACT_PDF_BYTES: usize = 20 * 1024 * 1024;

/// 合同 multipart 请求总上限，额外空间用于命令字段与协议边界。
pub(crate) const MAX_CONTRACT_MULTIPART_REQUEST_BYTES: usize = MAX_CONTRACT_PDF_BYTES + 1024 * 1024;

/// 多文件业务命令的累计请求上限；单文件仍受 5 MiB 限制。
pub(crate) const MAX_BATCH_MULTIPART_REQUEST_BYTES: usize = 32 * 1024 * 1024;

const MAX_UPLOADS_PER_WINDOW: usize = 10;
const MAX_GLOBAL_UPLOADS_PER_WINDOW: usize = 100;
const UPLOAD_WINDOW: Duration = Duration::from_secs(60);
const MAX_CONCURRENT_UPLOADS: usize = 4;

/// 创建上传路由使用的进程内准入控制器。
///
/// # 返回值
/// 返回每主体 10 次/60 秒、全局 100 次/60 秒、并发 4 个请求的限流器。
pub(crate) fn limiter() -> RateLimiter {
    static SHARED_LIMITER: OnceLock<RateLimiter> = OnceLock::new();
    SHARED_LIMITER.get_or_init(new_limiter).clone()
}

/// 为单个 multipart 方法安装共享上传准入和显式请求体上限。
///
/// # 参数
/// * `route` - 已注册 handler 的方法路由
/// * `request_limit` - 包含文件、命令字段和协议边界的累计字节上限
///
/// # 返回值
/// 返回仅对当前 multipart 方法生效的受保护路由。
pub(crate) fn multipart_route(route: MethodRouter<AppState>, request_limit: usize) -> MethodRouter<AppState> {
    route
        .route_layer(middleware::from_fn_with_state(limiter(), enforce_admission))
        .layer(DefaultBodyLimit::max(request_limit))
}

/// 创建一份上传策略；生产路由通过 `limiter` 共享同一份实例。
fn new_limiter() -> RateLimiter {
    RateLimiter::new(
        MAX_UPLOADS_PER_WINDOW,
        MAX_GLOBAL_UPLOADS_PER_WINDOW,
        UPLOAD_WINDOW,
        MAX_CONCURRENT_UPLOADS,
    )
}

/// Multipart 上传解析或校验错误。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum Error {
    #[error("上传文件名无效")]
    InvalidFilename,

    #[error("Multipart 表单无效")]
    InvalidFormData,

    #[error("上传文件读取失败")]
    ReadFailed,

    #[error("上传文件大小不能超过 5 MiB")]
    FileTooLarge,

    #[error("仅支持 jpeg、jpg、png、webp、gif 扩展名")]
    UnsupportedExtension,

    #[error("仅支持 image/jpeg、image/png、image/webp、image/gif MIME 类型")]
    UnsupportedMime,

    #[error("文件扩展名与 MIME 类型不匹配")]
    ExtensionMimeMismatch,

    #[error("文件内容不是受支持的图片格式")]
    UnsupportedImageContent,

    #[error("文件真实类型与扩展名或 MIME 类型不匹配")]
    ImageContentMismatch,
}

impl From<Error> for errors::Error {
    /// 将上传输入校验错误映射为 400。
    fn from(error: Error) -> Self {
        Self::BadRequest(error.to_string())
    }
}

/// 在读取 Multipart 请求体前执行上传配额与并发限制。
///
/// 该中间件必须放在后台认证中间件之后；缺少后台主体时会失败关闭。
pub(crate) async fn enforce_admission(
    State(admission): State<RateLimiter>,
    request: Request,
    next: Next,
) -> Response {
    let Some(subject) = upload_subject(&request) else {
        warn!("Upload denied without authenticated backoffice subject");
        return ApiResponse::<()>::unauthorized().into_response();
    };
    let permit = match admission.admit(subject) {
        Ok(permit) => permit,
        Err(admission_error) => {
            if admission_error.retry_after_secs().is_none() {
                error!("Upload admission state unavailable");
            } else {
                warn!(
                    subject,
                    retry_after_secs = ?admission_error.retry_after_secs(),
                    "Upload admission rejected"
                );
            }
            return errors::Error::from(admission_error).into_response();
        }
    };

    let response = next.run(request).await;
    drop(permit);
    response
}

fn upload_subject(request: &Request) -> Option<&str> {
    request
        .extensions()
        .get::<RbacSubject>()
        .map(|subject| subject.0.as_str())
}

/// 从 Multipart 表单中提取的原始文件。
#[derive(Debug)]
pub(crate) struct FormFile {
    filename: String,
    content: Vec<u8>,
    content_type: Option<String>,
}

impl FormFile {
    /// 校验图片大小、扩展名及 MIME，并返回只能使用安全扩展名的文件。
    ///
    /// # 返回值
    /// 校验成功后返回可持久化的图片。
    ///
    /// # 错误
    /// 文件超过 5 MiB、类型不受支持或扩展名、MIME、真实内容不一致时返回错误。
    pub(crate) fn validate_image(self) -> Result<ValidatedImage, Error> {
        if self.content.len() > MAX_UPLOAD_FILE_BYTES {
            return Err(Error::FileTooLarge);
        }

        let extension = normalized_extension(&self.filename).ok_or(Error::UnsupportedExtension)?;
        let expected_mime = expected_mime(extension.as_str()).ok_or(Error::UnsupportedExtension)?;
        let content_type = self.content_type.ok_or(Error::UnsupportedMime)?;

        if !is_supported_mime(&content_type) {
            return Err(Error::UnsupportedMime);
        }
        if !content_type.eq_ignore_ascii_case(expected_mime) {
            return Err(Error::ExtensionMimeMismatch);
        }

        let detected_mime = detect_image_mime(&self.content).ok_or(Error::UnsupportedImageContent)?;
        if detected_mime != expected_mime {
            return Err(Error::ImageContentMismatch);
        }

        Ok(ValidatedImage {
            content: self.content,
            extension,
            content_type: expected_mime,
        })
    }

    /// 读取 Multipart 字段，并在累计内容超过限制时立即失败。
    async fn from_field(mut field: Field<'_>) -> Result<Self, Error> {
        let filename = field
            .file_name()
            .filter(|name| !name.trim().is_empty())
            .ok_or(Error::InvalidFilename)?
            .to_string();
        let content_type = field.content_type().map(ToString::to_string);
        let mut content = Vec::new();

        while let Some(chunk) = field.chunk().await.map_err(|_| Error::ReadFailed)? {
            if content.len().saturating_add(chunk.len()) > MAX_UPLOAD_FILE_BYTES {
                return Err(Error::FileTooLarge);
            }
            content.extend_from_slice(&chunk);
        }

        Ok(Self {
            filename,
            content,
            content_type,
        })
    }
}

/// 已通过服务端图片上传校验的文件。
#[derive(Debug)]
pub(crate) struct ValidatedImage {
    content: Vec<u8>,
    extension: String,
    content_type: &'static str,
}

impl ValidatedImage {
    /// 返回图片内容。
    pub(crate) fn content(&self) -> &[u8] {
        &self.content
    }

    /// 返回与已验证扩展名一致的图片 MIME。
    pub(crate) fn content_type(&self) -> &'static str {
        self.content_type
    }

    /// 使用已验证扩展名生成唯一文件名。
    pub(crate) fn unique_name(&self) -> String {
        format!("{}.{}", id_generator::next_id(), self.extension)
    }
}

/// 提取 Multipart 表单中的第一个文件字段。
///
/// # 返回值
/// 没有文件字段时返回 `None`。
///
/// # 错误
/// Multipart 无效、字段读取失败或文件超过 5 MiB 时返回错误。
pub(crate) async fn extract_file(multipart: &mut Multipart) -> Result<Option<FormFile>, Error> {
    while let Some(field) = multipart.next_field().await.map_err(|_| Error::InvalidFormData)? {
        if field.file_name().is_some() {
            return FormFile::from_field(field).await.map(Some);
        }
    }

    Ok(None)
}

pub(crate) fn normalized_extension(filename: &str) -> Option<String> {
    Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .map(str::to_ascii_lowercase)
}

pub(crate) fn expected_mime(extension: &str) -> Option<&'static str> {
    match extension {
        "jpeg" | "jpg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

fn is_supported_mime(content_type: &str) -> bool {
    matches!(
        content_type.to_ascii_lowercase().as_str(),
        "image/jpeg" | "image/png" | "image/webp" | "image/gif"
    )
}

/// 根据文件头识别受支持的图片 MIME，不信任客户端声明。
pub(crate) fn detect_image_mime(content: &[u8]) -> Option<&'static str> {
    if content.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if content.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if content.starts_with(b"GIF87a") || content.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if content.len() >= 12 && content.starts_with(b"RIFF") && &content[8..12] == b"WEBP" {
        return Some("image/webp");
    }

    None
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, extract::Request};

    use crate::core::{errors, middleware::RbacSubject, rate_limit::Error as RateLimitError};

    use super::{limiter, new_limiter, upload_subject, Error, FormFile, MAX_UPLOAD_FILE_BYTES};

    fn form_file(filename: &str, content_type: Option<&str>, content: &[u8]) -> FormFile {
        FormFile {
            filename: filename.to_string(),
            content: content.to_vec(),
            content_type: content_type.map(ToString::to_string),
        }
    }

    #[test]
    fn validates_each_supported_image_format() {
        let cases = [
            ("avatar.JpG", "image/jpeg", &b"\xff\xd8\xff\xe0"[..], "jpg"),
            ("avatar.png", "image/png", &b"\x89PNG\r\n\x1a\n"[..], "png"),
            (
                "avatar.webp",
                "image/webp",
                &b"RIFF\x04\x00\x00\x00WEBP"[..],
                "webp",
            ),
            ("legacy.gif", "image/gif", &b"GIF87a"[..], "gif"),
            ("avatar.gif", "image/gif", &b"GIF89a"[..], "gif"),
        ];

        for (filename, content_type, content, expected_extension) in cases {
            let image = form_file(filename, Some(content_type), content)
                .validate_image()
                .expect("supported image should pass");

            assert_eq!(image.extension, expected_extension);
            assert_eq!(image.content(), content);
            assert_eq!(image.content_type(), content_type);
        }
    }

    #[test]
    fn accepts_file_at_size_limit() {
        let mut content = vec![0; MAX_UPLOAD_FILE_BYTES];
        content[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        let image = form_file("avatar.png", Some("image/png"), &content)
            .validate_image()
            .expect("file at limit should pass");

        assert_eq!(image.content().len(), MAX_UPLOAD_FILE_BYTES);
    }

    #[test]
    fn rejects_file_over_size_limit() {
        let mut content = vec![0; MAX_UPLOAD_FILE_BYTES + 1];
        content[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        let error = form_file("avatar.png", Some("image/png"), &content)
            .validate_image()
            .expect_err("oversized file should fail");

        assert_eq!(error, Error::FileTooLarge);
    }

    #[test]
    fn rejects_unsupported_extension_and_mime() {
        let extension_error = form_file("avatar.svg", Some("image/svg+xml"), b"<svg/>")
            .validate_image()
            .expect_err("unsupported extension should fail");
        let mime_error = form_file(
            "avatar.png",
            Some("application/octet-stream"),
            b"\x89PNG\r\n\x1a\n",
        )
        .validate_image()
        .expect_err("unsupported mime should fail");

        assert_eq!(extension_error, Error::UnsupportedExtension);
        assert_eq!(mime_error, Error::UnsupportedMime);
    }

    #[test]
    fn rejects_extension_mime_mismatch() {
        let error = form_file("avatar.png", Some("image/jpeg"), b"\x89PNG\r\n\x1a\n")
            .validate_image()
            .expect_err("mismatched type should fail");

        assert_eq!(error, Error::ExtensionMimeMismatch);
    }

    #[test]
    fn rejects_forged_png_content() {
        let error = form_file("avatar.png", Some("image/png"), b"not a png")
            .validate_image()
            .expect_err("forged image content should fail");

        assert_eq!(error, Error::UnsupportedImageContent);
    }

    #[test]
    fn rejects_detected_type_mismatch() {
        let error = form_file("avatar.png", Some("image/png"), b"\xff\xd8\xff\xe0")
            .validate_image()
            .expect_err("detected image type must match extension and mime");

        assert_eq!(error, Error::ImageContentMismatch);
    }

    #[test]
    fn rejects_empty_content() {
        let error = form_file("avatar.png", Some("image/png"), b"")
            .validate_image()
            .expect_err("empty image content should fail");

        assert_eq!(error, Error::UnsupportedImageContent);
    }

    #[test]
    fn maps_upload_validation_error_to_bad_request() {
        let error: errors::Error = Error::FileTooLarge.into();

        assert!(matches!(error, errors::Error::BadRequest(_)));
    }

    #[tokio::test]
    async fn generated_name_uses_validated_extension() {
        let image = form_file("avatar.PNG", Some("image/png"), b"\x89PNG\r\n\x1a\n")
            .validate_image()
            .expect("supported image should pass");

        let name = image.unique_name();

        assert!(name.ends_with(".png"));
        assert_eq!(name.matches('.').count(), 1);
    }

    #[test]
    fn upload_policy_limits_each_authenticated_subject() {
        let limiter = new_limiter();
        for _ in 0..10 {
            drop(limiter.admit("user:admin:1").unwrap());
        }

        assert!(matches!(
            limiter.admit("user:admin:1"),
            Err(RateLimitError::KeyExceeded { .. })
        ));
        assert!(limiter.admit("user:admin:2").is_ok());
    }

    #[test]
    fn upload_policy_is_shared_by_all_upload_routes() {
        let first_route = limiter();
        let second_route = limiter();
        for _ in 0..10 {
            drop(first_route.admit("test:shared-upload-policy").unwrap());
        }

        assert!(matches!(
            second_route.admit("test:shared-upload-policy"),
            Err(RateLimitError::KeyExceeded { .. })
        ));
    }

    #[test]
    fn upload_rate_key_requires_authenticated_backoffice_subject() {
        let mut request = Request::new(Body::empty());
        assert_eq!(upload_subject(&request), None);

        request
            .extensions_mut()
            .insert(RbacSubject("user:admin:1".to_string()));
        assert_eq!(upload_subject(&request), Some("user:admin:1"));
    }
}
