//! 域 D05 `file_asset` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `services::file_asset` 的 DTO，禁止重复定义同构类型、禁止直连数据库。
//!
//! 上传接口参照 `core/upload.rs` 既有模式：S3 对象写入在 Service 之外完成，
//! Service 只编排元数据
//! （TRANSACTIONS.md：事务闭包内禁止文件 I/O）。

use axum::{
    extract::{Multipart, Path, Query, State},
    Extension, Json,
};
use services::{
    audit::AuditActor,
    file_asset::{
        AttachToDocumentRequest, DestroyFileAssetRequest, DocumentAttachmentView, FileAssetListItemView,
        FileAssetListParams, FileAssetService, FileAssetView, MarkScanResultRequest, PageView,
        RegisterFileAssetRequest,
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
    let request = RegisterFileAssetRequest {
        storage_object_key: unique_name,
        file_name: file.file_name,
        content_type: file.content_type,
        byte_size: file.content.len() as u64,
        content_hmac,
        sensitivity_class: fields.sensitivity_class,
        retention_class: fields.retention_class,
        expires_at: fields.expires_at,
    };
    let service = FileAssetService::new(state.db());
    let mut asset = service.register_file_asset(request, &actor).await?;
    prepare_asset_response(&state, &mut asset);
    if let Some(document_id) = fields.document_id {
        service
            .attach_to_document(
                AttachToDocumentRequest {
                    document_id,
                    file_asset_id: entities::ids::FileAssetId::new(asset.id.clone()),
                    usage: fields.usage,
                },
                &actor,
            )
            .await?;
    }

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
async fn extract_asset_file(multipart: &mut Multipart) -> std::result::Result<AssetFile, Error> {
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
            if content.len().saturating_add(chunk.len()) > upload::MAX_UPLOAD_FILE_BYTES {
                return Err(Error::BadRequest("上传文件大小不能超过 5 MiB".to_string()));
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
    use super::{storage_key_with_extension, validate_asset_file, AssetFile};

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
}
