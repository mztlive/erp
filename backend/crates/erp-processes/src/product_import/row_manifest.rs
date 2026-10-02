//! 提交时预提行图片并构建行级清单；小清单随任务事务保存，较大清单沿用对象存储。
//! 执行优先读取私有输入，旧任务或损坏清单按对象清单、源文件顺序回退。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::ids::FileAssetId;
use erp_support::repository::bulk_job::{BACKGROUND_JOB_INPUT_LIMIT, BackgroundJobInput};
use erp_support::{
    PENDING_FILE_REFERENCE_PREFIX, PendingFileAssetRequest, RegisterFileAssetRequest, RetentionClass,
    SensitivityClass, content_fingerprint,
};
use serde::{Deserialize, Serialize};
use storage::S3Storage;

use super::images::RowMedia;
use super::media::{PreparedImage, WorkbookMedia, row_media_targets};
use super::parse::ParsedProductSheet;
use crate::{Error, Result};

/// 行级清单版本号；版本不一致时执行阶段回退到源文件链路。
const ROW_MANIFEST_VERSION: u32 = 1;
/// 私有后台输入的格式标识；仍须验证清单自身版本及请求身份。
pub(super) const ROW_MANIFEST_INPUT_FORMAT: &str = "product_import_rows_v1";
/// 行级清单对象键前缀。
const ROW_MANIFEST_KEY_PREFIX: &str = "product-import-rows";
/// 预提图片对象键前缀。
const ROW_IMAGE_KEY_PREFIX: &str = "product-import-images";

/// 行级清单；与任务请求身份绑定，私有输入及对象存储共用相同 JSON 格式。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RowManifest {
    /// 清单版本。
    pub version: u32,
    /// 幂等请求身份。
    pub request_id: String,
    /// 逐行数据。
    pub rows: Vec<RowManifestRow>,
}

/// 清单中的一行。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct RowManifestRow {
    /// Excel 行号（1 起，含表头）。
    pub row_number: u32,
    /// 按模板列序的单元格文本。
    pub cells: Vec<String>,
    /// 主图（第 3 列图片同时登记为 SKU 主图与轮播首图）。
    pub main_image: Option<ManifestImage>,
    /// 轮播图。
    pub carousel: Vec<ManifestCarouselImage>,
    /// 图片预提失败说明；有值时执行阶段直接记该行失败。
    pub media_error: Option<String>,
}

/// 清单中的已上传图片。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ManifestImage {
    /// 临时文件引用（行内产品命令使用）。
    pub reference: String,
    /// 已上传对象的登记命令（对象已在存储中，执行阶段只登记）。
    pub registration: RegisterFileAssetRequest,
}

/// 清单中的轮播图。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ManifestCarouselImage {
    /// 临时文件引用。
    pub reference: String,
    /// 轮播顺序。
    pub sort_order: i32,
    /// 已上传对象的登记命令。
    pub registration: RegisterFileAssetRequest,
}

/// 清单构建产物。
pub(super) struct BuiltRowManifest {
    /// 行级清单。
    pub manifest: RowManifest,
    /// 已上传对象键（含图片与清单键本身调用方写入后追加），用于失败补偿。
    pub uploaded_object_keys: Vec<String>,
}

/// 由请求身份推导行级清单对象键。
pub(super) fn manifest_object_key(request_id: &str) -> Result<String> {
    validate_manifest_request_id(request_id)?;
    Ok(format!("{ROW_MANIFEST_KEY_PREFIX}/{request_id}.json"))
}

