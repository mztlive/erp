//! `background_job`：后台任务中心统一注册表（数据模型 §6.1）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::command::CommandFingerprint;
use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{BackgroundJobId, BulkSelectionSnapshotId, FileAssetId};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 任务编号最大长度。
const JOB_NO_MAX_LEN: usize = 128;
/// 领域任务类型代码最大长度。
const DOMAIN_JOB_TYPE_MAX_LEN: usize = 64;
/// 领域任务 ID 最大长度。
const DOMAIN_JOB_ID_MAX_LEN: usize = 128;
/// 发起人标识最大长度。
const REQUESTED_BY_MAX_LEN: usize = 128;
/// 请求幂等身份最大长度。
const REQUEST_ID_MAX_LEN: usize = 128;
/// 错误摘要最大长度。
const ERROR_SUMMARY_MAX_LEN: usize = 1024;

/// 任务类型（数据模型 §6.1：导入、导出、批量、同步、回填、对账等固定类型）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    /// 导入。
    Import,
    /// 导出。
    Export,
    /// 批量。
    Batch,
    /// 同步。
    Sync,
    /// 回填。
    Backfill,
    /// 对账。
    Reconciliation,
}

impl JobType {
    /// 返回任务类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Import => "导入",
            Self::Export => "导出",
            Self::Batch => "批量",
            Self::Sync => "同步",
            Self::Backfill => "回填",
            Self::Reconciliation => "对账",
        }
    }

    /// 返回任务类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Export => "export",
            Self::Batch => "batch",
            Self::Sync => "sync",
            Self::Backfill => "backfill",
            Self::Reconciliation => "reconciliation",
        }
    }
}

/// 任务状态（数据模型 §6.1：等待执行、执行中、部分成功、成功、失败、已取消）。
///
/// 固定状态机（无运行时扩展）：
/// `PENDING → RUNNING → PARTIALLY_SUCCEEDED → SUCCEEDED | FAILED`；
/// `RUNNING → SUCCEEDED | FAILED`；`PENDING | RUNNING | PARTIALLY_SUCCEEDED
/// → CANCELLED`（任务取消只停止尚未开始的项目）。终态 `SUCCEEDED` / `FAILED` /
/// `CANCELLED`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// 等待执行。
    #[default]
    Pending,
    /// 执行中。
    Running,
    /// 部分成功。
    PartiallySucceeded,
    /// 成功。
    Succeeded,
    /// 失败。
    Failed,
    /// 已取消。
    Cancelled,
}

impl JobStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "等待执行",
            Self::Running => "执行中",
            Self::PartiallySucceeded => "部分成功",
            Self::Succeeded => "成功",
            Self::Failed => "失败",
            Self::Cancelled => "已取消",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::PartiallySucceeded => "partially_succeeded",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl DocumentState for JobStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[Self::Running, Self::Cancelled],
            Self::Running => &[
                Self::PartiallySucceeded,
                Self::Succeeded,
                Self::Failed,
                Self::Cancelled,
            ],
            Self::PartiallySucceeded => &[Self::Succeeded, Self::Failed, Self::Cancelled],
            Self::Succeeded | Self::Failed | Self::Cancelled => &[],
        }
    }
}

/// 后台任务创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundJobData {
    /// 任务编号（唯一）。
    pub job_no: String,
    /// 任务类型。
    pub job_type: JobType,
    /// 适用时关联强类型领域任务类型代码。
    pub domain_job_type: Option<String>,
    /// 适用时关联强类型领域任务 ID。
    pub domain_job_id: Option<String>,
    /// 批量或导出使用的不可变选择快照。
    pub selection_snapshot_id: Option<BulkSelectionSnapshotId>,
    /// 发起人。
    pub requested_by: String,
    /// 请求幂等身份（唯一）。
    pub request_id: String,
    /// 合规输入包文件资产。
    pub input_file_asset_id: Option<FileAssetId>,
    /// 结果文件资产。
    pub result_file_asset_id: Option<FileAssetId>,
    /// 目标总数。
    pub total_count: u64,
}

/// 后台任务更新数据（仅进度与结果，不修改任何关键字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct JobUpdate {
    /// 结果文件资产。
    pub result_file_asset_id: Option<FileAssetId>,
    /// 结果下载到期时间。
    pub result_expires_at: Option<Instant>,
}

