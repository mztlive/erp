//! 目录只投影稳定编号、当前名称和状态；授权和搜索先于有界读取。
use application_core::directory::{DirectoryItem, DirectoryQuery};
use erp_identity::CompiledAccess;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Result as RepositoryResult, aggregate,
    insert_literal_regex_filter,
};

use crate::authorization::WarehouseQuery;
use crate::dto::warehouse::WAREHOUSE_SORT_FIELDS;
use crate::repository::warehouse_projection::warehouse_projection;
use crate::repository::{WarehouseExt, WarehouseFilter, WarehouseRow};
use crate::{Error, Result};

/// 查询授权目录快照。
///
/// # 参数
/// * `db` - 本域数据库。
/// * `scope` - 本域授权范围。
/// * `query` - 已规范化的目录条件。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 最多10001项轻量结果；服务层对超限整体拒绝。
///
/// # 错误
/// 数据库读取或反序列化失败时传播原始错误。
pub(crate) async fn snapshot(
    db: &Database,
    scope: &WarehouseQuery,
    query: &DirectoryQuery,
    executor: &mut dyn Executor,
) -> Result<Vec<DirectoryItem>> {
    scope.validate(executor).await?;
    let collection = db.collection::<DirectoryItem>(<Database as WarehouseExt>::WAREHOUSES);
    let items = aggregate::rows(&collection, pipeline(scope.stages(), query), executor).await?;
    scope.validate(executor).await?;
    Ok(items)
}

