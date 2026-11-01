//! 导入批次应用请求（后台应用阶段逐行结果汇总）的 DTO。

use erp_core::ids::LegacyImportRowId;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// 单行导入结果（`outcome` 决定行级状态迁移，数据模型 §6.12/§11.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyRowOutcome {
    /// 已导入（登记成功目标）。
    Imported,
    /// 失败（必须携带错误码）。
    Failed,
    /// 跳过（必须携带原因错误码）。
    Skipped,
}

/// 行级导入结果请求。
///
/// 唯一 ID 与 `imported`/`failed`/`skipped` 精确字段形状由
/// [`crate::entity::legacy_import::ApplyResultSet`] 在查库前强制收紧。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ApplyRowResult {
    /// 目标导入行。
    pub row_id: LegacyImportRowId,
    /// 行级结果。
    pub outcome: ApplyRowOutcome,
    /// 来源稳定身份（试算阶段在 D01 建立；待映射行必填）。
    pub external_identity_map_id: Option<erp_core::ids::ExternalIdentityMapId>,
    /// 成功结果目标单据 ID（`Imported` 必填）。
    pub target_document_id: Option<String>,
    /// 成功结果目标对象引用。
    pub target_object_reference: Option<String>,
    /// 失败原因错误码（`Failed`/`Skipped` 必填）。
    pub error_code: Option<String>,
    /// 失败原因明细。
    pub error_detail: Option<String>,
}

/// 导入批次应用请求（后台应用阶段的逐行结果汇总）。
///
/// 集合长度由本 DTO 校验；重复 ID 与 outcome 字段形状由 `ApplyResultSet` 拒绝。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ApplyLegacyImportBatchRequest {
    /// 逐行结果（可分批提交；未提交行保持待导入）。
    #[validate(length(min = 1, max = 1000, message = "行结果数量必须在1-1000之间"))]
    pub results: Vec<ApplyRowResult>,
}

/// 客户行导入前必须命中的 ERP 主体类型代码（数据模型 §6.12 来源对象集合）。
pub const CUSTOMER_OBJECT_TYPE: &str = "CUSTOMER";
/// 客户行目标主体缺失的错误码（W18 问题代码口径）。
pub const CUSTOMER_NOT_FOUND_ERROR_CODE: &str = "CUSTOMER_NOT_FOUND";
/// 客户行目标主体缺失的错误明细。
pub const CUSTOMER_NOT_FOUND_ERROR_DETAIL: &str = "目标客户主体不存在，禁止导入";