/// 后台任务实体（数据模型 §6.1）。
///
/// 本表是后台任务中心的统一注册表，只登记统一进度、发起人、输入输出和安全
/// 边界，不替代领域任务强类型表（§6.1）。`job_no` / `request_id` 唯一约束由
/// P2 索引保证；进度计数与逐项结果的一致性、逐项权限/数据范围/版本重验由
/// P3 事务编排。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct BackgroundJob {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 任务编号。
    pub job_no: String,
    /// 任务类型。
    pub job_type: JobType,
    /// 关联强类型领域任务类型代码。
    pub domain_job_type: Option<String>,
    /// 关联强类型领域任务 ID。
    pub domain_job_id: Option<String>,
    /// 批量或导出使用的不可变选择快照。
    pub selection_snapshot_id: Option<BulkSelectionSnapshotId>,
    /// 发起人。
    pub requested_by: String,
    /// 请求幂等身份。
    pub request_id: String,
    /// v1 规范请求指纹；历史行可能缺失。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_fingerprint: Option<CommandFingerprint>,
    /// 合规输入包文件资产。
    pub input_file_asset_id: Option<FileAssetId>,
    /// 结果文件资产。
    pub result_file_asset_id: Option<FileAssetId>,
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
    /// 开始执行时间。
    pub started_at: Option<Instant>,
    /// 结束时间。
    pub finished_at: Option<Instant>,
    /// 最近进度时间。
    pub last_progress_at: Option<Instant>,
    /// 结果下载到期时间。
    pub result_expires_at: Option<Instant>,
    /// 脱敏任务级错误摘要。
    pub error_summary: Option<String>,
}

