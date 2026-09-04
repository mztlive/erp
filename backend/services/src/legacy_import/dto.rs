//! 域 D22 `legacy_import` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；业务日期 `baseline_date`
//! 为 `YYYY-MM-DD` 字符串；本域无金额字段。

use entities::bulk_job::JobStatus;
use entities::common::time::BusinessDate;
use entities::ids::{FileAssetId, LegacyImportBatchId, LegacyImportRowId, SourceSystemId, WorkItemId};
use entities::legacy_import::{
    ConfirmationDecision, ConfirmationScope, ConfirmationStatus, ImportStatus, LegacyImportBatch,
    LegacyImportBatchStatus, LegacyImportConfirmation, MappingStatus, ParseStatus,
};
use entities::work_item::{WorkItemStatus, WorkItemType};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

use super::receipt::{optional_text, parse_command_version, required_text};

/// 导入批次列表允许的排序字段白名单（与仓储投影白名单一致，Service 层先校验）。
pub(crate) const LEGACY_IMPORT_BATCH_SORT_FIELDS: &[&str] = &["created_at", "batch_no", "baseline_date"];
/// 导入行列表允许的排序字段白名单。
pub(crate) const LEGACY_IMPORT_ROW_SORT_FIELDS: &[&str] = &["created_at", "source_row_key"];
/// 导入确认列表允许的排序字段白名单。
pub(crate) const LEGACY_IMPORT_CONFIRMATION_SORT_FIELDS: &[&str] = &["created_at", "trial_version"];

/// 排序方向。
pub use crate::query::SortDir;

