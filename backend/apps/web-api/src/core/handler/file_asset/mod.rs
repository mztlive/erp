//! 域 D05 `file_asset` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::file_asset` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//!
//! 上传接口参照 `core/upload.rs` 既有模式：S3 对象写入在 Service 之外完成，
//! Service 只编排元数据
//! （TRANSACTIONS.md：事务闭包内禁止文件 I/O）。

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS},
        HeaderValue, StatusCode,
    },
    response::Response,
    Extension, Json,
};
use serde::de::DeserializeOwned;
use services::{
    audit::AuditActor,
    file_asset::{
        AttachToDocumentRequest, DestroyFileAssetRequest, DocumentAttachmentView, FileAssetListItemView,
        FileAssetListParams, FileAssetService, FileAssetView, MarkScanResultRequest, PageView,
        PendingFileAssetRequest, RegisterFileAssetRequest,
    },
};
use tracing::error;

use crate::{
    app_state::AppState,
    core::{
        errors::{Error, Result},
        response::ApiResponse,
        upload,
    },
};

#[permission_macros::permission(
    group = "文件资产",
    group_desc = "文件元数据、单据附件与安全检查",
    desc = "查询文件资产列表",
    resource = "file_asset",
    action = "list"
)]
/// 查询文件资产列表（列表不暴露敏感对象存储键）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`file_name`/`security_scan_status` 等）
///
/// # 返回
/// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
pub async fn file_asset_list(
    State(state): State<AppState>,
    Query(params): Query<FileAssetListParams>,
) -> Result<PageView<FileAssetListItemView>> {
    let page = FileAssetService::new(state.db()).file_asset_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

#[permission_macros::permission(
    group = "文件资产",
    group_desc = "文件元数据、单据附件与安全检查",
    desc = "查询文件资产详情",
    resource = "file_asset",
    action = "detail"
)]
/// 查询文件资产详情；敏感资产不返回对象存储键或公开 URL。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 文件资产 ID
///
/// # 返回
/// 返回完整资产视图。
pub async fn file_asset_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<FileAssetView> {
    let mut view = FileAssetService::new(state.db()).file_asset_detail(&id).await?;
    prepare_asset_response(&state, &mut view);
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "文件资产",
    group_desc = "文件元数据、单据附件与安全检查",
    desc = "预览文件资产内容",
    resource = "file_asset",
    action = "preview"
)]
/// 在鉴权与审计后以内联内容响应返回文件资产；对象存储键不进入响应。
///
/// # Errors
/// 文件不存在、已拒绝/隔离、审计失败或对象存储读取失败时返回错误。
pub async fn file_asset_preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> std::result::Result<Response, Error> {
    let view = FileAssetService::new(state.db())
        .file_asset_preview(&id, &actor)
        .await?;
    if !matches!(
        view.content_type.as_str(),
        "image/jpeg" | "image/png" | "image/webp" | "application/pdf"
    ) {
        return Err(Error::Unprocessable("当前文件类型不支持在线预览".to_string()));
    }
    if matches!(
        view.security_scan_status,
        entities::file_asset::SecurityScanStatus::Rejected
            | entities::file_asset::SecurityScanStatus::Quarantined
    ) {
        return Err(Error::Unprocessable("文件未通过安全检查，不能预览".to_string()));
    }
    let content = state
        .storage()
        .read(&view.storage_object_key)
        .await
        .map_err(|storage_error| {
            error!(error = %storage_error, file_asset_id = %id, "Failed to read file asset preview");
            Error::Internal("Object storage operation failed".to_string())
        })?;
    let content_type = HeaderValue::from_str(&view.content_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    let mut response = Response::new(Body::from(content));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(CONTENT_TYPE, content_type);
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
        .headers_mut()
        .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}

#[permission_macros::permission(
    group = "文件资产",
    group_desc = "文件元数据、单据附件与安全检查",
    desc = "上传并登记文件资产",
    resource = "file_asset",
    action = "create"
)]
/// 上传文件并登记文件资产。
///
/// Multipart 表单：`file` 文件字段 + `sensitivity_class`/`retention_class`/
/// `expires_at`（可选）/`document_id`（可选，携带时同事务建立附件关联）/
/// `usage`（可选）。文件保存到启动时注入的 `storage::S3Storage`，
/// 内容指纹为带密钥 HMAC-SHA256（密钥取 `app.secret`，见最终报告协调事项）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `multipart` - Multipart 表单数据
///
/// # 返回
/// 返回新建的资产详情视图。
pub async fn file_asset_upload(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<FileAssetView> {
    let file = extract_asset_file(&mut multipart).await?;
    let fields = FileAssetFormFields::from_multipart(&mut multipart).await?;
    let request = store_asset_file(
        &state,
        file,
        fields.sensitivity_class,
        fields.retention_class,
        fields.expires_at,
    )
    .await?;
    let cleanup = [PendingFileAssetRequest {
        reference: "file-upload".to_string(),
        registration: request.clone(),
    }];
    let service = FileAssetService::new(state.db());
    let result = match fields.document_id {
        Some(document_id) => {
            service
                .register_file_asset_with_attachment(request, document_id, fields.usage, &actor)
                .await
        }
        None => service.register_file_asset(request, &actor).await,
    };
    let mut asset = match result {
        Ok(asset) => asset,
        Err(error) => {
            if should_compensate_pending_assets(&error) {
                delete_pending_asset_objects(&state, &cleanup).await;
            }
            return Err(error.into());
        }
    };
    prepare_asset_response(&state, &mut asset);

    Ok(ApiResponse::ok_with_data(asset))
}

/// 用存储层公开 URL 拼接规则生成对象访问地址。
///
/// # 参数
/// * `state` - 应用状态（提供 S3 存储客户端）
/// * `storage_object_key` - 对象存储键
///
/// # 返回
/// 返回浏览器可访问的公开 URL。
fn build_public_url(state: &AppState, storage_object_key: &str) -> String {
    match state.storage().public_url(storage_object_key) {
        Ok(url) => url,
        Err(storage_error) => {
            error!(
                error = %storage_error,
                object_key = %storage_object_key,
                "Failed to build S3 public URL; fallback to object key"
            );
            storage_object_key.to_string()
        }
    }
}

/// 仅普通资产暴露公开访问信息；敏感资产的底层对象键不进入 HTTP 响应。
fn prepare_asset_response(state: &AppState, view: &mut FileAssetView) {
    if view.sensitivity_class == entities::file_asset::SensitivityClass::General {
        view.public_url = Some(build_public_url(state, &view.storage_object_key));
        return;
    }
    view.storage_object_key.clear();
    view.content_hmac.clear();
    view.public_url = None;
}

#[permission_macros::permission(
    group = "文件资产",
    group_desc = "文件元数据、单据附件与安全检查",
    desc = "登记文件资产",
    resource = "file_asset",
    action = "create"
)]
/// 登记文件资产（元数据登记；文件已由调用方落盘到受控对象存储）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 登记请求（`{ storage_object_key, file_name, content_type, byte_size,
///   content_hmac, sensitivity_class, retention_class, expires_at }`）
///
/// # 返回
/// 返回新建的资产详情视图。
pub async fn file_asset_register(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RegisterFileAssetRequest>,
) -> Result<FileAssetView> {
    let mut view = FileAssetService::new(state.db())
        .register_file_asset(req, &actor)
        .await?;
    prepare_asset_response(&state, &mut view);

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "文件资产",
    group_desc = "文件元数据、单据附件与安全检查",
    desc = "建立单据附件关联",
    resource = "document_attachment",
    action = "create"
)]
/// 建立单据附件关联（安全检查、保留期与销毁状态不阻断关联）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 关联请求（`{ document_id, file_asset_id, usage }`）
///
/// # 返回
/// 返回新建的附件关联视图。
pub async fn document_attachment_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<AttachToDocumentRequest>,
) -> Result<DocumentAttachmentView> {
    let view = FileAssetService::new(state.db())
        .attach_to_document(req, &actor)
        .await?;

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "文件资产",
    group_desc = "文件元数据、单据附件与安全检查",
    desc = "查询单据附件列表",
    resource = "document_attachment",
    action = "list"
)]
/// 按业务单据查询附件关联。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 业务单据 ID
///
/// # 返回
/// 返回附件关联视图列表。
pub async fn document_attachment_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Vec<DocumentAttachmentView>> {
    let items = FileAssetService::new(state.db())
        .document_attachment_list(&entities::ids::BusinessDocumentId::new(id))
        .await?;

    Ok(ApiResponse::ok_with_data(items))
}

