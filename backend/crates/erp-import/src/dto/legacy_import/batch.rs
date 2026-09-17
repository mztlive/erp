//! 导入批次与行请求的 DTO（批次头、来源行、列表查询与响应视图）。

use application_core::{non_blank, normalized_text};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{FileAssetId, SourceSystemId};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{LEGACY_IMPORT_BATCH_SORT_FIELDS, PageParams, normalize_paging};
use crate::entity::legacy_import::{LegacyImportBatch, LegacyImportBatchStatus};
use crate::error::Result;

/// 导入行创建请求（行级来源身份与规范化载荷，数据模型 §6.12）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ImportRowRequest {
    /// 来源对象类型（客户、供应商、SPU、SKU、卡券销售等）。
    #[validate(custom(function = "non_blank", message = "来源对象类型不能为空"))]
    pub source_object_type: String,
    /// 批次内来源行身份（与批次、对象类型构成唯一约束）。
    #[validate(custom(function = "non_blank", message = "来源行键不能为空"))]
    pub source_row_key: String,
    /// 仅含白名单字段的规范化行。
    #[validate(custom(function = "non_blank", message = "规范化载荷不能为空"))]
    pub normalized_payload_reference: String,
}

/// 导入批次创建请求（HTTP 契约：批次头 + 来源行列表，1–1000 行）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateLegacyImportBatchRequest {
    /// 导入批次号（唯一；重复提交视为幂等，返回既有批次）。
    #[validate(custom(function = "non_blank", message = "批次号不能为空"))]
    pub batch_no: String,
    /// 来源系统。
    pub source_system_id: SourceSystemId,
    /// 本批来源对象集合。
    #[validate(custom(function = "non_blank", message = "来源对象集合不能为空"))]
    pub source_object_set: String,
    /// 期初业务基准日（`YYYY-MM-DD`）。
    pub baseline_date: BusinessDate,
    /// 本批解析、清理和映射规则版本。
    #[validate(custom(function = "non_blank", message = "导入规则版本不能为空"))]
    pub import_rule_version: String,
    /// 受控临时区计算的 keyed HMAC（仅用于审计去重）。
    pub source_file_hmac: Option<String>,
    /// 成功白名单包资产（存在时校验对应 `file_asset`）。
    pub successful_sanitized_file_asset_id: Option<FileAssetId>,
    /// 成功 manifest 资产（存在时校验对应 `file_asset`）。
    pub success_manifest_file_asset_id: Option<FileAssetId>,
    /// 失败诊断包资产（存在时校验对应 `file_asset`）。
    pub failure_diagnostic_file_asset_id: Option<FileAssetId>,
    /// 本批来源行。
    #[validate(length(min = 1, max = 1000, message = "导入行数量必须在1-1000之间"))]
    #[validate(nested)]
    pub rows: Vec<ImportRowRequest>,
}

