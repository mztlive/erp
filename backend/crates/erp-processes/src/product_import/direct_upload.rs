//! 浏览器直传导入：分片地址签发、合并与任务登记。
//!
//! 大文件不再经过网关内存：初始化接口在对象存储创建分片上传，
//! 浏览器逐片 PUT 到预签名地址，合并接口完成分片后复用既有解析与落库链路。

use std::time::Duration;

use application_core::AuditActor;
use erp_catalog::{
    MAX_PRODUCT_IMPORT_FILE_BYTES, PRODUCT_IMPORT_DIRECT_PART_BYTES, PRODUCT_IMPORT_DIRECT_PART_URL_TTL_SECS,
    PRODUCT_IMPORT_XLSX_MIME, ProductImportDirectUploadCompleteRequest, ProductImportDirectUploadInitRequest,
    ProductImportDirectUploadInitView, ProductImportJobView,
};
use erp_support::{
    FileAsset, FileAssetId, RegisterFileAssetRequest, RetentionClass, SensitivityClass, content_fingerprint,
};
use id_generator::next_id;
use sha2::{Digest, Sha256};
use storage::UploadedPart;
use validator::Validate;

use super::ProductImportProcess;
use super::parse::parse_product_quote_xlsx;
use crate::{Error, Result};

/// 直传对象键前缀（与表单上传的随机键区分，便于排查与生命周期管理）。
const DIRECT_OBJECT_KEY_PREFIX: &str = "product-import-direct";

impl ProductImportProcess {
    /// 初始化浏览器直传：在对象存储创建分片上传并返回分片口径。
    ///
    /// # 参数
    /// * `req` - 文件名、总字节数与幂等请求身份
    ///
    /// # 返回
    /// 返回分片上传标识、对象键与分片口径；分片地址由调用方按需获取。
    ///
    /// # 错误
    /// 文件名不是 `.xlsx`、大小越限或对象存储失败时返回错误。
    pub async fn init_direct_upload(
        &self,
        req: ProductImportDirectUploadInitRequest,
    ) -> Result<ProductImportDirectUploadInitView> {
        req.validate()?;
        validate_xlsx_file_name(&req.file_name)?;
        validate_byte_size(req.byte_size)?;
        let object_key = direct_object_key(&req.request_id)?;
        let upload_id = self
            .storage
            .create_multipart_upload(&object_key, Some(PRODUCT_IMPORT_XLSX_MIME))
            .await
            .map_err(|error| Error::Internal(format!("初始化直传失败: {error}")))?;
        let total_parts = req.byte_size.div_ceil(PRODUCT_IMPORT_DIRECT_PART_BYTES).max(1) as u32;
        Ok(ProductImportDirectUploadInitView {
            upload_id,
            object_key,
            part_size: PRODUCT_IMPORT_DIRECT_PART_BYTES,
            total_parts,
            part_url_ttl_secs: PRODUCT_IMPORT_DIRECT_PART_URL_TTL_SECS,
        })
    }

    /// 为单个分片签发浏览器可直接 PUT 的预签名地址。
    ///
    /// # 参数
    /// * `object_key` - 初始化接口返回的相对对象键
    /// * `upload_id` - 分片上传标识
    /// * `part_number` - 分片序号（1 起）
    /// * `request_id` - 幂等请求身份（须与对象键绑定）
    ///
    /// # 返回
    /// 返回预签名 PUT 地址。
    ///
    /// # 错误
    /// 对象键与请求身份不一致、分片序号非法或签名失败时返回错误。
    pub async fn direct_upload_part_url(
        &self,
        object_key: &str,
        upload_id: &str,
        part_number: i32,
        request_id: &str,
    ) -> Result<String> {
        require_bound_object_key(object_key, request_id)?;
        if !(1..=10_000).contains(&part_number) {
            return Err(Error::ValidationError("分片序号必须在 1-10000 之间".to_string()));
        }
        self.storage
            .presign_upload_part(
                object_key,
                upload_id,
                part_number,
                Duration::from_secs(PRODUCT_IMPORT_DIRECT_PART_URL_TTL_SECS),
            )
            .await
            .map_err(|error| Error::Internal(format!("签发分片地址失败: {error}")))
    }

