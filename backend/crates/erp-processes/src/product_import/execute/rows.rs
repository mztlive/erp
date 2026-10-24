//! 逐行导入编排：每行结果持久化完成后，才执行下一行。

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_support::{BackgroundJob, BackgroundJobItem, ItemStatus};

use super::{RecordedOutcome, is_concurrent_claim};
use crate::Result;
use crate::background::import_budget::ImportBudget;

/// 导入行的业务写入与结果保存；单行保存必须原子推进明细和任务版本。
#[async_trait]
pub(super) trait ImportRowExecution: Send {
    /// 执行前重验取消状态与当前任务版本。
    ///
    /// # 参数
    /// * `job` - 当前切片持有的任务版本。
    ///
    /// # 返回
    /// 仍可推进时返回 true；取消、完成或版本变化时返回 false。
    ///
    /// # 错误
    /// 读取任务失败时返回错误。
    async fn prepare(&mut self, job: &BackgroundJob) -> Result<bool>;

    /// 执行一行并返回可持久化的业务结果；失败行也必须带上结果，而不是中断整批。
    ///
    /// # 参数
    /// * `row_number` - 源表行号。
    ///
    /// # 返回
    /// 返回该行的状态、错误码、摘要和可选商品身份。
    ///
    /// # 错误
    /// 不返回错误。
    async fn import(&mut self, row_number: u32) -> RecordedOutcome;

    /// 保存当前行和任务进度；取消或其他执行器写入必须通过版本冲突拒绝。
    ///
    /// # 参数
    /// * `job` - 当前导入任务，保存成功后反映新进度。
    /// * `item` - 已写入本行结果的明细。
    ///
    /// # 返回
    /// 明细与任务版本已原子推进。
    ///
    /// # 错误
    /// 仓储失败时返回对应错误。
    async fn persist(&mut self, job: &mut BackgroundJob, item: BackgroundJobItem) -> Result<()>;
}

/// 跳过已有结果，只逐行执行未完成项；保存失败时禁止继续执行后续行。
///
/// # 参数
/// * `execution` - 单行导入与进度保存。
/// * `job` - 当前导入任务。
/// * `items` - 任务明细；已有 `status` 的行跳过。
/// * `budget` - 本任务的行数与时间预算。
///
/// # 返回
/// 一个切片已执行并保存，或取消、并发认领后停止；未处理行留待下轮。
///
/// # 错误
/// 行结果无法写入明细，或保存失败且不是并发认领时返回对应错误。
pub(super) async fn execute_rows(
    execution: &mut impl ImportRowExecution,
    job: &mut BackgroundJob,
    items: Vec<BackgroundJobItem>,
    mut budget: ImportBudget,
) -> Result<()> {
    let remaining = items.iter().filter(|item| item.status.is_none()).count();
    for (index, mut item) in items.into_iter().filter(|item| item.status.is_none()).enumerate() {
        if !execution.prepare(job).await? {
            return Ok(());
        }
        let outcome = execution.import(item.source_row_no.unwrap_or(item.item_no)).await;
        record_progress(job, &mut item, outcome, index + 1 == remaining)?;
        let should_yield = budget.after_row();
        if let Err(error) = execution.persist(job, item).await {
            if is_concurrent_claim(&error) {
                return Ok(());
            }
            return Err(error);
        }
        if should_yield {
            break;
        }
    }
    Ok(())
}

