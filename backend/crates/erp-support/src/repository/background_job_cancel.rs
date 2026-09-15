//! 批量取消候选查询：MongoDB 条件与执行器只在仓储层解释。
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};

use super::owned::BackgroundJobRepository;
use crate::{BackgroundJob, JobStatus, JobType};

impl BackgroundJobRepository<'_> {
    /// 查询未删除且可能被批量取消的后台任务。
    ///
    /// # 参数
    /// * `job_type` - 可选任务类型；省略时查询全部类型。
    /// * `executor` - 由服务层指定的数据访问执行器。
    ///
    /// # 返回
    /// 返回等待、执行中和部分成功的候选实体；服务层逐个复核终态与取消规则。
    /// 已完成的部分成功仍作为候选返回，以保留批量取消的跳过计数口径。
    ///
    /// # 错误
    /// MongoDB 查询或实体解码失败时返回持久层错误。
    pub async fn cancellation_candidates(
        &self,
        job_type: Option<JobType>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<BackgroundJob>> {
        mongo_ops::find_many(
            &self.collection(),
            cancellation_filter(job_type),
            FindOptions::default(),
            executor,
        )
        .await
    }
}

/// 保留原候选范围和软删除过滤；不在查询中提前改变服务层的跳过计数。
fn cancellation_filter(job_type: Option<JobType>) -> Document {
    let mut filter = doc! {
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "status": { "$in": [JobStatus::Pending.as_str(), JobStatus::Running.as_str(), JobStatus::PartiallySucceeded.as_str()] },
    };
    if let Some(job_type) = job_type {
        filter.insert("job_type", job_type.as_str());
    }
    filter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_keep_active_states_and_soft_delete_boundary() {
        let filter = cancellation_filter(None);
        assert_eq!(filter.get_i64("deleted_at").unwrap(), NOT_DELETED_TIMESTAMP_BSON);
        let states = filter.get_document("status").unwrap().get_array("$in").unwrap();
        assert_eq!(states, &vec!["pending".into(), "running".into(), "partially_succeeded".into()]);
        assert!(!filter.contains_key("job_type"));
        // 已完成的部分成功交给服务层计为跳过，而非从候选清单中消失。
        assert!(!filter.contains_key("finished_at"));
    }

    #[test]
    fn type_filter_preserves_all_other_candidate_constraints() {
        for kind in [
            JobType::Import,
            JobType::Export,
            JobType::Batch,
            JobType::Sync,
            JobType::Backfill,
            JobType::Reconciliation,
        ] {
            let mut filter = cancellation_filter(Some(kind));
            assert_eq!(filter.remove("job_type"), Some(kind.as_str().into()));
            assert_eq!(filter, cancellation_filter(None));
        }
    }
}
