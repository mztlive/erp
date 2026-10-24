//! 供应商导入行边界：续期、执行业务、原子保存，达到预算后释放认领。

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_supplier::dto::import::SupplierImportResult;
use erp_support::{BackgroundJob, BackgroundJobItem, ItemStatus};

use super::record;
use crate::Result;
use crate::background::import_budget::ImportBudget;

/// 供应商导入的单行执行步骤；进度保存必须沿用原事务。
#[async_trait]
pub(super) trait SupplierRowExecution: Send {
    /// 逐行重验取消及任务版本，再续期执行权。
    ///
    /// # 参数
    /// * `job` - 当前持有的任务版本。
    ///
    /// # 返回
    /// 可继续时返回 true，取消、完成或其他执行器更新后返回 false。
    ///
    /// # 错误
    /// 读取、续期或版本保存失败时返回错误。
    async fn renew(&mut self, job: &mut BackgroundJob) -> Result<bool>;

    /// 执行一个供应商行，无法确认的结果保留为 None。
    ///
    /// # 参数
    /// * `item` - 待执行的任务行。
    ///
    /// # 返回
    /// 返回行结果；None 表示结果待核对。
    ///
    /// # 错误
    /// 不返回错误。
    async fn import(&mut self, item: &BackgroundJobItem) -> Option<SupplierImportResult>;

    /// 原子保存当前行与任务进度，失败时禁止继续执行后续行。
    ///
    /// # 参数
    /// * `job` - 待保存的任务，成功后带有新版本。
    /// * `item` - 本行确定或待确认结果。
    ///
    /// # 返回
    /// 当前行和任务进度已经提交。
    ///
    /// # 错误
    /// 行写入、版本冲突或事务失败时返回错误。
    async fn persist(&mut self, job: &mut BackgroundJob, item: BackgroundJobItem) -> Result<()>;
}

/// 只在当前行保存成功后让出；未完成任务同时释放租约，供下一轮重新认领。
///
/// # 参数
/// * `execution` - 供应商行执行步骤。
/// * `job` - 当前任务版本。
/// * `items` - 按原序号排列的任务行。
/// * `budget` - 当前任务的切片预算。
///
/// # 返回
/// 一个切片已经提交，或任务被取消、接管后停止。
///
/// # 错误
/// 续期、状态迁移或进度持久化失败时返回错误，并停止后续行。
pub(super) async fn execute_rows(
    execution: &mut impl SupplierRowExecution,
    job: &mut BackgroundJob,
    items: Vec<BackgroundJobItem>,
    mut budget: ImportBudget,
) -> Result<()> {
    for mut item in items.into_iter().filter(|item| item.status.is_none()) {
        if !execution.renew(job).await? {
            return Ok(());
        }
        let outcome = execution.import(&item).await;
        record(&mut item, outcome)?;
        record_progress(job, &item)?;
        let should_yield = budget.after_row();
        if should_yield && job.finished_at.is_none() {
            job.last_progress_at = None;
        }
        execution.persist(job, item).await?;
        if should_yield {
            break;
        }
    }
    Ok(())
}