/// 导入批次响应视图（详情与创建共用，字段与数据模型 §6.12 一致）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LegacyImportBatchView {
    /// 实体主键。
    pub id: String,
    /// 导入批次号。
    pub batch_no: String,
    /// 来源系统。
    pub source_system_id: String,
    /// 本批来源对象集合。
    pub source_object_set: String,
    /// 期初业务基准日。
    pub baseline_date: BusinessDate,
    /// 成功对象的白名单包。
    pub successful_sanitized_file_asset_id: Option<String>,
    /// 成功对象的 manifest。
    pub success_manifest_file_asset_id: Option<String>,
    /// 失败对象的合规诊断包。
    pub failure_diagnostic_file_asset_id: Option<String>,
    /// 本批解析、清理和映射规则版本。
    pub import_rule_version: String,
    /// 受控临时区计算的 keyed HMAC。
    pub source_file_hmac: Option<String>,
    /// 批次状态。
    pub status: LegacyImportBatchStatus,
    /// 处理统计：总行数。
    pub total_rows: u64,
    /// 处理统计：成功行数。
    pub success_rows: u64,
    /// 处理统计：失败行数。
    pub failed_rows: u64,
    /// 脱敏错误码及计数。
    pub failure_code_summary: Option<String>,
    /// 各必要 `legacy_import_confirmation` 的派生摘要。
    pub confirmation_status_summary: Option<String>,
    /// 登记的后台任务 ID（`background_job.request_id` = 批次号）。
    pub background_job_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<LegacyImportBatch> for LegacyImportBatchView {
    /// 从实体构造响应视图（不含后台任务关联，由 Service 另行补充）。
    ///
    /// # 参数
    /// * `batch` - 导入批次实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(batch: LegacyImportBatch) -> Self {
        Self {
            id: batch.base.id,
            batch_no: batch.batch_no,
            source_system_id: batch.source_system_id.to_string(),
            source_object_set: batch.source_object_set,
            baseline_date: batch.baseline_date,
            successful_sanitized_file_asset_id: batch
                .successful_sanitized_file_asset_id
                .map(|id| id.to_string()),
            success_manifest_file_asset_id: batch.success_manifest_file_asset_id.map(|id| id.to_string()),
            failure_diagnostic_file_asset_id: batch.failure_diagnostic_file_asset_id.map(|id| id.to_string()),
            import_rule_version: batch.import_rule_version,
            source_file_hmac: batch.source_file_hmac,
            status: batch.status,
            total_rows: batch.total_rows,
            success_rows: batch.success_rows,
            failed_rows: batch.failed_rows,
            failure_code_summary: batch.failure_code_summary,
            confirmation_status_summary: batch.confirmation_status_summary,
            background_job_id: None,
            version: batch.base.version,
            created_at: batch.base.created_at,
        }
    }
}

/// 导入批次列表响应项（投影形状，与仓储 `LegacyImportBatchRow` 对齐）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LegacyImportBatchListItem {
    /// 实体主键。
    pub id: String,
    /// 导入批次号。
    pub batch_no: String,
    /// 来源系统。
    pub source_system_id: String,
    /// 本批来源对象集合。
    pub source_object_set: String,
    /// 期初业务基准日。
    pub baseline_date: BusinessDate,
    /// 导入规则版本。
    pub import_rule_version: String,
    /// 批次状态。
    pub status: LegacyImportBatchStatus,
    /// 处理统计：总行数。
    pub total_rows: u64,
    /// 处理统计：成功行数。
    pub success_rows: u64,
    /// 处理统计：失败行数。
    pub failed_rows: u64,
    /// 脱敏错误码摘要。
    pub failure_code_summary: Option<String>,
    /// 确认状态派生摘要。
    pub confirmation_status_summary: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 导入批次列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct LegacyImportBatchListParams {
    /// 批次号模糊筛选（字面量、忽略大小写）。
    pub batch_no: Option<String>,
    /// 来源系统筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 批次状态筛选。
    pub status: Option<LegacyImportBatchStatus>,
    /// 期初基准日起（含）。
    pub baseline_date_from: Option<BusinessDate>,
    /// 期初基准日止（含）。
    pub baseline_date_to: Option<BusinessDate>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`batch_no`/`baseline_date`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的导入批次列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyImportBatchListQuery {
    /// 批次号模糊筛选。
    pub batch_no: Option<String>,
    /// 来源系统筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 批次状态筛选。
    pub status: Option<LegacyImportBatchStatus>,
    /// 期初基准日起（含）。
    pub baseline_date_from: Option<BusinessDate>,
    /// 期初基准日止（含）。
    pub baseline_date_to: Option<BusinessDate>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl LegacyImportBatchListParams {
    /// 归一化导入批次列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub fn normalized(&self) -> Result<LegacyImportBatchListQuery> {
        Ok(LegacyImportBatchListQuery {
            batch_no: normalized_text(self.batch_no.as_deref()),
            source_system_id: self.source_system_id.clone(),
            status: self.status,
            baseline_date_from: self.baseline_date_from,
            baseline_date_to: self.baseline_date_to,
            paging: normalize_paging(
                &self.sort_by,
                &self.sort_dir,
                self.page,
                self.page_size,
                LEGACY_IMPORT_BATCH_SORT_FIELDS,
            )?,
        })
    }
}
