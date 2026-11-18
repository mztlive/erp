//! 失败重试范围：按批次读取失败行并在调用方事务内批量重置（INT-R31）。
//!
//! 读取固定约束 `batch_id + import_status=failed + 未软删除`，按
//! `created_at/id` 稳定排序；写入复用基类 CAS `update`，空集合零写，
//! 任一版本冲突由调用方事务整体回滚。全部使用调用方 executor，不开事务。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::LegacyImportBatchId;
use mongodb::bson::doc;
use persistence_core::{Executor, Result};

use crate::entity::legacy_import::{ImportStatus, LegacyImportRow};

/// 导入行集合仓储扩展：失败重试范围读取与批量写回。
#[allow(async_fn_in_trait)]
pub trait LegacyImportRowFailedRetryExt {
    /// 用调用方执行器只取指定批次未删除的失败行，按 `created_at/id` 稳定排序，避免为重试读入全批数据。无匹配则返回空集合；查询或游标失败直接返回，不另开事务或给出授权结论。
    async fn list_failed_by_batch(
        &self,
        batch_id: &LegacyImportBatchId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<LegacyImportRow>>;

    /// 用调用方原事务逐行按 `id + version` 写回已由 `prepare_failed_retry` 重置为待导入的记录；行集合为空时不访问数据库。任何 CAS 未命中或写入失败由调用方整体回滚，不能部分提交；查询快照本身不能防并发重试。
    async fn persist_failed_retry_rows(
        &self,
        rows: &mut [LegacyImportRow],
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

impl LegacyImportRowFailedRetryExt for persistence_core::Repository<'_, LegacyImportRow> {
    async fn list_failed_by_batch(
        &self,
        batch_id: &LegacyImportBatchId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<LegacyImportRow>> {
        self.find_many_sorted(failed_retry_filter(batch_id), failed_retry_sort(), executor).await
    }

    async fn persist_failed_retry_rows(
        &self,
        rows: &mut [LegacyImportRow],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for row in rows {
            self.update(row, executor).await?;
        }
        Ok(())
    }
}

// 为失败重试准备精确条件，只匹配该批次未软删除且应用失败的行。
fn failed_retry_filter(batch_id: &LegacyImportBatchId) -> mongodb::bson::Document {
    doc! {
        "batch_id": batch_id.to_string(),
        "import_status": ImportStatus::Failed.as_str(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

// 让重试按 `created_at`、`id` 升序读取，创建时间相同也能稳定排序。
fn failed_retry_sort() -> mongodb::bson::Document {
    doc! { "created_at": 1, "id": 1 }
}

#[cfg(test)]
mod tests {
    use entity_core::NOT_DELETED_TIMESTAMP_BSON;
    use erp_core::ids::LegacyImportBatchId;

    use super::{failed_retry_filter, failed_retry_sort};

    #[test]
    fn failed_retry_filter_pins_batch_status_and_not_deleted() {
        let filter = failed_retry_filter(&LegacyImportBatchId::new("batch-1"));
        assert_eq!(filter.get_str("batch_id").unwrap(), "batch-1");
        assert_eq!(filter.get_str("import_status").unwrap(), "failed");
        assert_eq!(filter.get_i64("deleted_at").unwrap(), NOT_DELETED_TIMESTAMP_BSON);
    }

    #[test]
    fn failed_retry_sort_is_stable_by_created_at_then_id() {
        let sort = failed_retry_sort();
        assert_eq!(sort.get_i32("created_at").unwrap(), 1);
        assert_eq!(sort.get_i32("id").unwrap(), 1);
    }
}
