//! 数据库内校验授权来源；仅把本次目标的有效 ID 返回应用层。
use std::collections::BTreeSet;

use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Error as PersistenceError, Executor};
use serde::Deserialize;

use super::{GRANTS, SOURCES};
use crate::Result;
use crate::entity::resource_access::ResourceKind;
use crate::entity::resource_access::query::{AuthorizedResourceQuery, ResourceTargets};

/// 编译列表读取的授权阶段，原范围与有效指定记录取并集。
/// # 参数
/// query 为当前事务解析的授权上下文；base_filter 为业务仓储编译的原范围条件。
/// # 返回
/// 必须放在业务筛选之后、分页、计数、汇总之前的聚合阶段；输入记录的 id 为稳定业务主键。
/// # 错误
/// 不执行 I/O；不得对输入预分页，不得把目标检查结果代入完整列表查询。
pub fn read_stages(query: &AuthorizedResourceQuery, base_filter: Document) -> Vec<Document> {
    let mut pipeline = vec![doc! { "$match": grant_filter(&query.user_id, query.resource) }];
    pipeline.extend(valid_source(&query.user_id, query.resource));
    pipeline.extend([doc! { "$limit": 1 }, doc! { "$project": { "_id": 0, "id": 1 } }]);
    vec![
        doc! { "$lookup": { "from": GRANTS, "localField": "id", "foreignField": "resource_id",
        "pipeline": pipeline, "as": "__resource_grant" } },
        doc! { "$match": { "$or": [base_filter, { "__resource_grant.0": { "$exists": true } }] } },
        doc! { "$unset": "__resource_grant" },
    ]
}

/// 只读取本次目标的有效授权；数据库关联来源并按目标去重。
/// # 参数
/// user、resource 为可信入口身份；targets 为已校验目标；executor 为调用事务。
/// # 返回
/// 至多与目标数相等的有效 ID 集合，空目标不访问数据库。
/// # 错误
/// 数据库读取或反序列化失败时拒绝。
pub(crate) async fn target_ids(
    db: &Database,
    user: &str,
    resource: ResourceKind,
    targets: &ResourceTargets,
    executor: &mut dyn Executor,
) -> Result<BTreeSet<String>> {
    if targets.ids().is_empty() {
        return Ok(BTreeSet::new());
    }
    let collection = db.collection::<Document>(GRANTS);
    let aggregate = collection.aggregate(target_pipeline(user, resource, targets)).with_type::<TargetId>();
    let rows: Vec<TargetId> = match executor.session() {
        Some(session) => aggregate
            .session(&mut *session)
            .await
            .map_err(PersistenceError::from)?
            .stream(session)
            .try_collect()
            .await
            .map_err(PersistenceError::from)?,
        None => aggregate
            .await
            .map_err(PersistenceError::from)?
            .try_collect()
            .await
            .map_err(PersistenceError::from)?,
    };
    Ok(rows.into_iter().map(|row| row.id).collect())
}

#[derive(Deserialize)]
struct TargetId {
    id: String,
}

/// 精确命中账号、资源及本次 ID，关联校验后在数据库去重。
fn target_pipeline(user: &str, resource: ResourceKind, targets: &ResourceTargets) -> Vec<Document> {
    let mut filter = grant_filter(user, resource);
    filter.insert("resource_id", doc! { "$in": targets.ids().iter().cloned().collect::<Vec<_>>() });
    let mut pipeline = vec![doc! { "$match": filter }];
    pipeline.extend(valid_source(user, resource));
    pipeline.extend([
        doc! { "$group": { "_id": "$resource_id" } },
        doc! { "$project": { "_id": 0, "id": "$_id" } },
    ]);
    pipeline
}

/// 精确限定读取授权，不接纳其他账号、资源、动作及失效行。
fn grant_filter(user: &str, resource: ResourceKind) -> Document {
    doc! { "recipient": user, "resource": resource.code(), "action": "read", "active": true, "deleted_at": 0 }
}

