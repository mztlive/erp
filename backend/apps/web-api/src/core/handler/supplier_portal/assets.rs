//! 门户素材仅通过原申请授权及受控字节下载；对象存储地址不进入外部响应。

use std::io::Cursor;
use std::result::Result as StdResult;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, Path, Query, State};
use axum::response::Response;
use erp_identity::PortalActor;
use erp_processes::supplier_portal::{PortalAssetView, PreparedPortalAsset, SupplierPortalProcess};
use erp_read_models::supplier_portal::PortalSkuImageSource;
use erp_support::{
    FileAssetView, PendingFileAssetRequest, RegisterFileAssetRequest, RetentionClass, SensitivityClass,
    content_fingerprint,
};
use image::{ImageFormat, ImageReader, Limits};
use jwt_sha2::{Digest, Sha256};
use persistence_core::NoTransaction;
use serde::Deserialize;
use tokio::task::spawn_blocking;
use tracing::error;
use zune_core::bytestream::ZCursor;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use super::asset_pdf::inspect_pdf;
use super::process;
use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::handler::file_asset::{
    asset_download_response, finish_asset_command, read_asset, revalidate_asset,
};
use crate::core::response::ApiResponse;
use crate::core::upload::{ValidatedPortalAsset, extract_file};

/// 将上传协议的查询字段拆成过程参数，不接受客户端文件治理元数据。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadQuery {
    /// 新品原稿当前版本或需恢复原结果的提交版本。
    pub expected_version: u64,
    /// 此文件操作的稳定操作号。
    pub idempotency_key: String,
}

/// 下载协议通过申请、自有供给或定向开放SKU证明唯一来源。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DownloadQuery {
    /// 本供应商申请及其原稿历史。
    pub request_id: Option<String>,
    /// 本供应商正式供给。
    pub offering_id: Option<String>,
    /// 当前定向开放的公司SKU。
    pub sku_id: Option<String>,
}

const MAX_IMAGE_DIMENSION: u32 = 8192;
const MAX_IMAGE_DECODE_BYTES: u64 = 64 * 1024 * 1024;

enum AssetSource {
    Request(String),
    Offering(String),
    Sku(String),
}

struct AuthorizedAsset {
    view: FileAssetView,
    catalog_source: Option<(PortalSkuImageSource, Option<u64>)>,
}

impl TryFrom<DownloadQuery> for AssetSource {
    type Error = Error;

    fn try_from(query: DownloadQuery) -> StdResult<Self, Error> {
        match (query.request_id, query.offering_id, query.sku_id) {
            (Some(id), None, None) => Ok(Self::Request(id)),
            (None, Some(id), None) => Ok(Self::Offering(id)),
            (None, None, Some(id)) => Ok(Self::Sku(id)),
            _ => Err(Error::BadRequest("请通过对应申请或商品下载图片".into())),
        }
    }
}

impl AssetSource {
    async fn access(
        &self,
        portal: &SupplierPortalProcess,
        file_id: &str,
        actor: &PortalActor,
    ) -> StdResult<AuthorizedAsset, Error> {
        match self {
            Self::Request(id) => Ok(AuthorizedAsset {
                view: portal.asset_access(id, file_id, actor, &mut NoTransaction).await?.into(),
                catalog_source: None,
            }),
            Self::Offering(id) => {
                let access =
                    portal.asset_catalog_access(file_id, Some(id), None, actor, &mut NoTransaction).await?;
                Ok(AuthorizedAsset {
                    view: access.asset.into(),
                    catalog_source: Some((access.source, access.offering_version)),
                })
            },
            Self::Sku(id) => {
                let access =
                    portal.asset_catalog_access(file_id, None, Some(id), actor, &mut NoTransaction).await?;
                Ok(AuthorizedAsset {
                    view: access.asset.into(),
                    catalog_source: Some((access.source, access.offering_version)),
                })
            },
        }
    }
}

