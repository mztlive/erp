//! 单据关系集合仓储：按单据读取双向关系。

use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Repository, Result};

use crate::entity::document_registry::{BusinessDocumentId, DocumentRelation};

/// 单据关系集合仓储扩展。
#[allow(async_fn_in_trait)]
pub trait DocumentRelationRepositoryExt {
    /// 单次查询与指定单据相关的全部出向及入向关系。
    ///
    /// # 参数
    /// * `document_id` - 业务单据 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `created_at, id` 升序稳定排列的关系；历史自关联脏数据只返回一次。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn list_for_document(
        &self,
        document_id: &BusinessDocumentId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DocumentRelation>>;
}

impl DocumentRelationRepositoryExt for Repository<'_, DocumentRelation> {
    async fn list_for_document(
        &self,
        document_id: &BusinessDocumentId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<DocumentRelation>> {
        self.find_many_sorted(
            document_relation_filter(document_id),
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }
}

/// 构造单据关系双向查询条件。
fn document_relation_filter(document_id: &BusinessDocumentId) -> Document {
    let document_id = document_id.to_string();
    doc! {
        "$or": [
            { "from_document_id": &document_id },
            { "to_document_id": &document_id },
        ]
    }
}
