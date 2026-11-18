//! 只访问身份领域自有集合，所有读写复用调用方执行器。
mod query;
pub mod queue;
mod revision;
mod rows;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, mongo_ops, persisted_document};
pub(crate) use query::{grant_lookup, source_target, target_ids};
pub(crate) use revision::{advance, revision};
pub(crate) use rows::sync_rows;
use serde::de::DeserializeOwned;

use crate::dto::GrantSourcePage;
use crate::entity::resource_access::event::GrantEvent;
use crate::entity::resource_access::{GrantSource, ResourceGrant, ResourceKind};
use crate::{Error, Result};
pub(crate) const GRANTS: &str = "resource_grants";
pub(crate) const SOURCES: &str = "resource_grant_sources";
pub(crate) const EVENTS: &str = "resource_grant_events";
// 取得来源集合仓储。
// 构造仓储不执行 I/O；异步操作传播持久化、规模上限及事务约束错误。
//
// 数据库及函数声明中的身份或查询事实；异步访问复用调用方执行器。
pub(crate) fn sources(db: &Database) -> Repository<'_, GrantSource> {
    Repository::new(db, SOURCES)
}
// 取得指定记录授权集合仓储。
// 构造仓储不执行 I/O；异步操作传播持久化、规模上限及事务约束错误。
//
// 数据库及函数声明中的身份或查询事实；异步访问复用调用方执行器。
pub(crate) fn grants(db: &Database) -> Repository<'_, ResourceGrant> {
    Repository::new(db, GRANTS)
}
// 取得正式分配事件集合仓储。
// 构造仓储不执行 I/O；异步操作传播持久化、规模上限及事务约束错误。
//
// 数据库及函数声明中的身份或查询事实；异步访问复用调用方执行器。
pub(crate) fn events(db: &Database) -> Repository<'_, GrantEvent> {
    Repository::new(db, EVENTS)
}
// 按业务幂等键读取正式事件，不依赖候选事件的新记录 ID。
// 已持久化事件，缺失返回空值。
//
// 数据库读取失败时返回错误。
//
// db 为身份数据库；dedup_key 为业务幂等键；executor 为调用方事务。
pub(crate) async fn event_by_key(
    db: &Database,
    dedup_key: &str,
    executor: &mut dyn Executor,
) -> Result<Option<GrantEvent>> {
    Ok(events(db).find_one(doc! { "dedup_key": dedup_key }, executor).await?)
}

// 管理列表按稳定主键分页，保留终止来源供审计。
// 最多 100 个来源及下一页游标。
//
// 数据库读取失败时拒绝。
//
// user 为接收人；after 为上一页游标；executor 为调用执行器。
pub(crate) async fn for_recipient(
    db: &Database,
    user: &str,
    after: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<GrantSourcePage> {
    let mut filter = doc! { "key.recipient": user, "deleted_at": 0 };
    if let Some(after) = after {
        filter.insert("id", doc! { "$gt": after });
    }

    let mut items: Vec<GrantSource> = page(db, SOURCES, filter, 101, executor).await?;
    let next_cursor = if items.len() > 100 {
        items.pop();
        items.last().map(|item| item.base.id.clone())
    } else {
        None
    };
    Ok(GrantSourcePage { items, next_cursor })
}

// 读取单个来源的一页授权行；更新方通过主键游标继续，不装载历史全集。
// 最多 200 行，空页表示结束。
//
// 数据库读取失败时拒绝。
//
// source_id 为唯一来源；after 为已处理的最后一个主键；executor 为发布事务。
pub(crate) async fn source_rows(
    db: &Database,
    source_id: &str,
    after: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Vec<ResourceGrant>> {
    let mut filter = doc! { "source_id": source_id, "deleted_at": 0 };
    if let Some(after) = after {
        filter.insert("id", doc! { "$gt": after });
    }
    page(db, GRANTS, filter, 200, executor).await
}

// 管理记录授权行时按调用方提供的索引限定条件读取一页，分页结果不能当作完整授权集合。
async fn page<T: DeserializeOwned + Unpin + Send + Sync>(
    db: &Database,
    collection: &str,
    filter: Document,
    limit: i64,
    executor: &mut dyn Executor,
) -> Result<Vec<T>> {
    let collection = db.collection::<T>(collection);
    let options = FindOptions::builder().sort(doc! { "id": 1 }).limit(limit).build();
    Ok(mongo_ops::find_many(&collection, filter, options, executor).await?)
}
// 授权行身份使用结构化编码，避免任意分隔符碰撞。
// 构造仓储不执行 I/O；异步操作传播持久化、规模上限及事务约束错误。
//
// 数据库及函数声明中的身份或查询事实；异步访问复用调用方执行器。
pub(crate) fn row_id(source: &str, resource: ResourceKind, id: &str) -> Result<String> {
    Ok(serde_json::to_string(&(source, resource, id))?)
}
// 在授权事务中追加不可变来源审计快照。
// 构造仓储不执行 I/O；异步操作传播持久化、规模上限及事务约束错误。
//
// 数据库及函数声明中的身份或查询事实；异步访问复用调用方执行器。
pub(crate) async fn audit_source(
    db: &Database,
    source: &GrantSource,
    executor: &mut dyn Executor,
) -> Result<()> {
    let event = doc! { "id": format!("{}:{}", source.base.id, source.base.version), "source_id": &source.base.id,
    "snapshot": persisted_document(source)? };
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::Internal("来源审计必须使用快照事务".into()));
    }
    mongo_ops::insert_one(&db.collection("resource_grant_audits"), &event, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use mongodb::bson::deserialize_from_document;

    use super::*;
    use crate::entity::resource_access::event::{GrantEventEvidence, GrantEventPayload};

    #[test]
    fn typed_event_bson_roundtrip_keeps_source_index_fields_and_payload() {
        let payloads = [
            GrantEventPayload::InvoiceApproved {
                invoice_request_id: "request".into(),
                approval_instance_id: "approval".into(),
                work_item_id: "task".into(),
            },
            GrantEventPayload::SalesOrderApprovalAssigned {
                sales_order_id: "sales".into(),
                approval_instance_id: "approval".into(),
                node_id: "node".into(),
                assignment_id: "assignment".into(),
            },
            GrantEventPayload::PurchaseOrderApprovalAssigned {
                purchase_order_id: "purchase".into(),
                approval_instance_id: "approval".into(),
                node_id: "node".into(),
                assignment_id: "assignment".into(),
            },
        ];
        for payload in payloads {
            let event = GrantEvent::new(GrantEventEvidence {
                payload,
                recipient: "operator".into(),
                fact_version: 1,
                occurred_at: 100,
            })
            .unwrap();
            let document = persisted_document(&event).unwrap();
            assert_eq!(document.get_str("status").unwrap(), "pending");
            assert_eq!(document.get_str("id").unwrap(), event.base.id);
            assert_eq!(document.get_str("dedup_key").unwrap(), event.dedup_key);
            assert_eq!(
                document.get_document("source").unwrap().get_str("business_type").unwrap(),
                event.source.business_type
            );
            assert_eq!(
                document
                    .get_document("evidence")
                    .unwrap()
                    .get_document("payload")
                    .unwrap()
                    .get_str("type")
                    .unwrap(),
                event.source.rule
            );

            let restored: GrantEvent = deserialize_from_document(document).unwrap();
            event.ensure_replay(&restored).unwrap();
        }
    }
}