#[permission_macros::permission(
    group = "文件资产",
    group_desc = "文件元数据、单据附件与安全检查",
    desc = "记录安全检查结果",
    resource = "file_asset",
    action = "update"
)]
/// 记录安全检查结果（通过/拒绝/隔离；隔离后经人工复核可放行或拒绝）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 文件资产 ID
/// * `req` - 更新请求（含期望版本与检查结果）
///
/// # 返回
/// 返回更新后的资产视图。
pub async fn file_asset_scan_result(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<MarkScanResultRequest>,
) -> Result<FileAssetView> {
    let mut view = FileAssetService::new(state.db())
        .mark_scan_result(&id, req, &actor)
        .await?;
    prepare_asset_response(&state, &mut view);

    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "文件资产",
    group_desc = "文件元数据、单据附件与安全检查",
    desc = "销毁文件资产",
    resource = "file_asset",
    action = "delete"
)]
/// 销毁文件资产（销毁审计只记录一次；已销毁资产不得再用于业务关联）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 文件资产 ID
/// * `req` - 销毁请求（含期望版本）
///
/// # 返回
/// 返回销毁后的资产视图。
pub async fn file_asset_destroy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<DestroyFileAssetRequest>,
) -> Result<FileAssetView> {
    let mut view = FileAssetService::new(state.db())
        .destroy_file_asset(&id, req, &actor)
        .await?;
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

/// 一次解析 multipart 中的 JSON 命令和全部具名文件。
///
/// 字段顺序不构成协议；`command` 是唯一 JSON 字段，其余带文件名的字段名均
/// 作为临时文件引用。单文件沿用 5 MiB 上限，单次命令最多携带 32 个文件。
pub(crate) async fn extract_command_with_asset_files<T: DeserializeOwned>(
    multipart: &mut Multipart,
) -> std::result::Result<(T, Vec<PendingAssetFile>), Error> {
    const MAX_FILE_COUNT: usize = 32;

    let mut command = None;
    let mut files = Vec::new();
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| Error::BadRequest("Multipart 表单无效".to_string()))?
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
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|_| Error::BadRequest("上传文件读取失败".to_string()))?
            {
                if content.len().saturating_add(chunk.len()) > upload::MAX_UPLOAD_FILE_BYTES {
                    return Err(Error::BadRequest("上传文件大小不能超过 5 MiB".to_string()));
                }
                content.extend_from_slice(&chunk);
            }
            files.push(PendingAssetFile {
                reference: name,
                file: validate_asset_file(AssetFile {
                    file_name,
                    content_type,
                    content,
                })?,
            });
            continue;
        }
        if name != "command" {
            continue;
        }
        if command.is_some() {
            return Err(Error::BadRequest("业务命令不能重复".to_string()));
        }
        let text = field
            .text()
            .await
            .map_err(|_| Error::BadRequest("业务命令读取失败".to_string()))?;
        command = Some(
            serde_json::from_str::<T>(&text)
                .map_err(|_| Error::BadRequest("业务命令格式无效".to_string()))?,
        );
    }
    let command = command.ok_or_else(|| Error::BadRequest("缺少业务命令".to_string()))?;
    Ok((command, files))
}

