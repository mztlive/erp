//! 已提交变更的技术订阅；集合与收件人字段只能由拥有领域登记。
//! 不返回业务文档。消费者重新连接后必须完整同步，不把此流当作业务命令队列。
use std::time::Duration;

use mongodb::Database;
use mongodb::bson::{Bson, Document, doc, serialize_to_bson};
use mongodb::change_stream::ChangeStream;
use mongodb::options::FullDocumentType;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::Result;

/// 拥有领域公开的变更来源；topic 由组合层指定。
#[derive(Clone)]
pub struct ChangeSource<T> {
    collection: String,
    topic: T,
    recipient_field: Option<&'static str>,
}
impl<T> ChangeSource<T> {
    /// 登记本领域集合的共享刷新主题，不读取数据库。通知本身没有业务数据或收件人，
    /// 消费者收到后须按当前权限重新查询。
    pub fn shared(collection: &str, topic: T) -> Self {
        Self { collection: collection.to_owned(), topic, recipient_field: None }
    }
    /// 为收件人归属不可变的资源登记账号字段和刷新主题，不执行 I/O。
    /// 变更流取不到文档或账号时，退化为没有收件人信息的刷新通知。
    pub fn personal(collection: &str, topic: T, recipient_field: &'static str) -> Self {
        Self { collection: collection.to_owned(), topic, recipient_field: Some(recipient_field) }
    }
}

/// 服务端内部刷新信号，不携带业务正文或对象 ID。
#[derive(Debug, Deserialize)]
pub struct CommittedChange<T> {
    /// None 表示数据库结构变化，消费者须完整同步。
    pub topic: Option<T>,
    /// None 表示共享资源；消费者仍须执行当前业务授权查询。
    pub recipient: Option<String>,
}

/// 打开数据库级订阅，只输出已登记集合的刷新信号及数据库结构失效通知，不传业务文档。
/// 应先订阅再读取初始快照，以免漏掉窗口内的变更。BSON、权限或部署能力错误必须返回，不当作空流。
pub async fn subscribe<T: Serialize + DeserializeOwned + Send + Sync>(
    db: &Database,
    sources: &[ChangeSource<T>],
) -> Result<ChangeStream<CommittedChange<T>>> {
    let stream = db
        .watch()
        .pipeline(pipeline(sources)?)
        .full_document(FullDocumentType::UpdateLookup)
        .max_await_time(Duration::from_secs(1))
        .await?;
    Ok(stream.with_type())
}

fn pipeline<T: Serialize>(sources: &[ChangeSource<T>]) -> Result<Vec<Document>> {
    let collections = sources.iter().map(|source| source.collection.as_str()).collect::<Vec<_>>();
    let mut topics = Vec::new();
    let mut recipients = Vec::new();
    for source in sources {
        let condition = doc! { "$eq": ["$ns.coll", &source.collection] };
        topics.push(
            doc! { "case": condition.clone(), "then": { "$literal": serialize_to_bson(&source.topic)? } },
        );

        let recipient = source
            .recipient_field
            .map(|field| Bson::String(format!("$fullDocument.{field}")))
            .unwrap_or(Bson::Null);
        recipients.push(doc! { "case": condition, "then": { "$ifNull": [recipient, Bson::Null] } });
    }

    Ok(vec![
        doc! { "$match": { "$or": [
            { "ns.coll": { "$in": collections }, "operationType": { "$in": ["insert", "update", "replace", "delete", "drop", "rename"] } },
            { "operationType": { "$in": ["invalidate", "dropDatabase"] } }
        ] } },
        doc! { "$project": {
            "_id": 1, "operationType": 1,
            "topic": { "$switch": { "branches": topics, "default": Bson::Null } },
            "recipient": { "$switch": { "branches": recipients, "default": Bson::Null } }
        } },
    ])
}

#[cfg(test)]
mod tests {
    use mongodb::bson::deserialize_from_document;

    use super::*;

    #[test]
    fn projection_retains_resume_identity_and_excludes_business_documents() {
        let stages = pipeline(&[ChangeSource::personal("messages", "inbox", "recipient_id")]).unwrap();
        let projection = stages[1].get_document("$project").unwrap();
        assert_eq!(projection.len(), 4);
        assert_eq!(projection.get_i32("_id").unwrap(), 1);
        assert!(!projection.contains_key("fullDocument"));

        let topics = projection.get_document("topic").unwrap().get_document("$switch").unwrap();
        let first = topics.get_array("branches").unwrap()[0].as_document().unwrap();
        assert_eq!(first.get_document("then").unwrap().get_str("$literal").unwrap(), "inbox");
    }

    #[test]
    fn shared_and_missing_documents_deserialize_without_exposing_identifiers() {
        let event: CommittedChange<String> =
            deserialize_from_document(doc! { "topic": "tasks", "recipient": null }).unwrap();
        assert_eq!(event.topic.as_deref(), Some("tasks"));
        assert_eq!(event.recipient, None);

        let reset: CommittedChange<String> =
            deserialize_from_document(doc! { "topic": null, "recipient": null }).unwrap();
        assert!(reset.topic.is_none());
    }
}
