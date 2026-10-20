//! 抄送配置和收件箱集合；所有查询先限定接收人。
use std::collections::BTreeMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, insert_literal_regex_filter, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::entity::approval_copy::{ApprovalCopyConfig, ApprovalCopyReceipt};

pub const CONFIGS: &str = "approval_copy_configs";
pub const RECEIPTS: &str = "approval_copy_receipts";

/// 有界收件箱查询。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyListQuery {
    pub page: Option<u32>,
    pub read: Option<bool>,
    pub document_type: Option<String>,
    pub query: Option<String>,
}

/// 同一快照中计算的收件箱及统计。
#[derive(Debug, Serialize)]
pub struct CopyPage {
    pub items: Vec<CopyListItem>,
    pub total: u64,
    pub unread_count: u64,
    pub document_type_counts: BTreeMap<String, u64>,
}
/// 收件行携带当前审批状态，整页一次聚合取得。
#[derive(Debug, Serialize, Deserialize)]
pub struct CopyListItem {
    #[serde(flatten)]
    pub receipt: ApprovalCopyReceipt,
    pub instance_status: String,
}
#[derive(Deserialize)]
struct Count {
    count: u64,
}
#[derive(Deserialize)]
struct TypeCount {
    #[serde(rename = "_id")]
    kind: String,
    count: u64,
}
#[derive(Deserialize)]
struct Facet {
    items: Vec<CopyListItem>,
    total: Vec<Count>,
    unread: Vec<Count>,
    types: Vec<TypeCount>,
}

