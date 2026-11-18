//! 以任务版本作为 fencing token；心跳、取消和结果提交竞争同一 CAS。
use erp_core::common::time::Instant;
use erp_core::{Error, Result};

use super::{BackgroundJob, JobStatus, JobType};

impl BackgroundJob {
    /// 判断后台任务能否进入取消流程，仅未结束的等待、运行或部分成功任务可继续检查；数据迁移导入一旦启动便不可取消。
    pub fn cancellation_allowed(&self) -> bool {
        self.finished_at.is_none()
            && matches!(self.status, JobStatus::Pending | JobStatus::Running | JobStatus::PartiallySucceeded)
            && self.ensure_transfer_cancellable().is_ok()
    }

    pub(super) fn ensure_transfer_cancellable(&self) -> Result<()> {
        if self.domain_job_type.as_deref() == Some("DATA_TRANSFER")
            && self.job_type == JobType::Import
            && self.status != JobStatus::Pending
        {
            return Err(Error::from("该导入任务已开始处理，请等待结果后核对，不能中途取消"));
        }
        Ok(())
    }

    /// 执行器在开始处理前领取等待任务，或恢复至少 120 秒未更新心跳的运行中、部分成功任务。
    /// 领取会更新状态或心跳；只有用原版本 CAS 保存成功后才可处理。仍活动或已结束时返回 `false`，非法迁移返回错误。
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

    /// 长任务处理期间用服务端时间续期，只改心跳不改进度。调用方必须以自己持有的版本保存，不能重读版本抢写。
    /// 任务结束、无心跳或距离上次心跳达到 120 秒时拒绝续期。
    pub fn heartbeat_execution(&mut self, now: Instant) -> Result<()> {
        if !self.can_record_progress()
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
    fn heartbeat_preserves_state_and_lease_boundaries() {
        let now = Instant::from_unix_secs(1200);
        for status in [
            JobStatus::Pending,
            JobStatus::Running,
            JobStatus::PartiallySucceeded,
            JobStatus::Succeeded,
            JobStatus::Failed,
            JobStatus::Cancelled,
        ] {
            for finished_at in [None, Some(Instant::from_unix_secs(1000))] {
                for last_progress_at in
                    [None, Some(Instant::from_unix_secs(1080)), Some(Instant::from_unix_secs(1081))]
                {
                    let mut original = job();
                    original.status = status;
                    original.finished_at = finished_at;
                    original.last_progress_at = last_progress_at;

                    let mut updated = original.clone();
                    let result = updated.heartbeat_execution(now);
                    let allowed = finished_at.is_none()
                        && matches!(status, JobStatus::Running | JobStatus::PartiallySucceeded)
                        && last_progress_at == Some(Instant::from_unix_secs(1081));

                    if allowed {
                        result.unwrap();
                        original.last_progress_at = Some(now);
                        assert_eq!(updated, original);
                    } else {
                        assert_eq!(result.unwrap_err().to_string(), "任务执行权已失效，请查询原任务");
                        assert_eq!(updated, original);
                    }
                }
            }
        }
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
