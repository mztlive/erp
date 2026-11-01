//! 目录只投影稳定编号、当前名称和状态；公司资格和搜索先于有界读取。
use application_core::directory::{DirectoryItem, DirectoryQuery};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result, insert_literal_regex_filter};

use crate::repository::PartyExt;

/// 查询我方公司目录快照。
/// # 参数
/// `query` 为规范化条件，`executor` 为调用方执行器。
/// # 返回
/// 最多10001项轻量结果；服务层对超限整体拒绝。
/// # 错误
/// 数据库读取或反序列化失败时传播原始错误。
pub(crate) async fn snapshot(
    db: &Database,
    query: &DirectoryQuery,
    executor: &mut dyn Executor,
) -> Result<Vec<DirectoryItem>> {
    let collection = db.collection::<Document>(<Database as PartyExt>::PARTIES);
    let pipeline = pipeline(query);
    let mut items = Vec::new();
    if let Some(session) = executor.session() {
        let mut cursor =
            collection.aggregate(pipeline).with_type::<DirectoryItem>().session(&mut *session).await?;
        while cursor.advance(session).await? {
            items.push(cursor.deserialize_current()?);
        }
    } else {
        let mut cursor = collection.aggregate(pipeline).with_type::<DirectoryItem>().await?;
        while cursor.advance().await? {
            items.push(cursor.deserialize_current()?);
        }
    }
    Ok(items)
}

/// 在关联修订前筛选我方企业及回显 ID；停用主体保留供历史回显。
fn pipeline(query: &DirectoryQuery) -> Vec<Document> {
    let mut filter = doc! { "deleted_at": 0 };
    filter.insert("party_kind", "enterprise");
    filter.insert("company_profile", doc! { "$exists": true, "$ne": null });
    if let Some(ids) = &query.ids {
        filter.insert("id", doc! { "$in": ids.as_slice() });
    }
    let mut pipeline = vec![
        doc! { "$match": filter },
        doc! { "$lookup": {
            "from": <Database as PartyExt>::PARTY_REVISIONS, "localField": "current_revision_id", "foreignField": "id",
            "pipeline": [{ "$project": { "_id": 0, "legal_name": 1 } }], "as": "directory_revision"
        } },
        doc! { "$project": { "_id": 0, "id": 1, "code": "$party_no", "status": 1,
            "name": { "$ifNull": [{ "$arrayElemAt": ["$directory_revision.legal_name", 0] }, "$party_no"] }
        } },
    ];
    if let Some(q) = &query.q {
        let mut name = Document::new();
        let mut code = Document::new();
        insert_literal_regex_filter(&mut name, "name", Some(q));
        insert_literal_regex_filter(&mut code, "code", Some(q));
        pipeline.push(doc! { "$match": { "$or": [name, code] } });
    }
    pipeline.extend([doc! { "$sort": { "id": 1 } }, doc! { "$limit": 10001 }]);
    pipeline
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn list_includes_all_companies_and_selected_only_filters_requested_ids() {
        let list = pipeline(&DirectoryQuery::default());
        let filter = list[0].get_document("$match").unwrap();
        assert_eq!(
            filter,
            &doc! {
                "deleted_at": 0, "party_kind": "enterprise",
                "company_profile": { "$exists": true, "$ne": null }
            }
        );
        let query = DirectoryQuery {
            ids: Some(serde_json::from_value(serde_json::json!("company-a,company-b")).unwrap()),
            ..Default::default()
        };
        let selected = pipeline(&query);
        let selected_filter = selected[0].get_document("$match").unwrap();
        assert_eq!(selected_filter.get_document("id").unwrap(), &doc! {"$in": ["company-a", "company-b"]});
        assert!(!selected_filter.contains_key("status"));
        assert_eq!(
            selected[1].get_document("$lookup").unwrap().get_str("localField").unwrap(),
            "current_revision_id"
        );
    }
}
