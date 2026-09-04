//! W18 导入后台任务工厂（INT-E26：`BackgroundJob` 构造归属领域层）。
//!
//! Service 只注入任务 ID、批次事实与发起人，本模块独占任务编号、
//! 领域任务类型与幂等 `request_id` 合同。无 I/O、时钟或密钥。

use crate::errors::Result;
use crate::ids::BackgroundJobId;

use super::{BackgroundJob, BackgroundJobData, JobType};

/// 与批次创建同时登记的后台任务编号前缀（`background_job.job_no` 全局唯一）。
pub const LEGACY_IMPORT_JOB_NO_PREFIX: &str = "BJ";

/// 与批次创建同时登记的后台任务类型代码（`background_job.domain_job_type`）。
pub const LEGACY_IMPORT_DOMAIN_JOB_TYPE: &str = "LEGACY_IMPORT";

/// 构造批次后台任务编号（INT-E26 确定性构造）。
///
/// `BJ-<batch_no>`，批次号全局唯一 → 任务编号全局唯一。
///
/// # 参数
/// * `batch_no` - 导入批次号
///
/// # 返回
/// 返回后台任务编号字符串。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯字符串拼接，不访问数据库或时钟。
pub fn legacy_import_job_no(batch_no: &str) -> String {
    format!("{LEGACY_IMPORT_JOB_NO_PREFIX}-{batch_no}")
}

impl BackgroundJob {
    /// 为导入批次登记后台任务（INT-E26 领域工厂）。
    ///
    /// 固定任务类型 `Import`、领域任务类型 `LEGACY_IMPORT`、领域任务 ID
    /// 为批次 ID、`request_id` 为批次号（幂等定位）、总数为批次总行数，
    /// 初始 `PENDING`。任务 ID 与发起人由 Service 显式注入。
    ///
    /// # 参数
    /// * `job_id` - 任务主键（由 Service 预分配并注入）
    /// * `batch_no` - 导入批次号
    /// * `batch_id` - 导入批次实体 ID
    /// * `total_rows` - 批次总行数
    /// * `requested_by` - 发起人账号 ID
    ///
    /// # 返回
    /// 返回新建的后台任务实体。
    ///
    /// # 错误
    /// 当编号/发起人/请求身份校验失败时返回错误。
    ///
    /// # 约束
    /// 纯确定性构造；不访问 MongoDB、时钟、ID 生成器或密钥。
    pub fn for_legacy_import(
        job_id: BackgroundJobId,
        batch_no: &str,
        batch_id: &str,
        total_rows: u64,
        requested_by: &str,
    ) -> Result<Self> {
        Self::new(
            job_id,
            BackgroundJobData {
                job_no: legacy_import_job_no(batch_no),
                job_type: JobType::Import,
                domain_job_type: Some(LEGACY_IMPORT_DOMAIN_JOB_TYPE.to_string()),
                domain_job_id: Some(batch_id.to_string()),
                selection_snapshot_id: None,
                requested_by: requested_by.to_string(),
                request_id: batch_no.to_string(),
                input_file_asset_id: None,
                result_file_asset_id: None,
                total_count: total_rows,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{legacy_import_job_no, LEGACY_IMPORT_DOMAIN_JOB_TYPE};
    use crate::bulk_job::JobType;
    use crate::ids::BackgroundJobId;
    use std::collections::HashSet;

    #[test]
    fn job_no_is_prefixed_and_unique_per_batch() {
        assert_eq!(legacy_import_job_no("IMP-1"), "BJ-IMP-1");
        let numbers = ["IMP-1", "IMP-2", "IMP-1"]
            .iter()
            .map(|batch| legacy_import_job_no(batch))
            .collect::<Vec<_>>();
        assert_eq!(numbers[0], "BJ-IMP-1");
        assert_eq!(numbers[1], "BJ-IMP-2");
        assert_eq!(numbers[0], numbers[2]);
    }

    #[test]
    fn factory_pins_import_type_domain_and_idempotency() {
        let job = super::BackgroundJob::for_legacy_import(
            BackgroundJobId::new("job-1"),
            "IMP-1",
            "batch-1",
            3,
            " admin-1 ",
        )
        .unwrap();
        assert_eq!(job.job_no, "BJ-IMP-1");
        assert_eq!(job.job_type, JobType::Import);
        assert_eq!(
            job.domain_job_type.as_deref(),
            Some(LEGACY_IMPORT_DOMAIN_JOB_TYPE)
        );
        assert_eq!(job.domain_job_id.as_deref(), Some("batch-1"));
        assert_eq!(job.request_id, "IMP-1");
        assert_eq!(job.requested_by, "admin-1");
        assert_eq!(job.total_count, 3);
        let _ = HashSet::<String>::new();
    }

    #[test]
    fn factory_rejects_blank_actor_or_batch() {
        assert!(super::BackgroundJob::for_legacy_import(
            BackgroundJobId::new("job-1"),
            "IMP-1",
            "batch-1",
            1,
            "   ",
        )
        .is_err());
    }
}
