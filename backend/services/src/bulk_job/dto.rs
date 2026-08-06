//! 域 D04 `bulk_job` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；本域无金额字段。

use entities::bulk_job::{
    BackgroundJob, BulkSelectionSnapshot, ItemStatus, JobStatus, JobType, SelectionItemStatus,
    SelectionStatus, SelectionType,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 选择快照列表允许的排序字段白名单。
pub(crate) const SNAPSHOT_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];
/// 后台任务列表允许的排序字段白名单。
pub(crate) const BACKGROUND_JOB_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];

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

/// 选择快照响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BulkSelectionSnapshotView {
    /// 实体主键。
    pub id: String,
    /// 选择类型。
    pub selection_type: SelectionType,
    /// 数据截止水位（秒级时间戳）。
    pub data_cutoff_at: u64,
    /// 冻结目标数。
    pub item_count: u32,
    /// 创建人。
    pub created_by: String,
    /// 有效期截止时间（秒级时间戳）。
    pub expires_at: u64,
    /// 快照状态。
    pub status: SelectionStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<BulkSelectionSnapshot> for BulkSelectionSnapshotView {
    /// 从实体构造响应视图。
    fn from(snapshot: BulkSelectionSnapshot) -> Self {
        Self {
            id: snapshot.base.id,
            selection_type: snapshot.selection_type,
            data_cutoff_at: snapshot.data_cutoff_at.unix_secs() as u64,
            item_count: snapshot.item_count,
            created_by: snapshot.created_by,
            expires_at: snapshot.expires_at.unix_secs() as u64,
            status: snapshot.status,
            version: snapshot.base.version,
            created_at: snapshot.base.created_at,
        }
    }
}

/// 选择快照列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BulkSelectionSnapshotListParams {
    /// 选择类型筛选。
    pub selection_type: Option<SelectionType>,
    /// 快照状态筛选。
    pub status: Option<SelectionStatus>,
    /// 创建人筛选。
    pub created_by: Option<String>,
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

/// 归一化后的选择快照列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BulkSelectionSnapshotListQuery {
    /// 选择类型筛选。
    pub selection_type: Option<SelectionType>,
    /// 快照状态筛选。
    pub status: Option<SelectionStatus>,
    /// 创建人筛选。
    pub created_by: Option<String>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl BulkSelectionSnapshotListParams {
    /// 归一化选择快照列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<BulkSelectionSnapshotListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, SNAPSHOT_SORT_FIELDS)?;
        Ok(BulkSelectionSnapshotListQuery {
            selection_type: self.selection_type,
            status: self.status,
            created_by: normalized_text(self.created_by.as_deref()),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 选择快照逐项目标请求（预览时冻结目标）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateBulkSelectionItemRequest {
    /// 目标对象类型代码（跨域开放目录）。
    #[validate(custom(function = "non_blank", message = "对象类型不能为空"))]
    pub object_type: String,
    /// 目标对象 ID。
    #[validate(custom(function = "non_blank", message = "对象ID不能为空"))]
    pub object_id: String,
    /// 预览时版本（与内容摘要成对出现）。
    #[validate(length(max = 64, message = "预期版本过长"))]
    pub expected_version: Option<String>,
    /// 预览时内容摘要（与预期版本成对出现）。
    #[validate(length(max = 128, message = "内容摘要过长"))]
    pub expected_hash: Option<String>,
}

/// 创建选择快照请求（HTTP 契约：`{ selection_type, data_cutoff_at, expires_at,
/// items }`；`item_count` 由服务端按 `items` 长度计算）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateBulkSelectionSnapshotRequest {
    /// 选择类型。
    pub selection_type: SelectionType,
    /// 数据截止水位（秒级时间戳）。
    #[validate(range(min = 1, message = "数据截止水位必须大于 0"))]
    pub data_cutoff_at: u64,
    /// 有效期截止时间（秒级时间戳）。
    #[validate(range(min = 1, message = "有效期截止时间必须大于 0"))]
    pub expires_at: u64,
    /// 冻结目标集合。
    #[validate(length(min = 1, message = "冻结目标不能为空"))]
    pub items: Vec<CreateBulkSelectionItemRequest>,
}

/// 选择项响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BulkSelectionItemView {
    /// 实体主键。
    pub id: String,
    /// 所属选择快照 ID。
    pub selection_snapshot_id: String,
    /// 目标对象类型代码。
    pub object_type: String,
    /// 目标对象 ID。
    pub object_id: String,
    /// 预览时版本。
    pub expected_version: Option<String>,
    /// 预览时内容摘要。
    pub expected_hash: Option<String>,
    /// 逐项执行结果（未执行为 `None`）。
    pub result_status: Option<SelectionItemStatus>,
    /// 失败原因代码（适用时）。
    pub result_code: Option<String>,
}

/// 快照确认请求（携带乐观锁版本，冲突返回 409）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ConfirmBulkSelectionSnapshotRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
}