/// 上传真实内容已检查的图片或静态 PDF，并原子登记新品原稿关联。
///
/// # 参数
/// 当前门户身份、原申请、原版本和操作号，以及唯一图片或资料文件。
/// # 返回
/// 文件 ID、版本及申请新版本；已完成操作恢复其原结果。
/// # 错误
/// 身份或来源无效、版本变化、非法内容、存储失败或登记失败时拒绝。
pub async fn upload(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Path(request_id): Path<String>,
    Query(query): Query<UploadQuery>,
    mut multipart: Multipart,
) -> Result<PortalAssetView> {
    let portal = process(&state);
    portal
        .asset_precheck_command(
            &request_id,
            query.expected_version,
            &query.idempotency_key,
            &actor,
            &mut NoTransaction,
        )
        .await?;
    let asset = extract_file(&mut multipart)
        .await?
        .ok_or_else(|| Error::BadRequest("未上传文件，请选择图片或 PDF 资料".into()))?
        .validate_portal_asset()?;
    let (asset, checked) = check_prepared_asset(asset).await?;
    let registration = asset_registration(&asset, state.config_snapshot().app.secret.as_bytes())?;
    let data = registration.clone().into_data(&actor.account_id)?;
    if let Some(result) = portal
        .asset_upload_replay(&request_id, query.expected_version, &data, &query.idempotency_key, &actor)
        .await?
    {
        return Ok(ApiResponse::ok_with_data(result));
    }
    let prepared = match checked {
        CheckedContent::Image(width, height) => PreparedPortalAsset::from_decoded_image(data, width, height)?,
        CheckedContent::Pdf(pages) => PreparedPortalAsset::from_checked_pdf(data, pages)?,
    };
    let result = persist_asset(&state, &actor, &request_id, query, registration, prepared, asset).await?;
    Ok(ApiResponse::ok_with_data(result))
}

/// 按当前申请及历史原稿引用下载完整受控文件字节。
///
/// # 参数
/// 当前门户身份、文件 ID 及其唯一申请、供给或开放SKU来源。
/// # 返回
/// 带禁止缓存和内容嗅探响应头的下载响应。
/// # 错误
/// 归属变化、版本变化、治理状态禁止或存储内容与登记不符时拒绝。
pub async fn download(
    State(state): State<AppState>,
    Extension(actor): Extension<PortalActor>,
    Path(file_id): Path<String>,
    Query(query): Query<DownloadQuery>,
) -> StdResult<Response, Error> {
    let portal = process(&state);
    let source = AssetSource::try_from(query)?;
    let before = source.access(&portal, &file_id, &actor).await?;
    require_portal_type(&before.view.content_type)?;
    let (view, bytes) = read_asset(&state, &actor.audit_actor(), &file_id).await?;
    require_same_asset(&before.view, &view)?;
    let after = source.access(&portal, &file_id, &actor).await?;
    require_same_asset(&view, &after.view)?;
    if before.catalog_source != after.catalog_source {
        return Err(Error::Conflict("商品图片来源已变化，请刷新后重新下载".into()));
    }
    revalidate_asset(&state, &view).await?;
    asset_download_response(&view, bytes)
}

/// 下载当前专项审核资格允许查看的供应商原稿图片与资料。
///
/// # 参数
/// 真实后台操作人、当前申请与原稿明确引用的文件。
/// # 返回
/// 完整受控文件字节及禁止缓存、嗅探的响应头。
/// # 错误
/// 专项任务资格失效、来源不符、版本变化或文件治理禁止时拒绝。
#[permission_macros::permission(
    group = "供应商门户管理",
    group_desc = "管理外部账号、定向报价与专项申请",
    desc = "下载供应商专项审核素材",
    resource = "supplier_portal_request",
    action = "detail"
)]
pub async fn review_download(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((request_id, file_id)): Path<(String, String)>,
) -> StdResult<Response, Error> {
    let portal = process(&state);
    let before: FileAssetView =
        portal.asset_review_access(&request_id, &file_id, &actor, &mut NoTransaction).await?.into();
    require_portal_type(&before.content_type)?;
    let (view, bytes) = read_asset(&state, &actor, &file_id).await?;
    require_same_asset(&before, &view)?;
    let after: FileAssetView =
        portal.asset_review_access(&request_id, &file_id, &actor, &mut NoTransaction).await?.into();
    require_same_asset(&view, &after)?;
    revalidate_asset(&state, &view).await?;
    asset_download_response(&view, bytes)
}