/// 抄送持久化入口。
pub struct CopyRepository<'a> {
    db: &'a Database,
}
impl<'a> CopyRepository<'a> {
    /// 创建仓储。
    /// # 参数
    /// 领域数据库。
    /// # 返回
    /// 抄送仓储。
    /// # 错误
    /// 无。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
    /// 取得不可变定义对应的配置。
    /// # 参数
    /// 定义 ID 和当前执行器。
    /// # 返回
    /// 未配置时为空。
    /// # 错误
    /// 数据库错误。
    pub async fn config(&self, id: &str, ex: &mut dyn Executor) -> Result<Option<ApprovalCopyConfig>> {
        Repository::new(self.db, CONFIGS).find_by_id(id, ex).await
    }
    /// 在原定义 CAS 事务中整组替换配置。
    /// # 参数
    /// 新配置及原事务。
    /// # 返回
    /// 无。
    /// # 错误
    /// 数据库错误须回滚整个事务。
    pub async fn save_config(&self, config: &ApprovalCopyConfig, ex: &mut dyn Executor) -> Result<()> {
        let col = self.db.collection::<ApprovalCopyConfig>(CONFIGS);
        mongo_ops::delete_many(&col, doc! { "id": &config.base.id }, ex).await?;
        mongo_ops::insert_one(&col, config, ex).await
    }
    /// 追加一批已经去重的个人抄送。
    /// # 参数
    /// 同一审批事件批次和事务。
    /// # 返回
    /// 无。
    /// # 错误
    /// 唯一冲突或写入失败。
    pub async fn insert(&self, rows: &[ApprovalCopyReceipt], ex: &mut dyn Executor) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        mongo_ops::insert_many(&self.db.collection::<ApprovalCopyReceipt>(RECEIPTS), rows, ex).await
    }
    /// 只取得本人的已送达抄送。
    /// # 参数
    /// 收件 ID、认证账号、执行器。
    /// # 返回
    /// 越界时为空。
    /// # 错误
    /// 数据库错误。
    pub async fn receipt(
        &self,
        id: &str,
        actor: &str,
        ex: &mut dyn Executor,
    ) -> Result<Option<ApprovalCopyReceipt>> {
        let mut filter = recipient_filter(actor);
        filter.insert("id", id);
        Repository::new(self.db, RECEIPTS).find_one(filter, ex).await
    }
    /// 首次阅读原子写入；重复阅读保留原时间。
    /// # 参数
    /// 已授权 ID 集合、账号、服务端时间、执行器。
    /// # 返回
    /// 实际改变的条数。
    /// # 错误
    /// 数据库错误。
    pub async fn mark_read(
        &self,
        ids: &[String],
        actor: &str,
        at: i64,
        ex: &mut dyn Executor,
    ) -> Result<u64> {
        let mut filter = recipient_filter(actor);
        filter.insert("id", doc! { "$in": ids });
        filter.insert("read_at", Bson::Null);
        Ok(mongo_ops::update_many(
            &self.db.collection::<ApprovalCopyReceipt>(RECEIPTS),
            filter,
            doc! { "$set": { "read_at": at } },
            ex,
        )
        .await?
        .modified_count)
    }
    /// 按已授权实例分页查询抄送记录，不扩大实例读取权。
    /// # 参数
    /// 实例 ID、上页末尾记录 ID 和执行器。
    /// # 返回
    /// 最多 101 条，调用方截取 100 条。
    /// # 错误
    /// 数据库错误。
    pub async fn history(
        &self,
        instance: &str,
        after: Option<&str>,
        ex: &mut dyn Executor,
    ) -> Result<Vec<ApprovalCopyReceipt>> {
        let mut filter = doc! { "instance_id": instance, "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(after) = after {
            filter.insert("id", doc! { "$gt": after });
        }
        mongo_ops::find_many(
            &self.db.collection(RECEIPTS),
            filter,
            FindOptions::builder().sort(doc! { "id": 1 }).limit(101).build(),
            ex,
        )
        .await
    }
    /// 返回本人分页和完整计数，不逐条查询。
    /// # 参数
    /// 认证账号、已规范化查询、执行器。
    /// # 返回
    /// 每页 20 条及服务端统计。
    /// # 错误
    /// 数据库或解码错误。
    pub async fn page(&self, actor: &str, query: &CopyListQuery, ex: &mut dyn Executor) -> Result<CopyPage> {
        let collection = self.db.collection::<Document>(RECEIPTS);
        let pipeline = list_pipeline(actor, query);
        let mut rows = match ex.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<Facet>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            },
            None => {
                collection.aggregate(pipeline).with_type::<Facet>().await?.try_collect::<Vec<_>>().await?
            },
        };
        let facet =
            rows.pop().unwrap_or(Facet { items: vec![], total: vec![], unread: vec![], types: vec![] });
        Ok(CopyPage {
            items: facet.items,
            total: facet.total.first().map_or(0, |c| c.count),
            unread_count: facet.unread.first().map_or(0, |c| c.count),
            document_type_counts: facet.types.into_iter().map(|c| (c.kind, c.count)).collect(),
        })
    }
}
fn recipient_filter(actor: &str) -> Document {
    doc! { "recipient_user_id": actor, "delivered": true, "deleted_at": NOT_DELETED_TIMESTAMP_BSON }
}
fn list_pipeline(actor: &str, query: &CopyListQuery) -> Vec<Document> {
    let mut filter = Document::new();
    if let Some(read) = query.read {
        filter.insert(
            "read_at",
            if read {
                doc! { "$ne": Bson::Null }
            } else {
                doc! { "$eq": Bson::Null }
            },
        );
    }
    if let Some(text) = &query.query {
        insert_literal_regex_filter(&mut filter, "document_no", Some(text));
    }
    let type_filter = filter.clone();
    if let Some(kind) = &query.document_type {
        filter.insert("document_type", kind);
    }
    let offset = u64::from(query.page.unwrap_or(1).saturating_sub(1)) * 20;
    vec![
        doc! { "$match": recipient_filter(actor) },
        doc! { "$lookup": { "from": "approval_subject_snapshots", "localField": "snapshot_id", "foreignField": "id", "as": "snapshot" } },
        doc! { "$unwind": "$snapshot" },
        doc! { "$match": { "snapshot.deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$expr": { "$and": [
            { "$eq": ["$snapshot.approval_process_instance_id", "$instance_id"] },
            { "$eq": ["$snapshot.document_type", "$document_type"] },
            { "$eq": ["$snapshot.subject_version", "$subject_version"] }
        ] } } },
        doc! { "$lookup": { "from": "approval_process_instances", "localField": "instance_id", "foreignField": "id", "as": "instance" } },
        doc! { "$unwind": "$instance" },
        doc! { "$match": { "instance.deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$expr": { "$and": [
            { "$eq": ["$instance.subject_version", "$subject_version"] },
            { "$eq": ["$instance.subject.subject_id", "$snapshot.business_object_id"] },
            { "$eq": ["$instance.subject.subject_kind", "$document_type"] },
            { "$eq": ["$instance.process_kind", "$document_type"] }
        ] } } },
        doc! { "$set": { "instance_status": "$instance.status" } },
        doc! { "$unset": ["snapshot", "instance"] },
        doc! { "$facet": {
            "items": [{ "$match": filter.clone() }, { "$sort": { "copied_at": -1, "id": -1 } }, { "$skip": i64::try_from(offset).unwrap_or(i64::MAX) }, { "$limit": 20 }],
            "total": [{ "$match": filter.clone() }, { "$count": "count" }],
            "types": [{ "$match": type_filter }, { "$group": { "_id": "$document_type", "count": { "$sum": 1 } } }],
            "unread": [{ "$match": { "read_at": Bson::Null } }, { "$count": "count" }]
        } },
    ]
}

/// 工作流领域的抄送仓储访问器。
pub trait ApprovalCopyExt {
    /// 取得配置和收件仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库的抄送仓储。
    /// # 错误
    /// 无。
    fn approval_copies(&self) -> CopyRepository<'_>;
}
impl ApprovalCopyExt for Database {
    fn approval_copies(&self) -> CopyRepository<'_> {
        CopyRepository::new(self)
    }
}