/// 提交时预提各行图片并构建行级清单（图片即时上传，执行阶段只登记）。
///
/// # 参数
/// * `storage` - 对象存储
/// * `secret` - 内容指纹密钥
/// * `request_id` - 幂等请求身份（推导清单与图片对象键）
/// * `parsed` - 已解析报价表
/// * `xlsx` - 源文件字节（仅本次读取，执行阶段不再需要）
///
/// # 返回
/// 返回行级清单与已上传对象键（调用方负责写清单与失败补偿）。
///
/// # 错误
/// 请求身份非法时返回错误；单行图片上传失败记入该行，不阻断其他行。
pub(super) async fn build_row_manifest(
    storage: &S3Storage,
    secret: &[u8],
    request_id: &str,
    parsed: &ParsedProductSheet,
    xlsx: Arc<Vec<u8>>,
) -> Result<BuiltRowManifest> {
    validate_manifest_request_id(request_id)?;
    let mut reader = WorkbookMedia::open(xlsx).await?;
    let writer = StoredManifestImages { storage, secret, request_id };
    let mut rows = Vec::with_capacity(parsed.rows.len());
    let mut uploaded_object_keys = Vec::new();
    for row in &parsed.rows {
        let mut entry =
            RowManifestRow { row_number: row.row_number, cells: row.cells.clone(), ..Default::default() };
        reader = match upload_manifest_row(
            &writer,
            &mut entry,
            reader,
            row_media_targets(&row.cells, parsed),
            &mut uploaded_object_keys,
        )
        .await
        {
            Ok(reader) => reader,
            Err(error) => {
                delete_manifest_objects(storage, &uploaded_object_keys).await;
                return Err(error);
            },
        };
        rows.push(entry);
    }
    Ok(BuiltRowManifest {
        manifest: RowManifest { version: ROW_MANIFEST_VERSION, request_id: request_id.to_string(), rows },
        uploaded_object_keys,
    })
}

/// 准备行级清单；小清单返回私有输入，达到独立容量边界时写入原对象存储。
///
/// # 参数
/// * `storage` - 对象存储
/// * `manifest` - 行级清单
///
/// # 返回
/// 返回私有输入及对象键，两者恰一项有值；对象键由调用方纳入原失败补偿。
///
/// # 错误
/// 请求身份非法或写入失败时返回错误。
pub(super) async fn prepare_row_manifest(
    storage: &S3Storage,
    manifest: &RowManifest,
) -> Result<(Option<BackgroundJobInput>, Option<String>)> {
    match manifest_payload(manifest)? {
        ManifestPayload::Input(input) => Ok((Some(input), None)),
        ManifestPayload::Object(bytes) => {
            let key = manifest_object_key(&manifest.request_id)?;
            storage
                .save_with_content_type(&key, &bytes, Some("application/json"))
                .await
                .map_err(|error| Error::Internal(format!("写入行清单失败: {error}")))?;
            Ok((None, Some(key)))
        },
    }
}

/// 序列化后决定私有输入或对象字节，不能截断清单以满足容量界限。
enum ManifestPayload {
    Input(BackgroundJobInput),
    Object(Vec<u8>),
}

/// 小清单仍经过拥有领域的私有容量校验，大清单保留相同完整 JSON 字节。
fn manifest_payload(manifest: &RowManifest) -> Result<ManifestPayload> {
    validate_manifest_request_id(&manifest.request_id)?;
    let bytes = serde_json::to_vec(manifest).map_err(|_| Error::Internal("行清单序列化失败".into()))?;
    if bytes.len() < BACKGROUND_JOB_INPUT_LIMIT {
        Ok(ManifestPayload::Input(BackgroundJobInput::new(ROW_MANIFEST_INPUT_FORMAT, bytes)?))
    } else {
        Ok(ManifestPayload::Object(bytes))
    }
}

/// 读取行级清单；缺失或版本不一致时由调用方回退到源文件链路。
///
/// # 参数
/// * `storage` - 对象存储
/// * `request_id` - 幂等请求身份
///
/// # 返回
/// 返回行级清单。
///
/// # 错误
/// 清单缺失、损坏或版本不一致时返回错误（调用方回退，不直接失败任务）。
pub(super) async fn read_row_manifest(storage: &S3Storage, request_id: &str) -> Result<RowManifest> {
    let key = manifest_object_key(request_id)?;
    let bytes =
        storage.read(&key).await.map_err(|error| Error::Internal(format!("读取行清单失败: {error}")))?;
    decode_row_manifest(&bytes, request_id)
}

/// 私有输入和旧对象清单采用相同解析及绑定校验，错误由执行器回退到下一来源。
///
/// # 参数
/// 已读取清单字节与任务当前的请求身份。
/// # 返回
/// 返回绑定同一请求及清单版本的全部原始行。
/// # 错误
/// JSON 损坏、请求身份不符或版本不一致时拒绝该来源。
pub(super) fn decode_row_manifest(bytes: &[u8], request_id: &str) -> Result<RowManifest> {
    let manifest: RowManifest =
        serde_json::from_slice(bytes).map_err(|_| Error::Internal("行清单损坏".to_string()))?;
    if manifest.version != ROW_MANIFEST_VERSION || manifest.request_id != request_id {
        return Err(Error::Internal("行清单版本不一致".to_string()));
    }
    Ok(manifest)
}

