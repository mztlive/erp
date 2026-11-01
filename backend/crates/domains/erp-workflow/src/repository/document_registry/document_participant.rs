//! 单据参与人集合仓储：按用户读取参与单据。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, mongo_ops};
use serde::Deserialize;

use crate::entity::document_registry::DocumentParticipant;

/// 单据参与记录的单据 ID 窄投影行。
#[derive(Debug, Clone, Deserialize)]
struct ParticipantDocumentIdRow {
    /// 业务单据 ID。
    document_id: String,
}

/// 单据参与人集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait DocumentParticipantRepositoryExt {
    /// 按参与人返回去重后的业务单据 ID，仅投影 `document_id`。
    ///
    /// # 参数
    /// * `user_id` - 参与人用户 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按 ID 升序排列的未删除参与单据 ID。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn document_ids_by_user(&self, user_id: &str, executor: &mut dyn Executor) -> Result<Vec<String>>;

    /// 按参与人查询其参与过的全部单据（“我的参与单据”）。
    ///
    /// # 参数
    /// * `user_id` - 参与人用户 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按参与时间倒序排列的参与记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn list_by_user(
        &self,
        user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DocumentParticipant>>;
}

impl DocumentParticipantRepositoryExt for Repository<'_, DocumentParticipant> {
    async fn document_ids_by_user(&self, user_id: &str, executor: &mut dyn Executor) -> Result<Vec<String>> {
        let collection = self.collection().clone_with_type::<ParticipantDocumentIdRow>();
        let options = FindOptions::builder().projection(doc! { "document_id": 1 }).build();
        let mut ids = mongo_ops::find_many(
            &collection,
            doc! {
                "participant_user_id": user_id,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await?
        .into_iter()
        .map(|row| row.document_id)
        .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        Ok(ids)
    }

    async fn list_by_user(
        &self,
        user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DocumentParticipant>> {
        self.find_many_sorted(doc! { "participant_user_id": user_id }, doc! { "created_at": -1 }, executor)
            .await
    }
}