/// 把一组已校验文件写入对象存储，并形成尚未登记的文件资产事务载荷。
///
/// 任一对象写入失败时删除本批次此前已经写入的对象。
pub(crate) async fn store_pending_asset_files(
    state: &AppState,
    files: Vec<PendingAssetFile>,
    sensitivity_for: impl Fn(&str) -> entities::file_asset::SensitivityClass,
) -> std::result::Result<Vec<PendingFileAssetRequest>, Error> {
    let mut requests = Vec::with_capacity(files.len());
    for pending in files {
        let request = match store_asset_file(
            state,
            pending.file,
            sensitivity_for(&pending.reference),
            entities::file_asset::RetentionClass::LongTerm,
            None,
        )
        .await
        {
            Ok(request) => request,
            Err(error) => {
                delete_pending_asset_objects(state, &requests).await;
                return Err(error);
            }
        };
        requests.push(PendingFileAssetRequest {
            reference: pending.reference,
            registration: request,
        });
    }
    Ok(requests)
}

/// 删除一次尚未登记或事务已回滚的上传批次，作为对象存储补偿。
pub(crate) async fn delete_pending_asset_objects(state: &AppState, requests: &[PendingFileAssetRequest]) {
    for request in requests {
        if let Err(storage_error) = state
            .storage()
            .delete(&request.registration.storage_object_key)
            .await
        {
            error!(
                error = %storage_error,
                object_key = %request.registration.storage_object_key,
                "Failed to compensate unregistered file object"
            );
        }
    }
}

