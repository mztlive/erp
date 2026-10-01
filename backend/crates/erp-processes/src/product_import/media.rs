//! 有界提取报价表媒体；ZIP 目录、近期媒体与摘要在阻塞工作中复用。

use std::collections::VecDeque;
use std::io::{Cursor, Read};
use std::sync::Arc;

use erp_catalog::entity::catalog::product_import::dispimg_id;
use sha2::{Digest, Sha256};
use tokio::task::spawn_blocking;
use zip::ZipArchive;

use super::parse::ParsedProductSheet;
use crate::{Error, Result};

/// 近期图片缓存同时受条数与解压后字节数限制；超大图片仅保留当前行引用。
const MEDIA_CACHE_BYTES: usize = 8 * 1024 * 1024;
/// 缺失或无法识别的条目也占缓存条数，避免缓存无限增长。
const MEDIA_CACHE_ENTRIES: usize = 16;

/// 一张完成格式检测与摘要计算的图片；同路径的多个单元格共享字节。
pub(super) struct PreparedImage {
    /// 已解压的原始图片。
    pub bytes: Vec<u8>,
    /// 识别出的 MIME。
    pub content_type: &'static str,
    /// 与原实现一致的扩展名。
    pub extension: &'static str,
    /// 图片内容 SHA256（十六进制）。
    pub digest: String,
}

/// 拥有源文件共享引用，供 ZIP 游标跨阻塞任务移动且不复制整份工作簿。
struct WorkbookBytes(Arc<Vec<u8>>);

impl AsRef<[u8]> for WorkbookBytes {
    /// 返回共享工作簿的完整字节切片。
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

/// 一份工作簿的媒体读取状态；每次仅提取一个媒体并归还给串行上传步骤。
pub(super) struct WorkbookMedia {
    archive: Option<ZipArchive<Cursor<WorkbookBytes>>>,
    cache: VecDeque<(String, Option<Arc<PreparedImage>>)>,
    cached_bytes: usize,
}

impl WorkbookMedia {
    /// 在阻塞任务中打开 ZIP；损坏工作簿与原逐图读取一致，图片均跳过。
    ///
    /// # 参数
    /// * `bytes` - 本次提交共享的源文件字节
    ///
    /// # 返回
    /// 返回拥有 ZIP 目录与有界缓存的读取状态。
    ///
    /// # 错误
    /// 阻塞任务异常终止时返回内部错误。
    pub(super) async fn open(bytes: Arc<Vec<u8>>) -> Result<Self> {
        spawn_blocking(move || Self {
            archive: ZipArchive::new(Cursor::new(WorkbookBytes(bytes))).ok(),
            cache: VecDeque::new(),
            cached_bytes: 0,
        })
        .await
        .map_err(|_| Error::Internal("读取导入图片失败".into()))
    }

    /// 将单个媒体解压、检测与摘要交给一个阻塞任务，完成后归还同一 ZIP 状态。
    ///
    /// # 参数
    /// * `path` - 当前模板单元格对应的 ZIP 内媒体路径
    ///
    /// # 返回
    /// 返回同一读取状态与该媒体；缺失、损坏或格式未知时返回空媒体。
    ///
    /// # 错误
    /// 阻塞任务异常终止时返回内部错误。取消读取最多留下一个媒体的阻塞工作，
    /// 不读取后续媒体，也不执行上传。
    pub(super) async fn read(mut self, path: String) -> Result<(Self, Option<Arc<PreparedImage>>)> {
        spawn_blocking(move || {
            let image = self.image(&path);
            (self, image)
        })
        .await
        .map_err(|_| Error::Internal("读取导入图片失败".into()))
    }

    /// 测试查询实际缓存状态，以验证上传首错后没有提前读取后续媒体。
    #[cfg(test)]
    pub(super) fn has_cached(&self, path: &str) -> bool {
        self.cache.iter().any(|(cached, _)| cached == path)
    }

