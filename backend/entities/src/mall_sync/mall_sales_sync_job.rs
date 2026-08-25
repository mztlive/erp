//! `mall_sales_sync_job`：商城卡券销售单同步作业（数据模型 §6.13）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{MallSalesSyncJobId, SourceSystemId};
use crate::validation::normalize_optional_text;

const ORDER_NO_MAX_LEN: usize = 128;
const REASON_MAX_LEN: usize = 1024;
const ACTOR_MAX_LEN: usize = 128;

/// 同步作业触发来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MallSyncTriggerSource {
    /// 系统调度。
    Scheduled,
    /// 授权用户人工触发。
    Manual,
}

/// 同步作业类型（数据模型 §6.13：期初基线、增量拉取、按月全量核对、单号补拉）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MallSalesSyncJobType {
    /// 期初基线。
    Baseline,
    /// 增量拉取。
    Incremental,
    /// 按月全量核对。
    MonthlyReconciliation,
    /// 单号补拉。
    SingleOrderBackfill,
}

impl MallSalesSyncJobType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Baseline => "期初基线",
            Self::Incremental => "增量拉取",
            Self::MonthlyReconciliation => "按月全量核对",
            Self::SingleOrderBackfill => "单号补拉",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Incremental => "incremental",
            Self::MonthlyReconciliation => "monthly_reconciliation",
            Self::SingleOrderBackfill => "single_order_backfill",
        }
    }
}

/// 同步作业状态（数据模型 §6.13：运行中、成功、部分失败、失败）。
///
/// 固定状态机：运行中单向推进到成功、部分失败或失败，终态不可回退；
/// 下一周期重试由 P3 创建新作业完成（§8.4：单次同步失败水位不前移）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MallSalesSyncJobStatus {
    /// 运行中。
    Running,
    /// 成功。
    Success,
    /// 部分失败。
    PartialFailure,
    /// 失败。
    Failed,
}

impl MallSalesSyncJobStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Running => "运行中",
            Self::Success => "成功",
            Self::PartialFailure => "部分失败",
            Self::Failed => "失败",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Success => "success",
            Self::PartialFailure => "partial_failure",
            Self::Failed => "failed",
        }
    }
}

impl DocumentState for MallSalesSyncJobStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Running => &[Self::Success, Self::PartialFailure, Self::Failed],
            Self::Success | Self::PartialFailure | Self::Failed => &[],
        }
    }
}

/// 同步作业完成命令相对当前实体状态的幂等判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncJobCompletionDisposition {
    /// 作业仍在运行，应提交本次终态。
    Apply,
    /// 作业已处于同一终态，可按幂等返回。
    AlreadyApplied,
    /// 作业已处于不同终态，命令冲突。
    ConflictingTerminal,
}

/// 同步作业的闭区间查询范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MallSyncTimeRange {
    /// 查询起点。
    pub start: Instant,
    /// 查询终点。
    pub end: Instant,
}

impl MallSyncTimeRange {
    /// 从安全水位与重叠窗口派生增量查询范围。
    ///
    /// # 参数
    /// * `high_water` - 最近安全水位
    /// * `now` - 当前安全时间
    /// * `overlap_seconds` - 为吸收迟到数据向前重叠的秒数
    ///
    /// # 返回
    /// 返回 `[high_water - overlap, now]` 查询范围。
    ///
    /// # 错误
    /// 派生起点晚于当前安全时间时返回错误。
    pub fn incremental(high_water: Instant, now: Instant, overlap_seconds: i64) -> Result<Self> {
        let start = Instant::from_unix_secs(high_water.unix_secs().saturating_sub(overlap_seconds.max(0)));
        if start > now {
            return Err(Error::from("同步水位晚于当前安全时间，禁止创建无效增量区间"));
        }
        Ok(Self { start, end: now })
    }
}