    /// 合并浏览器已直传的分片并登记导入任务。
    ///
    /// # 参数
    /// * `upload_id` - 分片上传标识
    /// * `req` - 对象键、文件名、总字节数、请求身份与已上传分片
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回新建或幂等回放的导入任务。
    ///
    /// # 错误
    /// 对象键与请求身份不一致、对象缺失或损坏、解析失败时返回错误；
    /// 结果未知以外失败会尽力删除已合并对象，避免残留孤儿文件。
    pub async fn submit_direct_upload(
        &self,
        upload_id: &str,
        req: ProductImportDirectUploadCompleteRequest,
        actor: &AuditActor,
    ) -> Result<ProductImportJobView> {
        req.validate()?;
        validate_xlsx_file_name(&req.file_name)?;
        validate_byte_size(req.byte_size)?;
        require_bound_object_key(&req.object_key, &req.request_id)?;
        if upload_id.trim().is_empty() {
            return Err(Error::ValidationError("分片上传标识不能为空".to_string()));
        }
        let parts = req
            .parts
            .iter()
            .map(|part| UploadedPart { part_number: part.part_number, etag: part.etag.trim().to_string() })
            .collect::<Vec<_>>();
        self.storage
            .complete_multipart_upload(&req.object_key, upload_id, parts)
            .await
            .map_err(|error| Error::Internal(format!("合并直传分片失败，请重试: {error}")))?;
        let result = self.register_merged_object(&req, actor).await;
        if let Err(error) = &result {
            if !matches!(error, Error::OutcomeUnknown(_)) {
                let _ = self.storage.delete(&req.object_key).await;
            }
        }
        result
    }

    /// 取消分片上传并清理对象存储侧已上传的分片。
    ///
    /// # 参数
    /// * `object_key` - 初始化接口返回的相对对象键
    /// * `upload_id` - 分片上传标识
    /// * `request_id` - 幂等请求身份（须与对象键绑定）
    ///
    /// # 错误
    /// 对象键与请求身份不一致或取消失败时返回错误；上传已不存在视为成功。
    pub async fn abort_direct_upload(
        &self,
        object_key: &str,
        upload_id: &str,
        request_id: &str,
    ) -> Result<()> {
        require_bound_object_key(object_key, request_id)?;
        match self.storage.abort_multipart_upload(object_key, upload_id).await {
            Ok(()) => Ok(()),
            Err(storage::Error::NotFound) => Ok(()),
            Err(error) => Err(Error::Internal(format!("取消直传失败: {error}"))),
        }
    }

