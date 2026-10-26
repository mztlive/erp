//! 失联任务结束旧执行权，新任务沿已保存游标继续；配置变化时重新扫描。
use erp_core::common::time::Instant;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_catalog::SupplierCatalogRun;
use erp_supply::repository::supplier_catalog::SupplierCatalogExt;
use erp_support::repository::prelude::*;
use erp_support::{BulkJobExt, JobStatus};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 结束超过五分钟无心跳的目录任务；正常任务禁止重叠。
/// # 参数
/// 数据库、连接身份及调用方事务执行器。
/// # 返回
/// 中断任务已在本事务结束。
/// # 错误
/// 仍有正常任务、未结束任务过多或存储失败时拒绝。
pub(super) async fn clear_stalled(
    db: &Database,
    connection: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let jobs = db.background_jobs().active_catalog_jobs(connection, executor).await?;
    if jobs.len() >= 100 {
        return Err(Error::ConflictError("未结束同步任务过多，请联系管理员".into()));
    }
    let now = Instant::now();
    for mut job in jobs {
        if !job.catalog_stalled(now) {
            return Err(Error::ConflictError("已有商品同步任务正在运行，请等待完成".into()));
        }
        job.close_stalled_catalog(now)?;
        db.background_jobs().update(&mut job, executor).await?;
    }
    Ok(())
}

/// 仅恢复最近失败任务且配置一致的未完成检查点；已完成扫描不沿用游标。
/// # 参数
/// 数据库、新任务身份、当前连接和事务执行器。
/// # 返回
/// 新的或沿用已保存进度的扫描检查点。
/// # 错误
/// 任务或检查点读取失败时返回错误。
pub(super) async fn next_run(
    db: &Database,
    job: &str,
    connection: &SupplierApiConnection,
    executor: &mut dyn Executor,
) -> Result<SupplierCatalogRun> {
    let mut next = SupplierCatalogRun::new(job, connection);
    let history = db.background_jobs().recent_catalog_jobs(&connection.base.id, executor).await?;
    if let Some(previous) = history.first().filter(|job| job.status == JobStatus::Failed)
        && let Some(run) = db.supplier_catalog_runs().find_by_id(&previous.base.id, executor).await?
        && !run.finished
        && run.technical_config_version == connection.technical_config_version
    {
        next.cursor = run.cursor;
        next.pages = run.pages;
        next.observed_count = run.observed_count;
    }
    Ok(next)
}
