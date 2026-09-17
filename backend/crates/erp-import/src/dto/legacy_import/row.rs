//! 导入行列表查询与响应视图的 DTO。

use application_core::normalized_text;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{LEGACY_IMPORT_ROW_SORT_FIELDS, PageParams, normalize_paging};
use crate::entity::legacy_import::{ImportStatus, MappingStatus, ParseStatus};
use crate::error::Result;

/// 导入行响应视图（列表投影形状，规范化载荷不进入列表）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LegacyImportRowView {
    /// 实体主键。
    pub id: String,
    /// 所属导入批次。
    pub batch_id: String,
    /// 来源对象类型。
    pub source_object_type: String,
    /// 批次内来源行身份。
    pub source_row_key: String,
    /// 解析状态。
    pub parse_status: ParseStatus,
    /// 映射状态。
    pub mapping_status: MappingStatus,
    /// 导入状态。
    pub import_status: ImportStatus,
    /// 来源稳定身份。
    pub external_identity_map_id: Option<String>,
    /// 失败原因错误码。
    pub error_code: Option<String>,
    /// 成功结果目标单据 ID。
    pub target_document_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 导入行列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LegacyImportRowListParams {
    /// 来源对象类型筛选。
    pub source_object_type: Option<String>,
    /// 解析状态筛选。
    pub parse_status: Option<ParseStatus>,
    /// 映射状态筛选。
    pub mapping_status: Option<MappingStatus>,
    /// 导入状态筛选。
    pub import_status: Option<ImportStatus>,
    /// 来源行键模糊筛选。
    pub source_row_key: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`source_row_key`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的导入行列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyImportRowListQuery {
    /// 来源对象类型筛选。
    pub source_object_type: Option<String>,
    /// 解析状态筛选。
    pub parse_status: Option<ParseStatus>,
    /// 映射状态筛选。
    pub mapping_status: Option<MappingStatus>,
    /// 导入状态筛选。
    pub import_status: Option<ImportStatus>,
    /// 来源行键模糊筛选。
    pub source_row_key: Option<String>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl LegacyImportRowListParams {
    /// 归一化导入行列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub fn normalized(&self) -> Result<LegacyImportRowListQuery> {
        Ok(LegacyImportRowListQuery {
            source_object_type: normalized_text(self.source_object_type.as_deref()),
            parse_status: self.parse_status,
            mapping_status: self.mapping_status,
            import_status: self.import_status,
            source_row_key: normalized_text(self.source_row_key.as_deref()),
            paging: normalize_paging(
                &self.sort_by,
                &self.sort_dir,
                self.page,
                self.page_size,
                LEGACY_IMPORT_ROW_SORT_FIELDS,
            )?,
        })
    }
}
