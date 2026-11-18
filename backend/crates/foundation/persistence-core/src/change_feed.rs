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
    /// 登记需要按当前授权重新查询的共享资源。
    /// # 参数
    /// collection 为本域集合；topic 为消费者的刷新主题。
    /// # 返回
    /// 不包含业务数据的来源声明。
    /// # 错误
    /// 不执行 I/O。
    pub fn shared(collection: &str, topic: T) -> Self {
        Self { collection: collection.to_owned(), topic, recipient_field: None }
    }
    /// 登记收件关系不可变的个人资源。
    /// # 参数
    /// recipient_field 为文档中的账号字段；缺少文档时退化为无载荷刷新。
    /// collection 和 topic 分别为本域集合与消费者主题。
    /// # 返回
    /// 限定个人收件人的来源声明。
    /// # 错误
    /// 不执行 I/O。
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

/// 打开一个数据库级变更流，只订阅各领域登记的集合。
/// # 参数
/// db 为目标数据库；sources 为拥有领域提供的来源白名单。
/// # 返回
/// 已建立的订阅；调用方随后读取初始快照可避免订阅窗口丢失。
/// # 错误
/// BSON、权限或数据库不支持变更流时返回错误；禁止回退为成功空流。
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
