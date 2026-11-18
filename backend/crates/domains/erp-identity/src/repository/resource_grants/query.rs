//! 数据库内校验授权来源；仅把本次目标的有效 ID 返回应用层。
use std::collections::BTreeSet;

use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, aggregate};
use serde::Deserialize;

use super::{GRANTS, SOURCES};
use crate::Result;
use crate::entity::authorization::ResourceTargets;
use crate::entity::resource_access::{GrantTarget, ResourceKind, SourceKey};

/// 对指定业务主键构造存在性读取授权关联，不丢弃未命中的根记录。
pub(crate) fn grant_lookup(user: &str, resource: ResourceKind, key: Bson) -> Document {
    let mut filter = grant_filter(user, resource);
    filter.insert("$expr", doc! { "$eq": ["$resource_id", "$$identity_key"] });
    let mut pipeline = vec![doc! { "$match": filter }];
    pipeline.extend(valid_source(user, resource, None));
    pipeline.extend([doc! { "$limit": 1 }, doc! { "$project": { "_id": 0, "id": 1 } }]);
    doc! { "from": GRANTS, "let": { "identity_key": key }, "pipeline": pipeline }
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
    let collection = db.collection::<TargetId>(GRANTS);
    let rows = aggregate::rows(&collection, target_pipeline(user, resource, targets), executor).await?;
    Ok(rows.into_iter().map(|row| row.id).collect())
}

/// 在同一事务中核验精确来源、当前来源版本和真实目标；不接纳其他实例授权。
/// # 参数
/// db为身份数据库，source为当前真实审批实例来源，target为真实订单，executor为原快照。
/// # 返回
/// 存在接收人、来源身份、当前来源版本与真实目标均匹配的有效读取行时返回true。
/// # 错误
/// 来源身份非法、数据库读取或反序列化失败时返回错误。
pub(crate) async fn source_target(
    db: &Database,
    source: &SourceKey,
    target: &GrantTarget,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let mut filter = grant_filter(&source.recipient, target.resource);
    filter.insert("source_id", source.id()?);
    filter.insert("resource_id", &target.id);
    let mut pipeline = vec![doc! { "$match": filter }];
    pipeline.extend(valid_source(&source.recipient, target.resource, Some(source)));
    pipeline.extend([doc! { "$limit": 1 }, doc! { "$project": { "_id": 0, "id": "$resource_id" } }]);

    let rows = aggregate::rows(&db.collection::<TargetId>(GRANTS), pipeline, executor).await?;
    Ok(!rows.is_empty())
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
    pipeline.extend(valid_source(user, resource, None));
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
fn valid_source(user: &str, resource: ResourceKind, source: Option<&SourceKey>) -> Vec<Document> {
    let mut filter = doc! {
        "key.recipient": user, "terminated": false, "deleted_at": 0,
        "$expr": { "$and": [
            { "$eq": ["$id", "$$source_id"] },
            { "$eq": ["$version", "$$source_version"] },
            { "$anyElementTrue": [{ "$map": { "input": "$targets", "as": "target", "in": { "$and": [
                { "$eq": ["$$target.resource", { "$literal": resource.code() }] },
                { "$eq": ["$$target.id", "$$resource_id"] }
            ] } } }] }
        ] }
    };
    if let Some(source) = source {
        filter.insert("key.rule", &source.rule);
        filter.insert("key.business_type", &source.business_type);
        filter.insert("key.business_id", &source.business_id);
    }

    vec![
        doc! { "$lookup": {
            "from": SOURCES,
            "let": { "source_id": "$source_id", "source_version": "$source_version", "resource_id": "$resource_id" },
            "pipeline": [
                { "$match": filter },
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
    fn lookup_binds_business_key_and_checks_live_source() {
        let lookup = grant_lookup("u", ResourceKind::SalesOrder, "$order_id".into());
        assert_eq!(lookup.get_document("let").unwrap(), &doc! { "identity_key": "$order_id" });
        let pipeline = lookup.get_array("pipeline").unwrap();
        assert_eq!(pipeline[1], valid_source("u", ResourceKind::SalesOrder, None)[0].clone().into());
        assert_eq!(pipeline[2], doc! { "$match": { "__grant_source.0": { "$exists": true } } }.into());
        assert_eq!(pipeline[3], doc! { "$limit": 1 }.into());
    }

    #[test]
    fn source_join_requires_live_matching_recipient_version_and_target() {
        let stages = valid_source("u", ResourceKind::PurchaseOrder, None);
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
