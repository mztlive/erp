//! 导入应用批次进度与终态的单一迁移（INT-E27）。
//!
//! 禁止 Service 连续组合 `mark_partially_succeeded` 与 `mark_succeeded`：
//! 后者会把含失败项的任务覆盖成纯 `Succeeded`。

use crate::common::time::Instant;
use crate::errors::{Error, Result};

use super::background_job::{BackgroundJob, JobStatus};

impl BackgroundJob {
    /// 记录一批导入行结果并在全部终态时选择唯一终态。
    ///
    /// 先按 `success + skipped + failed` 累加进度（零增量不写计数）；
    /// 尚未全部结束时，累计失败数大于 0 则进入 `PartiallySucceeded` 并保持可继续
    /// 记录进度。全部结束后按累计计数选择唯一终态：无失败为 `Succeeded`，
    /// 仅失败为 `Failed`，成功/跳过与失败混合为已完成的 `PartiallySucceeded`
    ///（写入 `finished_at`，禁止再累加进度，且不得落到纯 `Succeeded`）。
    ///
    /// # 参数
    /// * `success` - 本批成功数
    /// * `skipped` - 本批跳过数
    /// * `failed` - 本批失败数
    /// * `all_terminal` - 批次全部行是否已离开待导入
    /// * `at` - 进度或完成时刻
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 任务状态不允许记录导入结果、计数溢出、已处理数超过目标总数，
    /// 或全部终态时已处理数不等于目标总数时返回错误。
    ///
    /// # 约束
    /// 终态只由本方法一次判定；失败或混合结果不得落到纯 `Succeeded`。
    /// 不访问数据库或全局时钟。
    pub fn record_import_result_batch(
        &mut self,
        success: u64,
        skipped: u64,
        failed: u64,
        all_terminal: bool,
        at: Instant,
    ) -> Result<()> {
        if self.finished_at.is_some()
            || !matches!(self.status, JobStatus::Running | JobStatus::PartiallySucceeded)
        {
            return Err(Error::from(format!("状态 {:?} 不允许记录导入结果", self.status)));
        }
        let next_processed = next_processed_count(self.processed_count, success, skipped, failed)?;
        if next_processed > self.total_count {
            return Err(Error::from("已处理数不能超过目标总数"));
        }
        if all_terminal && next_processed != self.total_count {
            return Err(Error::from("全部行终态时已处理数必须等于目标总数"));
        }
        if success > 0 || skipped > 0 || failed > 0 {
            self.record_progress(success, skipped, failed, at)?;
        }
        if !all_terminal {
            return self.mark_partial_if_failed();
        }
        self.complete_import_terminal(at)
    }

    /// 累计已有失败且仍在执行中时进入部分成功。
    ///
    /// # 返回
    /// 无需迁移或迁移成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不允许进入部分成功时返回错误。
    fn mark_partial_if_failed(&mut self) -> Result<()> {
        if self.failed_count > 0 && self.status == JobStatus::Running {
            self.mark_partially_succeeded()?;
        }
        Ok(())
    }

    /// 按累计计数选择导入完成终态。
    ///
    /// # 参数
    /// * `at` - 完成时刻
    ///
    /// # 返回
    /// 终态迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不允许完成或失败时返回错误。
    fn complete_import_terminal(&mut self, at: Instant) -> Result<()> {
        if self.failed_count == 0 {
            return self.mark_succeeded(at);
        }
        if self.success_count == 0 && self.skipped_count == 0 {
            return self.mark_failed(None, at);
        }
        self.mark_partial_if_failed()?;
        self.finished_at = Some(at);
        self.last_progress_at = Some(at);
        Ok(())
    }
}

/// 计算本批之后的已处理数。
///
/// # 参数
/// * `processed` - 当前已处理数
/// * `success` / `skipped` / `failed` - 本批增量
///
/// # 返回
/// 返回累加后的已处理数。
///
/// # 错误
/// 加法溢出时返回错误，不修改调用方状态。
fn next_processed_count(processed: u64, success: u64, skipped: u64, failed: u64) -> Result<u64> {
    processed
        .checked_add(success)
        .and_then(|value| value.checked_add(skipped))
        .and_then(|value| value.checked_add(failed))
        .ok_or_else(|| Error::from("处理计数溢出"))
}

#[cfg(test)]
mod tests {
    use super::BackgroundJob;
    use crate::bulk_job::background_job::{BackgroundJobData, JobStatus, JobType};
    use crate::common::time::Instant;
    use crate::ids::BackgroundJobId;

    fn job() -> BackgroundJob {
        let mut job = BackgroundJob::new(
            BackgroundJobId::new("job-import"),
            BackgroundJobData {
                job_no: "BJ-IMP-1".to_string(),
                job_type: JobType::Import,
                domain_job_type: Some("LEGACY_IMPORT".to_string()),
                domain_job_id: Some("batch-1".to_string()),
                selection_snapshot_id: None,
                requested_by: "admin-1".to_string(),
                request_id: "IMP-1".to_string(),
                input_file_asset_id: None,
                result_file_asset_id: None,
                total_count: 4,
            },
        )
        .unwrap();
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        job
    }