impl BackgroundJob {
    /// 创建后台任务。
    ///
    /// 完成 job_no/requested_by/request_id 的校验与规范化（trim、非空、长度
    /// 上限），初始状态 `PENDING`，计数全零。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::BackgroundJobId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的任务实体。
    ///
    /// # 错误
    /// 当编号/发起人/请求身份为空或超长时返回错误。
    pub fn new(id: BackgroundJobId, data: BackgroundJobData) -> Result<Self> {
        let job_no =
            normalize_required_text(data.job_no, "任务编号不能为空", JOB_NO_MAX_LEN, "任务编号过长")?;
        let requested_by = normalize_required_text(
            data.requested_by,
            "发起人不能为空",
            REQUESTED_BY_MAX_LEN,
            "发起人过长",
        )?;
        let request_id = normalize_required_text(
            data.request_id,
            "请求身份不能为空",
            REQUEST_ID_MAX_LEN,
            "请求身份过长",
        )?;
        let domain_job_type =
            normalize_optional_text(data.domain_job_type, "领域任务类型", DOMAIN_JOB_TYPE_MAX_LEN)?;
        let domain_job_id = normalize_optional_text(data.domain_job_id, "领域任务ID", DOMAIN_JOB_ID_MAX_LEN)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            job_no,
            job_type: data.job_type,
            domain_job_type,
            domain_job_id,
            selection_snapshot_id: data.selection_snapshot_id,
            requested_by,
            request_id,
            request_fingerprint: None,
            input_file_asset_id: data.input_file_asset_id,
            result_file_asset_id: data.result_file_asset_id,
            status: JobStatus::Pending,
            total_count: data.total_count,
            processed_count: 0,
            success_count: 0,
            skipped_count: 0,
            failed_count: 0,
            started_at: None,
            finished_at: None,
            last_progress_at: None,
            result_expires_at: None,
            error_summary: None,
        })
    }

    /// 在聚合创建阶段附加规范请求指纹。
    ///
    /// # 错误
    /// 指纹已附加或任务已离开初始待执行状态时返回错误。
    pub fn attach_request_fingerprint(&mut self, fingerprint: CommandFingerprint) -> Result<()> {
        if self.request_fingerprint.is_some()
            || self.status != JobStatus::Pending
            || self.processed_count != 0
        {
            return Err(Error::from("后台任务请求指纹只能在创建阶段附加一次"));
        }
        self.request_fingerprint = Some(fingerprint);
        Ok(())
    }

    /// 开始执行。
    ///
    /// # 参数
    /// * `at` - 开始时刻
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当任务不是 `PENDING` 状态时返回错误。
    pub fn start(&mut self, at: Instant) -> Result<()> {
        ensure_transition(self.status, JobStatus::Running)?;
        self.status = JobStatus::Running;
        self.started_at = Some(at);
        self.last_progress_at = Some(at);
        Ok(())
    }

    /// 记录一批逐项执行结果。
    ///
    /// 仅 `RUNNING` / `PARTIALLY_SUCCEEDED` 可记录进度；计数按
    /// `processed = success + skipped + failed` 累加，且不得超过 `total_count`
    /// （§6.1 普通任务允许逐项提交并显示部分成功）。
    ///
    /// # 参数
    /// * `success` - 本批成功数
    /// * `skipped` - 本批跳过数
    /// * `failed` - 本批失败数
    /// * `at` - 进度时刻
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当任务状态不允许记录进度，或累计已处理数超过目标总数时返回错误。
    pub fn record_progress(&mut self, success: u64, skipped: u64, failed: u64, at: Instant) -> Result<()> {
        if self.finished_at.is_some()
            || !matches!(self.status, JobStatus::Running | JobStatus::PartiallySucceeded)
        {
            return Err(Error::from(format!("状态 {:?} 不允许记录进度", self.status)));
        }
        let processed = self
            .processed_count
            .checked_add(success)
            .and_then(|value| value.checked_add(skipped))
            .and_then(|value| value.checked_add(failed))
            .ok_or_else(|| Error::from("处理计数溢出"))?;
        if processed > self.total_count {
            return Err(Error::from("已处理数不能超过目标总数"));
        }
        self.processed_count = processed;
        self.success_count = self.success_count.saturating_add(success);
        self.skipped_count = self.skipped_count.saturating_add(skipped);
        self.failed_count = self.failed_count.saturating_add(failed);
        self.last_progress_at = Some(at);
        Ok(())
    }

    /// 标记部分成功。
    ///
    /// 有成功有失败（且未取消）时使用；之后仍可继续记录进度。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当任务不是 `RUNNING` 状态时返回错误。
    pub fn mark_partially_succeeded(&mut self) -> Result<()> {
        ensure_transition(self.status, JobStatus::PartiallySucceeded)?;
        self.status = JobStatus::PartiallySucceeded;
        Ok(())
    }

    /// 标记成功完成。
    ///
    /// 要求全部目标已处理（`processed_count == total_count`，§6.1 完成口径）。
    ///
    /// # 参数
    /// * `at` - 完成时刻
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当任务状态不允许完成，或仍有目标未处理时返回错误。
    pub fn mark_succeeded(&mut self, at: Instant) -> Result<()> {
        if self.finished_at.is_some()
            || !matches!(self.status, JobStatus::Running | JobStatus::PartiallySucceeded)
        {
            return Err(Error::from(format!("状态 {:?} 不允许完成", self.status)));
        }
        if self.processed_count != self.total_count {
            return Err(Error::from("仍有目标未处理，不能标记成功"));
        }
        self.status = JobStatus::Succeeded;
        self.finished_at = Some(at);
        self.last_progress_at = Some(at);
        Ok(())
    }

    /// 标记失败。
    ///
    /// # 参数
    /// * `error_summary` - 脱敏任务级错误摘要
    /// * `at` - 失败时刻
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当任务状态不允许失败时返回错误。
    pub fn mark_failed(&mut self, error_summary: Option<String>, at: Instant) -> Result<()> {
        if self.finished_at.is_some()
            || !matches!(self.status, JobStatus::Running | JobStatus::PartiallySucceeded)
        {
            return Err(Error::from(format!("状态 {:?} 不允许失败", self.status)));
        }
        self.status = JobStatus::Failed;
        self.finished_at = Some(at);
        self.last_progress_at = Some(at);
        self.error_summary = normalize_optional_text(error_summary, "错误摘要", ERROR_SUMMARY_MAX_LEN)?;
        Ok(())
    }

    /// 取消任务。
    ///
    /// 任务取消只停止尚未开始的项目；已经提交的正式事实不回滚、不删除（§6.1）。
    ///
    /// # 参数
    /// * `at` - 取消时刻
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当任务处于终态时返回错误。
    pub fn cancel(&mut self, at: Instant) -> Result<()> {
        ensure_transition(self.status, JobStatus::Cancelled)?;
        self.status = JobStatus::Cancelled;
        self.finished_at = Some(at);
        self.last_progress_at = Some(at);
        Ok(())
    }

    /// 仅将上一轮失败项重新准备为待执行。
    ///
    /// 该操作只适用于明确的可重试结果态：失败、部分成功，
    /// 或“完成但含失败项”的兼容记录。已取消任务不可重新打开。
    /// 保留已成功与已跳过计数，
    /// 仅从已处理数中扣除失败项并清零失败计数；未处理项保持未处理。
    ///
    /// # 参数
    /// * `failed_item_count` - 由领域行事实重验得到的失败项数
    /// * `at` - 重新准备时刻
    ///
    /// # 返回
    /// 准备成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当任务不在可重试结果态、没有失败项，或领域失败项计数
    /// 与任务计数不一致时返回错误。
    pub fn prepare_failed_retry(&mut self, failed_item_count: u64, at: Instant) -> Result<()> {
        let retryable = matches!(self.status, JobStatus::PartiallySucceeded | JobStatus::Failed)
            || (self.status == JobStatus::Succeeded && self.failed_count > 0);
        if !retryable {
            return Err(Error::from("当前任务状态不允许重试失败项"));
        }
        if failed_item_count == 0 || self.failed_count != failed_item_count {
            return Err(Error::from("领域失败项与后台任务计数不一致"));
        }
        self.processed_count = self
            .processed_count
            .checked_sub(failed_item_count)
            .ok_or_else(|| Error::from("失败项数超过已处理数"))?;
        self.failed_count = 0;
        self.status = JobStatus::Pending;
        self.started_at = None;
        self.finished_at = None;
        self.last_progress_at = Some(at);
        self.error_summary = None;
        Ok(())
    }

    /// 更新结果文件与下载到期时间。
    ///
    /// 更新复用创建校验；关键字段（编号、类型、发起人、请求身份、目标总数）
    /// 不允许在通用更新中修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 无返回值。
    pub fn update(&mut self, update: JobUpdate) {
        if let Some(result_file_asset_id) = update.result_file_asset_id {
            self.result_file_asset_id = Some(result_file_asset_id);
        }
        if let Some(result_expires_at) = update.result_expires_at {
            self.result_expires_at = Some(result_expires_at);
        }
    }

    /// 判断任务是否已处于终态。
    ///
    /// # 返回
    /// `SUCCEEDED` / `FAILED` / `CANCELLED`，或已全部结束的混合
    /// `PARTIALLY_SUCCEEDED`（已写入 `finished_at`）时返回 `true`。
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled
        ) || (self.status == JobStatus::PartiallySucceeded && self.finished_at.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::{BackgroundJob, BackgroundJobData, JobStatus, JobType, JobUpdate};
    use crate::common::state::ensure_transition;
    use crate::common::time::Instant;
    use crate::ids::{BackgroundJobId, FileAssetId};

    fn data() -> BackgroundJobData {
        BackgroundJobData {
            job_no: " JOB-2025-001 ".to_string(),
            job_type: JobType::Import,
            domain_job_type: Some("legacy_import_batch".to_string()),
            domain_job_id: Some("batch-1".to_string()),
            selection_snapshot_id: None,
            requested_by: " admin-1 ".to_string(),
            request_id: "req-001".to_string(),
            input_file_asset_id: None,
            result_file_asset_id: None,
            total_count: 5,
        }
    }

    /// happy path：字段 trim、初始 PENDING、计数全零。
    #[test]
    fn new_trims_fields_and_starts_pending() {
        let job = BackgroundJob::new(BackgroundJobId::new("job-1"), data()).unwrap();
        assert_eq!(job.job_no, "JOB-2025-001");
        assert_eq!(job.requested_by, "admin-1");
        assert_eq!(job.domain_job_id.as_deref(), Some("batch-1"));
        assert_eq!(job.status, JobStatus::Pending);
        assert_eq!(job.processed_count, 0);
        assert!(!job.is_terminal());
    }

    /// 失败路径：必填为空被拒。
    #[test]
    fn new_rejects_empty_request_id() {
        let payload = BackgroundJobData {
            request_id: "  ".to_string(),
            ..data()
        };
        assert!(BackgroundJob::new(BackgroundJobId::new("job-1"), payload).is_err());
    }

    /// 失败路径：超长错误摘要被拒。
    #[test]
    fn mark_failed_rejects_overlong_summary() {
        let mut job = BackgroundJob::new(BackgroundJobId::new("job-1"), data()).unwrap();
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        let error = job.mark_failed(Some("e".repeat(1025)), Instant::from_unix_secs(1_700_000_100));
        assert!(error.is_err());
    }

    /// 进度不变量：processed = success + skipped + failed 且不超过 total。
    #[test]
    fn record_progress_enforces_count_arithmetic() {
        let mut job = BackgroundJob::new(BackgroundJobId::new("job-1"), data()).unwrap();
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job.record_progress(3, 1, 0, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        assert_eq!(job.processed_count, 4);
        assert_eq!(job.success_count, 3);
        assert_eq!(job.skipped_count, 1);
        assert_eq!(job.failed_count, 0);

        assert!(
            job.record_progress(2, 0, 0, Instant::from_unix_secs(1_700_000_200))
                .is_err(),
            "超过目标总数被拒"
        );
        assert!(job
            .record_progress(1, 0, 0, Instant::from_unix_secs(1_700_000_200))
            .is_ok());
        assert_eq!(job.processed_count, 5);
    }

    /// 失败路径：非 RUNNING 状态不能记录进度。
    #[test]
    fn record_progress_rejects_pending_job() {
        let mut job = BackgroundJob::new(BackgroundJobId::new("job-1"), data()).unwrap();
        assert!(job
            .record_progress(1, 0, 0, Instant::from_unix_secs(1_700_000_100))
            .is_err());
    }

    /// 状态机：合法迁移（部分成功 → 成功、取消）。
    #[test]
    fn lifecycle_running_to_terminal() {
        let mut job = BackgroundJob::new(BackgroundJobId::new("job-1"), data()).unwrap();
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job.record_progress(3, 0, 1, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        job.mark_partially_succeeded().unwrap();
        assert_eq!(job.status, JobStatus::PartiallySucceeded);
        job.record_progress(1, 0, 0, Instant::from_unix_secs(1_700_000_200))
            .unwrap();
        job.mark_succeeded(Instant::from_unix_secs(1_700_000_300))
            .unwrap();
        assert_eq!(job.status, JobStatus::Succeeded);
        assert!(job.is_terminal());

        let mut cancelled = BackgroundJob::new(BackgroundJobId::new("job-2"), data()).unwrap();
        cancelled.cancel(Instant::from_unix_secs(1_700_000_100)).unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
    }

    /// 失败项重试仅回退失败计数，保留已成功/跳过结果。
    #[test]
    fn prepare_failed_retry_preserves_committed_results() {
        let mut job = BackgroundJob::new(BackgroundJobId::new("job-retry"), data()).unwrap();
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job.record_progress(2, 1, 2, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        job.mark_partially_succeeded().unwrap();
        job.mark_succeeded(Instant::from_unix_secs(1_700_000_200))
            .unwrap();

        job.prepare_failed_retry(2, Instant::from_unix_secs(1_700_000_300))
            .unwrap();

        assert_eq!(job.status, JobStatus::Pending);
        assert_eq!(job.processed_count, 3);
        assert_eq!(job.success_count, 2);
        assert_eq!(job.skipped_count, 1);
        assert_eq!(job.failed_count, 0);
        assert!(job.finished_at.is_none());
    }

    /// 失败项计数不一致时失败关闭，不修改任务。
    #[test]
    fn prepare_failed_retry_rejects_mismatched_count() {
        let mut job = BackgroundJob::new(BackgroundJobId::new("job-retry"), data()).unwrap();
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job.record_progress(2, 1, 2, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        job.mark_partially_succeeded().unwrap();

        assert!(job
            .prepare_failed_retry(1, Instant::from_unix_secs(1_700_000_200))
            .is_err());
        assert_eq!(job.status, JobStatus::PartiallySucceeded);
        assert_eq!(job.processed_count, 5);
        assert_eq!(job.failed_count, 2);
    }

    /// 已取消任务不得通过失败重试重新打开未执行项。
    #[test]
    fn prepare_failed_retry_rejects_cancelled_job() {
        let mut job = BackgroundJob::new(BackgroundJobId::new("job-cancelled"), data()).unwrap();
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job.record_progress(1, 0, 1, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        job.cancel(Instant::from_unix_secs(1_700_000_200)).unwrap();

        assert!(job
            .prepare_failed_retry(1, Instant::from_unix_secs(1_700_000_300))
            .is_err());
        assert_eq!(job.status, JobStatus::Cancelled);
        assert_eq!(job.processed_count, 2);
        assert_eq!(job.failed_count, 1);
    }

    /// 状态机：非法迁移被拒（终态回退、跳步、未处理完即成功）。
    #[test]
    fn illegal_transitions_are_rejected() {
        let mut job = BackgroundJob::new(BackgroundJobId::new("job-1"), data()).unwrap();
        assert!(job
            .record_progress(1, 0, 0, Instant::from_unix_secs(1_700_000_100))
            .is_err());
        assert!(
            job.mark_succeeded(Instant::from_unix_secs(1_700_000_100))
                .is_err(),
            "未开始不能成功"
        );

        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        assert!(
            job.mark_succeeded(Instant::from_unix_secs(1_700_000_100))
                .is_err(),
            "未处理完不能成功"
        );
        job.cancel(Instant::from_unix_secs(1_700_000_100)).unwrap();
        assert!(
            job.record_progress(1, 0, 0, Instant::from_unix_secs(1_700_000_200))
                .is_err(),
            "终态不能记录进度"
        );
    }

    /// 状态机：逐边定向断言（含不可逆终态）。
    #[test]
    fn directed_edge_assertions() {
        for &(from, to) in &[
            (JobStatus::Pending, JobStatus::Running),
            (JobStatus::Pending, JobStatus::Cancelled),
            (JobStatus::Running, JobStatus::PartiallySucceeded),
            (JobStatus::Running, JobStatus::Succeeded),
            (JobStatus::Running, JobStatus::Failed),
            (JobStatus::Running, JobStatus::Cancelled),
            (JobStatus::PartiallySucceeded, JobStatus::Succeeded),
            (JobStatus::PartiallySucceeded, JobStatus::Failed),
            (JobStatus::PartiallySucceeded, JobStatus::Cancelled),
        ] {
            assert!(ensure_transition(from, to).is_ok(), "{from:?} → {to:?}");
        }
        assert!(ensure_transition(JobStatus::Pending, JobStatus::Succeeded).is_err());
        assert!(ensure_transition(JobStatus::Succeeded, JobStatus::Failed).is_err());
        assert!(ensure_transition(JobStatus::Cancelled, JobStatus::Running).is_err());
    }

    /// 更新：结果文件与到期时间可更新，关键字段不可改。
    #[test]
    fn update_applies_result_fields_only() {
        let mut job = BackgroundJob::new(BackgroundJobId::new("job-1"), data()).unwrap();
        job.update(JobUpdate {
            result_file_asset_id: Some(FileAssetId::new("file-1")),
            result_expires_at: Some(Instant::from_unix_secs(1_700_604_800)),
        });
        assert_eq!(job.result_file_asset_id, Some(FileAssetId::new("file-1")));
        assert_eq!(job.job_no, "JOB-2025-001");
        assert_eq!(job.total_count, 5);
    }

    /// 枚举序列化与标签稳定。
    #[test]
    fn job_codes_and_labels_are_stable() {
        assert_eq!(serde_json::to_string(&JobType::Export).unwrap(), "\"export\"");
        assert_eq!(JobType::Reconciliation.as_str(), "reconciliation");
        assert_eq!(JobType::Backfill.label(), "回填");
        assert_eq!(JobStatus::PartiallySucceeded.as_str(), "partially_succeeded");
        assert_eq!(JobStatus::Cancelled.label(), "已取消");
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let job = BackgroundJob::new(BackgroundJobId::new("job-1"), data()).unwrap();
        let roundtrip: BackgroundJob =
            bson::deserialize_from_document(bson::serialize_to_document(&job).unwrap()).unwrap();
        assert_eq!(roundtrip, job);
    }
}
