//! 连接治理消费的后台任务查询，任务集合与状态归 support。
use super::owned::BackgroundJobRepository;
use crate::BackgroundJob;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use persistence_core::{mongo_ops, Executor, Result};

impl BackgroundJobRepository<'_> {
    /// 返回属于指定连接及任务类型的后台任务；空白名单直接无结果。
    pub async fn find_supplier_connection_job(
        &self,
        connection_id: &str,
        job_id: &str,
        job_types: &[&str],
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>> {
        if job_types.is_empty() {
            return Ok(None);
        }
        self.find_one(connection_job_filter(connection_id, job_id, job_types), executor)
            .await
    }

    /// 统计阻止连接停用的目录同步任务，使用原三个未结束状态。
    pub async fn count_active_supplier_catalog_jobs(
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
    fn connection_job_filter_keeps_identity_and_type_scope() {
        assert_eq!(
            connection_job_filter(
                "conn-1",
                "job-1",
                &["SUPPLIER_HEALTH_CHECK", "SUPPLIER_CATALOG_SYNC"]
            ),
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