/// 归一化后的分页查询参数（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验）。
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
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串不生效）。
use crate::query::non_blank;

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
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
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
pub(crate) struct LegacyImportBatchListQuery {
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
    pub(crate) fn normalized(&self) -> Result<LegacyImportBatchListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, LEGACY_IMPORT_BATCH_SORT_FIELDS)?;
        Ok(LegacyImportBatchListQuery {
            batch_no: normalized_text(self.batch_no.as_deref()),
            source_system_id: self.source_system_id.clone(),
            status: self.status,
            baseline_date_from: self.baseline_date_from,
            baseline_date_to: self.baseline_date_to,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

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
pub(crate) struct LegacyImportRowListQuery {
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
    pub(crate) fn normalized(&self) -> Result<LegacyImportRowListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, LEGACY_IMPORT_ROW_SORT_FIELDS)?;
        Ok(LegacyImportRowListQuery {
            source_object_type: normalized_text(self.source_object_type.as_deref()),
            parse_status: self.parse_status,
            mapping_status: self.mapping_status,
            import_status: self.import_status,
            source_row_key: normalized_text(self.source_row_key.as_deref()),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 导入确认创建请求（数据模型 §6.12：每批按版本化确认矩阵为必要范围各创建一个事实）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateLegacyImportConfirmationRequest {
    /// 所属导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 责任范围（销售、采购、运营、仓储、财务等）。
    #[validate(custom(function = "non_blank", message = "确认范围不能为空"))]
    pub confirmation_scope: String,
    /// 本次确认针对的批次版本。
    pub batch_version: u32,
    /// 本次确认针对的试算版本（`(batch_id, scope, trial_version)` 唯一）。
    pub trial_version: u32,
    /// 本次确认针对的导入规则版本。
    #[validate(custom(function = "non_blank", message = "导入规则版本不能为空"))]
    pub import_rule_version: String,
}

/// 导入业务确认的领域决定。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ImportBusinessConfirmationDecision {
    /// 命令锁定的导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 客户端最近查询到的批次乐观锁版本。
    #[validate(custom(function = "non_blank", message = "批次版本不能为空"))]
    pub expected_batch_version: String,
    /// 命令锁定的试算版本。
    #[validate(custom(function = "non_blank", message = "试算版本不能为空"))]
    pub expected_trial_version: String,
    /// 服务端固定注册表中的责任范围。
    #[validate(custom(function = "non_blank", message = "确认范围不能为空"))]
    pub confirmation_scope: String,
    /// 确认本范围或退回修复。
    pub action: ConfirmationDecision,
    /// 退回原因代码（退回时必填）。
    pub reason_code: Option<String>,
    /// 意见说明（确认意见可选）。
    pub comment: Option<String>,
}

/// `CompleteImportBusinessConfirmation` 强类型命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CompleteImportBusinessConfirmationCommand {
    /// 当前 `IMPORT_BUSINESS_CONFIRMATION` 正式任务。
    pub work_item_id: WorkItemId,
    /// 客户端读取到的任务乐观锁版本。
    #[validate(custom(function = "non_blank", message = "任务版本不能为空"))]
    pub expected_task_version: String,
    /// 任务冻结的批次试算主体版本。
    #[validate(custom(function = "non_blank", message = "任务主体版本不能为空"))]
    pub expected_subject_version: String,
    /// 业务对象、版本与正式结论只存在于该强类型信封。
    #[validate(nested)]
    pub decision: ImportBusinessConfirmationDecision,
    /// 正式操作幂等键；只以服务端不可逆摘要进入审计。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 导入确认任务的服务端真实投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportBusinessConfirmationWorkItemView {
    /// 任务稳定 ID。
    pub work_item_id: String,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 任务乐观锁版本。
    pub task_version: String,
    /// 任务冻结的主体版本。
    pub subject_version: String,
    /// 任务生命周期状态。
    pub status: WorkItemStatus,
    /// 固定责任角色。
    pub owner_role: String,
    /// 固定责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人。
    pub owner_user_id: Option<String>,
    /// 导入确认不依赖审批步骤，固定为 `READY`。
    pub processing_state: String,
    /// 当前查询无操作人上下文，动作必须失败关闭。
    pub allowed_actions: Vec<String>,
    /// 当前投影无额外审批阻断事实。
    pub action_blockers: Vec<String>,
    /// 服务端任务处理器键。
    pub handler_key: String,
    /// 服务端目标工作面。
    pub destination_workspace_id: String,
}

/// 导入确认响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LegacyImportConfirmationView {
    /// 实体主键。
    pub id: String,
    /// 所属导入批次。
    pub batch_id: String,
    /// 责任范围。
    pub confirmation_scope: String,
    /// 责任角色。
    pub owner_role: String,
    /// 本次确认针对的批次版本。
    pub batch_version: u32,
    /// 本次确认针对的试算版本。
    pub trial_version: u32,
    /// 确认状态。
    pub status: ConfirmationStatus,
    /// 确认决策；待确认/失效时为空。
    pub decision: Option<ConfirmationDecision>,
    /// 退回原因代码。
    pub reason_code: Option<String>,
    /// 意见说明。
    pub comment: Option<String>,
    /// 对应 `IMPORT_BUSINESS_CONFIRMATION` 正式任务。
    pub work_item_id: String,
    /// 任务实时投影；关联任务缺失时为空并由客户端失败关闭。
    pub work_item: Option<ImportBusinessConfirmationWorkItemView>,
    /// 实际确认或退回人。
    pub decided_by: Option<String>,
    /// 实际确认或退回时间（秒级时间戳）。
    pub decided_at: Option<i64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<LegacyImportConfirmation> for LegacyImportConfirmationView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `confirmation` - 导入确认事实实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(confirmation: LegacyImportConfirmation) -> Self {
        Self {
            id: confirmation.base.id,
            batch_id: confirmation.batch_id.to_string(),
            confirmation_scope: confirmation.confirmation_scope,
            owner_role: confirmation.owner_role,
            batch_version: confirmation.batch_version,
            trial_version: confirmation.trial_version,
            status: confirmation.status,
            decision: confirmation.decision,
            reason_code: confirmation.reason_code,
            comment: confirmation.comment,
            work_item_id: confirmation.work_item_id.to_string(),
            work_item: None,
            decided_by: confirmation.decided_by,
            decided_at: confirmation.decided_at.map(|at| at.unix_secs()),
            version: confirmation.base.version,
            created_at: confirmation.base.created_at,
        }
    }
}

/// 强类型确认命令的最终结果状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportBusinessConfirmationResultStatus {
    /// 已确认责任范围。
    Confirmed,
    /// 已形成退回修复结论。
    Rejected,
    /// 结果未知；当前同步实现不主动返回此值。
    Unknown,
}