    /// 复用最近媒体；无法读取或格式不支持时保持逐图跳过。
    fn image(&mut self, path: &str) -> Option<Arc<PreparedImage>> {
        if let Some((_, image)) = self.cache.iter().find(|(cached, _)| cached == path) {
            return image.clone();
        }
        let image = self.archive.as_mut().and_then(|archive| prepare_image(archive, path)).map(Arc::new);
        let byte_size = image.as_ref().map_or(0, |image| image.bytes.len());
        if byte_size <= MEDIA_CACHE_BYTES {
            self.retain_image(path, image.clone(), byte_size);
        }
        image
    }

    /// 写入有限近期缓存，淘汰最早条目直至条数和解压字节数均符合上限。
    fn retain_image(&mut self, path: &str, image: Option<Arc<PreparedImage>>, byte_size: usize) {
        while self.cache.len() >= MEDIA_CACHE_ENTRIES || self.cached_bytes + byte_size > MEDIA_CACHE_BYTES {
            if let Some((_, old)) = self.cache.pop_front() {
                self.cached_bytes -= old.map_or(0, |image| image.bytes.len());
            }
        }
        self.cached_bytes += byte_size;
        self.cache.push_back((path.to_string(), image));
    }
}

/// 按原第 3 至第 5 列顺序解析图片目标，缺公式或缺映射时跳过。
///
/// # 参数
/// * `cells` - 当前模板行的单元格
/// * `sheet` - 工作簿解析结果中的图片路径映射
///
/// # 返回
/// 返回最多 3 个按模板列序排列的图片目标。
///
/// # 错误
/// 无。
pub(super) fn row_media_targets(cells: &[String], sheet: &ParsedProductSheet) -> Vec<(usize, String)> {
    cells
        .iter()
        .enumerate()
        .take(5)
        .skip(2)
        .filter_map(|(column, cell)| {
            let id = dispimg_id(cell)?;
            Some((column, sheet.image_targets.get(id)?.clone()))
        })
        .collect()
}

/// 从同一 ZIP 提取并识别图片，损坏条目、缺条目或未知格式均沿用跳过语义。
fn prepare_image(archive: &mut ZipArchive<Cursor<WorkbookBytes>>, path: &str) -> Option<PreparedImage> {
    let mut file = archive.by_name(path).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let (content_type, extension) = detect_image(&bytes)?;
    let digest = hex::encode(Sha256::digest(&bytes));
    Some(PreparedImage { bytes, content_type, extension, digest })
}

/// 按原文件签名识别受支持图片，不根据扩展名接受其他格式。
fn detect_image(content: &[u8]) -> Option<(&'static str, &'static str)> {
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Write;

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::{
        Arc, Cursor, Digest, MEDIA_CACHE_BYTES, MEDIA_CACHE_ENTRIES, Sha256, WorkbookMedia, row_media_targets,
    };
    use crate::product_import::parse::ParsedProductSheet;

    /// 生成纯内存 ZIP，覆盖实际生产解压与格式识别路径。
    fn workbook(entries: &[(&str, Vec<u8>)]) -> Arc<Vec<u8>> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer.start_file(*name, SimpleFileOptions::default()).unwrap();
            writer.write_all(bytes).unwrap();
        }
        Arc::new(writer.finish().unwrap().into_inner())
    }

    /// 同路径跨单元格与跨行共享图片和摘要，不复制整份源工作簿。
    #[tokio::test]
    async fn media_reuses_archive_and_shared_paths() {
        let png = b"\x89PNG\r\n\x1a\ncontents".to_vec();
        let source = workbook(&[("xl/media/a.png", png.clone())]);
        let reader = WorkbookMedia::open(source.clone()).await.unwrap();
        let (reader, first) = reader.read("xl/media/a.png".into()).await.unwrap();
        let first = first.unwrap();
        assert_eq!(first.bytes, png);
        assert_eq!(first.digest, hex::encode(Sha256::digest(&png)));
        assert_eq!(first.content_type, "image/png");
        let (_, second) = reader.read("xl/media/a.png".into()).await.unwrap();
        assert!(Arc::ptr_eq(&first, &second.unwrap()));
        assert_eq!(Arc::strong_count(&source), 1);
    }

    /// 缺媒体、坏格式、坏压缩包均保持跳过，之后正常媒体继续被提取。
    #[tokio::test]
    async fn media_skips_missing_unsupported_and_corrupt_sources() {
        let source = workbook(&[("bad", b"invalid-image".to_vec()), ("good", b"GIF89apayload".to_vec())]);
        let reader = WorkbookMedia::open(source).await.unwrap();
        let (reader, missing) = reader.read("missing".into()).await.unwrap();
        assert!(missing.is_none());
        let (reader, invalid) = reader.read("bad".into()).await.unwrap();
        assert!(invalid.is_none());
        let (_, valid) = reader.read("good".into()).await.unwrap();
        assert_eq!(valid.unwrap().extension, "gif");
        let reader = WorkbookMedia::open(Arc::new(b"not-zip".to_vec())).await.unwrap();
        let (_, invalid) = reader.read("good".into()).await.unwrap();
        assert!(invalid.is_none());
    }

    /// 近期缓存始终有界，超限图片只在当前上传步骤驻留并立即允许释放。
    #[tokio::test]
    async fn media_cache_is_bounded_by_entries_and_bytes() {
        let mut large = b"\x89PNG\r\n\x1a\n".to_vec();
        large.resize(MEDIA_CACHE_BYTES + 1, 0);
        let mut medium_a = b"GIF89a".to_vec();
        medium_a.resize(4 * 1024 * 1024, 0);
        let mut medium_b = b"GIF87a".to_vec();
        medium_b.resize(5 * 1024 * 1024, 0);
        let reader = WorkbookMedia::open(workbook(&[
            ("large", large),
            ("medium-a", medium_a),
            ("medium-b", medium_b),
        ]))
        .await
        .unwrap();
        let (mut reader, image) = reader.read("large".into()).await.unwrap();
        let image = image.unwrap();
        let weak = Arc::downgrade(&image);
        assert_eq!(reader.cached_bytes, 0);
        assert!(reader.cache.is_empty());
        drop(image);
        assert!(weak.upgrade().is_none());
        for index in 0..MEDIA_CACHE_ENTRIES + 5 {
            (reader, _) = reader.read(format!("missing-{index}")).await.unwrap();
            assert!(reader.cache.len() <= MEDIA_CACHE_ENTRIES);
            assert!(reader.cached_bytes <= MEDIA_CACHE_BYTES);
        }
        let (reader, medium) = reader.read("medium-a".into()).await.unwrap();
        let weak = Arc::downgrade(&medium.unwrap());
        let (reader, _) = reader.read("medium-b".into()).await.unwrap();
        assert!(weak.upgrade().is_none());
        assert_eq!(reader.cached_bytes, 5 * 1024 * 1024);
        assert!(reader.cached_bytes <= MEDIA_CACHE_BYTES);
    }

    /// 目标采集仅保留模板图片列顺序，未知引用或其他列不进入提取批次。
    #[test]
    fn media_targets_limit_each_row_to_three_columns() {
        let sheet = ParsedProductSheet {
            sheet_name: "对内".into(),
            rows: Vec::new(),
            image_targets: HashMap::from([
                ("ID_A".into(), "a".into()),
                ("ID_B".into(), "b".into()),
                ("ID_C".into(), "c".into()),
            ]),
        };
        let cells =
            ["ID_A", "ID_A", "ID_C", "ID_A", "ID_B", "ID_A"].map(|id| format!("=DISPIMG(\"{id}\",1)"));
        assert_eq!(row_media_targets(&cells, &sheet), [(2, "c".into()), (3, "a".into()), (4, "b".into())]);
    }
}