    #[test]
    fn only_success_all_terminal_marks_succeeded() {
        let mut job = job();
        job.record_import_result_batch(4, 0, 0, true, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        assert_eq!(job.status, JobStatus::Succeeded);
        assert_eq!(job.processed_count, 4);
        assert_eq!(job.success_count, 4);
        assert_eq!(job.failed_count, 0);
        assert!(job.is_terminal());
    }

    #[test]
    fn only_failed_all_terminal_marks_failed_not_succeeded() {
        let mut job = job();
        job.record_import_result_batch(0, 0, 4, true, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.failed_count, 4);
        assert_eq!(job.success_count, 0);
        assert_ne!(job.status, JobStatus::Succeeded);
        assert!(job.is_terminal());
    }

    #[test]
    fn mixed_success_and_failed_all_terminal_is_not_succeeded() {
        let mut job = job();
        job.record_import_result_batch(2, 1, 1, true, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        assert_eq!(job.status, JobStatus::PartiallySucceeded);
        assert_eq!(job.processed_count, 4);
        assert_eq!(job.success_count, 2);
        assert_eq!(job.skipped_count, 1);
        assert_eq!(job.failed_count, 1);
        assert_eq!(job.finished_at, Some(Instant::from_unix_secs(1_700_000_100)));
        assert_ne!(job.status, JobStatus::Succeeded);
        assert!(job.is_terminal());
        let snapshot = job.clone();
        assert!(job
            .record_import_result_batch(0, 0, 0, true, Instant::from_unix_secs(1_700_000_200))
            .is_err());
        assert!(job
            .record_progress(0, 0, 0, Instant::from_unix_secs(1_700_000_200))
            .is_err());
        assert_eq!(job, snapshot);
    }

    #[test]
    fn not_all_terminal_keeps_job_open_and_conserves_counts() {
        let mut job = job();
        job.record_import_result_batch(1, 0, 1, false, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        assert_eq!(job.status, JobStatus::PartiallySucceeded);
        assert_eq!(job.processed_count, 2);
        assert_eq!(job.success_count + job.skipped_count + job.failed_count, 2);
        assert!(job.finished_at.is_none());
        assert!(!job.is_terminal());

        job.record_import_result_batch(1, 1, 0, true, Instant::from_unix_secs(1_700_000_200))
            .unwrap();
        assert_eq!(job.processed_count, 4);
        assert_eq!(job.success_count, 2);
        assert_eq!(job.skipped_count, 1);
        assert_eq!(job.failed_count, 1);
        assert_eq!(job.status, JobStatus::PartiallySucceeded);
        assert_ne!(job.status, JobStatus::Succeeded);
        assert!(job.finished_at.is_some());
        assert!(job.is_terminal());
    }

    #[test]
    fn zero_delta_not_terminal_does_not_change_counts() {
        let mut job = job();
        job.record_import_result_batch(0, 0, 0, false, Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        assert_eq!(job.status, JobStatus::Running);
        assert_eq!(job.processed_count, 0);
    }

    #[test]
    fn illegal_start_states_are_rejected_without_mutation() {
        let at = Instant::from_unix_secs(1_700_000_100);
        let mut pending = BackgroundJob::new(
            BackgroundJobId::new("job-pending"),
            BackgroundJobData {
                job_no: "BJ-P".to_string(),
                job_type: JobType::Import,
                domain_job_type: None,
                domain_job_id: None,
                selection_snapshot_id: None,
                requested_by: "admin-1".to_string(),
                request_id: "req-p".to_string(),
                input_file_asset_id: None,
                result_file_asset_id: None,
                total_count: 4,
            },
        )
        .unwrap();
        let pending_snapshot = pending.clone();
        assert!(pending.record_import_result_batch(1, 0, 0, false, at).is_err());
        assert_eq!(pending, pending_snapshot);

        let mut succeeded = job();
        succeeded.record_import_result_batch(4, 0, 0, true, at).unwrap();
        let succeeded_snapshot = succeeded.clone();
        assert!(succeeded.record_import_result_batch(0, 0, 0, true, at).is_err());
        assert_eq!(succeeded, succeeded_snapshot);

        let mut failed = job();
        failed.record_import_result_batch(0, 0, 4, true, at).unwrap();
        let failed_snapshot = failed.clone();
        assert!(failed.record_import_result_batch(0, 0, 0, true, at).is_err());
        assert_eq!(failed, failed_snapshot);

        let mut cancelled = job();
        cancelled.cancel(at).unwrap();
        let cancelled_snapshot = cancelled.clone();
        assert!(cancelled.record_import_result_batch(1, 0, 0, false, at).is_err());
        assert_eq!(cancelled, cancelled_snapshot);
    }

    #[test]
    fn all_terminal_count_mismatch_does_not_mutate() {
        let mut job = job();
        let snapshot = job.clone();
        let error = job
            .record_import_result_batch(1, 0, 0, true, Instant::from_unix_secs(1_700_000_100))
            .unwrap_err();
        assert!(error.to_string().contains("已处理数必须等于目标总数"));
        assert_eq!(job.status, snapshot.status);
        assert_eq!(job.processed_count, snapshot.processed_count);
        assert_eq!(job.finished_at, snapshot.finished_at);
    }

    #[test]
    fn overflow_and_exceeding_total_do_not_mutate() {
        let mut overflow = job();
        overflow.processed_count = u64::MAX;
        let overflow_snapshot = overflow.clone();
        assert!(overflow
            .record_import_result_batch(1, 0, 0, false, Instant::from_unix_secs(1_700_000_100))
            .is_err());
        assert_eq!(overflow, overflow_snapshot);

        let mut exceed = job();
        let exceed_snapshot = exceed.clone();
        assert!(exceed
            .record_import_result_batch(5, 0, 0, false, Instant::from_unix_secs(1_700_000_100))
            .is_err());
        assert_eq!(exceed, exceed_snapshot);
    }
}