/// 列表与目标查询共用来源校验；存在其他有效来源时仍可读取。
fn valid_source(user: &str, resource: ResourceKind) -> Vec<Document> {
    vec![
        doc! { "$lookup": {
            "from": SOURCES,
            "let": { "source_id": "$source_id", "source_version": "$source_version", "resource_id": "$resource_id" },
            "pipeline": [
                { "$match": {
                    "key.recipient": user, "terminated": false, "deleted_at": 0,
                    "$expr": { "$and": [
                        { "$eq": ["$id", "$$source_id"] },
                        { "$eq": ["$version", "$$source_version"] },
                        { "$anyElementTrue": [{ "$map": { "input": "$targets", "as": "target", "in": { "$and": [
                            { "$eq": ["$$target.resource", { "$literal": resource.code() }] },
                            { "$eq": ["$$target.id", "$$resource_id"] }
                        ] } } }] }
                    ] }
                } },
                { "$project": { "_id": 0, "id": 1 } },
                { "$limit": 1 }
            ],
            "as": "__grant_source"
        } },
        doc! { "$match": { "__grant_source.0": { "$exists": true } } },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::access_control::ResolvedScope;

    #[test]
    fn targets_are_restricted_before_join_and_grouped_after_source_validation() {
        let targets = ResourceTargets::new(&["s1", "s2", "s1"]).unwrap();
        let stages = target_pipeline("u", ResourceKind::SalesOrder, &targets);
        assert_eq!(
            stages[0],
            doc! { "$match": { "recipient": "u", "resource": "sales_order", "action": "read",
            "active": true, "deleted_at": 0, "resource_id": { "$in": ["s1", "s2"] } } }
        );
        assert_eq!(stages[2], doc! { "$match": { "__grant_source.0": { "$exists": true } } });
        assert_eq!(stages[3], doc! { "$group": { "_id": "$resource_id" } });
        let empty = target_pipeline("u", ResourceKind::SalesOrder, &ResourceTargets::new(&[]).unwrap());
        assert_eq!(
            empty[0].get_document("$match").unwrap().get_document("resource_id").unwrap(),
            &doc! { "$in": [] }
        );
    }

    #[test]
    fn list_uses_target_lookup_and_unions_scope_before_pagination() {
        let query = AuthorizedResourceQuery {
            user_id: "u".into(),
            resource: ResourceKind::SalesOrder,
            scope: ResolvedScope { role_clauses: vec![], user_limit: None },
            scope_version: "v".into(),
            grant_revision: 0,
        };
        let base = doc! { "owner_id": "u" };
        let stages = read_stages(&query, base.clone());
        let lookup = stages[0].get_document("$lookup").unwrap();
        assert_eq!(lookup.get_str("localField").unwrap(), "id");
        assert_eq!(lookup.get_str("foreignField").unwrap(), "resource_id");
        let pipeline = lookup.get_array("pipeline").unwrap();
        assert_eq!(pipeline[1], valid_source("u", ResourceKind::SalesOrder)[0].clone().into());
        assert_eq!(pipeline[2], doc! { "$match": { "__grant_source.0": { "$exists": true } } }.into());
        assert_eq!(pipeline[3], doc! { "$limit": 1 }.into());
        assert_eq!(
            stages[1],
            doc! { "$match": { "$or": [base, { "__resource_grant.0": { "$exists": true } }] } }
        );
        assert_eq!(stages[2], doc! { "$unset": "__resource_grant" });
    }

    #[test]
    fn source_join_requires_live_matching_recipient_version_and_target() {
        let stages = valid_source("u", ResourceKind::PurchaseOrder);
        let lookup = stages[0].get_document("$lookup").unwrap();
        let pipeline = lookup.get_array("pipeline").unwrap();
        let filter = pipeline[0].as_document().unwrap().get_document("$match").unwrap();
        assert_eq!(filter.get_str("key.recipient").unwrap(), "u");
        assert!(!filter.get_bool("terminated").unwrap());
        assert_eq!(filter.get_i32("deleted_at").unwrap(), 0);
        let predicates = filter.get_document("$expr").unwrap().get_array("$and").unwrap();
        assert_eq!(predicates[0], doc! { "$eq": ["$id", "$$source_id"] }.into());
        assert_eq!(predicates[1], doc! { "$eq": ["$version", "$$source_version"] }.into());
        assert_eq!(
            predicates[2],
            doc! { "$anyElementTrue": [{ "$map": { "input": "$targets", "as": "target", "in": { "$and": [
                { "$eq": ["$$target.resource", { "$literal": "purchase_order" }] },
                { "$eq": ["$$target.id", "$$resource_id"] }
            ] } } }] }
            .into()
        );
    }
}
