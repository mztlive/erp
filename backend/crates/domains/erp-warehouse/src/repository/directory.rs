//! 目录只投影稳定编号、当前名称和状态；授权和搜索先于有界读取。
use application_core::directory::{DirectoryItem, DirectoryQuery};
use erp_identity::{CompiledAccess, Result as IdentityResult};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::error::Error as MongoError;
use mongodb::options::FindOptions;
use persistence_core::{
    Error as RepositoryError, Executor, PageResult, Pagination, QueryFilter, aggregate,
    insert_literal_regex_filter,
};

use crate::authorization::{WarehouseQuery, ensure_warehouse_query};
use crate::dto::warehouse::WAREHOUSE_SORT_FIELDS;
use crate::repository::warehouse::sort_doc;
use crate::repository::warehouse_projection::warehouse_projection;
use crate::repository::{WarehouseExt, WarehouseFilter, WarehouseRow};
use crate::{Error, Result};

// 用原事务及完整仓库授权查询轻量目录，最多取 `10001` 项，以便服务识别并拒绝超限；数据库读取或反序列化失败保留原错误。
pub(crate) async fn snapshot(
    db: &Database,
    scope: &WarehouseQuery,
    query: &DirectoryQuery,
    executor: &mut dyn Executor,
) -> Result<Vec<DirectoryItem>> {
    scope.ensure_current(executor)?;
    let collection = db.collection::<DirectoryItem>(<Database as WarehouseExt>::WAREHOUSES);
    let items = aggregate::rows(&collection, pipeline(scope.stages(), query), executor).await?;
    scope.ensure_current(executor)?;
    Ok(items)
}

// 在关联当前修订前执行完整对象范围；已选 ID 只收窄授权集合，不能用作额外授权。
fn pipeline(authorization: &[Document], query: &DirectoryQuery) -> Vec<Document> {
    let mut filter = doc! { "deleted_at": 0 };
    if let Some(ids) = &query.ids {
        filter.insert("$and", vec![doc! { "id": { "$in": ids.as_slice() } }]);
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

// 将同一套仓库筛选和全部授权阶段用于分页与计数，避免结果与范围口径不一致；凭证失效、数量越界或读取失败时拒绝。
pub(crate) async fn warehouses(
    db: &Database,
    scope: &WarehouseQuery,
    filter: &WarehouseFilter,
    executor: &mut dyn Executor,
) -> Result<PageResult<WarehouseRow>> {
    scope.ensure_current(executor)?;
    let collection = db.collection::<WarehouseRow>(<Database as WarehouseExt>::WAREHOUSES);
    let prefix = warehouse_prefix(scope.stages(), filter);
    let items =
        aggregate::find(&collection, doc! {}, warehouse_options(filter), prefix.clone(), executor).await?;
    let total = aggregate::count(&collection, doc! {}, prefix, executor).await?;

    scope.ensure_current(executor)?;
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
    FindOptions::builder()
        .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending, WAREHOUSE_SORT_FIELDS))
        .skip(filter.skip())
        .limit(filter.limit())
        .projection(warehouse_projection())
        .build()
}

// 在验证授权模型和读取边界后执行完整阶段，最多查一行身份判断是否命中真仓库；模型或边界错配返回校验错误，读取或解码失败直接返回。
pub(crate) async fn authorized_rows_exist(
    db: &Database,
    access: &CompiledAccess,
    executor: &mut dyn Executor,
) -> IdentityResult<bool> {
    ensure_warehouse_query(access, executor)?;
    let mut stages = access.stages().to_vec();
    stages.extend([doc! { "$project": { "_id": 0, "id": 1 } }, doc! { "$limit": 1 }]);
    let collection = db.collection::<Document>(<Database as WarehouseExt>::WAREHOUSES);
    let rows = aggregate::rows(&collection, stages, executor).await?;
    ensure_warehouse_query(access, executor)?;
    Ok(!rows.is_empty())
}

// 在原事务按候选 ID 收窄完整授权查询，只返回命中的真实 ID，不截断。模型或边界错配返回校验错误，数据库或解码失败透传。
pub(crate) async fn authorized_ids_in(
    db: &Database,
    access: &CompiledAccess,
    ids: &[String],
    executor: &mut dyn Executor,
) -> IdentityResult<Vec<String>> {
    ensure_warehouse_query(access, executor)?;
    let collection = db.collection::<Document>(<Database as WarehouseExt>::WAREHOUSES);
    let mut pipeline = vec![doc! {"$match": {"id": {"$in": ids}}}];
    pipeline.extend_from_slice(access.stages());
    pipeline.push(doc! {"$project":{"_id":0,"id":1}});
    let rows = aggregate::rows(&collection, pipeline, executor).await?;

    ensure_warehouse_query(access, executor)?;
    rows.into_iter()
        .map(|row| {
            row.get_str("id")
                .map(str::to_owned)
                .map_err(|error| RepositoryError::from(MongoError::custom(error.to_string())).into())
        })
        .collect()
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