/// 强类型确认完成后的固定下一步。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportBusinessConfirmationNextStep {
    /// 等待同一试算矩阵的其它责任范围。
    AwaitOtherConfirmations,
    /// 全部必要范围已确认，可进入应用阶段。
    StartApply,
    /// 修复问题并产生新试算版本。
    FixAndRevalidate,
}

/// `CompleteImportBusinessConfirmation` 稳定结果信封。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompleteImportBusinessConfirmationResult {
    /// 领域结论状态。
    pub result_status: ImportBusinessConfirmationResultStatus,
    /// 不可变的确认或退回事实。
    pub confirmation: LegacyImportConfirmationView,
    /// 已完成的正式任务投影。
    pub work_item: ImportBusinessConfirmationWorkItemView,
    /// 事务提交后的批次乐观锁版本。
    pub batch_version: u64,
    /// 服务端确定的下一步。
    pub next_step: ImportBusinessConfirmationNextStep,
    /// 不含原始幂等键的稳定审计收据 ID。
    pub audit_receipt: String,
}

/// 导入应用阶段强命令动作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportExecutionAction {
    /// 提交应用：待应用批次进入导入中，后台任务开始运行。
    StartApply,
    /// 取消尚未应用的项；已形成的业务事实不回滚。
    CancelPending,
    /// 仅把上一轮失败项重新准备为待应用。
    RetryFailed,
}

impl ImportExecutionAction {
    /// 返回稳定的 wire code。
    ///
    /// # 返回
    /// 返回 `START_APPLY` / `CANCEL_PENDING` / `RETRY_FAILED`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StartApply => "START_APPLY",
            Self::CancelPending => "CANCEL_PENDING",
            Self::RetryFailed => "RETRY_FAILED",
        }
    }
}

/// W18 导入执行强类型命令。
///
/// 命令与责任确认分离：确认完成只使批次就绪，本命令中的
/// `START_APPLY` 才能启动后台应用。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ImportExecutionCommand {
    /// 命令锁定的导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 客户端最近读取到的批次乐观锁版本（wire string）。
    #[validate(custom(function = "non_blank", message = "批次版本不能为空"))]
    pub expected_batch_version: String,
    /// 命令锁定的当前试算版本；提交应用和重试失败项时必填。
    pub expected_trial_version: Option<String>,
    /// 执行动作。
    pub action: ImportExecutionAction,
    /// 结构化原因码；取消尚未应用项时必填。
    #[validate(length(max = 128, message = "原因码过长"))]
    pub reason_code: Option<String>,
    /// 操作说明。
    #[validate(length(max = 1024, message = "操作说明过长"))]
    pub comment: Option<String>,
    /// 请求幂等身份；原值不进入审计消息。
    #[validate(
        length(max = 128, message = "请求身份过长"),
        custom(function = "non_blank", message = "请求身份不能为空")
    )]
    pub request_id: String,
}

/// 导入执行命令的稳定结果状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportExecutionResultStatus {
    /// 后台应用已启动。
    Started,
    /// 尚未应用的项已取消。
    Cancelled,
    /// 失败项已重新准备，尚未启动应用。
    RetryPrepared,
    /// 交易结果待核实；当前同步实现不主动返回此值。
    Unknown,
}

/// 导入执行命令完成后的固定下一步。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportExecutionNextStep {
    /// 查看后台应用进度。
    MonitorProgress,
    /// 查看取消后的最终分区结果。
    ReviewResult,
    /// 失败项已准备，需要显式提交应用。
    StartApply,
}