/// 保存对象后提交原子元数据命令，按明确提交结果清理本次独立对象。
async fn persist_asset(
    state: &AppState,
    actor: &PortalActor,
    request_id: &str,
    query: UploadQuery,
    registration: RegisterFileAssetRequest,
    prepared: PreparedPortalAsset,
    asset: ValidatedPortalAsset,
) -> StdResult<PortalAssetView, Error> {
    state
        .storage()
        .save_with_content_type(&registration.storage_object_key, asset.content(), Some(asset.content_type()))
        .await
        .map_err(|storage_error| {
            error!(error = %storage_error, "Failed to save supplier portal asset");
            Error::Internal("文件保存失败，请重试".into())
        })?;
    let pending = PendingFileAssetRequest { reference: "portal-asset".into(), registration };
    let result = process(state)
        .asset_register(request_id, query.expected_version, prepared, &query.idempotency_key, actor)
        .await;
    let result = finish_asset_command(state, &[pending], result, |view| view.assets_committed).await?;
    Ok(result)
}

enum CheckedContent {
    Image(u32, u32),
    Pdf(u32),
}

/// 完整内容检查放在阻塞线程；保留供应商原始文件，不自行重编码。
async fn check_prepared_asset(
    asset: ValidatedPortalAsset,
) -> StdResult<(ValidatedPortalAsset, CheckedContent), Error> {
    spawn_blocking(move || {
        let checked = if asset.content_type() == "application/pdf" {
            CheckedContent::Pdf(inspect_pdf(asset.content())?)
        } else {
            let (width, height) = decode_image(asset.content(), asset.content_type())?;
            CheckedContent::Image(width, height)
        };
        Ok((asset, checked))
    })
    .await
    .map_err(|error| {
        error!(error = %error, "Supplier portal asset content checker failed");
        Error::Internal("文件检查失败，请重新选择文件".into())
    })?
}

/// 用当前验证的MIME选解码器，并限制边长与解码分配预算。
fn decode_image(bytes: &[u8], content_type: &str) -> StdResult<(u32, u32), Error> {
    decode_image_with_budget(bytes, content_type, MAX_IMAGE_DECODE_BYTES)
}

pub(super) fn decode_image_with_budget(
    bytes: &[u8],
    content_type: &str,
    allocation_budget: u64,
) -> StdResult<(u32, u32), Error> {
    let format = image_format(content_type)?;
    if format == ImageFormat::Jpeg {
        return decode_jpeg(bytes, allocation_budget);
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(allocation_budget);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| invalid_image())?;
    Ok((decoded.width(), decoded.height()))
}

/// JPEG 仅用标量解码路径；先验尺寸及像素预算，再完整解码并验证输出。
fn decode_jpeg(bytes: &[u8], allocation_budget: u64) -> StdResult<(u32, u32), Error> {
    // 基线解码器允许缺失最终 EOI，门户图片须保留完整的结束标记。
    if !bytes.starts_with(&[0xff, 0xd8]) || !bytes.ends_with(&[0xff, 0xd9]) {
        return Err(invalid_image());
    }
    let dimension_limit = usize::try_from(MAX_IMAGE_DIMENSION).map_err(|_| invalid_image())?;
    let options = DecoderOptions::new_safe()
        .set_use_unsafe(false)
        .set_strict_mode(true)
        .set_max_width(dimension_limit)
        .set_max_height(dimension_limit);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().map_err(|_| invalid_image())?;
    let (width, height) = decoder.dimensions().ok_or_else(invalid_image)?;
    if width == 0 || height == 0 || width > dimension_limit || height > dimension_limit {
        return Err(invalid_image());
    }
    let expected_size = decoder.output_buffer_size().ok_or_else(invalid_image)?;
    if expected_size == 0 || u64::try_from(expected_size).map_err(|_| invalid_image())? > allocation_budget {
        return Err(invalid_image());
    }
    let decoded = decoder.decode().map_err(|_| invalid_image())?;
    if decoded.len() != expected_size {
        return Err(invalid_image());
    }
    Ok((
        u32::try_from(width).map_err(|_| invalid_image())?,
        u32::try_from(height).map_err(|_| invalid_image())?,
    ))
}

fn invalid_image() -> Error {
    Error::BadRequest("图片内容损坏或尺寸过大，请重新选择图片".into())
}

