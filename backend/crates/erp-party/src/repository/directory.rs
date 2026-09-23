//! 目录只投影稳定编号、当前名称和状态；授权和搜索先于有界读取。
use application_core::directory::{DirectoryItem, DirectoryQuery, DirectoryScope};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result, insert_literal_regex_filter};

use crate::repository::PartyExt;

/// 查询授权目录快照。
/// # 参数
/// `scope` 为本域授权，`query` 为规范化条件，`executor` 为调用方事务。
/// # 返回
/// 最多10001项轻量结果；服务层对超限整体拒绝。
/// # 错误
/// 数据库读取或反序列化失败时传播原始错误。
pub(crate) async fn snapshot(
    db: &Database,
    scope: &DirectoryScope,
    query: &DirectoryQuery,
    executor: &mut dyn Executor,
) -> Result<Vec<DirectoryItem>> {
    let collection = db.collection::<Document>(<Database as PartyExt>::PARTIES);
    let pipeline = pipeline(scope, query);
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

/// 在关联修订前落实对象范围，回显 ID 仅收窄授权集合。
fn pipeline(scope: &DirectoryScope, query: &DirectoryQuery) -> Vec<Document> {
    let mut filter = doc! { "deleted_at": 0 };
    filter.insert("party_kind", "enterprise");
    let mut clauses = Vec::new();
    if let Some(ids) = &scope.ids {
        clauses.push(doc! { "id": { "$in": ids } });
    }
    if let Some(ids) = &query.ids {
        clauses.push(doc! { "id": { "$in": ids.as_slice() } });
    }
    if !clauses.is_empty() {
        filter.insert("$and", clauses);
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
    fn selected_ids_intersect_scope_before_revision_lookup() {
        let scope = DirectoryScope {
            ids: Some(vec!["allowed".into()]),
            scope_version: "v1".into(),
            policy_version: 1,
            organization_version: 1,
            as_of: "now".into(),
            no_scope: false,
        };
        let query = DirectoryQuery {
            ids: Some(serde_json::from_value(serde_json::json!("allowed,outside")).unwrap()),
            ..Default::default()
        };
        let stages = pipeline(&scope, &query);
        let filter = stages[0].get_document("$match").unwrap();
        let clauses = filter.get_array("$and").unwrap();
        assert_eq!(clauses.len(), 2);
        assert_eq!(clauses[0].as_document().unwrap(), &doc! {"id": {"$in": ["allowed"]}});
        assert_eq!(clauses[1].as_document().unwrap(), &doc! {"id": {"$in": ["allowed", "outside"]}});
        assert!(!filter.contains_key("status"));
        assert_eq!(
            stages[1].get_document("$lookup").unwrap().get_str("localField").unwrap(),
            "current_revision_id"
        );
    }
}