/// 判断数据库失败是否已经确定回滚，因而可以安全删除刚上传的对象。
///
/// 提交结果未知时事务可能已经落库，必须保留对象并等待同一业务命令核对结果；
/// 其余错误由事务合同保证没有提交，可以立即执行对象存储补偿。
pub(crate) fn should_compensate_pending_assets(error: &services::Error) -> bool {
    !matches!(error, services::Error::OutcomeUnknown(_))
}

/// 提取 Multipart 表单中的第一个文件字段并校验大小与 MIME。
///
/// # 参数
/// * `multipart` - Multipart 表单数据
///
/// # 返回
/// 返回校验通过的文件。
///
/// # 错误
/// 无文件字段、文件超过 5 MiB 或缺少 MIME 类型时返回错误。
pub(crate) async fn extract_asset_file(multipart: &mut Multipart) -> std::result::Result<AssetFile, Error> {
    extract_asset_file_with_limit(multipart, upload::MAX_UPLOAD_FILE_BYTES).await
}

/// 按调用方业务上限提取首个文件，并继续执行扩展名、MIME 与真实文件头校验。
///
/// # 参数
/// * `multipart` - Multipart 表单数据
/// * `max_file_bytes` - 当前业务允许的单文件最大字节数
///
/// # 返回
/// 返回校验通过的文件。
///
/// # 错误
/// 无文件字段、文件超过业务上限或文件类型不受支持时返回错误。
pub(crate) async fn extract_asset_file_with_limit(
    multipart: &mut Multipart,
    max_file_bytes: usize,
) -> std::result::Result<AssetFile, Error> {
    let mut selected = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|_| Error::BadRequest("Multipart 表单无效".to_string()))?
    {
        let Some(file_name) = field.file_name().map(ToString::to_string) else {
            continue;
        };
        let file_name = file_name.trim().to_string();
        if file_name.is_empty() {
            continue;
        }
        let content_type = field
            .content_type()
            .map(ToString::to_string)
            .ok_or_else(|| Error::BadRequest("缺少文件 MIME 类型".to_string()))?;
        let mut content = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|_| Error::BadRequest("上传文件读取失败".to_string()))?
        {
            if content.len().saturating_add(chunk.len()) > max_file_bytes {
                return Err(Error::BadRequest(format!(
                    "上传文件大小不能超过 {} MiB",
                    max_file_bytes / (1024 * 1024)
                )));
            }
            content.extend_from_slice(&chunk);
        }
        selected = Some(validate_asset_file(AssetFile {
            file_name,
            content_type,
            content,
        })?);
        break;
    }
    selected.ok_or_else(|| Error::BadRequest("未上传文件".to_string()))
}

/// 把已校验文件写入对象存储并构造尚未落库的文件资产登记命令。
///
/// 数据库登记由调用方服务事务负责；调用方失败时必须删除返回命令中的对象键。
pub(crate) async fn store_asset_file(
    state: &AppState,
    file: AssetFile,
    sensitivity_class: entities::file_asset::SensitivityClass,
    retention_class: entities::file_asset::RetentionClass,
    expires_at: Option<u64>,
) -> std::result::Result<RegisterFileAssetRequest, Error> {
    let config = state.config_snapshot();
    let unique_name = storage_key_with_extension(id_generator::next_id(), &file.file_name);
    state
        .storage()
        .save_with_content_type(&unique_name, &file.content, Some(file.content_type.as_str()))
        .await
        .map_err(|storage_error| {
            error!(error = %storage_error, object_key = %unique_name, "Failed to save file asset to S3");
            Error::Internal("Object storage operation failed".to_string())
        })?;
    let content_hmac =
        entities::file_asset::content_fingerprint(&sha256_hex(&file.content), config.app.secret.as_bytes());
    Ok(RegisterFileAssetRequest {
        storage_object_key: unique_name,
        file_name: file.file_name,
        content_type: file.content_type,
        byte_size: file.content.len() as u64,
        content_hmac,
        sensitivity_class,
        retention_class,
        expires_at,
    })
}

/// 校验附件扩展名、声明 MIME 与真实文件头一致；支持受控图片和 PDF。
fn validate_asset_file(file: AssetFile) -> std::result::Result<AssetFile, Error> {
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
    pub sensitivity_class: entities::file_asset::SensitivityClass,
    /// 保留策略。
    pub retention_class: entities::file_asset::RetentionClass,
    /// 到期时间（秒级时间戳）。
    pub expires_at: Option<u64>,
    /// 业务单据 ID（携带时同事务建立附件关联）。
    pub document_id: Option<entities::ids::BusinessDocumentId>,
    /// 附件用途。
    pub usage: entities::file_asset::AttachmentUsage,
}

