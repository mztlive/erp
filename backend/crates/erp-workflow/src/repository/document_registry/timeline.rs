//! 单据历史的有界读取，权限由调用方先行验证。
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::document_registry::WorkflowAction;

/// 跨单据历史查询。
#[allow(async_fn_in_trait)]
pub trait WorkflowTimelineRepositoryExt {
    /// 读取已授权单据的下一批动作。
    ///
    /// # 参数
    /// `ids` 为已授权单据；`before` 为时间及原始动作 ID；`limit` 为批次上限。
    /// `executor` 为调用方的同一读取事务。
    /// # 返回
    /// 按时间、ID 倒序排列的历史动作。
    /// # 错误
    /// 数据库读取失败时返回错误。
    async fn timeline_actions(
        &self,
        ids: &[String],
        before: Option<(u64, &str)>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkflowAction>>;
}

impl WorkflowTimelineRepositoryExt for Repository<'_, WorkflowAction> {
    async fn timeline_actions(
        &self,
        ids: &[String],
        before: Option<(u64, &str)>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkflowAction>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut filter = doc! { "deleted_at": 0_i64, "document_id": { "$in": ids } };
        if let Some((time, id)) = before {
            let time = i64::try_from(time).unwrap_or(i64::MAX);
            filter.insert(
                "$or",
                vec![
                    doc! { "created_at": { "$lt": time } },
                    doc! { "created_at": time, "id": { "$lt": id } },
                ],
            );
        }
        let options =
            FindOptions::builder().sort(doc! { "created_at": -1, "id": -1 }).limit(i64::from(limit)).build();
        mongo_ops::find_many(&self.collection(), filter, options, executor).await
    }
}