fn record_progress(
    job: &mut BackgroundJob,
    item: &mut BackgroundJobItem,
    outcome: RecordedOutcome,
    finished: bool,
) -> Result<()> {
    item.record_result(
        outcome.status,
        outcome.code,
        outcome.summary,
        outcome.object_type,
        outcome.object_id,
    )?;
    Ok(job.record_import_result_batch(
        u64::from(outcome.status == ItemStatus::Success),
        u64::from(outcome.status == ItemStatus::Skipped),
        u64::from(outcome.status == ItemStatus::Failed),
        finished,
        Instant::now(),
    )?)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use erp_core::ids::{BackgroundJobId, BackgroundJobItemId};
    use erp_support::{BackgroundJobData, BackgroundJobItemData, JobStatus, JobType};
    use tokio::sync::Mutex;

    use super::super::{CONCURRENT_CLAIM_MESSAGE, recorded};
    use super::*;
    use crate::Error;
    use crate::background::import_source::ImportSourceCache;
    use crate::background::{BackgroundRunner, BackgroundTaskAdapter};
    use crate::product_import::execute::JobRowSource;
    use crate::product_import::parse::{ParsedProductRow, ParsedProductSheet};

    /// 记录真实编排的执行与保存交替顺序，并保留可供恢复的已提交结果。
    #[derive(Default)]
    struct RecordingRows {
        events: Vec<(&'static str, u32)>,
        saved_items: Vec<BackgroundJobItem>,
        saved_job: Option<BackgroundJob>,
        fail_at: Option<u32>,
        concurrent: bool,
        cancel_at: Option<u64>,
    }

    #[async_trait]
    impl ImportRowExecution for RecordingRows {
        async fn prepare(&mut self, job: &BackgroundJob) -> Result<bool> {
            Ok(self.cancel_at != Some(job.processed_count))
        }

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
        execute_rows(&mut execution, &mut job, items, ImportBudget::rows(200)).await.unwrap();
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
        execute_rows(&mut execution, &mut job, items, ImportBudget::rows(200)).await.unwrap();
        assert_eq!(execution.events, [("import", 1), ("persist", 1), ("import", 2), ("persist", 2)]);
        assert_eq!(execution.saved_items.len(), 1);
        assert_eq!(execution.saved_job.unwrap().processed_count, 1);
    }

    /// 保存失败只留下当前未提交行；恢复时保留之前的成功和失败结果，不再导入这些行。
    #[tokio::test]
    async fn recovery_keeps_previously_saved_outcomes_and_runs_only_unfinished_rows() {
        let (mut job, items) = fixture(4);
        let mut execution = RecordingRows { fail_at: Some(3), ..Default::default() };
        let error =
            execute_rows(&mut execution, &mut job, items.clone(), ImportBudget::rows(200)).await.unwrap_err();
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
        execute_rows(&mut resumed, &mut restored_job, restored_items, ImportBudget::rows(200)).await.unwrap();
        assert_eq!(resumed.events, [("import", 3), ("persist", 3), ("import", 4), ("persist", 4)]);
        assert_eq!(restored_job.success_count, 3);
        assert_eq!(restored_job.failed_count, 1);
        assert_eq!(restored_job.skipped_count, 0);
        assert_eq!(execution.saved_items[0].status, Some(ItemStatus::Success));
        assert_eq!(execution.saved_items[0].result_object_id.as_deref(), Some("product-1"));
        assert_eq!(execution.saved_items[1].status, Some(ItemStatus::Failed));
        assert!(restored_job.finished_at.is_some());
    }
    #[tokio::test]
    async fn row_budget_saves_the_last_row_and_resumes_without_marking_the_job_finished() {
        let (mut job, items) = fixture(4);
        let mut execution = RecordingRows::default();
        execute_rows(&mut execution, &mut job, items.clone(), ImportBudget::rows(2)).await.unwrap();
        assert_eq!(execution.events, [("import", 1), ("persist", 1), ("import", 2), ("persist", 2)]);
        assert_eq!(job.processed_count, 2);
        assert!(job.finished_at.is_none());
        let resumed_items = items
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
        execute_rows(&mut resumed, &mut job, resumed_items, ImportBudget::rows(2)).await.unwrap();
        assert_eq!(resumed.events, [("import", 3), ("persist", 3), ("import", 4), ("persist", 4)]);
        assert_eq!(job.processed_count, 4);
        assert!(job.finished_at.is_some());
    }

    #[tokio::test]
    async fn legacy_source_is_loaded_once_across_one_hundred_slices() {
        let (mut job, mut items) = fixture(1000);
        let cache = ImportSourceCache::new(1024 * 1024);
        let mut loads = 0;
        let mut imported = Vec::new();
        for _ in 0..100 {
            cache.retain_jobs(&[job.clone()]).await;
            let source = cache
                .get_or_load(&job, || async {
                    loads += 1;
                    let source = JobRowSource::Legacy {
                        bytes: Arc::new(vec![1, 2, 3]),
                        parsed: ParsedProductSheet {
                            sheet_name: "对内".into(),
                            rows: (1..=1000)
                                .map(|row_number| ParsedProductRow {
                                    row_number,
                                    cells: vec![format!("product-{row_number}")],
                                })
                                .collect(),
                            image_targets: Default::default(),
                        },
                    };
                    let bytes = source.input_bytes()?;
                    Ok((source, bytes))
                })
                .await
                .unwrap();
            let JobRowSource::Legacy { parsed, .. } = source.as_ref() else {
                panic!("应为旧任务来源")
            };
            assert_eq!(parsed.rows.len(), 1000);
            let mut execution = RecordingRows::default();
            execute_rows(&mut execution, &mut job, items.clone(), ImportBudget::rows(10)).await.unwrap();
            for saved in execution.saved_items {
                let item = items.iter_mut().find(|item| item.base.id == saved.base.id).unwrap();
                *item = saved;
            }
            imported.extend(execution.events.into_iter().filter(|(event, _)| *event == "import"));
            // 仓储逐行保存会改变版本；输入身份不应随版本失效。
            job.base.version += 10;
        }
        assert_eq!(loads, 1);
        assert_eq!(job.processed_count, 1000);
        assert!(job.finished_at.is_some());
        assert_eq!(imported, (1..=1000).map(|row| ("import", row)).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn cancellation_between_saved_rows_prevents_the_next_business_write() {
        let (mut job, items) = fixture(4);
        let mut execution = RecordingRows { cancel_at: Some(1), ..Default::default() };
        execute_rows(&mut execution, &mut job, items, ImportBudget::rows(4)).await.unwrap();
        assert_eq!(execution.events, [("import", 1), ("persist", 1)]);
        assert_eq!(job.processed_count, 1);
    }
    struct SliceState {
        job: BackgroundJob,
        items: Vec<BackgroundJobItem>,
        events: Vec<(&'static str, u32)>,
    }

    struct ImportAdapter(Arc<Mutex<SliceState>>);

    #[async_trait]
    impl BackgroundTaskAdapter for ImportAdapter {
        fn name(&self) -> &'static str {
            "import"
        }
        async fn run_due(&self) -> Result<usize> {
            let mut state = self.0.lock().await;
            let items = state.items.clone();
            let mut execution = RecordingRows::default();
            execute_rows(&mut execution, &mut state.job, items, ImportBudget::rows(2)).await?;
            for item in execution.saved_items {
                let saved = state.items.iter_mut().find(|saved| saved.base.id == item.base.id).unwrap();
                *saved = item;
            }
            state.events.extend(execution.events);
            Ok(1)
        }
    }

    struct RefreshAdapter(Arc<Mutex<SliceState>>);

    #[async_trait]
    impl BackgroundTaskAdapter for RefreshAdapter {
        fn name(&self) -> &'static str {
            "refresh"
        }
        async fn run_due(&self) -> Result<usize> {
            let mut state = self.0.lock().await;
            assert_eq!(state.job.processed_count, 2);
            assert_eq!(state.items.iter().filter(|item| item.status.is_none()).count(), 3);
            assert!(state.job.finished_at.is_none());
            state.events.push(("refresh", 0));
            Ok(1)
        }
    }

    #[tokio::test]
    async fn runner_gives_refresh_an_execution_opportunity_while_import_rows_are_still_pending() {
        let (job, items) = fixture(5);
        let state = Arc::new(Mutex::new(SliceState { job, items, events: Vec::new() }));
        let runner = BackgroundRunner::new(Duration::from_secs(2))
            .register(ImportAdapter(state.clone()))
            .register(RefreshAdapter(state.clone()));
        assert_eq!(runner.run_once().await, [("import", 1), ("refresh", 1)]);
        assert_eq!(
            state.lock().await.events,
            [("import", 1), ("persist", 1), ("import", 2), ("persist", 2), ("refresh", 0)]
        );
    }
}