/// 同步作业创建数据（数据模型 §6.13）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallSalesSyncJobData {
    /// 来源商城。
    pub source_system_id: SourceSystemId,
    /// 作业类型。
    pub job_type: MallSalesSyncJobType,
    /// 本次查询时间边界起；单号补拉等无区间任务为空。
    pub range_start: Option<Instant>,
    /// 本次查询时间边界止；与 `range_start` 必须成对出现。
    pub range_end: Option<Instant>,
    /// 按单补拉的原来源销售单号。
    pub external_order_no: Option<String>,
    /// 触发来源。
    pub trigger_source: MallSyncTriggerSource,
    /// 人工触发理由；系统调度为空。
    pub trigger_reason: Option<String>,
    /// 人工触发人；系统调度为空。
    pub triggered_by: Option<String>,
    /// 失败重试沿用的原作业。
    pub source_job_id: Option<MallSalesSyncJobId>,
    /// 任务开始时间。
    pub started_at: Instant,
}

/// 商城卡券销售单同步作业实体（数据模型 §6.13）。
///
/// 作业记录一次期初基线、增量拉取、按月全量核对或单号补拉执行；
/// 状态按固定状态机推进，统计计数随处理进度累计。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct MallSalesSyncJob {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 来源商城。
    pub source_system_id: SourceSystemId,
    /// 作业类型。
    pub job_type: MallSalesSyncJobType,
    /// 本次查询时间边界起。
    pub range_start: Option<Instant>,
    /// 本次查询时间边界止。
    pub range_end: Option<Instant>,
    /// 按单补拉的原来源销售单号。
    pub external_order_no: Option<String>,
    /// 触发来源。
    pub trigger_source: MallSyncTriggerSource,
    /// 人工触发理由。
    pub trigger_reason: Option<String>,
    /// 人工触发人。
    pub triggered_by: Option<String>,
    /// 失败重试沿用的原作业。
    pub source_job_id: Option<MallSalesSyncJobId>,
    /// 任务开始时间。
    pub started_at: Instant,
    /// 任务结束时间。
    pub finished_at: Option<Instant>,
    /// 作业状态。
    pub status: MallSalesSyncJobStatus,
    /// 结果统计：处理页数。
    pub page_count: u64,
    /// 结果统计：处理条数。
    pub item_count: u64,
    /// 结果统计：错误条数。
    pub error_count: u64,
}

