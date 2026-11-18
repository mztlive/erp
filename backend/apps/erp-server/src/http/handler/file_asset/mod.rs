//! 域 D05 `file_asset` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 支撑领域的 DTO，禁止重复定义同构类型、禁止直连数据库。
//!
//! 上传接口参照 `core/upload.rs` 既有模式：S3 对象写入在 Service 之外完成，
//! Service 只编排元数据
//! （TRANSACTIONS.md：事务闭包内禁止文件 I/O）。

mod compensation;
mod read;
use application_core::AuditActor;
use axum::Extension;
use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
pub(crate) use compensation::{
    delete_pending_asset_objects, finish_asset_command, should_compensate_pending_assets,
};
use erp_approval_runtime::approval_materials::authorize_attachment;
use erp_core::ids::BusinessDocumentId;
use erp_permissions::sales_order;
use erp_read_models::sales_center::access::SalesAccess;
use erp_support::{
    AttachToDocumentRequest, DestroyFileAssetRequest, DocumentAttachmentView, FileAssetListItemView,
    FileAssetListParams, FileAssetView, MarkScanResultRequest, PageView, PendingFileAssetRequest,
    RegisterFileAssetRequest,
};
pub(crate) use read::{asset_download_response, read_asset, revalidate_asset, verify_content};
use serde::de::DeserializeOwned;
use tracing::error;

use crate::http::errors::{Error, Result};
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::http::upload;

