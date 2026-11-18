//! 抄送配置和收件箱集合；个人收件查询使用 identity 编译约束。
pub(crate) mod authorization;
use std::collections::BTreeMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_identity::CompiledAccess;
use erp_identity::entity::authorization::{QueryModel, QueryOutput};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::FindOptions;
use persistence_core::change_feed::ChangeSource;
use persistence_core::{
    Error, Executor, Repository, Result, aggregate, insert_literal_regex_filter, mongo_ops,
};
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
    /// 抄送仓储。
    ///
    /// 领域数据库。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
    /// 取得不可变定义对应的配置。
    /// 未配置时为空。
    ///
    /// 数据库错误。
    ///
    /// 定义 ID 和当前执行器。
    pub async fn config(&self, id: &str, ex: &mut dyn Executor) -> Result<Option<ApprovalCopyConfig>> {
        Repository::new(self.db, CONFIGS).find_by_id(id, ex).await
    }
    /// 在原定义 CAS 事务中整组替换配置。
    /// 数据库错误须回滚整个事务。
    ///
    /// 新配置及原事务。
    pub async fn save_config(&self, config: &ApprovalCopyConfig, ex: &mut dyn Executor) -> Result<()> {
        let col = self.db.collection::<ApprovalCopyConfig>(CONFIGS);
        mongo_ops::delete_many(&col, doc! { "id": &config.base.id }, ex).await?;
        mongo_ops::insert_one(&col, config, ex).await
    }
    /// 追加一批已经去重的个人抄送。
    /// 唯一冲突或写入失败。
    ///
    /// 同一审批事件批次和事务。
    pub async fn insert(&self, rows: &[ApprovalCopyReceipt], ex: &mut dyn Executor) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        mongo_ops::insert_many(&self.db.collection::<ApprovalCopyReceipt>(RECEIPTS), rows, ex).await
    }
    /// 读取未删除的收件原始事实，个人访问资格由 identity 核验。
    /// 不存在时为空。
    ///
    /// 数据库错误。
    ///
    /// 收件 ID 和当前执行器。
    pub async fn receipt(&self, id: &str, ex: &mut dyn Executor) -> Result<Option<ApprovalCopyReceipt>> {
        Repository::new(self.db, RECEIPTS).find_by_id(id, ex).await
    }
    /// 以完整抄送授权条件读取精确收件记录。
    /// 当前账号可读的未删除、已送达记录；未命中时为空。
    ///
    /// 模型或执行阶段不匹配、数据库或解码失败时拒绝。
    ///
    /// id 为真实收件身份，access 为原查询凭证，ex 为原读取执行器。
    pub async fn authorized_receipt(
        &self,
        id: &str,
        access: &CompiledAccess,
        ex: &mut dyn Executor,
    ) -> Result<Option<ApprovalCopyReceipt>> {
        ensure_copy_model(access.model())?;
        access.ensure_current(ex).map_err(|_| Error::UnsupportedDeployment("抄送授权阶段已失效"))?;

        let mut pipeline = access.stages().to_vec();
        pipeline.extend([doc! { "$match": { "id": id } }, doc! { "$limit": 1 }]);
        let mut rows = aggregate::rows(&self.db.collection(RECEIPTS), pipeline, ex).await?;
        Ok(rows.pop())
    }
    /// 首次阅读原子写入；重复阅读保留原时间。
    /// 实际改变的条数。
    ///
    /// 数据库错误。
    ///
    /// 原事务已由 identity 完整授权的 ID 集合、服务端时间和原执行器。
    pub async fn mark_read(&self, ids: &[String], at: i64, ex: &mut dyn Executor) -> Result<u64> {
        let filter = doc! {
            "id": { "$in": ids },
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "read_at": Bson::Null,
        };
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
    /// 最多 101 条，调用方截取 100 条。
    ///
    /// 数据库错误。
    ///
    /// 实例 ID、上页末尾记录 ID 和执行器。
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
    /// 每页 20 条及服务端统计。
    ///
    /// 数据库或解码错误。
    ///
    /// identity 编译的收件约束、已规范化查询和原事务执行器。
    pub async fn page(
        &self,
        access: &CompiledAccess,
        query: &CopyListQuery,
        ex: &mut dyn Executor,
    ) -> Result<CopyPage> {
        ensure_copy_model(access.model())?;

        let collection = self.db.collection::<Document>(RECEIPTS);
        let mut pipeline = access.stages().to_vec();
        pipeline.extend(list_pipeline(query));
        access.ensure_current(ex).map_err(|_| Error::UnsupportedDeployment("抄送授权阶段已失效"))?;
        let mut rows = aggregate::rows(&collection.clone_with_type::<Facet>(), pipeline, ex).await?;

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
// 抄送仓储仅执行绑定收件记录集合的完整对象凭证；不能把份额或其他模型凭证用于收件记录。
fn ensure_copy_model(model: &QueryModel) -> Result<()> {
    if model.resource != "approval_copy"
        || model.collection != RECEIPTS
        || model.key.as_str() != "approval_copy_receipts"
        || model.output != QueryOutput::Object
        || model.id_field != "id"
    {
        return Err(Error::UnsupportedDeployment("抄送授权模型不匹配"));
    }
    Ok(())
}

fn list_pipeline(query: &CopyListQuery) -> Vec<Document> {
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
    /// 当前数据库的抄送仓储。
    fn approval_copies(&self) -> CopyRepository<'_>;
}
impl ApprovalCopyExt for Database {
    fn approval_copies(&self) -> CopyRepository<'_> {
        CopyRepository::new(self)
    }
}

/// 声明个人抄送已提交变化，包括批量已读。
/// 按不可变收件账号投递的来源声明。
///
/// 不执行 I/O。
///
/// topic 为消费方刷新主题。
pub fn changes<T>(topic: T) -> ChangeSource<T> {
    ChangeSource::personal(RECEIPTS, topic, "recipient_user_id")
}

#[cfg(test)]
mod tests {
    use erp_identity::entity::authorization::QueryModelKey;

    use super::*;
    use crate::repository::approval_copy::authorization::CopyMapping;

    #[test]
    fn exact_receipt_requires_the_complete_owned_copy_model() {
        let mapping = CopyMapping::shared().unwrap();
        let model = mapping.model().clone();
        ensure_copy_model(&model).unwrap();

        let mut wrong_collection = model.clone();
        wrong_collection.collection = "approval_process_instances".into();
        let mut wrong_resource = model.clone();
        wrong_resource.resource = "approval_instance".into();
        let mut wrong_id = model.clone();
        wrong_id.id_field = "instance_id".into();
        let mut wrong_key = model.clone();
        wrong_key.key = QueryModelKey::parse("other_copies").unwrap();
        for other in [wrong_collection, wrong_resource, wrong_id, wrong_key] {
            assert!(matches!(ensure_copy_model(&other), Err(Error::UnsupportedDeployment(_))));
        }

        for output in [
            QueryOutput::Share { fields: ["id".into()].into() },
            QueryOutput::Existence { fields: ["id".into()].into() },
        ] {
            let restricted = QueryModel { output, ..model.clone() };
            assert!(matches!(ensure_copy_model(&restricted), Err(Error::UnsupportedDeployment(_))));
        }
    }
}