/// 尽力删除已上传的清单相关对象（失败补偿，忽略单个删除错误）。
///
/// # 参数
/// * `storage` - 对象存储
/// * `keys` - 待删除对象键
pub(super) async fn delete_manifest_objects(storage: &S3Storage, keys: &[String]) {
    for key in keys {
        let _ = storage.delete(key).await;
    }
}

/// 按行号索引清单行。
///
/// # 参数
/// * `manifest` - 行级清单
///
/// # 返回
/// 返回行号到清单行的映射。
pub(super) fn row_entries_by_number(manifest: &RowManifest) -> HashMap<u32, &RowManifestRow> {
    manifest.rows.iter().map(|row| (row.row_number, row)).collect()
}

/// 由清单行构造行媒体（对象已在存储中，不产生上传）。
///
/// # 参数
/// * `entry` - 清单行
///
/// # 返回
/// 返回可直接用于产品命令的行媒体。
pub(super) fn row_media_from_entry(entry: &RowManifestRow) -> RowMedia {
    let mut media = RowMedia::default();
    let mut pending = Vec::new();
    if let Some(main) = &entry.main_image {
        media.main_image = Some(FileAssetId::new(main.reference.clone()));
        pending.push(PendingFileAssetRequest {
            reference: main.reference.clone(),
            registration: main.registration.clone(),
        });
    }
    for image in &entry.carousel {
        media.carousel.push((FileAssetId::new(image.reference.clone()), image.sort_order));
        pending.push(PendingFileAssetRequest {
            reference: image.reference.clone(),
            registration: image.registration.clone(),
        });
    }
    media.pending = pending;
    media
}

/// 校验请求身份可安全推导对象键。
fn validate_manifest_request_id(request_id: &str) -> Result<()> {
    if request_id.is_empty()
        || request_id.len() > 64
        || !request_id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(Error::ValidationError("请求身份非法，请重新选择文件".to_string()));
    }
    Ok(())
}

/// 行媒体上传边界；提交编排按单元格与对象槽位串行调用。
#[async_trait]
trait ManifestImageWriter: Sync {
    /// 上传一个槽位并返回引用、登记命令和补偿对象键。
    async fn put(
        &self,
        row_number: u32,
        slot: &str,
        image: &PreparedImage,
    ) -> Result<(String, RegisterFileAssetRequest, String)>;
}

/// 当前提交共享的对象存储与请求身份。
struct StoredManifestImages<'a> {
    storage: &'a S3Storage,
    secret: &'a [u8],
    request_id: &'a str,
}

#[async_trait]
impl ManifestImageWriter for StoredManifestImages<'_> {
    /// 上传独立对象；主图和轮播首图保留各自对象键和登记命令。
    async fn put(
        &self,
        row_number: u32,
        slot: &str,
        image: &PreparedImage,
    ) -> Result<(String, RegisterFileAssetRequest, String)> {
        let ext = image.extension;
        let object_key = format!("{ROW_IMAGE_KEY_PREFIX}/{}/{row_number}/{slot}.{ext}", self.request_id);
        self.storage
            .save_with_content_type(&object_key, &image.bytes, Some(image.content_type))
            .await
            .map_err(|error| Error::Internal(format!("保存预提图片失败: {error}")))?;
        let registration = RegisterFileAssetRequest {
            storage_object_key: object_key.clone(),
            file_name: format!("product-import.{ext}"),
            content_type: image.content_type.to_string(),
            byte_size: u64::try_from(image.bytes.len()).expect("图片字节数不超过 u64 范围"),
            content_hmac: content_fingerprint(&image.digest, self.secret),
            sensitivity_class: SensitivityClass::General,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
        };
        let reference = format!("{PENDING_FILE_REFERENCE_PREFIX}{}-{row_number}-{slot}", self.request_id);
        Ok((reference, registration, object_key))
    }
}

/// 保留轮播、主图和后续图片的写入顺序；首个上传失败后停止当前行。
async fn upload_manifest_row(
    writer: &impl ManifestImageWriter,
    entry: &mut RowManifestRow,
    mut reader: WorkbookMedia,
    targets: Vec<(usize, String)>,
    uploaded_keys: &mut Vec<String>,
) -> Result<WorkbookMedia> {
    let mut sort_order = 0;
    for (column, path) in targets {
        let (next_reader, image) = reader.read(path).await?;
        reader = next_reader;
        let Some(image) = image else {
            continue;
        };
        let slot = format!("carousel-{sort_order}");
        let result = writer.put(entry.row_number, &slot, &image).await;
        if !record_manifest_upload(entry, result, Some(sort_order), uploaded_keys) {
            break;
        }
        sort_order += 1;
        if column == 2 {
            let result = writer.put(entry.row_number, "main", &image).await;
            if !record_manifest_upload(entry, result, None, uploaded_keys) {
                break;
            }
        }
    }
    Ok(reader)
}

