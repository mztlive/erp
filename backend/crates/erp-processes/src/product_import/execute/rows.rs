//! 逐行导入编排：每行结果持久化完成后，才执行下一行。

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_support::{BackgroundJob, BackgroundJobItem, ItemStatus};

use super::{RecordedOutcome, is_concurrent_claim};
use crate::Result;

/// 导入行的业务写入与结果保存；单行保存必须原子推进明细和任务版本。
#[async_trait]
pub(super) trait ImportRowExecution: Send {
    /// 执行一行并返回原始业务结果，失败行也必须持久化其结果。
    async fn import(&mut self, row_number: u32) -> RecordedOutcome;

    /// 保存当前行和任务进度；取消或其他执行器写入必须通过版本冲突拒绝。
    async fn persist(&mut self, job: &mut BackgroundJob, item: BackgroundJobItem) -> Result<()>;
}

/// 跳过已有结果，只逐行执行未完成项；保存失败时禁止继续执行后续行。
pub(super) async fn execute_rows(
    execution: &mut impl ImportRowExecution,
    job: &mut BackgroundJob,
    items: Vec<BackgroundJobItem>,
) -> Result<()> {
    let remaining = items.iter().filter(|item| item.status.is_none()).count();
    for (index, mut item) in items.into_iter().filter(|item| item.status.is_none()).enumerate() {
        let outcome = execution.import(item.source_row_no.unwrap_or(item.item_no)).await;
        item.record_result(
            outcome.status,
            outcome.code,
            outcome.summary,
            outcome.object_type,
            outcome.object_id,
        )?;
        job.record_import_result_batch(
            u64::from(outcome.status == ItemStatus::Success),
            u64::from(outcome.status == ItemStatus::Skipped),
            u64::from(outcome.status == ItemStatus::Failed),
            index + 1 == remaining,
            Instant::now(),
        )?;
        if let Err(error) = execution.persist(job, item).await {
            if is_concurrent_claim(&error) {
                return Ok(());
            }
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{BackgroundJobId, BackgroundJobItemId};
    use erp_support::{BackgroundJobData, BackgroundJobItemData, JobStatus, JobType};

    use super::super::{CONCURRENT_CLAIM_MESSAGE, recorded};
    use super::*;
    use crate::Error;

    /// 记录真实编排的执行与保存交替顺序，并保留可供恢复的已提交结果。
    #[derive(Default)]
    struct RecordingRows {
        events: Vec<(&'static str, u32)>,
        saved_items: Vec<BackgroundJobItem>,
        saved_job: Option<BackgroundJob>,
        fail_at: Option<u32>,
        concurrent: bool,
    }

    #[async_trait]
    impl ImportRowExecution for RecordingRows {
        /// 第二行模拟业务失败，其他行返回成功及对应商品身份。
        async fn import(&mut self, row_number: u32) -> RecordedOutcome {
            self.events.push(("import", row_number));
            let status = if row_number == 2 { ItemStatus::Failed } else { ItemStatus::Success };
            recorded(
                status,
                (status == ItemStatus::Failed).then_some("import_failed"),
                format!("row {row_number}"),
                (status == ItemStatus::Success).then(|| format!("product-{row_number}")),
            )
        }

        /// 仅成功保存的任务和明细进入恢复快照。
        async fn persist(&mut self, job: &mut BackgroundJob, item: BackgroundJobItem) -> Result<()> {
            self.events.push(("persist", item.item_no));
            if self.fail_at == Some(item.item_no) {
                return Err(if self.concurrent {
                    Error::ConflictError(CONCURRENT_CLAIM_MESSAGE.into())
                } else {
                    Error::Internal("progress unavailable".into())
                });
            }
            self.saved_items.push(item);
            self.saved_job = Some(job.clone());
            Ok(())
        }
    }

    /// 构造运行中任务与稳定行号，所有测试只执行内存替身。
    fn fixture(count: u32) -> (BackgroundJob, Vec<BackgroundJobItem>) {
        let mut job = BackgroundJob::new(
            BackgroundJobId::new("job"),
            BackgroundJobData {
                job_no: "IMPORT-1".into(),
                job_type: JobType::Import,
                domain_job_type: Some("PRODUCT_IMPORT".into()),
                domain_job_id: Some("batch".into()),
                selection_snapshot_id: None,
                requested_by: "admin".into(),
                request_id: "request".into(),
                input_file_asset_id: None,
                result_file_asset_id: None,
                total_count: u64::from(count),
            },
        )
        .unwrap();
        job.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        let items = (1..=count)
            .map(|number| {
                BackgroundJobItem::new(
                    BackgroundJobItemId::new(format!("item-{number}")),
                    BackgroundJobItemData {
                        background_job_id: BackgroundJobId::new("job"),
                        item_no: number,
                        object_type: None,
                        object_id: None,
                        expected_version: None,
                        expected_hash: None,
                        worksheet_name: None,
                        source_row_no: Some(number),
                        source_column_name: None,
                    },
                )
                .unwrap()
            })
            .collect();
        (job, items)
    }

    /// 超过原分片大小的任务也必须逐行提交，失败行计数与混合终态保持正确。
    #[tokio::test]
    async fn each_row_is_saved_before_the_next_row_runs() {
        let (mut job, items) = fixture(105);
        let mut execution = RecordingRows::default();
        execute_rows(&mut execution, &mut job, items).await.unwrap();
        let expected: Vec<_> = (1..=105).flat_map(|row| [("import", row), ("persist", row)]).collect();
        assert_eq!(execution.events, expected);
        assert_eq!(execution.saved_items.len(), 105);
        assert_eq!(job.processed_count, 105);
        assert_eq!(job.success_count, 104);
        assert_eq!(job.failed_count, 1);
        assert_eq!(job.status, JobStatus::PartiallySucceeded);
        assert!(job.finished_at.is_some());
    }

    /// 取消或其他执行器造成进度版本冲突时，当前行之后不得继续创建商品。
    #[tokio::test]
    async fn concurrent_progress_change_stops_before_the_next_business_write() {
        let (mut job, items) = fixture(105);
        let mut execution = RecordingRows { fail_at: Some(2), concurrent: true, ..Default::default() };
        execute_rows(&mut execution, &mut job, items).await.unwrap();
        assert_eq!(execution.events, [("import", 1), ("persist", 1), ("import", 2), ("persist", 2)]);
        assert_eq!(execution.saved_items.len(), 1);
        assert_eq!(execution.saved_job.unwrap().processed_count, 1);
    }

    /// 保存失败只留下当前未提交行；恢复时保留之前的成功和失败结果，不再导入这些行。
    #[tokio::test]
    async fn recovery_keeps_previously_saved_outcomes_and_runs_only_unfinished_rows() {
        let (mut job, items) = fixture(4);
        let mut execution = RecordingRows { fail_at: Some(3), ..Default::default() };
        let error = execute_rows(&mut execution, &mut job, items.clone()).await.unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "progress unavailable"));
        assert!(!execution.events.contains(&("import", 4)));
        let mut restored_job = execution.saved_job.unwrap();
        let restored_items = items
            .into_iter()
            .map(|item| {
                execution
                    .saved_items
                    .iter()
                    .find(|saved| saved.base.id == item.base.id)
                    .cloned()
                    .unwrap_or(item)
            })
            .collect();
        let mut resumed = RecordingRows::default();
        execute_rows(&mut resumed, &mut restored_job, restored_items).await.unwrap();
        assert_eq!(resumed.events, [("import", 3), ("persist", 3), ("import", 4), ("persist", 4)]);
        assert_eq!(restored_job.success_count, 3);
        assert_eq!(restored_job.failed_count, 1);
        assert_eq!(restored_job.skipped_count, 0);
        assert_eq!(execution.saved_items[0].status, Some(ItemStatus::Success));
        assert_eq!(execution.saved_items[0].result_object_id.as_deref(), Some("product-1"));
        assert_eq!(execution.saved_items[1].status, Some(ItemStatus::Failed));
        assert!(restored_job.finished_at.is_some());
    }
}
