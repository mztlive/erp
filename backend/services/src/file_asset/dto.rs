//! 域 D05 `file_asset` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；本域无金额字段。

use entities::file_asset::{
    AttachmentUsage, ContentHmac, DocumentAttachment, DocumentAttachmentData, FileAsset, FileAssetData,
    RetentionClass, SecurityScanStatus, SensitivityClass,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 文件资产列表允许的排序字段白名单。
pub(crate) const FILE_ASSET_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
        None => SortDir::Desc,
    };
    Ok((sort_by, sort_dir))
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

/// 校验文本去除首尾空白后非空。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 文件资产列表响应视图（列表不暴露敏感对象存储键，§6.1）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileAssetListItemView {
    /// 实体主键。
    pub id: String,
    /// 展示文件名。
    pub file_name: String,
    /// 内容类型。
    pub content_type: String,
    /// 字节大小。
    pub byte_size: u64,
    /// 安全检查状态。
    pub security_scan_status: SecurityScanStatus,
    /// 敏感级别。
    pub sensitivity_class: SensitivityClass,
    /// 保留策略。
    pub retention_class: RetentionClass,
    /// 到期时间（秒级时间戳）。
    pub expires_at: Option<u64>,
    /// 创建人。
    pub created_by: String,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 文件资产详情视图（含对象存储键，供下载使用）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileAssetView {
    /// 实体主键。
    pub id: String,
    /// 加密受控对象存储中的不可猜测对象键。
    pub storage_object_key: String,
    /// 对象公开访问地址；仅上传/登记等写入路径由存储层拼接，其余查询路径为空。
    pub public_url: Option<String>,
    /// 展示文件名。
    pub file_name: String,
    /// 内容类型。
    pub content_type: String,
    /// 字节大小。
    pub byte_size: u64,
    /// 内容指纹（带密钥 HMAC-SHA256 十六进制）。
    pub content_hmac: String,
    /// 安全检查状态。
    pub security_scan_status: SecurityScanStatus,
    /// 敏感级别。
    pub sensitivity_class: SensitivityClass,
    /// 保留策略。
    pub retention_class: RetentionClass,
    /// 到期时间（秒级时间戳）。
    pub expires_at: Option<u64>,
    /// 销毁审计时间（秒级时间戳）。
    pub destroyed_at: Option<u64>,
    /// 创建人。
    pub created_by: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<FileAsset> for FileAssetView {
    /// 从实体构造详情视图。
    fn from(asset: FileAsset) -> Self {
        Self {
            id: asset.base.id,
            storage_object_key: asset.storage_object_key,
            public_url: None,
            file_name: asset.file_name,
            content_type: asset.content_type,
            byte_size: asset.byte_size,
            content_hmac: asset.content_hmac.to_string(),
            security_scan_status: asset.security_scan_status,
            sensitivity_class: asset.sensitivity_class,
            retention_class: asset.retention_class,
            expires_at: asset.expires_at.map(|instant| instant.unix_secs() as u64),
            destroyed_at: asset.destroyed_at.map(|instant| instant.unix_secs() as u64),
            created_by: asset.created_by,
            version: asset.base.version,
            created_at: asset.base.created_at,
        }
    }
}

/// 文件资产列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FileAssetListParams {
    /// 文件名模糊筛选（忽略大小写）。
    pub file_name: Option<String>,
    /// 安全检查状态筛选。
    pub security_scan_status: Option<SecurityScanStatus>,
    /// 保留策略筛选。
    pub retention_class: Option<RetentionClass>,
    /// 敏感级别筛选。
    pub sensitivity_class: Option<SensitivityClass>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的文件资产列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileAssetListQuery {
    /// 文件名模糊筛选。
    pub file_name: Option<String>,
    /// 安全检查状态筛选。
    pub security_scan_status: Option<SecurityScanStatus>,
    /// 保留策略筛选。
    pub retention_class: Option<RetentionClass>,
    /// 敏感级别筛选。
    pub sensitivity_class: Option<SensitivityClass>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl FileAssetListParams {
    /// 归一化文件资产列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<FileAssetListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, FILE_ASSET_SORT_FIELDS)?;
        Ok(FileAssetListQuery {
            file_name: normalized_text(self.file_name.as_deref()),
            security_scan_status: self.security_scan_status,
            retention_class: self.retention_class,
            sensitivity_class: self.sensitivity_class,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 文件资产登记请求（HTTP 契约：`{ storage_object_key, file_name, content_type,
/// byte_size, content_hmac, sensitivity_class, retention_class, expires_at }`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RegisterFileAssetRequest {
    /// 加密受控对象存储中的不可猜测对象键。
    #[validate(custom(function = "non_blank", message = "对象键不能为空"))]
    pub storage_object_key: String,
    /// 展示文件名。
    #[validate(custom(function = "non_blank", message = "文件名不能为空"))]
    pub file_name: String,
    /// 内容类型。
    #[validate(custom(function = "non_blank", message = "内容类型不能为空"))]
    pub content_type: String,
    /// 字节大小。
    pub byte_size: u64,
    /// 内容指纹（带密钥 HMAC-SHA256 十六进制，64 字符）。
    #[validate(custom(function = "non_blank", message = "内容指纹不能为空"))]
    pub content_hmac: String,
    /// 敏感级别。
    pub sensitivity_class: SensitivityClass,
    /// 保留策略。
    pub retention_class: RetentionClass,
    /// 到期时间（秒级时间戳；非长期保留策略必填）。
    #[validate(range(min = 1, message = "到期时间必须大于 0"))]
    pub expires_at: Option<u64>,
}

/// 随业务命令一并提交、尚未登记的文件资产。
///
/// `reference` 是本次 multipart 请求内的临时引用；业务 DTO 中对应的
/// `FileAssetId` 使用同一值，服务在事务前将其替换为新生成的正式资产 ID。
#[derive(Debug, Clone)]
pub struct PendingFileAssetRequest {
    /// 本次请求内唯一的临时引用。
    pub reference: String,
    /// 已写入对象存储、尚未登记到 MongoDB 的元数据。
    pub registration: RegisterFileAssetRequest,
}

impl RegisterFileAssetRequest {
    /// 转换为实体创建数据。
    ///
    /// # 参数
    /// * `created_by` - 创建人
    ///
    /// # 返回
    /// 返回实体层创建数据。
    ///
    /// # 错误
    /// 内容指纹形态非法时返回错误。
    pub(crate) fn into_data(self, created_by: &str) -> Result<FileAssetData> {
        Ok(FileAssetData {
            storage_object_key: self.storage_object_key,
            file_name: self.file_name,
            content_type: self.content_type,
            byte_size: self.byte_size,
            content_hmac: ContentHmac::parse(self.content_hmac)?,
            sensitivity_class: self.sensitivity_class,
            retention_class: self.retention_class,
            expires_at: self
                .expires_at
                .map(|secs| entities::common::time::Instant::from_unix_secs(secs as i64)),
            created_by: created_by.to_string(),
        })
    }
}

/// 单据附件关联请求（HTTP 契约：`{ document_id, file_asset_id, usage }`；
/// 记录人由服务端从鉴权上下文注入）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AttachToDocumentRequest {
    /// 业务单据（`business_document` 稳定注册）。
    pub document_id: entities::ids::BusinessDocumentId,
    /// 受控文件资产。
    pub file_asset_id: entities::ids::FileAssetId,
    /// 用途。
    pub usage: AttachmentUsage,
}

impl AttachToDocumentRequest {
    /// 转换为实体创建数据。
    ///
    /// # 参数
    /// * `created_by` - 记录人（账号或系统身份）
    ///
    /// # 返回
    /// 返回实体层创建数据。
    pub(crate) fn into_data(self, created_by: &str) -> DocumentAttachmentData {
        DocumentAttachmentData {
            document_id: self.document_id,
            file_asset_id: self.file_asset_id,
            usage: self.usage,
            created_by: created_by.to_string(),
        }
    }
}

/// 单据附件响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentAttachmentView {
    /// 实体主键。
    pub id: String,
    /// 业务单据 ID。
    pub document_id: String,
    /// 受控文件资产 ID。
    pub file_asset_id: String,
    /// 用途。
    pub usage: AttachmentUsage,
    /// 记录人。
    pub created_by: String,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<DocumentAttachment> for DocumentAttachmentView {
    /// 从实体构造响应视图。
    fn from(attachment: DocumentAttachment) -> Self {
        Self {
            id: attachment.base.id,
            document_id: attachment.document_id.to_string(),
            file_asset_id: attachment.file_asset_id.to_string(),
            usage: attachment.usage,
            created_by: attachment.created_by,
            created_at: attachment.base.created_at,
        }
    }
}

/// 安全检查结果更新请求（携带乐观锁版本，冲突返回 409）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MarkScanResultRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 检查结果（通过/拒绝/隔离；隔离后经人工复核可放行或拒绝）。
    pub security_scan_status: SecurityScanStatus,
}

/// 销毁文件资产请求（携带乐观锁版本，冲突返回 409）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DestroyFileAssetRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, AttachToDocumentRequest, FileAssetListParams, RegisterFileAssetRequest, SortDir,
    };
    use entities::file_asset::{AttachmentUsage, RetentionClass, SensitivityClass};
    use serde_json::json;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields() {
        assert!(normalize_sort(&Some("file_name".to_string()), &None, &["created_at"]).is_err());
        let (field, direction) = normalize_sort(
            &Some(" updated_at ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "updated_at"],
        )
        .unwrap();
        assert_eq!(field, "updated_at");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn list_params_normalize_paging_and_filters() {
        let params = FileAssetListParams {
            file_name: Some(" 导入.xlsx ".to_string()),
            security_scan_status: None,
            retention_class: Some(RetentionClass::ThirtyDays),
            sensitivity_class: Some(SensitivityClass::Sensitive),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.file_name.as_deref(), Some("导入.xlsx"));
        assert_eq!(query.retention_class, Some(RetentionClass::ThirtyDays));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
    }

    #[test]
    fn register_request_converts_with_validated_hmac() {
        let request: RegisterFileAssetRequest = serde_json::from_value(json!({
            "storage_object_key": "obj/2025/08/abc123",
            "file_name": "导入清单.xlsx",
            "content_type": "application/vnd.ms-excel",
            "byte_size": 2048,
            "content_hmac": "a".repeat(64),
            "sensitivity_class": "sensitive",
            "retention_class": "thirty_days",
            "expires_at": 1703260800,
        }))
        .unwrap();
        let data = request.into_data("admin-1").unwrap();
        assert_eq!(data.created_by, "admin-1");
        assert_eq!(data.content_hmac.as_str(), &"a".repeat(64));

        let bad = RegisterFileAssetRequest {
            content_hmac: "not-a-hex".to_string(),
            file_name: "x".to_string(),
            content_type: "x".to_string(),
            byte_size: 1,
            storage_object_key: "obj/x".to_string(),
            sensitivity_class: entities::file_asset::SensitivityClass::General,
            retention_class: entities::file_asset::RetentionClass::LongTerm,
            expires_at: None,
        };
        assert!(bad.into_data("admin-1").is_err());
    }

    #[test]
    fn attach_request_keeps_document_and_asset_links() {
        let request: AttachToDocumentRequest = serde_json::from_value(json!({
            "document_id": "doc-1",
            "file_asset_id": "file-1",
            "usage": "manifest",
        }))
        .unwrap();
        assert_eq!(request.usage, AttachmentUsage::Manifest);
        let data = request.into_data("admin-1");
        assert_eq!(data.document_id.as_ref(), "doc-1");
        assert_eq!(data.created_by, "admin-1");
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = FileAssetListParams {
            file_name: None,
            security_scan_status: None,
            retention_class: None,
            sensitivity_class: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }
}