impl Default for FileAssetFormFields {
    /// 提供安全默认值（表单未携带治理字段时兜底；业务侧由调用方保证显式指定）。
    fn default() -> Self {
        Self {
            sensitivity_class: entities::file_asset::SensitivityClass::General,
            retention_class: entities::file_asset::RetentionClass::LongTerm,
            expires_at: None,
            document_id: None,
            usage: entities::file_asset::AttachmentUsage::Attachment,
        }
    }
}

impl FileAssetFormFields {
    /// 从 Multipart 表单读取治理与关联字段。
    ///
    /// # 参数
    /// * `multipart` - Multipart 表单数据
    ///
    /// # 返回
    /// 返回解析后的表单字段。
    ///
    /// # 错误
    /// 表单字段解析失败或枚举值非法时返回错误。
    pub(crate) async fn from_multipart(multipart: &mut Multipart) -> std::result::Result<Self, Error> {
        let mut fields = Self::default();
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|_| Error::BadRequest("Multipart 表单无效".to_string()))?
        {
            let name = field.name().unwrap_or_default().to_string();
            if field.file_name().is_some() || name.is_empty() {
                continue;
            }
            let text = field
                .text()
                .await
                .map_err(|_| Error::BadRequest("Multipart 表单无效".to_string()))?;
            match name.as_str() {
                "sensitivity_class" => {
                    fields.sensitivity_class = serde_json::from_value(serde_json::Value::String(text))
                        .map_err(|_| Error::BadRequest("敏感级别非法".to_string()))?;
                }
                "retention_class" => {
                    fields.retention_class = serde_json::from_value(serde_json::Value::String(text))
                        .map_err(|_| Error::BadRequest("保留策略非法".to_string()))?;
                }
                "expires_at" => {
                    fields.expires_at = Some(
                        text.parse::<u64>()
                            .map_err(|_| Error::BadRequest("到期时间非法".to_string()))?,
                    );
                }
                "document_id" => {
                    fields.document_id = Some(entities::ids::BusinessDocumentId::new(text));
                }
                "usage" => {
                    fields.usage = serde_json::from_value(serde_json::Value::String(text))
                        .map_err(|_| Error::BadRequest("附件用途非法".to_string()))?;
                }
                _ => {}
            }
        }
        Ok(fields)
    }
}

/// 计算内容的 SHA-256 十六进制摘要（作为 HMAC 的明文形态）。
///
/// # 参数
/// * `content` - 文件内容
///
/// # 返回
/// 返回小写十六进制摘要。
fn sha256_hex(content: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 生成对象存储键：对受支持图片追加小写扩展名，便于 CDN 和浏览器识别资源类型。
///
/// 对象的 `Content-Type` 由 S3 `PutObject` 元数据明确写入；扩展名只作为 URL 可读性与
/// CDN 兼容性辅助，非图片文件保持原有无扩展名键。
///
/// # 参数
/// * `id` - 不可猜测的唯一对象标识
/// * `file_name` - 上传文件展示名
///
/// # 返回
/// 返回对象存储键。
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
    use super::{
        should_compensate_pending_assets, storage_key_with_extension, validate_asset_file, AssetFile,
    };

    #[test]
    fn supported_image_names_keep_normalized_extension() {
        assert_eq!(
            storage_key_with_extension("id-1".to_string(), "photo.JpG"),
            "id-1.jpg"
        );
        assert_eq!(
            storage_key_with_extension("id-2".to_string(), "photo.png"),
            "id-2.png"
        );
    }

    #[test]
    fn unsupported_or_extensionless_names_stay_unchanged() {
        assert_eq!(
            storage_key_with_extension("id-3".to_string(), "report.pdf"),
            "id-3"
        );
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
        let database_error =
            database::Error::CommitOutcomeUnknown(mongodb::error::Error::custom("unknown commit result"));
        let unknown = services::Error::OutcomeUnknown(database_error);
        let definite = services::Error::ConflictError("已确定回滚".to_string());

        assert!(!should_compensate_pending_assets(&unknown));
        assert!(should_compensate_pending_assets(&definite));
    }
}