fn image_format(content_type: &str) -> StdResult<ImageFormat, Error> {
    match content_type {
        "image/jpeg" => Ok(ImageFormat::Jpeg),
        "image/png" => Ok(ImageFormat::Png),
        "image/webp" => Ok(ImageFormat::WebP),
        _ => Err(Error::BadRequest("请选择 JPEG、PNG 或 WebP 图片".into())),
    }
}

/// 安全文件名、真实内容指纹及长期保留策略全部由服务端生成。
fn asset_registration(
    asset: &ValidatedPortalAsset,
    secret: &[u8],
) -> StdResult<RegisterFileAssetRequest, Error> {
    let digest: String = Sha256::digest(asset.content()).iter().map(|byte| format!("{byte:02x}")).collect();
    let name = asset.unique_name();
    Ok(RegisterFileAssetRequest {
        storage_object_key: name.clone(),
        file_name: name,
        content_type: asset.content_type().into(),
        byte_size: u64::try_from(asset.content().len())
            .map_err(|_| Error::BadRequest("文件大小无效，请重新选择".into()))?,
        content_hmac: content_fingerprint(&digest, secret),
        sensitivity_class: SensitivityClass::Sensitive,
        retention_class: RetentionClass::LongTerm,
        expires_at: None,
    })
}

fn require_portal_type(content_type: &str) -> StdResult<(), Error> {
    if content_type == "application/pdf" {
        return Ok(());
    }
    image_format(content_type).map(|_| ())
}