fn record_progress(job: &mut BackgroundJob, item: &BackgroundJobItem) -> Result<()> {
    let status = item.status.expect("record always sets status");
    Ok(job.record_import_result_batch(
        u64::from(status == ItemStatus::Success),
        u64::from(status == ItemStatus::Skipped),
        u64::from(status == ItemStatus::Failed),
        job.processed_count + 1 == job.total_count,
        Instant::now(),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::supplier_import::execute::claimable;
    use crate::supplier_import::submit::tests::fixture;

    #[derive(Default)]
    struct RecordingRows {
        events: Vec<(&'static str, u32)>,
        saved: Vec<BackgroundJobItem>,
        saved_job: Option<BackgroundJob>,
        fail_at: Option<u32>,
        cancelled_after: Option<u64>,
    }

    #[async_trait]
    impl SupplierRowExecution for RecordingRows {
        async fn renew(&mut self, job: &mut BackgroundJob) -> Result<bool> {
            self.events.push(("renew", u32::try_from(job.processed_count + 1).unwrap()));
            Ok(self.cancelled_after != Some(job.processed_count))
        }
        async fn import(&mut self, item: &BackgroundJobItem) -> Option<SupplierImportResult> {
            self.events.push(("import", item.item_no));
            Some(SupplierImportResult {
                row_number: item.source_row_no.unwrap(),
                name: "供应商".into(),
                status: "succeeded".into(),
                message: "资料已导入".into(),
                supplier_id: Some(format!("supplier-{}", item.item_no)),
                supplier_no: None,
            })
        }
        async fn persist(&mut self, job: &mut BackgroundJob, item: BackgroundJobItem) -> Result<()> {
            self.events.push(("persist", item.item_no));
            if self.fail_at == Some(item.item_no) {
                return Err(Error::Internal("progress unavailable".into()));
            }
            self.saved.push(item);
            self.saved_job = Some(job.clone());
            Ok(())
        }
    }

    fn rows(count: u32) -> (BackgroundJob, Vec<BackgroundJobItem>) {
        let (mut job, items) = fixture();
        job.total_count = u64::from(count);
        job.start(Instant::from_unix_secs(1000)).unwrap();
        let items = (1..=count)
            .map(|row| {
                let mut item = items[0].clone();
                item.base.id = format!("item-{row}");
                item.item_no = row;
                item.source_row_no = Some(row + 8);
                item
            })
            .collect();
        (job, items)
    }

    #[tokio::test]
    async fn slice_saves_before_yield_and_releases_the_lease_for_next_poll() {
        let (mut job, items) = rows(3);
        let mut execution = RecordingRows::default();
        execute_rows(&mut execution, &mut job, items.clone(), ImportBudget::rows(2)).await.unwrap();
        assert_eq!(
            execution.events,
            [("renew", 1), ("import", 1), ("persist", 1), ("renew", 2), ("import", 2), ("persist", 2)]
        );
        assert_eq!(job.processed_count, 2);
        assert!(job.finished_at.is_none());
        assert!(execution.saved_job.as_ref().unwrap().last_progress_at.is_none());
        assert!(claimable(&job, Instant::now()));
        let remaining = items
            .into_iter()
            .map(|item| {
                execution.saved.iter().find(|saved| saved.item_no == item.item_no).cloned().unwrap_or(item)
            })
            .collect();
        let mut resumed = RecordingRows::default();
        execute_rows(&mut resumed, &mut job, remaining, ImportBudget::rows(2)).await.unwrap();
        assert_eq!(resumed.events, [("renew", 3), ("import", 3), ("persist", 3)]);
        assert_eq!(job.processed_count, 3);
        assert!(job.finished_at.is_some());
        assert!(job.last_progress_at.is_some());
    }

    #[tokio::test]
    async fn persistence_failure_stops_before_the_next_row() {
        let (mut job, items) = rows(3);
        let mut execution = RecordingRows { fail_at: Some(2), ..Default::default() };
        let error = execute_rows(&mut execution, &mut job, items, ImportBudget::rows(3)).await.unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "progress unavailable"));
        assert_eq!(execution.saved.len(), 1);
        assert_eq!(execution.events.last(), Some(&("persist", 2)));
        assert!(!execution.events.contains(&("import", 3)));
    }

    #[tokio::test]
    async fn cancellation_after_a_saved_row_stops_before_the_next_business_write() {
        let (mut job, items) = rows(3);
        let mut execution = RecordingRows { cancelled_after: Some(1), ..Default::default() };
        execute_rows(&mut execution, &mut job, items, ImportBudget::rows(3)).await.unwrap();
        assert_eq!(execution.events, [("renew", 1), ("import", 1), ("persist", 1), ("renew", 2)]);
        assert_eq!(job.processed_count, 1);
    }
}