/// 快照失效请求（携带乐观锁版本，冲突返回 409）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ExpireBulkSelectionSnapshotRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
}

/// 后台任务响应视图（任务中心进度投影）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BackgroundJobView {
    /// 实体主键。
    pub id: String,
    /// 任务编号。
    pub job_no: String,
    /// 任务类型。
    pub job_type: JobType,
    /// 关联强类型领域任务类型代码。
    pub domain_job_type: Option<String>,
    /// 关联强类型领域任务 ID。
    pub domain_job_id: Option<String>,
    /// 批量或导出使用的不可变选择快照。
    pub selection_snapshot_id: Option<String>,
    /// 发起人。
    pub requested_by: String,
    /// 请求幂等身份。
    pub request_id: String,
    /// 合规输入包文件资产。
    pub input_file_asset_id: Option<String>,
    /// 结果文件资产。
    pub result_file_asset_id: Option<String>,
    /// 任务状态。
    pub status: JobStatus,
    /// 目标总数。
    pub total_count: u64,
    /// 已处理数。
    pub processed_count: u64,
    /// 成功数。
    pub success_count: u64,
    /// 跳过数。
    pub skipped_count: u64,
    /// 失败数。
    pub failed_count: u64,
    /// 开始执行时间（秒级时间戳）。
    pub started_at: Option<u64>,
    /// 结束时间（秒级时间戳）。
    pub finished_at: Option<u64>,
    /// 最近进度时间（秒级时间戳）。
    pub last_progress_at: Option<u64>,
    /// 结果下载到期时间（秒级时间戳）。
    pub result_expires_at: Option<u64>,
    /// 脱敏任务级错误摘要。
    pub error_summary: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<BackgroundJob> for BackgroundJobView {
    /// 从实体构造响应视图。
    fn from(job: BackgroundJob) -> Self {
        Self {
            id: job.base.id,
            job_no: job.job_no,
            job_type: job.job_type,
            domain_job_type: job.domain_job_type,
            domain_job_id: job.domain_job_id,
            selection_snapshot_id: job.selection_snapshot_id.map(|id| id.to_string()),
            requested_by: job.requested_by,
            request_id: job.request_id,
            input_file_asset_id: job.input_file_asset_id.map(|id| id.to_string()),
            result_file_asset_id: job.result_file_asset_id.map(|id| id.to_string()),
            status: job.status,
            total_count: job.total_count,
            processed_count: job.processed_count,
            success_count: job.success_count,
            skipped_count: job.skipped_count,
            failed_count: job.failed_count,
            started_at: job.started_at.map(|instant| instant.unix_secs() as u64),
            finished_at: job.finished_at.map(|instant| instant.unix_secs() as u64),
            last_progress_at: job.last_progress_at.map(|instant| instant.unix_secs() as u64),
            result_expires_at: job.result_expires_at.map(|instant| instant.unix_secs() as u64),
            error_summary: job.error_summary,
            version: job.base.version,
            created_at: job.base.created_at,
        }
    }
}

/// 后台任务列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BackgroundJobListParams {
    /// 任务编号模糊筛选（忽略大小写）。
    pub job_no: Option<String>,
    /// 任务类型筛选。
    pub job_type: Option<JobType>,
    /// 任务状态筛选。
    pub status: Option<JobStatus>,
    /// 发起人筛选。
    pub requested_by: Option<String>,
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

