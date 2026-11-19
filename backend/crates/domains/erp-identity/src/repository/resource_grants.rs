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
/// 绑定授权来源集合，供来源管理和发布事务使用；构造不执行 I/O。
pub(crate) fn sources(db: &Database) -> Repository<'_, GrantSource> {
    Repository::new(db, SOURCES)
}
/// 绑定记录读取授权行集合，供来源同步及精确目标查询使用；构造不执行 I/O。
pub(crate) fn grants(db: &Database) -> Repository<'_, ResourceGrant> {
    Repository::new(db, GRANTS)
}
/// 绑定正式分配事件集合，供事件回放及来源发布使用；构造不执行 I/O。
pub(crate) fn events(db: &Database) -> Repository<'_, GrantEvent> {
    Repository::new(db, EVENTS)
}
/// 沿用调用方事务按业务幂等键读取正式事件，不依赖候选事件 ID；缺失返回 `None`，数据库失败直接返回。
pub(crate) async fn event_by_key(
    db: &Database,
    dedup_key: &str,
    executor: &mut dyn Executor,
) -> Result<Option<GrantEvent>> {
    Ok(events(db).find_one(doc! { "dedup_key": dedup_key }, executor).await?)
}

/// 按接收人及稳定主键分页列出授权来源，保留终止来源供审计；每页最多 100 条并返回后续游标，数据库失败直接返回。
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

/// 在来源发布事务内按授权行 ID 分页读取指定来源，每页最多 200 行；空页结束，数据库错误直接返回而不把失败当空页。
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

// 使用调用方提供的索引条件和限量读一页，按稳定 ID 排序；单页不代表完整授权集合。
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
/// 将来源 ID、资源类型和目标 ID 编成结构化行键，避免原始 ID 含分隔符时产生碰撞。
pub(crate) fn row_id(source: &str, resource: ResourceKind, id: &str) -> Result<String> {
    Ok(serde_json::to_string(&(source, resource, id))?)
}
/// 在授权来源的同一快照事务内按来源与版本追加不可变审计快照；无事务、编码或写入失败时拒绝。
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

    // 验证类型化事件经 BSON 往返后保留来源索引字段及各分配事件的证据载荷。
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