/// 来源授权、存储读取及后置来源授权必须指向同一文件版本和内容。
fn require_same_asset(before: &FileAssetView, after: &FileAssetView) -> StdResult<(), Error> {
    if before.id != after.id
        || before.version != after.version
        || before.storage_object_key != after.storage_object_key
        || before.content_hmac != after.content_hmac
    {
        return Err(Error::Conflict("文件已变化，请刷新后重新下载".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use image::codecs::png::PngEncoder;
    use image::codecs::webp::WebPEncoder;
    use image::{ExtendedColorType, ImageEncoder, RgbImage};

    use super::*;

    #[test]
    fn upload_query_requires_original_version_and_rejects_metadata_injection() {
        assert!(
            serde_json::from_value::<UploadQuery>(serde_json::json!({
                "expected_version": 1, "idempotency_key": "upload-1"
            }))
            .is_ok()
        );
        assert!(
            serde_json::from_value::<UploadQuery>(serde_json::json!({
                "idempotency_key": "upload-1"
            }))
            .is_err()
        );
        for field in ["supplier_id", "created_by", "content_hmac", "storage_object_key"] {
            let mut value = serde_json::json!({"expected_version": 1, "idempotency_key": "upload-1"});
            value[field] = serde_json::json!("injected");
            assert!(serde_json::from_value::<UploadQuery>(value).is_err());
        }
    }

    #[test]
    fn download_requires_request_source_and_upload_formats_are_downloadable() {
        let no_source: DownloadQuery = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(AssetSource::try_from(no_source).is_err());
        assert!(
            serde_json::from_value::<DownloadQuery>(serde_json::json!({
                "request_id": "request-1", "supplier_id": "another"
            }))
            .is_err()
        );
        for source in ["request_id", "offering_id", "sku_id"] {
            let mut value = serde_json::json!({});
            value[source] = serde_json::json!("source-1");
            assert!(AssetSource::try_from(serde_json::from_value::<DownloadQuery>(value).unwrap()).is_ok());
        }
        let ambiguous: DownloadQuery = serde_json::from_value(serde_json::json!({
            "request_id": "request-1", "offering_id": "offering-1"
        }))
        .unwrap();
        assert!(AssetSource::try_from(ambiguous).is_err());
        for mime in ["image/jpeg", "image/png", "image/webp"] {
            assert!(image_format(mime).is_ok());
        }
        assert!(image_format("image/gif").is_err());
        assert!(image_format("image/svg+xml").is_err());
    }

    fn encoded_image(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        if format == ImageFormat::Jpeg {
            // 固定真实 JPEG 样本适配本地 Cranelift 测试工具链。
            assert_eq!((width, height), (2, 2));
            return include_bytes!("fixtures/black-2x2.jpg").to_vec();
        }
        let mut cursor = Cursor::new(Vec::new());
        let pixels = RgbImage::new(width, height);
        match format {
            ImageFormat::Png => PngEncoder::new(&mut cursor)
                .write_image(pixels.as_raw(), width, height, ExtendedColorType::Rgb8)
                .unwrap(),
            ImageFormat::WebP => WebPEncoder::new_lossless(&mut cursor)
                .write_image(pixels.as_raw(), width, height, ExtendedColorType::Rgb8)
                .unwrap(),
            _ => panic!("unsupported fixture format"),
        }
        cursor.into_inner()
    }

    #[test]
    fn complete_image_decode_accepts_real_pixels_and_rejects_forged_headers() {
        for (format, mime, header) in [
            (ImageFormat::Png, "image/png", &b"\x89PNG\r\n\x1a\n"[..]),
            (ImageFormat::Jpeg, "image/jpeg", &b"\xff\xd8\xff\xe0"[..]),
            (ImageFormat::WebP, "image/webp", &b"RIFF\x04\x00\x00\x00WEBP"[..]),
        ] {
            assert_eq!(decode_image(&encoded_image(2, 2, format), mime).unwrap(), (2, 2));
            assert!(decode_image(header, mime).is_err());
        }
        let image = encoded_image(2, 2, ImageFormat::Png);
        assert!(decode_image(&image, "image/jpeg").is_err());
        assert!(decode_image(&image[..image.len() / 2], "image/png").is_err());
    }

    #[test]
    fn image_decode_rejects_dimensions_over_the_server_limit() {
        let oversized = encoded_image(MAX_IMAGE_DIMENSION + 1, 1, ImageFormat::Png);
        assert!(decode_image(&oversized, "image/png").is_err());
    }

    #[test]
    fn complete_decode_respects_the_output_allocation_budget() {
        for (format, mime) in [(ImageFormat::Png, "image/png"), (ImageFormat::Jpeg, "image/jpeg")] {
            let image = encoded_image(2, 2, format);
            assert!(decode_image_with_budget(&image, mime, 1).is_err());
            assert_eq!(decode_image(&image, mime).unwrap(), (2, 2));
        }
    }

    #[test]
    fn jpeg_decode_requires_complete_content_and_bounded_nonzero_dimensions() {
        let jpeg = encoded_image(2, 2, ImageFormat::Jpeg);
        assert!(decode_image(&jpeg[..jpeg.len() - 2], "image/jpeg").is_err());
        let mut truncated = jpeg[..jpeg.len() / 2].to_vec();
        truncated.extend_from_slice(&[0xff, 0xd9]);
        assert!(decode_image(&truncated, "image/jpeg").is_err());
        assert!(decode_image(&jpeg, "image/png").is_err());
        let frame_start = jpeg.windows(2).position(|marker| marker == [0xff, 0xc0]).unwrap();
        let mut oversized = jpeg.clone();
        oversized[frame_start + 7..frame_start + 9]
            .copy_from_slice(&u16::try_from(MAX_IMAGE_DIMENSION + 1).unwrap().to_be_bytes());
        assert!(decode_image(&oversized, "image/jpeg").is_err());
        let mut empty = jpeg;
        empty[frame_start + 5..frame_start + 7].copy_from_slice(&[0, 0]);
        assert!(decode_image(&empty, "image/jpeg").is_err());
    }

    #[test]
    fn file_authorization_and_byte_reads_must_use_the_same_version_and_content() {
        let request = RegisterFileAssetRequest {
            storage_object_key: "image-object.png".into(),
            file_name: "image-object.png".into(),
            content_type: "image/png".into(),
            byte_size: 32,
            content_hmac: content_fingerprint("image-digest", b"secret"),
            sensitivity_class: SensitivityClass::Sensitive,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
        };
        let before: FileAssetView = erp_support::FileAsset::new(
            erp_core::ids::FileAssetId::new("image-1"),
            request.into_data("supplier-account-1").unwrap(),
        )
        .unwrap()
        .into();
        assert!(require_same_asset(&before, &before).is_ok());
        let mutations: [fn(&mut FileAssetView); 4] = [
            |view| view.id = "another-image".into(),
            |view| view.version += 1,
            |view| view.storage_object_key = "another-object".into(),
            |view| view.content_hmac = content_fingerprint("changed-image", b"secret"),
        ];
        for mutate in mutations {
            let mut after = before.clone();
            mutate(&mut after);
            assert!(require_same_asset(&before, &after).is_err());
        }
    }
}