/// 归一化后的后台任务列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackgroundJobListQuery {
    /// 任务编号模糊筛选。
    pub job_no: Option<String>,
    /// 任务类型筛选。
    pub job_type: Option<JobType>,
    /// 任务状态筛选。
    pub status: Option<JobStatus>,
    /// 发起人筛选。
    pub requested_by: Option<String>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl BackgroundJobListParams {
    /// 归一化后台任务列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<BackgroundJobListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, BACKGROUND_JOB_SORT_FIELDS)?;
        Ok(BackgroundJobListQuery {
            job_no: normalized_text(self.job_no.as_deref()),
            job_type: self.job_type,
            status: self.status,
            requested_by: normalized_text(self.requested_by.as_deref()),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 后台任务逐项请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateBackgroundJobItemRequest {
    /// 已有对象类型代码（可空，与对象 ID 成对）。
    #[validate(length(max = 64, message = "对象类型过长"))]
    pub object_type: Option<String>,
    /// 已有对象 ID（可空，与对象类型成对）。
    #[validate(length(max = 128, message = "对象ID过长"))]
    pub object_id: Option<String>,
    /// 执行前必须重验的预览版本。
    #[validate(length(max = 64, message = "预期版本过长"))]
    pub expected_version: Option<String>,
    /// 执行前必须重验的内容摘要。
    #[validate(length(max = 128, message = "内容摘要过长"))]
    pub expected_hash: Option<String>,
    /// 导入错误定位：工作表名（适用时）。
    #[validate(length(max = 128, message = "工作表名过长"))]
    pub worksheet_name: Option<String>,
    /// 导入错误定位：源行号（适用时）。
    pub source_row_no: Option<u32>,
    /// 导入错误定位：源列名（适用时）。
    #[validate(length(max = 128, message = "源列名过长"))]
    pub source_column_name: Option<String>,
}

/// 创建后台任务请求（HTTP 契约：`{ job_no, job_type, request_id, total_count,
/// items, ... }`；`request_id` 为幂等身份）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateBackgroundJobRequest {
    /// 任务编号（唯一）。
    #[validate(custom(function = "non_blank", message = "任务编号不能为空"))]
    pub job_no: String,
    /// 任务类型。
    pub job_type: JobType,
    /// 关联强类型领域任务类型代码。
    #[validate(length(max = 64, message = "领域任务类型过长"))]
    pub domain_job_type: Option<String>,
    /// 关联强类型领域任务 ID。
    #[validate(length(max = 128, message = "领域任务ID过长"))]
    pub domain_job_id: Option<String>,
    /// 批量或导出使用的不可变选择快照。
    pub selection_snapshot_id: Option<String>,
    /// 请求幂等身份（唯一，重复提交返回既有任务）。
    #[validate(custom(function = "non_blank", message = "请求身份不能为空"))]
    pub request_id: String,
    /// 合规输入包文件资产。
    pub input_file_asset_id: Option<String>,
    /// 目标总数。
    #[validate(range(min = 1, message = "目标总数必须大于 0"))]
    pub total_count: u64,
    /// 逐项结果行（`item_no` 由服务端按序分配）。
    #[validate(length(min = 1, message = "逐项结果不能为空"))]
    pub items: Vec<CreateBackgroundJobItemRequest>,
}

/// 后台任务逐项结果视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BackgroundJobItemView {
    /// 实体主键。
    pub id: String,
    /// 所属后台任务 ID。
    pub background_job_id: String,
    /// 稳定逐项序号。
    pub item_no: u32,
    /// 已有对象类型代码。
    pub object_type: Option<String>,
    /// 已有对象 ID。
    pub object_id: Option<String>,
    /// 逐项执行结果（未执行为 `None`）。
    pub status: Option<ItemStatus>,
    /// 脱敏原因代码。
    pub result_code: Option<String>,
    /// 脱敏结果摘要。
    pub result_summary: Option<String>,
    /// 成功形成的对象类型代码。
    pub result_object_type: Option<String>,
    /// 成功形成的对象 ID。
    pub result_object_id: Option<String>,
}

/// 取消后台任务请求（携带乐观锁版本，冲突返回 409）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CancelBackgroundJobRequest {
    /// 期望的乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
}

#[cfg(test)]
mod tests {
    use super::{normalize_sort, BackgroundJobListParams, BulkSelectionSnapshotListParams, SortDir};
    use entities::bulk_job::{JobStatus, JobType, SelectionStatus, SelectionType};
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields() {
        assert!(normalize_sort(&Some("job_no".to_string()), &None, &["created_at"]).is_err());
        let (field, direction) = normalize_sort(
            &Some(" updated_at ".to_string()),
            &None,
            &["created_at", "updated_at"],
        )
        .unwrap();
        assert_eq!(field, "updated_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn snapshot_list_params_normalize_filters() {
        let params = BulkSelectionSnapshotListParams {
            selection_type: Some(SelectionType::Export),
            status: Some(SelectionStatus::Pending),
            created_by: Some(" admin-1 ".to_string()),
            page: Some(2),
            page_size: Some(50),
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.selection_type, Some(SelectionType::Export));
        assert_eq!(query.status, Some(SelectionStatus::Pending));
        assert_eq!(query.created_by.as_deref(), Some("admin-1"));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
    }

    #[test]
    fn background_job_list_params_normalize_and_validate() {
        let params = BackgroundJobListParams {
            job_no: Some(" JOB-1 ".to_string()),
            job_type: Some(JobType::Import),
            status: Some(JobStatus::Running),
            requested_by: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.job_no.as_deref(), Some("JOB-1"));
        assert_eq!(query.job_type, Some(JobType::Import));
        assert_eq!(query.status, Some(JobStatus::Running));
        assert_eq!(query.paging.page_size, 20);

        let invalid = BackgroundJobListParams {
            job_no: None,
            job_type: None,
            status: None,
            requested_by: None,
            page: Some(0),
            page_size: Some(0),
            sort_by: None,
            sort_dir: None,
        };
        assert!(invalid.validate().is_err());
    }
}
