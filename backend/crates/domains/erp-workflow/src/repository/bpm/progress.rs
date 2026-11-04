//! 发起人进展计数与已读确认；只修改提醒字段，不推进审批版本。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result, mongo_ops};

use super::{BpmWorkflowRepository, INSTANCES, i64_version};
use crate::entity::approval_progress::ViewedApprovalProgress;

impl BpmWorkflowRepository<'_> {
    /// 按发起人与未读索引统计审批单数。
    /// # 参数
    /// `actor_id` 为当前账号，`executor` 为调用方执行器。
    /// # 返回
    /// 未软删除且有未读进展的审批单数量。
    /// # 错误
    /// 数据库读取失败时返回错误。
    pub async fn started_unread_count(&self, actor_id: &str, executor: &mut dyn Executor) -> Result<u64> {
        mongo_ops::count_documents(
            &self.db.collection::<Document>(INSTANCES),
            doc! { "started_by": actor_id, "initiator_unread": true, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 原子清除已展示版本以内的进展，保留并发产生的更新。
    /// # 参数
    /// 实例、登录账号、已授权的展示版本及调用方事务执行器。
    /// # 返回
    /// 无；重复确认、旧确认和没有提醒均为成功的空操作。
    /// # 错误
    /// 版本转换或数据库更新失败时返回错误。
    pub async fn mark_progress_read(
        &self,
        instance_id: &str,
        actor_id: &str,
        viewed: ViewedApprovalProgress,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::update_one(
            &self.db.collection::<Document>(INSTANCES),
            read_filter(instance_id, actor_id, viewed)?,
            doc! { "$set": { "initiator_unread": false } },
            false,
            executor,
        )
        .await?;
        Ok(())
    }
}

fn read_filter(instance_id: &str, actor_id: &str, viewed: ViewedApprovalProgress) -> Result<Document> {
    Ok(doc! {
        "id": instance_id,
        "started_by": actor_id,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "initiator_unread": true,
        "initiator_progress_version": { "$lte": i64_version(viewed.version())? },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acknowledgement_is_bounded_by_displayed_version_and_owner() {
        let viewed = ViewedApprovalProgress::new(4, 6, "u", "u").unwrap();
        let filter = read_filter("instance", "u", viewed).unwrap();
        assert_eq!(filter.get_str("id").unwrap(), "instance");
        assert_eq!(filter.get_str("started_by").unwrap(), "u");
        assert!(filter.get_bool("initiator_unread").unwrap());
        assert_eq!(filter.get_document("initiator_progress_version").unwrap().get_i64("$lte").unwrap(), 4);
        assert_eq!(filter.get_i64("deleted_at").unwrap(), NOT_DELETED_TIMESTAMP_BSON);
    }
}
