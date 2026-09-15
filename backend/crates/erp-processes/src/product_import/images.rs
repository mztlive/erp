//! 从报价表提取图片并登记为待写入文件资产。

use erp_catalog::entity::catalog::product_import::dispimg_id;
use erp_core::ids::FileAssetId;
use erp_support::{
    PENDING_FILE_REFERENCE_PREFIX, PendingFileAssetRequest, RegisterFileAssetRequest, RetentionClass,
    SensitivityClass, content_fingerprint,
};
use id_generator::next_id;
use sha2::{Digest, Sha256};
use storage::S3Storage;

use super::parse::{ParsedProductSheet, read_xlsx_media};
use crate::{Error, Result};

/// 一行已上传的媒体引用。
#[derive(Debug, Clone, Default)]
pub struct RowMedia {
    /// 主图临时引用。
    pub main_image: Option<FileAssetId>,
    /// 轮播/详情图临时引用。
    pub carousel: Vec<(FileAssetId, i32)>,
    /// 待登记文件。
    pub pending: Vec<PendingFileAssetRequest>,
}

/// 行媒体来源：清单复用零上传，回退链路沿用源文件提取。
#[derive(Debug, Clone, Copy)]
pub(super) enum RowMediaSource<'a> {
    /// 清单预提媒体（对象已在存储中，直接复用）。
    Manifest(&'a RowMedia),
    /// 源文件提取（老任务回退链路）。
    Workbook {
        /// 源文件字节。
        xlsx: &'a [u8],
        /// 解析结果。
        sheet: &'a ParsedProductSheet,
    },
}

/// 按来源获取行媒体；清单来源不产生任何上传。
///
/// # 参数
/// * `storage` - 对象存储（仅源文件来源使用）
/// * `secret` - 内容指纹密钥（仅源文件来源使用）
/// * `cells` - 当前行单元格（仅源文件来源使用）
/// * `source` - 行媒体来源
///
/// # 返回
/// 返回临时文件引用与待登记资产。
///
/// # 错误
/// 源文件来源上传失败时返回错误；清单来源无错误。
pub(super) async fn resolve_row_media(
    storage: &S3Storage,
    secret: &[u8],
    cells: &[String],
    source: &RowMediaSource<'_>,
) -> Result<RowMedia> {
    match source {
        RowMediaSource::Manifest(media) => Ok((*media).clone()),
        RowMediaSource::Workbook { xlsx, sheet } => {
            upload_row_images(storage, secret, xlsx, sheet, cells).await
        },
    }
}

/// 为指定行提取并上传主图、副图。
///
/// # 参数
/// * `storage` - 对象存储
/// * `secret` - 内容指纹密钥
/// * `xlsx` - 源文件字节
/// * `sheet` - 已解析工作表
/// * `cells` - 当前行单元格
///
/// # 返回
/// 返回临时文件引用与待登记资产。
///
/// # 错误
/// 对象存储写入失败时返回错误；找不到图片时跳过该图。
pub async fn upload_row_images(
    storage: &S3Storage,
    secret: &[u8],
    xlsx: &[u8],
    sheet: &ParsedProductSheet,
    cells: &[String],
) -> Result<RowMedia> {
    let mut media = RowMedia::default();
    let mut sort_order = 0;
    for (index, cell) in cells.iter().enumerate().take(5).skip(2) {
        let Some(image_id) = dispimg_id(cell) else {
            continue;
        };
        let Some(path) = sheet.image_targets.get(image_id) else {
            continue;
        };
        let Ok(bytes) = read_xlsx_media(xlsx, path) else {
            continue;
        };
        let Some((content_type, ext)) = detect_image(&bytes) else {
            continue;
        };
        if index == 2 {
            let carousel_pending = store_image(storage, secret, bytes.clone(), content_type, ext).await?;
            let carousel_reference = FileAssetId::new(carousel_pending.reference.clone());
            media.carousel.push((carousel_reference, sort_order));
            sort_order += 1;
            media.pending.push(carousel_pending);
            let sku_pending = store_image(storage, secret, bytes, content_type, ext).await?;
            media.main_image = Some(FileAssetId::new(sku_pending.reference.clone()));
            media.pending.push(sku_pending);
            continue;
        }
        let pending = store_image(storage, secret, bytes, content_type, ext).await?;
        let reference = FileAssetId::new(pending.reference.clone());
        media.carousel.push((reference, sort_order));
        sort_order += 1;
        media.pending.push(pending);
    }
    Ok(media)
}

async fn store_image(
    storage: &S3Storage,
    secret: &[u8],
    bytes: Vec<u8>,
    content_type: &'static str,
    ext: &str,
) -> Result<PendingFileAssetRequest> {
    let token = next_id();
    let object_key = format!("{token}.{ext}");
    storage
        .save_with_content_type(&object_key, &bytes, Some(content_type))
        .await
        .map_err(|error| Error::Internal(format!("保存商品图片失败: {error}")))?;
    let digest = hex::encode(Sha256::digest(&bytes));
    Ok(PendingFileAssetRequest {
        reference: format!("{PENDING_FILE_REFERENCE_PREFIX}{token}"),
        registration: RegisterFileAssetRequest {
            storage_object_key: object_key,
            file_name: format!("product-import.{ext}"),
            content_type: content_type.to_string(),
            byte_size: bytes.len() as u64,
            content_hmac: content_fingerprint(&digest, secret),
            sensitivity_class: SensitivityClass::General,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
        },
    })
}

pub(super) fn detect_image(content: &[u8]) -> Option<(&'static str, &'static str)> {
    if content.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(("image/png", "png"));
    }
    if content.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(("image/jpeg", "jpg"));
    }
    if content.starts_with(b"GIF87a") || content.starts_with(b"GIF89a") {
        return Some(("image/gif", "gif"));
    }
    if content.len() >= 12 && content.starts_with(b"RIFF") && &content[8..12] == b"WEBP" {
        return Some(("image/webp", "webp"));
    }
    None
}