/// 查询文件资产列表（列表不暴露敏感对象存储键）。
/// `query` 为分页与筛选参数（`file_name`/`security_scan_status` 等）。
/// 无文件列表资格、参数非法或读取失败时拒绝。
pub async fn file_asset_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<FileAssetListParams>,
) -> Result<PageView<FileAssetListItemView>> {
    let page = state.application().file_asset_service().file_asset_list(&params, &actor).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 查询文件资产详情；敏感资产不返回对象存储键或公开 URL。
/// 返回完整资产视图。
/// 无文件详情资格、资产缺失或读取失败时拒绝。
pub async fn file_asset_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<FileAssetView> {
    let mut view = state.application().file_asset_service().file_asset_detail(&id, &actor).await?;
    prepare_asset_response(&state, &mut view);
    Ok(ApiResponse::ok_with_data(view))
}

/// 在鉴权与审计后以内联内容响应返回文件资产；对象存储键不进入响应。
///
/// # Errors
///
/// 文件不存在、已拒绝/隔离、审计失败或对象存储读取失败时返回错误。
pub async fn file_asset_preview(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> std::result::Result<Response, Error> {
    let view = state.application().file_asset_service().file_asset_preview(&id, &actor).await?;
    if !matches!(view.content_type.as_str(), "image/jpeg" | "image/png" | "image/webp" | "application/pdf") {
        return Err(Error::Unprocessable("当前文件类型不支持在线预览".to_string()));
    }
    if matches!(
        view.security_scan_status,
        erp_support::SecurityScanStatus::Rejected | erp_support::SecurityScanStatus::Quarantined
    ) {
        return Err(Error::Unprocessable("文件未通过安全检查，不能预览".to_string()));
    }
    let content =
        state.application().storage().read(&view.storage_object_key).await.map_err(|storage_error| {
            error!(error = %storage_error, file_asset_id = %id, "Failed to read file asset preview");
            Error::Internal("Object storage operation failed".to_string())
        })?;
    let content_type = HeaderValue::from_str(&view.content_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    let mut response = Response::new(Body::from(content));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(CONTENT_TYPE, content_type);
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response.headers_mut().insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

/// 上传通用文件资产时先预检业务资格，再读取 `file` 和治理字段 `sensitivity_class`、`retention_class`、可选 `expires_at`。可选 `document_id` 需另验对象资格，附件关系与资产登记同事务；`usage` 可选。
/// 文件写入共享 S3，内容指纹使用 `app.secret` 计算 HMAC-SHA256；只在确认允许补偿时删除对象，提交结果未知时保留。
pub async fn file_asset_upload(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<FileAssetView> {
    let service = state.application().file_asset_service();
    service.prepare_upload(&actor).await?;
    let file = extract_asset_file(&mut multipart).await?;
    let fields = FileAssetFormFields::from_multipart(&mut multipart).await?;
    if let Some(document_id) = &fields.document_id {
        SalesAccess::new(state.application().db(), state.application().authorization())
            .require_attachment(&actor, &sales_order::HANDOVER, document_id.as_ref())
            .await?;
    }
    let request =
        store_asset_file(&state, file, fields.sensitivity_class, fields.retention_class, fields.expires_at)
            .await?;
    let cleanup =
        [PendingFileAssetRequest { reference: "file-upload".to_string(), registration: request.clone() }];
    let result = match fields.document_id {
        Some(document_id) => {
            service
                .register_content_checked_file_asset_with_attachment(
                    request,
                    document_id,
                    fields.usage,
                    &actor,
                )
                .await
        },
        None => service.register_content_checked_file_asset(request, &actor).await,
    };
    let mut asset = match result {
        Ok(asset) => asset,
        Err(error) => {
            let error = crate::wiring::errors::Error::from(error);
            if should_compensate_pending_assets(&error) {
                delete_pending_asset_objects(&state, &cleanup).await;
            }
            return Err(error.into());
        },
    };
    prepare_asset_response(&state, &mut asset);

    Ok(ApiResponse::ok_with_data(asset))
}

// 仅在生成普通资产响应时按存储配置拼接公开 URL；拼接失败记录错误并沿用对象键。
fn build_public_url(state: &HttpState, storage_object_key: &str) -> String {
    match state.application().storage().public_url(storage_object_key) {
        Ok(url) => url,
        Err(storage_error) => {
            error!(
                error = %storage_error,
                object_key = %storage_object_key,
                "Failed to build S3 public URL; fallback to object key"
            );
            storage_object_key.to_string()
        },
    }
}

// 交付文件元数据前，只为普通资产补公开 URL；敏感资产清除对象键、内容指纹和公开地址。
pub(crate) fn prepare_asset_response(state: &HttpState, view: &mut FileAssetView) {
    if view.sensitivity_class == erp_support::SensitivityClass::General {
        view.public_url = Some(build_public_url(state, &view.storage_object_key));
        return;
    }
    view.storage_object_key.clear();
    view.content_hmac.clear();
    view.public_url = None;
}

/// 登记文件资产（元数据登记；文件已由调用方落盘到受控对象存储）。
/// 返回新建的资产详情视图。
pub async fn file_asset_register(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RegisterFileAssetRequest>,
) -> Result<FileAssetView> {
    let mut view = state.application().file_asset_service().register_file_asset(req, &actor).await?;
    prepare_asset_response(&state, &mut view);

    Ok(ApiResponse::ok_with_data(view))
}

/// 建立单据附件关联（安全检查、保留期与销毁状态不阻断关联）。
/// `req` 为关联请求（`{ document_id, file_asset_id, usage }`）。
/// 返回新建的附件关联视图。
/// 销售单据或销售变更单不可见时拒绝。
///
/// 销售附件必须独立重验来源销售单范围。
pub async fn document_attachment_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<AttachToDocumentRequest>,
) -> Result<DocumentAttachmentView> {
    SalesAccess::new(state.application().db(), state.application().authorization())
        .require_attachment(&actor, &sales_order::HANDOVER, req.document_id.as_ref())
        .await?;
    authorize_attachment(
        &state.application().authorization(),
        &state.application().db(),
        &actor,
        req.file_asset_id.as_ref(),
    )
    .await?;
    let view = state.application().file_asset_service().attach_to_document(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 按业务单据查询附件关联。
/// 返回附件关联视图列表。
/// 销售单据或销售变更单不可见时拒绝。
///
/// 列表已授权不能作为附件凭证；销售附件独立重验来源销售单。
pub async fn document_attachment_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<DocumentAttachmentView>> {
    SalesAccess::new(state.application().db(), state.application().authorization())
        .require_attachment(&actor, &sales_order::DETAIL, &id)
        .await?;
    let items = state
        .application()
        .file_asset_service()
        .document_attachment_list(&BusinessDocumentId::new(id), &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(items))
}

/// 记录安全检查结果（通过/拒绝/隔离；隔离后经人工复核可放行或拒绝）。
/// `req` 为更新请求（含期望版本与检查结果）。
/// 返回更新后的资产视图。
pub async fn file_asset_scan_result(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<MarkScanResultRequest>,
) -> Result<FileAssetView> {
    let mut view = state.application().file_asset_service().mark_scan_result(&id, req, &actor).await?;
    prepare_asset_response(&state, &mut view);

    Ok(ApiResponse::ok_with_data(view))
}

/// 销毁文件资产（销毁审计只记录一次；已销毁资产不得再用于业务关联）。
/// `req` 为销毁请求（含期望版本）。
/// 返回销毁后的资产视图。
pub async fn file_asset_destroy(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<DestroyFileAssetRequest>,
) -> Result<FileAssetView> {
    let mut view = state.application().file_asset_service().destroy_file_asset(&id, req, &actor).await?;
    prepare_asset_response(&state, &mut view);

    Ok(ApiResponse::ok_with_data(view))
}

/// 从 Multipart 表单中提取的通用文件（复用 `core/upload` 的大小/形态校验口径）。
#[derive(Debug)]
pub(crate) struct AssetFile {
    /// 展示文件名。
    pub file_name: String,
    /// 内容类型。
    pub content_type: String,
    /// 文件内容。
    pub content: Vec<u8>,
}

/// multipart 业务命令中的一个具名文件字段。
#[derive(Debug)]
pub(crate) struct PendingAssetFile {
    /// 与业务 DTO 内临时 `FileAssetId` 一致的字段名。
    pub reference: String,
    /// 已完成大小、扩展名、MIME 与真实文件头校验的文件。
    pub file: AssetFile,
}

// 从复合上传中读取唯一的 `command` JSON 字段，并将每个具名文件字段作为临时引用。解析不依赖字段顺序；文件上限为每个 5 MiB、每次 32 个。
pub(crate) async fn extract_command_with_asset_files<T: DeserializeOwned>(
    multipart: &mut Multipart,
) -> std::result::Result<(T, Vec<PendingAssetFile>), Error> {
    const MAX_FILE_COUNT: usize = 32;

    let mut command = None;
    let mut files = Vec::new();
    while let Some(mut field) =
        multipart.next_field().await.map_err(|_| Error::BadRequest("Multipart 表单无效".to_string()))?
    {
        let name = field.name().unwrap_or_default().trim().to_string();
        if let Some(file_name) = field.file_name().map(ToString::to_string) {
            if name.is_empty() {
                return Err(Error::BadRequest("上传文件缺少临时引用".to_string()));
            }
            if files.len() >= MAX_FILE_COUNT {
                return Err(Error::BadRequest("单次业务提交最多上传 32 个文件".to_string()));
            }
            let content_type = field
                .content_type()
                .map(ToString::to_string)
                .ok_or_else(|| Error::BadRequest("缺少文件 MIME 类型".to_string()))?;
            let mut content = Vec::new();
            while let Some(chunk) =
                field.chunk().await.map_err(|_| Error::BadRequest("上传文件读取失败".to_string()))?
            {
                if content.len().saturating_add(chunk.len()) > upload::MAX_UPLOAD_FILE_BYTES {
                    return Err(Error::BadRequest("上传文件大小不能超过 5 MiB".to_string()));
                }
                content.extend_from_slice(&chunk);
            }
            files.push(PendingAssetFile {
                reference: name,
                file: validate_asset_file(AssetFile { file_name, content_type, content })?,
            });
            continue;
        }
        if name != "command" {
            continue;
        }
        if command.is_some() {
            return Err(Error::BadRequest("业务命令不能重复".to_string()));
        }
        let text = field.text().await.map_err(|_| Error::BadRequest("业务命令读取失败".to_string()))?;
        command = Some(
            serde_json::from_str::<T>(&text)
                .map_err(|_| Error::BadRequest("业务命令格式无效".to_string()))?,
        );
    }
    let command = command.ok_or_else(|| Error::BadRequest("缺少业务命令".to_string()))?;
    Ok((command, files))
}

// 业务事务前逐个保存已校验文件并生成待登记载荷；若某个对象写入失败，尝试清理同批此前已保存对象。
pub(crate) async fn store_pending_asset_files(
    state: &HttpState,
    files: Vec<PendingAssetFile>,
    sensitivity_for: impl Fn(&str) -> erp_support::SensitivityClass,
) -> std::result::Result<Vec<PendingFileAssetRequest>, Error> {
    let mut requests = Vec::with_capacity(files.len());
    for pending in files {
        let request = match store_asset_file(
            state,
            pending.file,
            sensitivity_for(&pending.reference),
            erp_support::RetentionClass::LongTerm,
            None,
        )
        .await
        {
            Ok(request) => request,
            Err(error) => {
                delete_pending_asset_objects(state, &requests).await;
                return Err(error);
            },
        };
        requests.push(PendingFileAssetRequest { reference: pending.reference, registration: request });
    }
    Ok(requests)
}

// 提取 Multipart 表单中的第一个文件字段并校验大小与 MIME。
// 返回校验通过的文件。
// 无文件字段、文件超过 5 MiB 或缺少 MIME 类型时返回错误。
pub(crate) async fn extract_asset_file(multipart: &mut Multipart) -> std::result::Result<AssetFile, Error> {
    extract_asset_file_with_limit(multipart, upload::MAX_UPLOAD_FILE_BYTES).await
}

// 按调用方业务上限提取首个文件，并继续执行扩展名、MIME 与真实文件头校验。
// 返回校验通过的文件。
// 无文件字段、文件超过业务上限或文件类型不受支持时返回错误。
pub(crate) async fn extract_asset_file_with_limit(
    multipart: &mut Multipart,
    max_file_bytes: usize,
) -> std::result::Result<AssetFile, Error> {
    while let Some(mut field) =
        multipart.next_field().await.map_err(|_| Error::BadRequest("Multipart 表单无效".to_string()))?
    {
        let Some(file_name) = field.file_name().map(str::trim).filter(|name| !name.is_empty()) else {
            continue;
        };
        let file_name = file_name.to_string();
        let content_type = field
            .content_type()
            .map(ToString::to_string)
            .ok_or_else(|| Error::BadRequest("缺少文件 MIME 类型".to_string()))?;
        let mut content = Vec::new();
        while let Some(chunk) =
            field.chunk().await.map_err(|_| Error::BadRequest("上传文件读取失败".to_string()))?
        {
            if content.len().saturating_add(chunk.len()) > max_file_bytes {
                return Err(Error::BadRequest(format!(
                    "上传文件大小不能超过 {} MiB",
                    max_file_bytes / (1024 * 1024)
                )));
            }
            content.extend_from_slice(&chunk);
        }
        return validate_asset_file(AssetFile { file_name, content_type, content });
    }
    Err(Error::BadRequest("未上传文件".to_string()))
}

// 在业务登记事务前将已校验字节及内容类型写入 S3，生成携带真实大小及带密钥指纹的待登记命令。
// 调用方负责按提交结果处理补偿；结果未知时不能删除对象。
pub(crate) async fn store_asset_file(
    state: &HttpState,
    file: AssetFile,
    sensitivity_class: erp_support::SensitivityClass,
    retention_class: erp_support::RetentionClass,
    expires_at: Option<u64>,
) -> std::result::Result<RegisterFileAssetRequest, Error> {
    let config = state.application().config_snapshot();
    let unique_name = storage_key_with_extension(id_generator::next_id(), &file.file_name);
    let content_hmac =
        erp_support::content_fingerprint(&sha256_hex(&file.content), config.app.secret.as_bytes());
    let byte_size = u64::try_from(file.content.len())
        .map_err(|_| Error::BadRequest("上传文件大小超出支持范围".to_string()))?;
    state
        .application()
        .storage()
        .save_owned_with_content_type(&unique_name, file.content, Some(file.content_type.as_str()))
        .await
        .map_err(|storage_error| {
            error!(error = %storage_error, object_key = %unique_name, "Failed to save file asset to S3");
            Error::Internal("Object storage operation failed".to_string())
        })?;
    Ok(RegisterFileAssetRequest {
        storage_object_key: unique_name,
        file_name: file.file_name,
        content_type: file.content_type,
        byte_size,
        content_hmac,
        sensitivity_class,
        retention_class,
        expires_at,
    })
}

// 登记附件前验证支持的图片或 PDF；只接受扩展名、声明 MIME 和实际文件头一致的非空内容。
pub(crate) fn validate_asset_file(file: AssetFile) -> std::result::Result<AssetFile, Error> {
    if file.content.is_empty() {
        return Err(Error::BadRequest("上传文件不能为空".to_string()));
    }
    let extension = upload::normalized_extension(&file.file_name)
        .ok_or_else(|| Error::BadRequest("附件缺少受支持的扩展名".to_string()))?;
    let expected_mime = if extension == "pdf" {
        "application/pdf"
    } else {
        upload::expected_mime(&extension).ok_or_else(|| Error::BadRequest("附件类型不受支持".to_string()))?
    };
    if !file.content_type.eq_ignore_ascii_case(expected_mime) {
        return Err(Error::BadRequest("附件扩展名与 MIME 类型不一致".to_string()));
    }
    let content_matches = if extension == "pdf" {
        file.content.starts_with(b"%PDF-")
    } else {
        upload::detect_image_mime(&file.content) == Some(expected_mime)
    };
    if !content_matches {
        return Err(Error::BadRequest("附件真实内容与声明类型不一致".to_string()));
    }
    Ok(file)
}

/// 上传文件资产时解析 Multipart 表单字段。
#[derive(Debug)]
pub(crate) struct FileAssetFormFields {
    /// 敏感级别。
    pub sensitivity_class: erp_support::SensitivityClass,
    /// 保留策略。
    pub retention_class: erp_support::RetentionClass,
    /// 到期时间（秒级时间戳）。
    pub expires_at: Option<u64>,
    /// 业务单据 ID（携带时同事务建立附件关联）。
    pub document_id: Option<erp_core::ids::BusinessDocumentId>,
    /// 附件用途。
    pub usage: erp_support::AttachmentUsage,
}

impl Default for FileAssetFormFields {
    // 通用上传省略治理字段时使用安全默认值；需要特定治理策略的业务应显式传入。
    fn default() -> Self {
        Self {
            sensitivity_class: erp_support::SensitivityClass::General,
            retention_class: erp_support::RetentionClass::LongTerm,
            expires_at: None,
            document_id: None,
            usage: erp_support::AttachmentUsage::Attachment,
        }
    }
}

impl FileAssetFormFields {
    // 从 Multipart 表单读取治理与关联字段。
    // 返回解析后的表单字段。
    // 表单字段解析失败或枚举值非法时返回错误。
    pub(crate) async fn from_multipart(multipart: &mut Multipart) -> std::result::Result<Self, Error> {
        let mut fields = Self::default();
        while let Some(field) =
            multipart.next_field().await.map_err(|_| Error::BadRequest("Multipart 表单无效".to_string()))?
        {
            let name = field.name().unwrap_or_default().to_string();
            if field.file_name().is_some() || name.is_empty() {
                continue;
            }
            let text = field.text().await.map_err(|_| Error::BadRequest("Multipart 表单无效".to_string()))?;
            match name.as_str() {
                "sensitivity_class" => {
                    fields.sensitivity_class = serde_json::from_value(serde_json::Value::String(text))
                        .map_err(|_| Error::BadRequest("敏感级别非法".to_string()))?;
                },
                "retention_class" => {
                    fields.retention_class = serde_json::from_value(serde_json::Value::String(text))
                        .map_err(|_| Error::BadRequest("保留策略非法".to_string()))?;
                },
                "expires_at" => {
                    fields.expires_at =
                        Some(text.parse::<u64>().map_err(|_| Error::BadRequest("到期时间非法".to_string()))?);
                },
                "document_id" => {
                    fields.document_id = Some(erp_core::ids::BusinessDocumentId::new(text));
                },
                "usage" => {
                    fields.usage = serde_json::from_value(serde_json::Value::String(text))
                        .map_err(|_| Error::BadRequest("附件用途非法".to_string()))?;
                },
                _ => {},
            }
        }
        Ok(fields)
    }
}

// 计算内容的 SHA-256 十六进制摘要（作为 HMAC 的明文形态）。
// 返回小写十六进制摘要。
fn sha256_hex(content: &[u8]) -> String {
    use jwt_sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content);
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

// 上传时以服务端生成的对象 ID 为键；受支持的图片追加小写扩展名方便浏览器及 CDN 识别，不靠扩展名代替设置 S3 的 `Content-Type`。非图片保留无扩展名键。
fn storage_key_with_extension(id: String, file_name: &str) -> String {
    let extension = upload::normalized_extension(file_name)
        .filter(|extension| upload::expected_mime(extension).is_some());
    match extension {
        Some(extension) => format!("{id}.{extension}"),
        None => id,
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::{FromRequest, Multipart};
    use axum::http::Request;

    use super::{
        AssetFile, Error, extract_asset_file_with_limit, should_compensate_pending_assets,
        storage_key_with_extension, validate_asset_file,
    };

    async fn multipart(fields: &str) -> Multipart {
        let request = Request::builder()
            .header("content-type", "multipart/form-data; boundary=x")
            .body(Body::from(format!("{fields}--x--\r\n")))
            .unwrap();
        Multipart::from_request(request, &()).await.unwrap()
    }

    #[tokio::test]
    async fn extracts_first_named_file_and_leaves_later_fields_unread() {
        let mut multipart = multipart(concat!(
            "--x\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\nskip\r\n",
            "--x\r\nContent-Disposition: form-data; name=\"file\"; filename=\"  \"\r\n\r\nskip\r\n",
            "--x\r\nContent-Disposition: form-data; name=\"file\"; filename=\" contract.pdf \"\r\n",
            "Content-Type: application/pdf\r\n\r\n%PDF-1.7\n\r\n",
            "--x\r\nContent-Disposition: form-data; name=\"sensitivity_class\"\r\n\r\nSENSITIVE\r\n",
            "--x\r\nContent-Disposition: form-data; name=\"file\"; filename=\"spoofed.pdf\"\r\n",
            "Content-Type: text/html\r\n\r\ninvalid\r\n",
        ))
        .await;

        let file = extract_asset_file_with_limit(&mut multipart, 9).await.unwrap();
        assert_eq!(file.file_name, "contract.pdf");
        assert_eq!(file.content_type, "application/pdf");
        assert_eq!(file.content, b"%PDF-1.7\n");
        let next = multipart.next_field().await.unwrap().unwrap();
        assert_eq!(next.name(), Some("sensitivity_class"));
        assert_eq!(next.text().await.unwrap(), "SENSITIVE");
        assert_eq!(multipart.next_field().await.unwrap().unwrap().file_name(), Some("spoofed.pdf"));
    }

    #[tokio::test]
    async fn file_extraction_preserves_missing_mime_size_and_content_errors() {
        for (fields, limit, expected) in [
            ("", 9, "未上传文件"),
            (
                "--x\r\nContent-Disposition: form-data; name=\"file\"; filename=\"contract.pdf\"\r\n\r\n%PDF-1.7\n\r\n",
                9,
                "缺少文件 MIME 类型",
            ),
            (
                "--x\r\nContent-Disposition: form-data; name=\"file\"; filename=\"contract.pdf\"\r\nContent-Type: application/pdf\r\n\r\n%PDF-1.7\n\r\n",
                8,
                "上传文件大小不能超过 0 MiB",
            ),
            (
                "--x\r\nContent-Disposition: form-data; name=\"file\"; filename=\"contract.pdf\"\r\nContent-Type: application/pdf\r\n\r\ninvalid\r\n",
                9,
                "附件真实内容与声明类型不一致",
            ),
        ] {
            let error = extract_asset_file_with_limit(&mut multipart(fields).await, limit).await.unwrap_err();
            assert!(matches!(error, Error::BadRequest(message) if message == expected));
        }
    }

    #[test]
    fn supported_image_names_keep_normalized_extension() {
        assert_eq!(storage_key_with_extension("id-1".to_string(), "photo.JpG"), "id-1.jpg");
        assert_eq!(storage_key_with_extension("id-2".to_string(), "photo.png"), "id-2.png");
    }

    #[test]
    fn unsupported_or_extensionless_names_stay_unchanged() {
        assert_eq!(storage_key_with_extension("id-3".to_string(), "report.pdf"), "id-3");
        assert_eq!(storage_key_with_extension("id-4".to_string(), "photo"), "id-4");
    }

    #[test]
    fn asset_file_rejects_spoofed_pdf_and_accepts_valid_header() {
        let valid = AssetFile {
            file_name: "contract.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            content: b"%PDF-1.7\n".to_vec(),
        };
        assert!(validate_asset_file(valid).is_ok());

        let spoofed = AssetFile {
            file_name: "contract.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            content: b"not-a-pdf".to_vec(),
        };
        assert!(validate_asset_file(spoofed).is_err());
    }

    #[test]
    fn unknown_commit_outcome_keeps_uploaded_object() {
        let database_error = persistence_core::Error::CommitOutcomeUnknown(mongodb::error::Error::custom(
            "unknown commit result",
        ));
        let unknown = crate::wiring::errors::Error::OutcomeUnknown(database_error);
        let definite = crate::wiring::errors::Error::ConflictError("已确定回滚".to_string());

        assert!(!should_compensate_pending_assets(&unknown));
        assert!(should_compensate_pending_assets(&definite));
    }
}