    async fn register_merged_object(
        &self,
        req: &ProductImportDirectUploadCompleteRequest,
        actor: &AuditActor,
    ) -> Result<ProductImportJobView> {
        let bytes = match self.storage.read(&req.object_key).await {
            Ok(bytes) => bytes,
            Err(storage::Error::NotFound) => {
                return Err(Error::ValidationError("直传文件不存在或已过期，请重新上传".to_string()));
            },
            Err(error) => {
                return Err(Error::Internal(format!("读取直传文件失败: {error}")));
            },
        };
        if bytes.len() as u64 != req.byte_size || bytes.len() as u64 > MAX_PRODUCT_IMPORT_FILE_BYTES {
            let _ = self.storage.delete(&req.object_key).await;
            return Err(Error::ValidationError("文件大小与申报不一致，请重新上传".to_string()));
        }
        if bytes.len() < 4 || &bytes[..2] != b"PK" {
            let _ = self.storage.delete(&req.object_key).await;
            return Err(Error::ValidationError("文件不是有效的 Excel 工作簿".to_string()));
        }
        let byte_len = bytes.len() as u64;
        let digest = hex::encode(Sha256::digest(&bytes));
        let shared_bytes = std::sync::Arc::new(bytes);
        let for_parse = shared_bytes.clone();
        let parsed = match tokio::task::spawn_blocking(move || parse_product_quote_xlsx(&for_parse))
            .await
            .map_err(|_| Error::Internal("解析导入文件失败".to_string()))
        {
            Ok(Ok(parsed)) => parsed,
            Ok(Err(error)) => {
                let _ = self.storage.delete(&req.object_key).await;
                return Err(error);
            },
            Err(error) => return Err(error),
        };
        let registration = RegisterFileAssetRequest {
            storage_object_key: req.object_key.clone(),
            file_name: req.file_name.trim().to_string(),
            content_type: PRODUCT_IMPORT_XLSX_MIME.to_string(),
            byte_size: byte_len,
            content_hmac: content_fingerprint(&digest, &self.secret),
            sensitivity_class: SensitivityClass::General,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
        };
        let file_asset = FileAsset::new(FileAssetId::new(next_id()), registration.into_data(actor.id())?)?;
        self.create_job_from_parsed(
            file_asset,
            parsed,
            req.file_name.clone(),
            req.request_id.clone(),
            actor,
            &shared_bytes,
        )
        .await
    }
}

/// 由请求身份推导直传对象键；请求身份限定为 URL 安全字符。
fn direct_object_key(request_id: &str) -> Result<String> {
    if request_id.is_empty()
        || request_id.len() > 64
        || !request_id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(Error::ValidationError("请求身份非法，请重新选择文件".to_string()));
    }
    Ok(format!("{DIRECT_OBJECT_KEY_PREFIX}/{request_id}.xlsx"))
}

/// 校验对象键确由请求身份推导，防止操作他人直传对象。
fn require_bound_object_key(object_key: &str, request_id: &str) -> Result<()> {
    if object_key != direct_object_key(request_id)? {
        return Err(Error::ValidationError("对象键与请求身份不一致，请重新上传".to_string()));
    }
    Ok(())
}

/// 校验文件名为 `.xlsx`。
fn validate_xlsx_file_name(file_name: &str) -> Result<()> {
    let name = file_name.trim();
    if name.is_empty() || name.len() > 256 {
        return Err(Error::ValidationError("请选择产品报价表文件".to_string()));
    }
    let is_xlsx = name.rsplit('.').next().is_some_and(|ext| ext.eq_ignore_ascii_case("xlsx"));
    if !is_xlsx {
        return Err(Error::ValidationError("请上传 .xlsx 产品报价表".to_string()));
    }
    Ok(())
}

/// 校验申报大小在导入上限内。
fn validate_byte_size(byte_size: u64) -> Result<()> {
    if byte_size == 0 || byte_size > MAX_PRODUCT_IMPORT_FILE_BYTES {
        return Err(Error::ValidationError("导入文件不能超过 700 MB".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{direct_object_key, require_bound_object_key, validate_byte_size};

    #[test]
    fn direct_object_key_rejects_unsafe_request_id() {
        assert!(direct_object_key("a1B-_").is_ok());
        assert!(direct_object_key("").is_err());
        assert!(direct_object_key("../escape").is_err());
        assert!(direct_object_key("a/b").is_err());
        assert!(direct_object_key(&"a".repeat(65)).is_err());
    }

    #[test]
    fn bound_object_key_must_match_request_id() {
        let key = direct_object_key("req-1").unwrap();
        assert!(require_bound_object_key(&key, "req-1").is_ok());
        assert!(require_bound_object_key(&key, "req-2").is_err());
        assert!(require_bound_object_key("product-import-direct/other.xlsx", "req-1").is_err());
    }

    #[test]
    fn byte_size_must_be_within_limit() {
        assert!(validate_byte_size(1).is_ok());
        assert!(validate_byte_size(0).is_err());
        assert!(validate_byte_size(super::MAX_PRODUCT_IMPORT_FILE_BYTES + 1).is_err());
    }
}
