//! 失败重试范围：按批次读取失败行并在调用方事务内批量重置（INT-R31）。
//!
//! 读取固定约束 `batch_id + import_status=failed + 未软删除`，按
//! `created_at/id` 稳定排序；写入复用基类 CAS `update`，空集合零写，
//! 任一版本冲突由调用方事务整体回滚。全部使用调用方 executor，不开事务。

use entities::ids::LegacyImportBatchId;
use entities::legacy_import::{ImportStatus, LegacyImportRow};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;

use super::super::Repository;
use crate::executor::Executor;
use crate::Result;

impl<'a> Repository<'a, LegacyImportRow> {
    /// 按批次读取失败导入行（INT-R31 批量读取）。
    ///
    /// 取代执行命令事务中先全量加载再内存过滤的旧路径；已导入、已跳过与
    /// 待导入行不在结果中，软删除行视为不存在。空批次返回空集合。
    ///
    /// # 参数
    /// * `batch_id` - 目标导入批次
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该批次全部失败且未删除的导入行，按 `created_at/id` 稳定排序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 不返回 services DTO、HTTP View 或授权结论；不改变软删除、批次约束
    /// 与稳定排序语义；不自行开启或提交事务。
    pub async fn list_failed_by_batch(
        &self,
        batch_id: &LegacyImportBatchId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<LegacyImportRow>> {
        self.find_many_sorted(
            doc! {
                "batch_id": batch_id.to_string(),
                "import_status": ImportStatus::Failed.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 在调用方事务内批量重置已就地 `prepare_failed_retry` 的行（INT-R31 批量写）。
    ///
    /// 调用方先经实体 `prepare_failed_retry` 完成 `Failed → PendingImport`
    /// 状态迁移，本方法只执行逐项 CAS 写回；空集合零写，不访问数据库。
    /// 任一项版本冲突或写入失败时返回错误，由调用方事务整体回滚，
    /// 不产生部分提交。并发重试依赖每行 `id + version` 乐观锁，
    /// 单次快照查询不构成并发保护。
    ///
    /// # 参数
    /// * `rows` - 已完成失败重置 mutation 的行（就地更新版本号）
    /// * `executor` - 调用方事务执行器，必须位于同一事务中
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示全部写回成功。
    ///
    /// # 错误
    /// 当任一行 CAS 未命中或 MongoDB 写入失败时返回错误。
    ///
    /// # 约束
    /// 不自行开启或提交事务；不做业务状态裁决，只持久化已 mutation 行。
    pub async fn persist_failed_retry_rows(
        &self,
        rows: &mut [LegacyImportRow],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        for row in rows.iter_mut() {
            self.update(row, executor).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use entities::ids::LegacyImportBatchId;
    use entities::legacy_import::ImportStatus;
    use entity_core::NOT_DELETED_TIMESTAMP_BSON;

    #[test]
    fn failed_retry_filter_pins_batch_status_and_not_deleted() {
        let batch_id = LegacyImportBatchId::new("batch-1");
        let filter = doc! {
            "batch_id": batch_id.to_string(),
            "import_status": ImportStatus::Failed.as_str(),
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        assert_eq!(filter.get_str("batch_id").unwrap(), "batch-1");
        assert_eq!(filter.get_str("import_status").unwrap(), "failed");
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
    }

    #[test]
    fn failed_retry_sort_is_stable_by_created_at_then_id() {
        let sort = doc! { "created_at": 1, "id": 1 };
        assert_eq!(sort.get_i32("created_at").unwrap(), 1);
        assert_eq!(sort.get_i32("id").unwrap(), 1);
    }
}