/// 在关联修订前落实对象范围，回显 ID 仅收窄授权集合。
fn pipeline(authorization: &[Document], query: &DirectoryQuery) -> Vec<Document> {
    let mut filter = doc! { "deleted_at": 0 };

    let mut clauses = Vec::new();
    if let Some(ids) = &query.ids {
        clauses.push(doc! { "id": { "$in": ids.as_slice() } });
    }
    if !clauses.is_empty() {
        filter.insert("$and", clauses);
    }
    let mut pipeline = vec![doc! { "$match": filter }];
    pipeline.extend_from_slice(authorization);
    pipeline.extend([
        doc! { "$lookup": {
            "from": <Database as WarehouseExt>::WAREHOUSE_REVISIONS, "localField": "current_revision_id", "foreignField": "id",
            "pipeline": [{ "$project": { "_id": 0, "name": 1 } }], "as": "directory_revision"
        } },
        doc! { "$project": { "_id": 0, "id": 1, "code": "$warehouse_code", "status": 1,
            "name": { "$ifNull": [{ "$arrayElemAt": ["$directory_revision.name", 0] }, "$warehouse_code"] }
        } },
    ]);
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

/// 分页、计数使用相同业务筛选与完整仓库授权。
/// # 参数
/// db、filter 为仓库及规范化业务筛选，scope 为完整凭证，executor 为原事务。
/// # 返回
/// 当前授权交集内的列表页与总数。
/// # 错误
/// 授权失效、数量越界或数据库读取失败时拒绝。
pub(crate) async fn warehouses(
    db: &Database,
    scope: &WarehouseQuery,
    filter: &WarehouseFilter,
    executor: &mut dyn Executor,
) -> Result<PageResult<WarehouseRow>> {
    scope.validate(executor).await?;
    let collection = db.collection::<WarehouseRow>(<Database as WarehouseExt>::WAREHOUSES);
    let prefix = warehouse_prefix(scope.stages(), filter);
    let items =
        aggregate::find(&collection, doc! {}, warehouse_options(filter), prefix.clone(), executor).await?;
    let total = aggregate::count(&collection, doc! {}, prefix, executor).await?;
    scope.validate(executor).await?;
    let total = i64::try_from(total).map_err(|_| Error::ValidationError("仓库总数超过上限".into()))?;
    Ok(PageResult { items, total })
}

fn warehouse_prefix(authorization: &[Document], filter: &WarehouseFilter) -> Vec<Document> {
    let mut prefix = vec![doc! { "$match": filter.to_doc() }];
    prefix.extend_from_slice(authorization);
    if let Some(q) = filter.q.as_deref() {
        prefix.push(doc! { "$lookup": {
            "from": <Database as WarehouseExt>::WAREHOUSE_REVISIONS,
            "localField": "current_revision_id", "foreignField": "id",
            "pipeline": [{ "$match": { "deleted_at": 0 } }, { "$project": { "_id": 0, "name": 1 } }],
            "as": "directory_revision"
        } });
        let mut code = Document::new();
        let mut name = Document::new();
        insert_literal_regex_filter(&mut code, "warehouse_code", Some(q));
        insert_literal_regex_filter(&mut name, "directory_revision.name", Some(q));
        prefix.push(doc! { "$match": { "$or": [code, name] } });
        prefix.push(doc! { "$unset": "directory_revision" });
    }
    prefix
}

fn warehouse_options(filter: &WarehouseFilter) -> FindOptions {
    let direction = if filter.sort_ascending { 1 } else { -1 };
    let field = filter
        .sort_by
        .as_deref()
        .filter(|field| WAREHOUSE_SORT_FIELDS.contains(field))
        .unwrap_or("created_at");
    FindOptions::builder()
        .sort(doc! { field: direction, "id": direction })
        .skip(filter.skip())
        .limit(filter.limit())
        .projection(warehouse_projection())
        .build()
}

/// 用完整授权查询判断是否存在仓库，只读取一行身份。
/// # 参数
/// db 为仓库数据库，access 为完整凭证，executor 为原事务。
/// # 返回
/// 完整授权查询存在实际仓库时为真。
/// # 错误
/// 数据库读取或解码失败时返回原始错误。
pub(crate) async fn authorized_rows_exist(
    db: &Database,
    access: &CompiledAccess,
    executor: &mut dyn Executor,
) -> RepositoryResult<bool> {
    let mut stages = access.stages().to_vec();
    stages.extend([doc! { "$project": { "_id": 0, "id": 1 } }, doc! { "$limit": 1 }]);
    let collection = db.collection::<Document>(<Database as WarehouseExt>::WAREHOUSES);
    Ok(!aggregate::rows(&collection, stages, executor).await?.is_empty())
}

/// 执行统一授权阶段后返回真实仓库身份。
/// # 参数
/// db 为仓库数据库，access 为完整查询凭证，ids 为有界候选，executor 为原事务。
/// # 返回
/// 当前候选内的已授权仓库 ID，不截断。
/// # 错误
/// 数据库或解码失败透传。
pub(crate) async fn authorized_ids_in(
    db: &Database,
    access: &CompiledAccess,
    ids: &[String],
    executor: &mut dyn Executor,
) -> RepositoryResult<Vec<String>> {
    let collection = db.collection::<Document>(<Database as WarehouseExt>::WAREHOUSES);
    let mut pipeline = vec![doc! {"$match": {"id": {"$in": ids}}}];
    pipeline.extend_from_slice(access.stages());
    pipeline.push(doc! {"$project":{"_id":0,"id":1}});
    let mut ids = Vec::new();
    if let Some(session) = executor.session() {
        let mut cursor = collection.aggregate(pipeline).session(&mut *session).await?;
        while cursor.advance(session).await? {
            let row = cursor.deserialize_current()?;
            ids.push(
                row.get_str("id")
                    .map_err(|error| mongodb::error::Error::custom(error.to_string()))?
                    .to_owned(),
            );
        }
    } else {
        let mut cursor = collection.aggregate(pipeline).await?;
        while cursor.advance().await? {
            let row = cursor.deserialize_current()?;
            ids.push(
                row.get_str("id")
                    .map_err(|error| mongodb::error::Error::custom(error.to_string()))?
                    .to_owned(),
            );
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use mongodb::bson::Bson;

    use super::*;

    fn authorization() -> Vec<Document> {
        vec![
            doc! { "$lookup": { "from": "warehouse_grants", "as": "grant", "pipeline": [] } },
            doc! { "$match": { "grant.enabled": true } },
            doc! { "$unset": "grant" },
        ]
    }

    #[test]
    fn selected_ids_intersect_full_authorization_before_revision_lookup() {
        let authorization = authorization();
        let query = DirectoryQuery {
            ids: Some(serde_json::from_value(serde_json::json!("allowed,outside")).unwrap()),
            ..Default::default()
        };
        let stages = pipeline(&authorization, &query);
        let filter = stages[0].get_document("$match").unwrap();
        assert_eq!(
            filter.get_array("$and").unwrap(),
            &vec![Bson::Document(doc! { "id": { "$in": ["allowed", "outside"] } })]
        );
        assert_eq!(&stages[1..4], authorization.as_slice());
        assert_eq!(
            stages[4].get_document("$lookup").unwrap().get_str("localField").unwrap(),
            "current_revision_id"
        );
        assert!(!filter.contains_key("status"));
        assert_eq!(stages.last().unwrap(), &doc! { "$limit": 10001 });
    }

    #[test]
    fn directory_search_and_full_authorization_precede_result_limit() {
        let authorization = authorization();
        let query = DirectoryQuery { q: Some("A.*".into()), ..Default::default() };
        let stages = pipeline(&authorization, &query);
        assert_eq!(&stages[1..4], authorization.as_slice());
        let keyword = stages[6].get_document("$match").unwrap().get_array("$or").unwrap();
        assert_eq!(keyword.len(), 2);
        assert_eq!(
            keyword[0].as_document().unwrap().get_document("name").unwrap().get_str("$regex").unwrap(),
            r"A\.\*"
        );
        assert_eq!(stages[7], doc! { "$sort": { "id": 1 } });
        assert_eq!(stages[8], doc! { "$limit": 10001 });
        assert!(stages[..8].iter().all(|stage| !stage.contains_key("$limit")));
    }

    #[test]
    fn business_list_reuses_full_scope_with_keyword_and_stable_paging() {
        let filter = WarehouseFilter {
            warehouse_id: Some("warehouse-one".into()),
            q: Some("keyword".into()),
            page: 2,
            page_size: 10,
            sort_by: Some("warehouse_code".into()),
            sort_ascending: true,
            ..Default::default()
        };
        let authorization = authorization();
        let prefix = warehouse_prefix(&authorization, &filter);
        assert_eq!(prefix[0].get_document("$match").unwrap().get_str("id").unwrap(), "warehouse-one");
        assert_eq!(&prefix[1..4], authorization.as_slice());
        let lookup = prefix[4].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_str("localField").unwrap(), "current_revision_id");
        assert!(prefix[5].get_document("$match").unwrap().contains_key("$or"));
        assert!(prefix.iter().all(|stage| !stage.contains_key("$limit") && !stage.contains_key("$skip")));
        let options = warehouse_options(&filter);
        assert_eq!(options.sort, Some(doc! { "warehouse_code": 1, "id": 1 }));
        assert_eq!(options.skip, Some(10));
        assert_eq!(options.limit, Some(10));
        assert_eq!(options.projection, Some(warehouse_projection()));
        let unknown =
            warehouse_options(&WarehouseFilter { sort_by: Some("unknown".into()), ..Default::default() });
        assert_eq!(unknown.sort, Some(doc! { "created_at": -1, "id": -1 }));
    }

    #[test]
    fn empty_authorization_still_applies_to_selected_and_keyword_results() {
        let denied = [doc! { "$match": { "$expr": false } }];
        let selected = DirectoryQuery {
            ids: Some(serde_json::from_value(serde_json::json!("outside")).unwrap()),
            ..Default::default()
        };
        assert_eq!(pipeline(&denied, &selected)[1], denied[0]);
        let filter = WarehouseFilter { q: Some("outside".into()), ..Default::default() };
        assert_eq!(warehouse_prefix(&denied, &filter)[1], denied[0]);
    }
}
