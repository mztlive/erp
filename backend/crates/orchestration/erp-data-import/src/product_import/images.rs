//! 从报价表提取图片并登记为待写入文件资产。

use std::sync::Arc;

use erp_core::ids::FileAssetId;
use erp_support::{
    PENDING_FILE_REFERENCE_PREFIX, PendingFileAssetRequest, RegisterFileAssetRequest, RetentionClass,
    SensitivityClass, content_fingerprint,
};
use id_generator::next_id;
use storage::S3Storage;

use super::media::{PreparedImage, WorkbookMedia, row_media_targets};
use super::parse::ParsedProductSheet;
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
        xlsx: &'a Arc<Vec<u8>>,
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
    xlsx: &Arc<Vec<u8>>,
    sheet: &ParsedProductSheet,
    cells: &[String],
) -> Result<RowMedia> {
    let mut reader = WorkbookMedia::open(xlsx.clone()).await?;
    let mut media = RowMedia::default();
    let mut sort_order = 0;
    for (column, path) in row_media_targets(cells, sheet) {
        let (next_reader, image) = reader.read(path).await?;
        reader = next_reader;
        let Some(image) = image else {
            continue;
        };
        let carousel_pending = store_image(storage, secret, &image).await?;
        let reference = FileAssetId::new(carousel_pending.reference.clone());
        media.carousel.push((reference, sort_order));
        sort_order += 1;
        media.pending.push(carousel_pending);
        if column == 2 {
            let sku_pending = store_image(storage, secret, &image).await?;
            media.main_image = Some(FileAssetId::new(sku_pending.reference.clone()));
            media.pending.push(sku_pending);
        }
    }
    Ok(media)
}

/// 上传一个独立图片对象，复用阻塞任务生成的摘要构造待登记事实。
async fn store_image(
    storage: &S3Storage,
    secret: &[u8],
    image: &PreparedImage,
) -> Result<PendingFileAssetRequest> {
    let token = next_id();
    let ext = image.extension;
    let object_key = format!("{token}.{ext}");
    storage
        .save_with_content_type(&object_key, &image.bytes, Some(image.content_type))
        .await
        .map_err(|error| Error::Internal(format!("保存商品图片失败: {error}")))?;
    Ok(PendingFileAssetRequest {
        reference: format!("{PENDING_FILE_REFERENCE_PREFIX}{token}"),
        registration: RegisterFileAssetRequest {
            storage_object_key: object_key,
            file_name: format!("product-import.{ext}"),
            content_type: image.content_type.to_string(),
            byte_size: u64::try_from(image.bytes.len()).expect("图片字节数不超过 u64 范围"),
            content_hmac: content_fingerprint(&image.digest, secret),
            sensitivity_class: SensitivityClass::General,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
        },
    })
}
