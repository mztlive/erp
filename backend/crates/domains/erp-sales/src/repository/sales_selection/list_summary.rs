//! 仅为已授权的当前页补齐陈列计数及方案业务编号。

use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result};
use serde::Deserialize;

use super::{BOOKLETS, ITEMS, PROPOSALS, SalesSelectionDomainRepository};
use crate::repository::filter::push_undeleted;

/// 列表摘要，不读取方案收件信息或商品快照。
#[derive(Debug, Deserialize)]
pub struct BookletSummary {
    pub id: String,
    pub display_count: u32,
    pub proposal_no: Option<String>,
}

impl SalesSelectionDomainRepository<'_> {
    /// 批量读取已授权选品册页的摘要。
    /// # 参数
    /// `ids` 为完成权限过滤的本页册 ID；`executor` 为当前执行器。
    /// # 返回
    /// 每册当前有效、未移除陈列数及已关联方案编号。
    /// # 错误
    /// 聚合读取失败时返回错误。
    pub async fn booklet_summaries(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<BookletSummary>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let collection = self.db.collection::<Document>(BOOKLETS);
        let pipeline = summary_pipeline(ids);
        if let Some(session) = executor.session() {
            Ok(collection
                .aggregate(pipeline)
                .with_type::<BookletSummary>()
                .session(&mut *session)
                .await?
                .stream(session)
                .try_collect()
                .await?)
        } else {
            Ok(collection.aggregate(pipeline).with_type::<BookletSummary>().await?.try_collect().await?)
        }
    }
}

fn summary_pipeline(ids: &[String]) -> Vec<Document> {
    let mut books = doc! { "id": { "$in": ids } };
    let mut items = doc! { "effective": true, "removed": false, "$expr": { "$and": [
        { "$eq": ["$booklet_id", "$$book"] }, { "$eq": ["$batch_id", "$$batch"] }
    ] } };
    let mut proposals = doc! {};
    let mut undeleted = Vec::new();
    push_undeleted(&mut undeleted);
    books.insert("$and", undeleted.clone());
    items.insert("$and", undeleted.clone());
    proposals.insert("$and", undeleted);
    vec![
        doc! { "$match": books },
        doc! { "$lookup": { "from": ITEMS, "let": { "book": "$id", "batch": "$current_batch_id" },
        "pipeline": [{ "$match": items }, { "$count": "count" }], "as": "display" } },
        doc! { "$lookup": { "from": PROPOSALS, "localField": "proposal_id", "foreignField": "id",
        "pipeline": [{ "$match": proposals }, { "$project": { "proposal_no": 1 } }], "as": "proposal" } },
        doc! { "$project": { "_id": 0, "id": 1,
        "display_count": { "$ifNull": [{ "$arrayElemAt": ["$display.count", 0] }, 0] },
        "proposal_no": { "$ifNull": [{ "$arrayElemAt": ["$proposal.proposal_no", 0] }, null] } } },
    ]
}
