//! 以任务版本作为 fencing token；心跳、取消和结果提交竞争同一 CAS。
use erp_core::common::time::Instant;
use erp_core::{Error, Result};

use super::{BackgroundJob, JobStatus, JobType};

impl BackgroundJob {
    pub(super) fn ensure_transfer_cancellable(&self) -> Result<()> {
        if self.domain_job_type.as_deref() == Some("DATA_TRANSFER")
            && self.job_type == JobType::Import
            && self.status != JobStatus::Pending
        {
            return Err(Error::from("该导入任务已开始处理，请等待结果后核对，不能中途取消"));
        }
        Ok(())
    }

    /// 领取等待任务或超过两分钟没有心跳的任务。
    /// # 参数
    /// `now` 为服务端时间；必须将返回实体通过版本 CAS 保存后才执行。
    /// # 返回
    /// 可领取时更新状态及心跳并返回 true；活动任务或终态返回 false。
    /// # 错误
    /// 非法状态迁移时返回错误。
    pub fn claim_execution(&mut self, now: Instant) -> Result<bool> {
        if self.finished_at.is_some() {
            return Ok(false);
        }
        match self.status {
            JobStatus::Pending => self.start(now)?,
            JobStatus::Running | JobStatus::PartiallySucceeded => {
                if self.last_progress_at.is_some_and(|at| now.unix_secs() - at.unix_secs() < 120) {
                    return Ok(false);
                }
                self.last_progress_at = Some(now);
            },
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 延长当前执行权；调用方须用自己持有的版本保存，禁止重新读版本后抢写。
    /// # 参数
    /// `now` 为服务端当前时间。
    /// # 返回
    /// 更新心跳时间，不改变进度计数。
    /// # 错误
    /// 任务已结束或执行权已过期时拒绝续期。
    pub fn heartbeat_execution(&mut self, now: Instant) -> Result<()> {
        if self.finished_at.is_some()
            || !matches!(self.status, JobStatus::Running | JobStatus::PartiallySucceeded)
            || self.last_progress_at.is_none_or(|at| now.unix_secs() - at.unix_secs() >= 120)
        {
            return Err(Error::from("任务执行权已失效，请查询原任务"));
        }
        self.last_progress_at = Some(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackgroundJobData, BackgroundJobId, JobType};
    fn job() -> BackgroundJob {
        BackgroundJob::new(
            BackgroundJobId::new("j"),
            BackgroundJobData {
                job_no: "j".into(),
                job_type: JobType::Export,
                domain_job_type: None,
                domain_job_id: None,
                selection_snapshot_id: None,
                requested_by: "a".into(),
                request_id: "r".into(),
                input_file_asset_id: None,
                result_file_asset_id: None,
                total_count: 1,
            },
        )
        .unwrap()
    }
    #[test]
    fn live_work_cannot_be_reclaimed_but_expired_work_can() {
        let mut job = job();
        assert!(job.claim_execution(Instant::from_unix_secs(1000)).unwrap());
        assert!(!job.claim_execution(Instant::from_unix_secs(1119)).unwrap());
        assert!(job.claim_execution(Instant::from_unix_secs(1120)).unwrap());
        job.heartbeat_execution(Instant::from_unix_secs(1150)).unwrap();
        assert_eq!(job.processed_count, 0);
        assert!(!job.claim_execution(Instant::from_unix_secs(1239)).unwrap());
    }
    #[test]
    fn running_file_import_cannot_be_cancelled_by_single_or_bulk_paths() {
        let mut job = job();
        job.domain_job_type = Some("DATA_TRANSFER".into());
        job.job_type = JobType::Import;
        assert!(job.ensure_transfer_cancellable().is_ok());
        job.claim_execution(Instant::from_unix_secs(1000)).unwrap();
        assert!(job.cancel(Instant::from_unix_secs(1010)).is_err());
        assert_eq!(job.status, JobStatus::Running);
    }
    #[test]
    fn cancellation_and_expiration_fence_heartbeat() {
        let mut job = job();
        job.claim_execution(Instant::from_unix_secs(1000)).unwrap();
        assert!(job.heartbeat_execution(Instant::from_unix_secs(1120)).is_err());
        job.cancel(Instant::from_unix_secs(1120)).unwrap();
        assert!(!job.claim_execution(Instant::from_unix_secs(1300)).unwrap());
        assert!(job.heartbeat_execution(Instant::from_unix_secs(1300)).is_err());
    }
}