/// W18 导入执行强命令稳定结果信封。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportExecutionResult {
    /// 本次执行动作。
    pub action: ImportExecutionAction,
    /// 命令结果状态。
    pub result_status: ImportExecutionResultStatus,
    /// 导入批次 ID。
    pub batch_id: String,
    /// 交易提交后的批次状态。
    pub batch_status: LegacyImportBatchStatus,
    /// 交易提交后的批次版本（wire string）。
    pub batch_version: String,
    /// 本次命令锁定的试算版本。
    pub trial_version: Option<String>,
    /// 对应后台任务 ID。
    pub background_job_id: String,
    /// 交易提交后的后台任务状态。
    pub background_job_status: JobStatus,
    /// 交易提交后的后台任务版本（wire string）。
    pub background_job_version: String,
    /// 本次启动、取消或重新准备的项数。
    pub affected_items: u64,
    /// 服务端确定的下一步。
    pub next_step: ImportExecutionNextStep,
    /// 不含原始 `request_id` 的稳定审计收据 ID。
    pub audit_receipt: String,
}

/// 已规范化的确认完成命令（INT-E24 DTO 归属）。
///
/// 持有强类型版本与动作专属必填/禁止矩阵已校验的字段；Service 只解释
/// 持久化事实，不再做二次解析或字符串中转。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedConfirmationCompletion {
    /// 当前正式任务。
    pub work_item_id: WorkItemId,
    /// 命令锁定的导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 客户端读取到的任务乐观锁版本。
    pub expected_task_version: u64,
    /// 任务冻结的批次试算主体版本。
    pub expected_subject_version: String,
    /// 客户端读取到的批次乐观锁版本。
    pub expected_batch_version: u64,
    /// 命令锁定的试算版本。
    pub expected_trial_version: u32,
    /// 已注册确认范围。
    pub confirmation_scope: String,
    /// 确认或退回动作。
    pub decision: ConfirmationDecision,
    /// 退回原因代码（退回时必填，确认时禁止）。
    pub reason_code: Option<String>,
    /// 意见说明。
    pub comment: Option<String>,
    /// 正式操作幂等键。
    pub idempotency_key: String,
}

impl TryFrom<CompleteImportBusinessConfirmationCommand> for PreparedConfirmationCompletion {
    type Error = Error;

    /// 从强类型命令构造已规范化的完成准备（INT-E24 纯转换）。
    ///
    /// # 参数
    /// * `command` - 强类型完成命令
    ///
    /// # 返回
    /// 返回动作矩阵已校验的准备。
    ///
    /// # 错误
    /// 范围未注册、版本非法或动作专属原因矩阵非法时返回错误。
    fn try_from(command: CompleteImportBusinessConfirmationCommand) -> Result<Self> {
        let decision = command.decision;
        let confirmation_scope = ConfirmationScope::parse(&decision.confirmation_scope)?
            .as_str()
            .to_string();
        let reason_code = optional_text(decision.reason_code);
        if decision.action == ConfirmationDecision::ReturnForFix && reason_code.is_none() {
            return Err(Error::ValidationError("退回修复必须提供原因代码".to_string()));
        }
        if decision.action == ConfirmationDecision::ConfirmScope && reason_code.is_some() {
            return Err(Error::ValidationError("确认责任范围不得携带退回原因".to_string()));
        }
        Ok(Self {
            work_item_id: command.work_item_id,
            batch_id: decision.batch_id,
            expected_task_version: parse_command_version(&command.expected_task_version, "任务版本")?,
            expected_subject_version: required_text(
                &command.expected_subject_version,
                "任务主体版本不能为空",
            )?,
            expected_batch_version: parse_command_version(&decision.expected_batch_version, "批次版本")?,
            expected_trial_version: parse_command_version(&decision.expected_trial_version, "试算版本")?,
            confirmation_scope,
            decision: decision.action,
            reason_code,
            comment: optional_text(decision.comment),
            idempotency_key: required_text(&command.idempotency_key, "幂等键不能为空")?,
        })
    }
}

