//! 商品报价表导入后台任务工厂。
//!
//! Service 只注入任务 ID、源文件资产、行数与发起人；本模块独占任务编号、
//! 领域任务类型与幂等 `request_id` 合同。无 I/O、时钟或密钥。

use erp_core::Result;
use erp_core::ids::{BackgroundJobId, FileAssetId};

use super::{BackgroundJob, BackgroundJobData, JobType};

/// 商品导入任务编号前缀。
pub const PRODUCT_IMPORT_JOB_NO_PREFIX: &str = "PIMP";

/// 商品导入领域任务类型。
pub const PRODUCT_IMPORT_DOMAIN_JOB_TYPE: &str = "PRODUCT_IMPORT";

/// 构造商品导入任务编号。
///
/// # 参数
/// * `request_id` - 幂等请求身份
///
/// # 返回
/// 返回 `PIMP-<request_id>`。
///
/// # 错误
/// 无。
pub fn product_import_job_no(request_id: &str) -> String {
    format!("{PRODUCT_IMPORT_JOB_NO_PREFIX}-{request_id}")
}

impl BackgroundJob {
    /// 为产品报价表导入登记后台任务。
    ///
    /// 固定任务类型 `Import`、领域任务类型 `PRODUCT_IMPORT`，初始 `PENDING`。
    ///
    /// # 参数
    /// * `job_id` - 任务主键
    /// * `request_id` - 幂等请求身份
    /// * `file_asset_id` - 源 xlsx 文件资产
    /// * `total_rows` - 导入行数
    /// * `requested_by` - 发起人账号 ID
    ///
    /// # 返回
    /// 返回新建的后台任务实体。
    ///
    /// # 错误
    /// 编号或发起人校验失败时返回错误。
    pub fn for_product_import(
        job_id: BackgroundJobId,
        request_id: &str,
        file_asset_id: FileAssetId,
        total_rows: u64,
        requested_by: &str,
    ) -> Result<Self> {
        Self::new(
            job_id,
            BackgroundJobData {
                job_no: product_import_job_no(request_id),
                job_type: JobType::Import,
                domain_job_type: Some(PRODUCT_IMPORT_DOMAIN_JOB_TYPE.to_string()),
                domain_job_id: Some(file_asset_id.to_string()),
                selection_snapshot_id: None,
                requested_by: requested_by.to_string(),
                request_id: request_id.to_string(),
                input_file_asset_id: Some(file_asset_id),
                result_file_asset_id: None,
                total_count: total_rows,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{BackgroundJobId, FileAssetId};

    use super::{PRODUCT_IMPORT_DOMAIN_JOB_TYPE, product_import_job_no};
    use crate::entity::bulk_job::JobType;

    #[test]
    fn factory_pins_import_type_and_file() {
        let job = super::BackgroundJob::for_product_import(
            BackgroundJobId::new("job-1"),
            "req-1",
            FileAssetId::new("file-1"),
            3,
            " admin-1 ",
        )
        .unwrap();
        assert_eq!(job.job_no, product_import_job_no("req-1"));
        assert_eq!(job.job_type, JobType::Import);
        assert_eq!(job.domain_job_type.as_deref(), Some(PRODUCT_IMPORT_DOMAIN_JOB_TYPE));
        assert_eq!(job.requested_by, "admin-1");
        assert_eq!(job.total_count, 3);
    }
}
