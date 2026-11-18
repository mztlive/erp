//! 导入切片只在当前行全部提交后让出，不提前推进下一行。
use async_trait::async_trait;
use background_runtime::ExecutionBudget;
use erp_catalog::dto::{BrandImportResult, BrandImportStatus};
use erp_core::common::time::Instant;
use erp_support::{BackgroundJob, BackgroundJobItem, ItemStatus};

use crate::Result;

/// 行事务和认领释放的执行边界。
#[async_trait]
pub(super) trait BrandRowExecution: Send {
    /// 提交一整行，取消或已被接管时返回 false。
    async fn commit(&mut self, job: &mut BackgroundJob, item: BackgroundJobItem) -> Result<bool>;
    /// 达到预算后释放认领，已保存结果不受影响。
    async fn release(&mut self, job: &mut BackgroundJob) -> Result<()>;
}

/// 执行有界切片，失败必须停止后续业务写入。
pub(super) async fn execute_rows(
    execution: &mut impl BrandRowExecution,
    job: &mut BackgroundJob,
    items: Vec<BackgroundJobItem>,
    mut budget: ExecutionBudget,
) -> Result<()> {
    for item in items.into_iter().filter(|item| item.status.is_none()) {
        if !execution.commit(job, item).await? {
            return Ok(());
        }
        if budget.after_row() {
            break;
        }
    }
    execution.release(job).await
}

/// 将领域结果映射到任务行，并更新同一事务内的任务计数。
pub(super) fn record(
    job: &mut BackgroundJob,
    item: &mut BackgroundJobItem,
    result: BrandImportResult,
) -> Result<()> {
    let status = match result.status {
        BrandImportStatus::Created => ItemStatus::Success,
        BrandImportStatus::Skipped => ItemStatus::Skipped,
        BrandImportStatus::Failed => ItemStatus::Failed,
    };
    item.record_result(
        status,
        (status == ItemStatus::Failed).then(|| "import_failed".into()),
        Some(result.message.chars().take(160).collect()),
        result.brand_id.as_ref().map(|_| "product_brand".into()),
        result.brand_id,
    )?;

    job.record_import_result_batch(
        u64::from(status == ItemStatus::Success),
        u64::from(status == ItemStatus::Skipped),
        u64::from(status == ItemStatus::Failed),
        job.processed_count + 1 == job.total_count,
        Instant::now(),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use erp_support::JobStatus;

    use super::*;
    use crate::Error;
    use crate::brand_import::submit::tests::fixture;

    #[derive(Default)]
    struct Execution {
        committed: Vec<u32>,
        stop_after: Option<u64>,
        fail_at: Option<u32>,
        released: bool,
    }
    #[async_trait]
    impl BrandRowExecution for Execution {
        async fn commit(&mut self, job: &mut BackgroundJob, mut item: BackgroundJobItem) -> Result<bool> {
            if self.stop_after == Some(job.processed_count) {
                return Ok(false);
            }
            if self.fail_at == Some(item.item_no) {
                return Err(Error::Internal("提交失败".into()));
            }
            self.committed.push(item.item_no);
            let row_number = item.source_row_no.unwrap();
            record(
                job,
                &mut item,
                BrandImportResult {
                    row_number,
                    status: BrandImportStatus::Created,
                    message: "已新建".into(),
                    brand_code: Some("BR-1".into()),
                    brand_id: Some("brand-1".into()),
                },
            )?;
            Ok(true)
        }
        async fn release(&mut self, job: &mut BackgroundJob) -> Result<()> {
            self.released = true;
            if job.finished_at.is_none() {
                job.last_progress_at = None;
            }
            Ok(())
        }
    }
    fn rows() -> (BackgroundJob, Vec<BackgroundJobItem>) {
        let (mut job, items) = fixture();
        job.total_count = 3;
        job.start(Instant::from_unix_secs(1000)).unwrap();
        let rows = (1..=3)
            .map(|number| {
                let mut item = items[0].clone();
                item.item_no = number;
                item
            })
            .collect();
        (job, rows)
    }
    fn budget(count: usize) -> ExecutionBudget {
        ExecutionBudget::new(count, Duration::from_secs(3600))
    }

    #[tokio::test]
    async fn slice_releases_after_commit_and_resume_skips_saved_items() {
        let (mut job, mut items) = rows();
        let mut execution = Execution::default();
        execute_rows(&mut execution, &mut job, items.clone(), budget(1)).await.unwrap();
        assert_eq!(execution.committed, [1]);
        assert_eq!(job.processed_count, 1);
        assert!(execution.released);
        assert!(job.can_resume_import(Instant::now(), 120));

        items[0].status = Some(ItemStatus::Success);
        execute_rows(&mut execution, &mut job, items, budget(10)).await.unwrap();
        assert_eq!(execution.committed, [1, 2, 3]);
        assert_eq!(job.success_count, 3);
        assert!(job.finished_at.is_some());
    }
    #[tokio::test]
    async fn cancellation_or_commit_failure_stops_before_later_rows() {
        for fail in [false, true] {
            let (mut job, items) = rows();
            let mut execution = Execution {
                stop_after: (!fail).then_some(1),
                fail_at: fail.then_some(2),
                ..Default::default()
            };
            let result = execute_rows(&mut execution, &mut job, items, budget(10)).await;
            assert_eq!(result.is_err(), fail);
            assert_eq!(execution.committed, [1]);
            assert_eq!(job.processed_count, 1);
            assert!(!execution.released);
        }
    }
    #[test]
    fn records_success_skip_failure_and_claims_only_stale_jobs() {
        let (mut job, mut items) = rows();
        for (index, status) in
            [BrandImportStatus::Created, BrandImportStatus::Skipped, BrandImportStatus::Failed]
                .into_iter()
                .enumerate()
        {
            record(
                &mut job,
                &mut items[index],
                BrandImportResult {
                    row_number: 9,
                    status,
                    message: "结果".into(),
                    brand_code: None,
                    brand_id: None,
                },
            )
            .unwrap();
        }
        assert_eq!((job.success_count, job.skipped_count, job.failed_count), (1, 1, 1));
        assert!(job.finished_at.is_some());
        assert!(!job.can_resume_import(Instant::now(), 120));

        let (mut job, _) = rows();
        assert!(!job.can_resume_import(Instant::from_unix_secs(1119), 120));
        assert!(job.can_resume_import(Instant::from_unix_secs(1120), 120));
        job.status = JobStatus::Cancelled;
        assert!(!job.can_resume_import(Instant::now(), 120));
    }
}
