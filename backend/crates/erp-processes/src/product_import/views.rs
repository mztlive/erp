//! 导入任务视图映射。

use erp_catalog::{ProductImportItemView, ProductImportJobView};
use erp_support::{BackgroundJob, BackgroundJobItemRow, FileAsset};

/// 把后台任务投影为商品导入任务视图。
///
/// # 参数
/// * `job` - 后台任务
/// * `file_name` - 源文件名
///
/// # 返回
/// 返回导入任务视图。
///
/// # 错误
/// 无。
pub fn job_view(job: &BackgroundJob, file_name: Option<String>) -> ProductImportJobView {
    ProductImportJobView {
        id: job.base.id.clone(),
        job_no: job.job_no.clone(),
        status: job.status.as_str().to_string(),
        file_name,
        total_count: job.total_count,
        processed_count: job.processed_count,
        success_count: job.success_count,
        skipped_count: job.skipped_count,
        failed_count: job.failed_count,
        started_at: job.started_at.map(|instant| instant.unix_secs() as u64),
        finished_at: job.finished_at.map(|instant| instant.unix_secs() as u64),
        error_summary: job.error_summary.clone(),
        version: job.base.version,
        created_at: job.base.created_at,
    }
}

/// 从文件资产取展示文件名。
///
/// # 参数
/// * `asset` - 源文件资产
///
/// # 返回
/// 返回文件名。
///
/// # 错误
/// 无。
pub fn file_name_of(asset: Option<&FileAsset>) -> Option<String> {
    asset.map(|item| item.file_name.clone())
}

/// 把后台任务逐项投影为导入结果行。
///
/// # 参数
/// * `row` - 逐项投影
///
/// # 返回
/// 返回导入结果视图。
///
/// # 错误
/// 无。
pub fn item_view(row: BackgroundJobItemRow) -> ProductImportItemView {
    ProductImportItemView {
        item_no: row.item_no,
        source_row_no: row.source_row_no,
        name: row.object_id,
        status: row.status.map(|status| status.as_str().to_string()),
        result_summary: row.result_summary,
        product_id: row.result_object_id,
    }
}