/// 已规范化的导入执行命令（INT-E24 DTO 归属）。
///
/// 持有强类型版本与动作专属试算/原因矩阵已校验的字段；Service 只解释
/// 持久化事实，不再做二次解析。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedImportExecution {
    /// 命令锁定的导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 客户端读取到的批次乐观锁版本。
    pub expected_batch_version: u64,
    /// 命令锁定的试算版本；提交应用和重试失败项时必填。
    pub expected_trial_version: Option<u32>,
    /// 执行动作。
    pub action: ImportExecutionAction,
    /// 结构化原因码；取消时必填，提交应用时禁止。
    pub reason_code: Option<String>,
    /// 操作说明。
    pub comment: Option<String>,
    /// 请求幂等身份。
    pub request_id: String,
}

impl TryFrom<ImportExecutionCommand> for PreparedImportExecution {
    type Error = Error;

    /// 从强类型命令构造已规范化的执行准备（INT-E24 纯转换）。
    ///
    /// # 参数
    /// * `command` - 强类型执行命令
    ///
    /// # 返回
    /// 返回动作矩阵已校验的准备。
    ///
    /// # 错误
    /// 版本非法或动作专属试算/原因矩阵非法时返回错误。
    fn try_from(command: ImportExecutionCommand) -> Result<Self> {
        let expected_trial_version = command
            .expected_trial_version
            .as_deref()
            .map(|value| parse_command_version(value, "试算版本"))
            .transpose()?;
        if matches!(
            command.action,
            ImportExecutionAction::StartApply | ImportExecutionAction::RetryFailed
        ) && expected_trial_version.is_none()
        {
            return Err(Error::ValidationError(
                "提交应用或重试失败项必须携带试算版本".to_string(),
            ));
        }
        let reason_code = optional_text(command.reason_code);
        if command.action == ImportExecutionAction::CancelPending && reason_code.is_none() {
            return Err(Error::ValidationError("取消尚未应用项必须提供原因码".to_string()));
        }
        if command.action == ImportExecutionAction::StartApply && reason_code.is_some() {
            return Err(Error::ValidationError(
                "提交应用不得携带取消或重试原因码".to_string(),
            ));
        }
        Ok(Self {
            batch_id: command.batch_id,
            expected_batch_version: parse_command_version(&command.expected_batch_version, "批次版本")?,
            expected_trial_version,
            action: command.action,
            reason_code,
            comment: optional_text(command.comment),
            request_id: required_text(&command.request_id, "请求身份不能为空")?,
        })
    }
}