impl MallSalesSyncJob {
    /// 创建同步作业。
    ///
    /// 校验查询区间不变式：`range_start` 与 `range_end` 必须同时提供或
    /// 同时省略，且起不晚于止；作业创建即 `Running`，统计计数为零。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallSalesSyncJobId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的同步作业实体。
    ///
    /// # 错误
    /// 查询区间只提供一端或起晚于止时返回错误。
    pub fn new(id: crate::ids::MallSalesSyncJobId, data: MallSalesSyncJobData) -> Result<Self> {
        Self::ensure_range(data.range_start, data.range_end)?;
        let external_order_no =
            normalize_optional_text(data.external_order_no, "来源单号", ORDER_NO_MAX_LEN)?;
        let trigger_reason = normalize_optional_text(data.trigger_reason, "触发理由", REASON_MAX_LEN)?;
        let triggered_by = normalize_optional_text(data.triggered_by, "触发人", ACTOR_MAX_LEN)?;
        Self::ensure_trigger_contract(
            data.job_type,
            data.trigger_source,
            external_order_no.as_deref(),
            trigger_reason.as_deref(),
            triggered_by.as_deref(),
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            source_system_id: data.source_system_id,
            job_type: data.job_type,
            range_start: data.range_start,
            range_end: data.range_end,
            external_order_no,
            trigger_source: data.trigger_source,
            trigger_reason,
            triggered_by,
            source_job_id: data.source_job_id,
            started_at: data.started_at,
            finished_at: None,
            status: MallSalesSyncJobStatus::Running,
            page_count: 0,
            item_count: 0,
            error_count: 0,
        })
    }

    /// 判断作业是否仍接受快照落盘。
    ///
    /// # 返回
    /// 当前状态为运行中时返回 `true`。
    pub fn accepts_snapshots(&self) -> bool {
        self.status == MallSalesSyncJobStatus::Running
    }

    /// 判断失败作业是否允许沿原范围创建重试任务。
    ///
    /// # 返回
    /// 失败或部分失败时返回 `true`。
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.status,
            MallSalesSyncJobStatus::Failed | MallSalesSyncJobStatus::PartialFailure
        )
    }

    /// 判断乐观锁版本是否与命令期望一致。
    ///
    /// # 参数
    /// * `expected` - 客户端冻结版本
    ///
    /// # 返回
    /// 版本一致时返回 `true`。
    pub fn has_version(&self, expected: u64) -> bool {
        self.base.version == expected
    }

    /// 判定完成命令应提交、幂等返回还是冲突。
    ///
    /// # 参数
    /// * `outcome` - 命令请求的终态
    ///
    /// # 返回
    /// 返回相对当前作业状态的幂等判定。
    pub fn completion_disposition(&self, outcome: MallSalesSyncJobStatus) -> SyncJobCompletionDisposition {
        if self.status == MallSalesSyncJobStatus::Running {
            SyncJobCompletionDisposition::Apply
        } else if self.status == outcome {
            SyncJobCompletionDisposition::AlreadyApplied
        } else {
            SyncJobCompletionDisposition::ConflictingTerminal
        }
    }

    /// 完成作业并登记结束时间。
    ///
    /// 仅运行中作业可完成；成功要求错误计数为零，否则作业结果自相矛盾。
    ///
    /// # 参数
    /// * `outcome` - 成功、部分失败或失败
    /// * `finished_at` - 任务结束时间
    ///
    /// # 返回
    /// 完成操作返回 `Ok(())`。
    ///
    /// # 错误
    /// 非运行中状态，或成功结果携带非零错误计数时返回错误。
    pub fn finish(&mut self, outcome: MallSalesSyncJobStatus, finished_at: Instant) -> Result<()> {
        ensure_transition(self.status, outcome)?;
        if outcome == MallSalesSyncJobStatus::Success && self.error_count > 0 {
            return Err(Error::from("成功作业不得携带非零错误计数"));
        }
        self.status = outcome;
        self.finished_at = Some(finished_at);
        Ok(())
    }

    /// 累计处理进度统计。
    ///
    /// 每次报告一批处理结果并累加到作业计数；单批内错误条数不得超过
    /// 该批处理条数。
    ///
    /// # 参数
    /// * `pages` - 本批处理页数
    /// * `items` - 本批处理条数
    /// * `errors` - 本批错误条数
    ///
    /// # 返回
    /// 累加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 单批错误条数超过处理条数时返回错误。
    pub fn record_progress(&mut self, pages: u64, items: u64, errors: u64) -> Result<()> {
        if errors > items {
            return Err(Error::from("单批错误条数不能超过处理条数"));
        }
        self.page_count += pages;
        self.item_count += items;
        self.error_count += errors;
        Ok(())
    }

    /// 校验查询区间不变式。
    ///
    /// # 参数
    /// * `range_start` - 区间起
    /// * `range_end` - 区间止
    ///
    /// # 错误
    /// 区间只提供一端或起晚于止时返回错误。
    fn ensure_range(range_start: Option<Instant>, range_end: Option<Instant>) -> Result<()> {
        match (range_start, range_end) {
            (Some(start), Some(end)) if start <= end => Ok(()),
            (Some(_), Some(_)) => Err(Error::from("查询区间起点不得晚于终点")),
            (None, None) => Ok(()),
            _ => Err(Error::from("查询区间起点与终点必须同时提供或同时省略")),
        }
    }

    fn ensure_trigger_contract(
        job_type: MallSalesSyncJobType,
        trigger_source: MallSyncTriggerSource,
        external_order_no: Option<&str>,
        trigger_reason: Option<&str>,
        triggered_by: Option<&str>,
    ) -> Result<()> {
        if (job_type == MallSalesSyncJobType::SingleOrderBackfill) != external_order_no.is_some() {
            return Err(Error::from("只有单号补拉必须且只能携带原来源单号"));
        }
        match trigger_source {
            MallSyncTriggerSource::Scheduled if trigger_reason.is_none() && triggered_by.is_none() => Ok(()),
            MallSyncTriggerSource::Manual if trigger_reason.is_some() && triggered_by.is_some() => Ok(()),
            MallSyncTriggerSource::Scheduled => Err(Error::from("系统调度不得携带人工理由或操作人")),
            MallSyncTriggerSource::Manual => Err(Error::from("人工触发必须同时记录理由与操作人")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::ensure_transition;
    use crate::ids::MallSalesSyncJobId;

    fn job_data() -> MallSalesSyncJobData {
        MallSalesSyncJobData {
            source_system_id: SourceSystemId::new("sys-mall"),
            job_type: MallSalesSyncJobType::Incremental,
            range_start: Some(Instant::from_unix_secs(1_700_000_000)),
            range_end: Some(Instant::from_unix_secs(1_700_030_000)),
            external_order_no: None,
            trigger_source: MallSyncTriggerSource::Manual,
            trigger_reason: Some("人工检查同步进度".to_string()),
            triggered_by: Some("user-1".to_string()),
            source_job_id: None,
            started_at: Instant::from_unix_secs(1_700_000_100),
        }
    }

    #[test]
    fn new_starts_running_with_zero_counts() {
        let job = MallSalesSyncJob::new(MallSalesSyncJobId::new("j-1"), job_data()).unwrap();

        assert_eq!(job.job_type, MallSalesSyncJobType::Incremental);
        assert_eq!(job.status, MallSalesSyncJobStatus::Running);
        assert!(job.finished_at.is_none());
        assert_eq!((job.page_count, job.item_count, job.error_count), (0, 0, 0));
    }

    #[test]
    fn new_rejects_broken_range() {
        let one_sided = MallSalesSyncJobData {
            range_start: Some(Instant::from_unix_secs(1_700_000_000)),
            range_end: None,
            ..job_data()
        };
        assert!(MallSalesSyncJob::new(MallSalesSyncJobId::new("j-2"), one_sided).is_err());

        let reversed = MallSalesSyncJobData {
            range_start: Some(Instant::from_unix_secs(1_700_030_000)),
            range_end: Some(Instant::from_unix_secs(1_700_000_000)),
            ..job_data()
        };
        assert!(MallSalesSyncJob::new(MallSalesSyncJobId::new("j-3"), reversed).is_err());
    }

    #[test]
    fn single_order_backfill_allows_empty_range() {
        let backfill = MallSalesSyncJobData {
            job_type: MallSalesSyncJobType::SingleOrderBackfill,
            range_start: None,
            range_end: None,
            external_order_no: Some("MALL-001".to_string()),
            ..job_data()
        };
        let job = MallSalesSyncJob::new(MallSalesSyncJobId::new("j-4"), backfill).unwrap();
        assert!(job.range_start.is_none());
        assert!(job.range_end.is_none());
        assert_eq!(job.external_order_no.as_deref(), Some("MALL-001"));
    }

    #[test]
    fn trigger_contract_rejects_fake_single_order_and_manual_without_reason() {
        let missing_order = MallSalesSyncJobData {
            job_type: MallSalesSyncJobType::SingleOrderBackfill,
            range_start: None,
            range_end: None,
            ..job_data()
        };
        assert!(MallSalesSyncJob::new(MallSalesSyncJobId::new("j-missing"), missing_order).is_err());

        let missing_reason = MallSalesSyncJobData {
            trigger_reason: None,
            ..job_data()
        };
        assert!(MallSalesSyncJob::new(MallSalesSyncJobId::new("j-reason"), missing_reason).is_err());
    }

    #[test]
    fn finish_success_requires_zero_errors() {
        let mut job = MallSalesSyncJob::new(MallSalesSyncJobId::new("j-5"), job_data()).unwrap();
        job.record_progress(3, 100, 0).unwrap();
        assert_eq!(job.page_count, 3);
        assert_eq!(job.item_count, 100);

        job.finish(
            MallSalesSyncJobStatus::Success,
            Instant::from_unix_secs(1_700_000_200),
        )
        .unwrap();
        assert_eq!(job.status, MallSalesSyncJobStatus::Success);
        assert!(job.finished_at.is_some());

        let mut with_errors = MallSalesSyncJob::new(MallSalesSyncJobId::new("j-6"), job_data()).unwrap();
        with_errors.record_progress(1, 10, 2).unwrap();
        assert!(
            with_errors
                .finish(
                    MallSalesSyncJobStatus::Success,
                    Instant::from_unix_secs(1_700_000_200)
                )
                .is_err(),
            "有错误的作业只能记为部分失败或失败"
        );
        with_errors
            .finish(
                MallSalesSyncJobStatus::PartialFailure,
                Instant::from_unix_secs(1_700_000_200),
            )
            .unwrap();
    }

    #[test]
    fn record_progress_rejects_errors_exceeding_items() {
        let mut job = MallSalesSyncJob::new(MallSalesSyncJobId::new("j-7"), job_data()).unwrap();
        assert!(job.record_progress(1, 10, 11).is_err());
    }

    #[test]
    fn time_range_and_completion_disposition_are_deterministic() {
        let range = MallSyncTimeRange::incremental(
            Instant::from_unix_secs(1_000),
            Instant::from_unix_secs(1_200),
            300,
        )
        .unwrap();
        assert_eq!(range.start, Instant::from_unix_secs(700));
        assert_eq!(range.end, Instant::from_unix_secs(1_200));
        assert!(MallSyncTimeRange::incremental(
            Instant::from_unix_secs(2_000),
            Instant::from_unix_secs(1_000),
            100,
        )
        .is_err());

        let mut job = MallSalesSyncJob::new(MallSalesSyncJobId::new("j-disposition"), job_data()).unwrap();
        assert_eq!(
            job.completion_disposition(MallSalesSyncJobStatus::Success),
            SyncJobCompletionDisposition::Apply
        );
        job.finish(
            MallSalesSyncJobStatus::Success,
            Instant::from_unix_secs(1_700_000_200),
        )
        .unwrap();
        assert_eq!(
            job.completion_disposition(MallSalesSyncJobStatus::Success),
            SyncJobCompletionDisposition::AlreadyApplied
        );
        assert_eq!(
            job.completion_disposition(MallSalesSyncJobStatus::Failed),
            SyncJobCompletionDisposition::ConflictingTerminal
        );
    }

    #[test]
    fn finished_job_cannot_finish_again() {
        let mut job = MallSalesSyncJob::new(MallSalesSyncJobId::new("j-8"), job_data()).unwrap();
        job.finish(
            MallSalesSyncJobStatus::Failed,
            Instant::from_unix_secs(1_700_000_200),
        )
        .unwrap();
        assert!(
            job.finish(
                MallSalesSyncJobStatus::Success,
                Instant::from_unix_secs(1_700_000_300)
            )
            .is_err(),
            "终态不可回退"
        );
    }

    #[test]
    fn status_machine_is_directed() {
        assert!(ensure_transition(MallSalesSyncJobStatus::Running, MallSalesSyncJobStatus::Success).is_ok());
        assert!(ensure_transition(
            MallSalesSyncJobStatus::Running,
            MallSalesSyncJobStatus::PartialFailure
        )
        .is_ok());
        assert!(ensure_transition(MallSalesSyncJobStatus::Running, MallSalesSyncJobStatus::Failed).is_ok());
        assert!(ensure_transition(MallSalesSyncJobStatus::Failed, MallSalesSyncJobStatus::Running).is_err());
    }

    #[test]
    fn status_and_type_serde_use_stable_codes() {
        assert_eq!(
            serde_json::to_string(&MallSalesSyncJobType::MonthlyReconciliation).unwrap(),
            "\"monthly_reconciliation\""
        );
        assert_eq!(
            serde_json::to_string(&MallSalesSyncJobStatus::PartialFailure).unwrap(),
            "\"partial_failure\""
        );
        assert_eq!(MallSalesSyncJobType::Baseline.label(), "期初基线");
        assert_eq!(MallSalesSyncJobStatus::Running.label(), "运行中");
    }

    #[test]
    fn bson_roundtrip_preserves_entity() {
        let job = MallSalesSyncJob::new(MallSalesSyncJobId::new("j-9"), job_data()).unwrap();
        let roundtrip: MallSalesSyncJob =
            bson::deserialize_from_document(bson::serialize_to_document(&job).unwrap()).unwrap();
        assert_eq!(roundtrip, job);
    }
}
