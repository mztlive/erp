//! 连接治理消费的后台任务查询，任务集合与状态归 support。
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};

use crate::BackgroundJob;

/// 后台任务集合仓储的供应商连接治理查询。
#[allow(async_fn_in_trait)]
pub trait BackgroundJobRepositorySupplierConnectionExt {
    /// 读取异常登记之前同一连接、同一类型最近五十次失败，供精确来源匹配。
    /// 按创建时间和 ID 倒序返回候选；调用方必须核对原任务身份。
    /// `connection_id`、`job_type` 限定连接与任务类型；`at` 为异常登记时间。`executor` 沿用调用方读取执行器。
    /// 时间超出存储范围或数据库读取失败时返回错误。
    async fn connection_failure_candidates(
        &self,
        connection_id: &str,
        job_type: &str,
        at: u64,
        executor: &mut dyn Executor,
    ) -> Result<Vec<BackgroundJob>>;

    /// 读取连接最近的同步任务，最多二十条。
    /// 按创建时间倒序排列的目录任务。
    /// `connection_id` 为限定连接，`executor` 为调用方执行器。
    /// 传播数据库读取错误。
    async fn recent_catalog_jobs(
        &self,
        connection_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<BackgroundJob>>;

    /// 读取连接未结束的目录任务，最多一百条，用于串行恢复。
    /// 活动任务；达到上限时调用方必须拒绝创建新任务。
    /// 连接身份和当前事务执行器。
    /// 存储读取失败时返回错误。
    async fn active_catalog_jobs(
        &self,
        connection_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<BackgroundJob>>;

    /// 返回属于指定连接及任务类型的后台任务；空白名单直接无结果。
    /// 命中时返回任务；`job_types` 为空或无匹配时返回 `None`。
    /// `connection_id` 为供应商连接 ID，对应 `domain_job_id`；`job_id` 为后台任务 ID。
    /// `job_types` 为允许的领域任务类型；空切片不访问数据库；沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询错误。
    async fn find_supplier_connection_job(
        &self,
        connection_id: &str,
        job_id: &str,
        job_types: &[&str],
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>>;

    /// 统计阻止连接停用的目录同步任务，使用原三个未结束状态。
    /// 返回 `SUPPLIER_CATALOG_SYNC` 且状态为 `pending`、`running` 或 `partially_succeeded` 的未删除任务数量。
    /// `connection_id` 为供应商连接 ID，对应 `domain_job_id`；沿用调用方执行器 `executor`。
    /// MongoDB 计数失败时返回错误。
    async fn count_active_supplier_catalog_jobs(
        &self,
        connection_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<u64>;
}

impl BackgroundJobRepositorySupplierConnectionExt for persistence_core::Repository<'_, BackgroundJob> {
    async fn connection_failure_candidates(
        &self,
        connection_id: &str,
        job_type: &str,
        at: u64,
        executor: &mut dyn Executor,
    ) -> Result<Vec<BackgroundJob>> {
        let at =
            i64::try_from(at).map_err(|_| persistence_core::Error::EntityMetadataOutOfRange("异常时间"))?;
        mongo_ops::find_many(
            &self.collection(),
            failure_candidates_filter(connection_id, job_type, at),
            FindOptions::builder().sort(doc! { "created_at": -1, "id": -1 }).limit(50).build(),
            executor,
        )
        .await
    }

    async fn recent_catalog_jobs(
        &self,
        connection_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<BackgroundJob>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "domain_job_id": connection_id, "domain_job_type": "SUPPLIER_CATALOG_SYNC",
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder().sort(doc! { "created_at": -1, "id": -1 }).limit(20).build(),
            executor,
        )
        .await
    }

    async fn active_catalog_jobs(
        &self,
        connection_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<BackgroundJob>> {
        mongo_ops::find_many(
            &self.collection(),
            active_catalog_filter(connection_id),
            FindOptions::builder().limit(100).build(),
            executor,
        )
        .await
    }
    async fn find_supplier_connection_job(
        &self,
        connection_id: &str,
        job_id: &str,
        job_types: &[&str],
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>> {
        if job_types.is_empty() {
            return Ok(None);
        }
        self.find_one(connection_job_filter(connection_id, job_id, job_types), executor).await
    }

    async fn count_active_supplier_catalog_jobs(
        &self,
        connection_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        mongo_ops::count_documents(&self.collection(), active_catalog_filter(connection_id), executor).await
    }
}

fn connection_job_filter(connection_id: &str, job_id: &str, job_types: &[&str]) -> Document {
    doc! { "id": job_id, "domain_job_id": connection_id, "domain_job_type": { "$in": job_types } }
}
fn failure_candidates_filter(connection_id: &str, job_type: &str, at: i64) -> Document {
    doc! {
        "domain_job_id": connection_id, "domain_job_type": job_type,
        "status": "failed", "created_at": { "$lte": at },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}
fn active_catalog_filter(connection_id: &str) -> Document {
    doc! {
        "domain_job_type": "SUPPLIER_CATALOG_SYNC",
        "domain_job_id": connection_id,
        "status": { "$in": ["pending", "running", "partially_succeeded"] },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failure_candidates_keep_connection_type_time_and_failed_status() {
        let filter = failure_candidates_filter("conn-1", "SUPPLIER_HEALTH_CHECK", 123);
        assert_eq!(filter.get_str("domain_job_id").unwrap(), "conn-1");
        assert_eq!(filter.get_str("domain_job_type").unwrap(), "SUPPLIER_HEALTH_CHECK");
        assert_eq!(filter.get_str("status").unwrap(), "failed");
        assert_eq!(filter.get_document("created_at").unwrap().get_i64("$lte").unwrap(), 123);
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
    }
    #[test]
    fn connection_job_filter_keeps_identity_and_type_scope() {
        assert_eq!(
            connection_job_filter("conn-1", "job-1", &["SUPPLIER_HEALTH_CHECK", "SUPPLIER_CATALOG_SYNC"]),
            doc! {
                "id": "job-1", "domain_job_id": "conn-1",
                "domain_job_type": { "$in": ["SUPPLIER_HEALTH_CHECK", "SUPPLIER_CATALOG_SYNC"] },
            }
        );
    }
    #[test]
    fn active_catalog_filter_keeps_original_states_and_deleted_guard() {
        assert_eq!(
            active_catalog_filter("conn-1"),
            doc! {
                "domain_job_type": "SUPPLIER_CATALOG_SYNC", "domain_job_id": "conn-1",
                "status": { "$in": ["pending", "running", "partially_succeeded"] },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            }
        );
    }
}