/// 导入确认列表查询参数（按批次查询为主）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LegacyImportConfirmationListParams {
    /// 所属导入批次。
    pub batch_id: Option<LegacyImportBatchId>,
    /// 责任范围筛选。
    pub confirmation_scope: Option<String>,
    /// 确认状态筛选。
    pub status: Option<ConfirmationStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`trial_version`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的导入确认列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LegacyImportConfirmationListQuery {
    /// 所属导入批次。
    pub batch_id: Option<LegacyImportBatchId>,
    /// 责任范围筛选。
    pub confirmation_scope: Option<String>,
    /// 确认状态筛选。
    pub status: Option<ConfirmationStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl LegacyImportConfirmationListParams {
    /// 归一化导入确认列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<LegacyImportConfirmationListQuery> {
        let (sort_by, sort_dir) = normalize_sort(
            &self.sort_by,
            &self.sort_dir,
            LEGACY_IMPORT_CONFIRMATION_SORT_FIELDS,
        )?;
        Ok(LegacyImportConfirmationListQuery {
            batch_id: self.batch_id.clone(),
            confirmation_scope: normalized_text(self.confirmation_scope.as_deref()),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

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
/// [`entities::legacy_import::ApplyResultSet`] 在查库前强制收紧。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ApplyRowResult {
    /// 目标导入行。
    pub row_id: LegacyImportRowId,
    /// 行级结果。
    pub outcome: ApplyRowOutcome,
    /// 来源稳定身份（试算阶段在 D01 建立；待映射行必填）。
    pub external_identity_map_id: Option<entities::ids::ExternalIdentityMapId>,
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
pub(crate) const CUSTOMER_OBJECT_TYPE: &str = "CUSTOMER";
/// 客户行目标主体缺失的错误码（W18 问题代码口径）。
pub(crate) const CUSTOMER_NOT_FOUND_ERROR_CODE: &str = "CUSTOMER_NOT_FOUND";
/// 客户行目标主体缺失的错误明细。
pub(crate) const CUSTOMER_NOT_FOUND_ERROR_DETAIL: &str = "目标客户主体不存在，禁止导入";

#[cfg(test)]
mod tests {
    use super::{normalize_sort, LegacyImportBatchListParams, SortDir};
    use entities::common::time::BusinessDate;
    use serde_json::json;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("id".to_string()), &None, &["created_at", "batch_no"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" batch_no ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "batch_no"],
        )
        .unwrap();
        assert_eq!(field, "batch_no");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn batch_list_params_normalize_paging_filters_and_sort_defaults() {
        let params = LegacyImportBatchListParams {
            batch_no: Some(" IMP-1 ".to_string()),
            source_system_id: None,
            status: None,
            baseline_date_from: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
            baseline_date_to: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.batch_no.as_deref(), Some("IMP-1"));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = LegacyImportBatchListParams {
            batch_no: None,
            source_system_id: None,
            status: None,
            baseline_date_from: None,
            baseline_date_to: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn create_batch_request_rejects_empty_rows() {
        let request: super::CreateLegacyImportBatchRequest = serde_json::from_value(json!({
            "batch_no": "IMP-2026-001",
            "source_system_id": "sys-1",
            "source_object_set": "CUSTOMER",
            "baseline_date": "2026-01-01",
            "import_rule_version": "v1",
            "rows": []
        }))
        .unwrap();
        assert!(request.validate().is_err());
    }

    #[test]
    fn create_batch_request_rejects_illegal_inner_row() {
        let bad_row = super::ImportRowRequest {
            source_object_type: "   ".to_string(),
            source_row_key: "key-1".to_string(),
            normalized_payload_reference: "payload:1".to_string(),
        };
        let request = super::CreateLegacyImportBatchRequest {
            batch_no: "IMP-2026-001".to_string(),
            source_system_id: entities::ids::SourceSystemId::new("sys-1"),
            source_object_set: "CUSTOMER".to_string(),
            baseline_date: entities::common::time::BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            import_rule_version: "v1".to_string(),
            source_file_hmac: None,
            successful_sanitized_file_asset_id: None,
            success_manifest_file_asset_id: None,
            failure_diagnostic_file_asset_id: None,
            rows: vec![bad_row],
        };
        assert!(request.validate().is_err());
    }

    #[test]
    fn prepared_confirmation_enforces_action_reason_matrix() {
        use entities::legacy_import::ConfirmationDecision;
        fn decision(
            action: ConfirmationDecision,
            reason: Option<&str>,
        ) -> super::ImportBusinessConfirmationDecision {
            super::ImportBusinessConfirmationDecision {
                batch_id: entities::ids::LegacyImportBatchId::new("batch-1"),
                expected_batch_version: "1".to_string(),
                expected_trial_version: "2".to_string(),
                confirmation_scope: " sales ".to_string(),
                action,
                reason_code: reason.map(str::to_string),
                comment: Some(" 意见 ".to_string()),
            }
        }
        fn command(
            action: ConfirmationDecision,
            reason: Option<&str>,
        ) -> super::CompleteImportBusinessConfirmationCommand {
            super::CompleteImportBusinessConfirmationCommand {
                work_item_id: entities::ids::WorkItemId::new("work-item-1"),
                expected_task_version: "3".to_string(),
                expected_subject_version: "subject-1".to_string(),
                decision: decision(action, reason),
                idempotency_key: " request-1 ".to_string(),
            }
        }
        assert!(super::PreparedConfirmationCompletion::try_from(command(
            ConfirmationDecision::ReturnForFix,
            None
        ))
        .is_err());
        assert!(super::PreparedConfirmationCompletion::try_from(command(
            ConfirmationDecision::ConfirmScope,
            Some("REWORK")
        ))
        .is_err());
        let prepared = super::PreparedConfirmationCompletion::try_from(command(
            ConfirmationDecision::ReturnForFix,
            Some("REWORK"),
        ))
        .unwrap();
        assert_eq!(prepared.confirmation_scope, "SALES");
        assert_eq!(prepared.idempotency_key, "request-1");
        assert!(super::PreparedConfirmationCompletion::try_from(command(
            ConfirmationDecision::ConfirmScope,
            None
        ))
        .is_ok());
    }

    #[test]
    fn prepared_execution_enforces_trial_reason_matrix() {
        fn command(
            action: super::ImportExecutionAction,
            trial: Option<&str>,
            reason: Option<&str>,
        ) -> super::ImportExecutionCommand {
            super::ImportExecutionCommand {
                batch_id: entities::ids::LegacyImportBatchId::new("batch-1"),
                expected_batch_version: "4".to_string(),
                expected_trial_version: trial.map(str::to_string),
                action,
                reason_code: reason.map(str::to_string),
                comment: None,
                request_id: " request-1 ".to_string(),
            }
        }
        assert!(super::PreparedImportExecution::try_from(command(
            super::ImportExecutionAction::StartApply,
            None,
            None
        ))
        .is_err());
        assert!(super::PreparedImportExecution::try_from(command(
            super::ImportExecutionAction::CancelPending,
            None,
            None
        ))
        .is_err());
        assert!(super::PreparedImportExecution::try_from(command(
            super::ImportExecutionAction::StartApply,
            Some("2"),
            Some("REASON")
        ))
        .is_err());
        let prepared = super::PreparedImportExecution::try_from(command(
            super::ImportExecutionAction::RetryFailed,
            Some("2"),
            Some("REASON"),
        ))
        .unwrap();
        assert_eq!(prepared.request_id, "request-1");
        assert_eq!(prepared.expected_trial_version, Some(2));
    }

    #[test]
    fn background_job_no_is_prefixed_and_unique() {
        assert_eq!(entities::bulk_job::legacy_import_job_no("IMP-1"), "BJ-IMP-1");
    }

    #[test]
    fn import_execution_command_uses_frozen_wire_shape() {
        let request: super::ImportExecutionCommand = serde_json::from_value(json!({
            "batch_id": "batch-1",
            "expected_batch_version": "7",
            "expected_trial_version": "3",
            "action": "START_APPLY",
            "comment": "提交应用",
            "request_id": "request-1"
        }))
        .unwrap();

        assert_eq!(request.action, super::ImportExecutionAction::StartApply);
        assert_eq!(request.expected_batch_version, "7");
        assert_eq!(request.expected_trial_version.as_deref(), Some("3"));
        assert!(request.validate().is_ok());
    }

    #[test]
    fn import_execution_command_rejects_unknown_fields_and_numeric_versions() {
        let unknown = json!({
            "batch_id": "batch-1",
            "expected_batch_version": "7",
            "expected_trial_version": "3",
            "action": "START_APPLY",
            "request_id": "request-1",
            "start_immediately": true
        });
        assert!(serde_json::from_value::<super::ImportExecutionCommand>(unknown).is_err());

        let numeric = json!({
            "batch_id": "batch-1",
            "expected_batch_version": 7,
            "expected_trial_version": 3,
            "action": "START_APPLY",
            "request_id": "request-1"
        });
        assert!(serde_json::from_value::<super::ImportExecutionCommand>(numeric).is_err());
    }
}