/// 记录成功对象以供失败补偿；失败时写入原行级错误文案。
fn record_manifest_upload(
    entry: &mut RowManifestRow,
    result: Result<(String, RegisterFileAssetRequest, String)>,
    sort_order: Option<i32>,
    uploaded_keys: &mut Vec<String>,
) -> bool {
    match result {
        Ok((reference, registration, object_key)) => {
            uploaded_keys.push(object_key);
            if let Some(sort_order) = sort_order {
                entry.carousel.push(ManifestCarouselImage { reference, sort_order, registration });
            } else {
                entry.main_image = Some(ManifestImage { reference, registration });
            }
            true
        },
        Err(error) => {
            entry.media_error = Some(format!("第{}行图片预提失败: {error}", entry.row_number));
            false
        },
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use erp_support::{RegisterFileAssetRequest, RetentionClass, SensitivityClass};
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::{
        BACKGROUND_JOB_INPUT_LIMIT, ManifestCarouselImage, ManifestImage, ManifestImageWriter,
        ManifestPayload, PreparedImage, ROW_MANIFEST_VERSION, RowManifest, RowManifestRow, WorkbookMedia,
        decode_row_manifest, manifest_object_key, manifest_payload, row_media_from_entry,
        upload_manifest_row,
    };
    use crate::{Error, Result};

    /// 构造真实 JSON 序列化达到指定字节数的清单，容量测试不截断合法 JSON。
    fn manifest_with_size(size: usize) -> RowManifest {
        let mut manifest = RowManifest {
            version: ROW_MANIFEST_VERSION,
            request_id: "req-1".into(),
            rows: vec![RowManifestRow { row_number: 2, cells: vec![String::new()], ..Default::default() }],
        };
        let overhead = serde_json::to_vec(&manifest).unwrap().len();
        manifest.rows[0].cells[0] = "a".repeat(size.checked_sub(overhead).unwrap());
        assert_eq!(serde_json::to_vec(&manifest).unwrap().len(), size);
        manifest
    }

    /// 小清单在容量界限前一字节使用私有输入，达到界限和超限保留完整对象 JSON。
    #[test]
    fn manifest_payload_routes_exact_capacity_without_truncation() {
        let manifest = manifest_with_size(BACKGROUND_JOB_INPUT_LIMIT - 1);
        assert!(matches!(manifest_payload(&manifest).unwrap(), ManifestPayload::Input(_)));
        for size in [BACKGROUND_JOB_INPUT_LIMIT, BACKGROUND_JOB_INPUT_LIMIT + 1] {
            let manifest = manifest_with_size(size);
            let ManifestPayload::Object(bytes) = manifest_payload(&manifest).unwrap() else {
                panic!("达到容量界限应走对象存储");
            };
            assert_eq!(bytes.len(), size);
            let parsed = decode_row_manifest(&bytes, "req-1").unwrap();
            assert_eq!(parsed.rows[0].cells[0], manifest.rows[0].cells[0]);
        }
    }

    /// 私有输入和旧对象使用同一解析器；请求或版本不符及损坏 JSON 必须回退。
    #[test]
    fn manifest_decode_rejects_mismatched_bindings_and_corruption() {
        let original = serde_json::json!({"version":1,"request_id":"req-1","rows":[]});
        let bytes = serde_json::to_vec(&original).unwrap();
        assert!(decode_row_manifest(&bytes, "req-1").is_ok());
        assert!(matches!(decode_row_manifest(&bytes, "req-2"), Err(Error::Internal(message))
            if message == "行清单版本不一致"));
        let newer = serde_json::json!({"version":2,"request_id":"req-1","rows":[]});
        assert!(
            matches!(decode_row_manifest(&serde_json::to_vec(&newer).unwrap(), "req-1"), Err(Error::Internal(message))
            if message == "行清单版本不一致")
        );
        for damaged in [&b"{"[..], &b"{}"[..], &b""[..]] {
            assert!(matches!(decode_row_manifest(damaged, "req-1"), Err(Error::Internal(message))
                if message == "行清单损坏"));
        }
    }

    /// 旧版对象清单和已登记媒体不要求新增任务输入字段，额外历史字段继续兼容。
    #[test]
    fn legacy_manifest_remains_readable_without_private_metadata() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version":1,"request_id":"req-1","legacy_extra":true,"rows":[{
                "row_number":2,"cells":["product"],"main_image":null,"carousel":[],"media_error":null
            }]
        }))
        .unwrap();
        let parsed = decode_row_manifest(&bytes, "req-1").unwrap();
        assert_eq!(parsed.rows.len(), 1);
        assert_eq!(parsed.rows[0].cells, ["product"]);
        assert!(parsed.rows[0].main_image.is_none());
        assert!(parsed.rows[0].carousel.is_empty());
    }

    fn sample_registration() -> erp_support::RegisterFileAssetRequest {
        erp_support::RegisterFileAssetRequest {
            storage_object_key: "product-import-images/req-1/2/main.png".to_string(),
            file_name: "product-import.png".to_string(),
            content_type: "image/png".to_string(),
            byte_size: 12,
            content_hmac: "a".repeat(64),
            sensitivity_class: SensitivityClass::General,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
        }
    }

    #[test]
    fn manifest_key_rejects_unsafe_request_id() {
        assert_eq!(manifest_object_key("req-1_Aa").unwrap(), "product-import-rows/req-1_Aa.json");
        assert!(manifest_object_key("").is_err());
        assert!(manifest_object_key("../escape").is_err());
        assert!(manifest_object_key("a/b").is_err());
        assert!(manifest_object_key(&"a".repeat(65)).is_err());
    }

    #[test]
    fn manifest_roundtrips_through_json() {
        let manifest = RowManifest {
            version: ROW_MANIFEST_VERSION,
            request_id: "req-1".to_string(),
            rows: vec![RowManifestRow {
                row_number: 2,
                cells: vec!["a".to_string(), "b".to_string()],
                main_image: Some(ManifestImage {
                    reference: "pending-file:req-1-2-main".to_string(),
                    registration: sample_registration(),
                }),
                carousel: vec![ManifestCarouselImage {
                    reference: "pending-file:req-1-2-carousel-0".to_string(),
                    sort_order: 0,
                    registration: sample_registration(),
                }],
                media_error: None,
            }],
        };
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let decoded: RowManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.version, ROW_MANIFEST_VERSION);
        assert_eq!(decoded.request_id, "req-1");
        assert_eq!(decoded.rows.len(), 1);
        assert_eq!(decoded.rows[0].row_number, 2);
        assert_eq!(decoded.rows[0].cells, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(
            decoded.rows[0].main_image.as_ref().map(|image| image.reference.as_str()),
            Some("pending-file:req-1-2-main")
        );
        assert_eq!(decoded.rows[0].carousel.len(), 1);
        assert_eq!(decoded.rows[0].carousel[0].sort_order, 0);
        assert_eq!(decoded.rows[0].media_error, None);
    }

    #[test]
    fn entry_converts_to_row_media_without_upload() {
        let entry = RowManifestRow {
            row_number: 2,
            cells: Vec::new(),
            main_image: Some(ManifestImage {
                reference: "pending-file:req-1-2-main".to_string(),
                registration: sample_registration(),
            }),
            carousel: Vec::new(),
            media_error: None,
        };
        let media = row_media_from_entry(&entry);
        assert_eq!(media.main_image.as_ref().map(|id| id.as_ref()), Some("pending-file:req-1-2-main"));
        assert_eq!(media.pending.len(), 1);
        assert_eq!(
            media.pending[0].registration.storage_object_key,
            "product-import-images/req-1/2/main.png"
        );
    }

    /// 内存上传替身执行真实行编排，不访问 S3。
    struct RecordingImages {
        calls: Mutex<Vec<String>>,
        failure: Option<String>,
    }

    #[async_trait]
    impl ManifestImageWriter for RecordingImages {
        /// 记录槽位并按指定对象注入上传失败。
        async fn put(
            &self,
            row_number: u32,
            slot: &str,
            _image: &PreparedImage,
        ) -> Result<(String, RegisterFileAssetRequest, String)> {
            let key = format!("{row_number}-{slot}");
            self.calls.lock().unwrap().push(key.clone());
            if self.failure.as_deref() == Some(key.as_str()) {
                return Err(Error::Internal("upload failed".into()));
            }
            let mut registration = sample_registration();
            registration.storage_object_key = key.clone();
            Ok((format!("pending-file:{key}"), registration, key))
        }
    }

    /// 构造真实内存 ZIP，测试实际媒体读取与上传交替编排。
    async fn prepared_reader() -> WorkbookMedia {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        for (_, path) in prepared_targets() {
            archive.start_file(&path, SimpleFileOptions::default()).unwrap();
            archive.write_all(b"GIF89apayload").unwrap();
        }
        let bytes = Arc::new(archive.finish().unwrap().into_inner());
        WorkbookMedia::open(bytes).await.unwrap()
    }

    /// 按模板三列构造独立媒体路径，以检测是否提前提取后续图片。
    fn prepared_targets() -> Vec<(usize, String)> {
        (2..5).map(|column| (column, format!("image-{column}"))).collect()
    }

    /// 主图与轮播首图独立登记，全部上传顺序和轮播排序保持原合同。
    #[tokio::test]
    async fn manifest_upload_preserves_slot_order_and_distinct_registrations() {
        let writer = RecordingImages { calls: Mutex::new(Vec::new()), failure: None };
        let mut entry = RowManifestRow { row_number: 2, ..Default::default() };
        let mut keys = Vec::new();
        let reader = prepared_reader().await;
        let reader =
            upload_manifest_row(&writer, &mut entry, reader, prepared_targets(), &mut keys).await.unwrap();
        assert!(reader.has_cached("image-4"));
        assert_eq!(keys, ["2-carousel-0", "2-main", "2-carousel-1", "2-carousel-2"]);
        assert_eq!(*writer.calls.lock().unwrap(), keys);
        assert_eq!(entry.carousel.iter().map(|image| image.sort_order).collect::<Vec<_>>(), [0, 1, 2]);
        let main = entry.main_image.unwrap();
        assert_ne!(main.reference, entry.carousel[0].reference);
        assert_ne!(main.registration.storage_object_key, entry.carousel[0].registration.storage_object_key);
        assert!(entry.media_error.is_none());
    }

    /// 首错停止当前行并保留成功对象用于补偿，随后行仍继续执行。
    #[tokio::test]
    async fn manifest_upload_stops_on_first_error_and_retains_compensation_keys() {
        let writer = RecordingImages { calls: Mutex::new(Vec::new()), failure: Some("2-main".into()) };
        let mut entry = RowManifestRow { row_number: 2, ..Default::default() };
        let mut keys = Vec::new();
        let reader = prepared_reader().await;
        let reader =
            upload_manifest_row(&writer, &mut entry, reader, prepared_targets(), &mut keys).await.unwrap();
        assert_eq!(*writer.calls.lock().unwrap(), ["2-carousel-0", "2-main"]);
        assert!(reader.has_cached("image-2"));
        assert!(!reader.has_cached("image-3"));
        assert!(!reader.has_cached("image-4"));
        assert_eq!(keys, ["2-carousel-0"]);
        assert_eq!(entry.carousel.len(), 1);
        assert!(entry.main_image.is_none());
        assert!(entry.media_error.as_ref().unwrap().contains("第2行图片预提失败"));
        let mut next = RowManifestRow { row_number: 3, ..Default::default() };
        let _ = upload_manifest_row(&writer, &mut next, reader, prepared_targets(), &mut keys).await.unwrap();
        assert!(next.media_error.is_none());
        assert_eq!(next.carousel.len(), 3);
        assert_eq!(keys.len(), 5);
    }

    /// 轮播首图上传失败时既不尝试主图也不登记成功对象。
    #[tokio::test]
    async fn manifest_upload_first_carousel_failure_has_no_followup_uploads() {
        let writer = RecordingImages { calls: Mutex::new(Vec::new()), failure: Some("2-carousel-0".into()) };
        let mut entry = RowManifestRow { row_number: 2, ..Default::default() };
        let mut keys = Vec::new();
        let reader = prepared_reader().await;
        let reader =
            upload_manifest_row(&writer, &mut entry, reader, prepared_targets(), &mut keys).await.unwrap();
        assert_eq!(*writer.calls.lock().unwrap(), ["2-carousel-0"]);
        assert!(reader.has_cached("image-2"));
        assert!(!reader.has_cached("image-3"));
        assert!(!reader.has_cached("image-4"));
        assert!(keys.is_empty());
        assert!(entry.carousel.is_empty());
        assert!(entry.main_image.is_none());
    }
}
