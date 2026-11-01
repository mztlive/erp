//! 原业务命令回执只允许把待确认失败计数转换为成功，不重新执行任务。
use erp_core::common::time::Instant;
use erp_core::{Error, Result};

use super::{BackgroundJob, JobStatus};

impl BackgroundJob {
    /// 按已核实的原命令回执调整终态结果计数。
    /// # 参数
    /// `confirmed` 为本次从待确认项核实的成功数，`at` 为核对时间。
    /// # 返回
    /// 计数守恒，保留原完成时间，按剩余失败数更新终态。
    /// # 错误
    /// 非文件任务、尚未完成、计数不完整或确认数超过原失败数时拒绝。
    pub fn confirm_unknown_successes(&mut self, confirmed: u64, at: Instant) -> Result<()> {
        if self.domain_job_type.as_deref() != Some("DATA_TRANSFER")
            || self.finished_at.is_none()
            || !matches!(self.status, JobStatus::Failed | JobStatus::PartiallySucceeded)
            || self.processed_count != self.total_count
            || confirmed == 0
            || confirmed > self.failed_count
        {
            return Err(Error::from("任务状态或待确认结果计数不允许核对成功"));
        }
        let success =
            self.success_count.checked_add(confirmed).ok_or_else(|| Error::from("成功结果计数溢出"))?;
        self.success_count = success;
        self.failed_count -= confirmed;
        self.status =
            if self.failed_count == 0 { JobStatus::Succeeded } else { JobStatus::PartiallySucceeded };
        self.last_progress_at = Some(at);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackgroundJobData, BackgroundJobId, JobType};

    fn job() -> BackgroundJob {
        let mut job = BackgroundJob::new(
            BackgroundJobId::new("j"),
            BackgroundJobData {
                job_no: "任务".into(),
                job_type: JobType::Import,
                domain_job_type: Some("DATA_TRANSFER".into()),
                domain_job_id: None,
                selection_snapshot_id: None,
                requested_by: "a".into(),
                request_id: "r".into(),
                input_file_asset_id: None,
                result_file_asset_id: None,
                total_count: 2,
            },
        )
        .unwrap();
        job.start(Instant::from_unix_secs(100)).unwrap();
        job.record_import_result_batch(0, 0, 2, true, Instant::from_unix_secs(101)).unwrap();
        job
    }

    #[test]
    fn recovery_preserves_total_and_completion_time_and_rejects_overcount() {
        let mut job = job();
        assert!(job.confirm_unknown_successes(3, Instant::from_unix_secs(102)).is_err());
        assert_eq!(job.failed_count, 2);
        job.confirm_unknown_successes(1, Instant::from_unix_secs(102)).unwrap();
        assert_eq!((job.success_count, job.failed_count, job.processed_count), (1, 1, 2));
        assert_eq!(job.status, JobStatus::PartiallySucceeded);
        job.confirm_unknown_successes(1, Instant::from_unix_secs(103)).unwrap();
        assert_eq!(job.status, JobStatus::Succeeded);
        assert_eq!(job.finished_at, Some(Instant::from_unix_secs(101)));
        assert!(job.confirm_unknown_successes(1, Instant::from_unix_secs(104)).is_err());
    }
}
