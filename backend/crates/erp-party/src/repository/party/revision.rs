use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::PartyId;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

use super::shared::sort_doc;
use crate::entity::party::PartyRevision;

/// 主体修订列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 稳定主体 ID。
    pub party_id: PartyId,
    /// 修订序号。
    pub revision_no: u32,
    /// 法定名称。
    pub legal_name: String,
    /// 简称。
    pub short_name: Option<String>,
    /// 变更原因。
    pub change_reason: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 主体修订列表筛选条件。
#[derive(Debug, Clone)]
pub struct PartyRevisionFilter {
    /// 稳定主体 ID；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 法定名称模糊匹配（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub legal_name: Option<String>,
    /// 简称模糊匹配（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub short_name: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PartyRevisionFilter {
    /// 转换为 MongoDB 查询条件（修订集合无软删除，过滤条件为空时仍显式
    /// 追加未删除过滤，与基类语义保持一致）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
        }
        insert_literal_regex_filter(&mut filter, "legal_name", self.legal_name.as_deref());
        insert_literal_regex_filter(&mut filter, "short_name", self.short_name.as_deref());
        filter
    }
}

impl Pagination for PartyRevisionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 主体修订集合仓储的域查询扩展。
#[allow(async_fn_in_trait)]
pub trait PartyRevisionRepositoryExt {
    /// 按修订 ID 集合批量读取主体修订（erp-party-012）。
    ///
    /// 历史别名：与 [`Self::list_by_ids`] 同语义，保留以兼容公开签名；新调用统一使用 `list_by_ids`。
    ///
    /// # 参数
    /// * `revision_ids` - 主体修订 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配修订；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn find_revisions_by_ids(
        &self,
        revision_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyRevision>>;

    /// 按修订 ID 查找主体修订。
    ///
    /// # 参数
    /// * `id` - 主体修订 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的修订；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    async fn find_revision(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<PartyRevision>>;

    /// 分页检索主体修订列表（投影查询）。
    ///
    /// 只返回 [`PartyRevisionRow`] 所需的列表字段；排序字段经仓储白名单
    /// 校验（`created_at`/`revision_no`），非法字段回落默认 `created_at`。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    async fn search_party_revisions(
        &self,
        filter: &PartyRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PartyRevisionRow>>;

    /// 检索某主体的完整修订历史（按 `revision_no` 升序，§6.2 历史查询）。
    ///
    /// # 参数
    /// * `party_id` - 稳定主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该主体的全部修订（修订集合追加式写入，无软删除）。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    async fn list_revision_history(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyRevision>>;

    /// 按修订 ID 集合批量读取主体修订（erp-party-012）。
    ///
    /// 本域修订批量读取入口；`find_revisions_by_ids` 保留以兼容公开签名，实现委托本方法。
    ///
    /// # 参数
    /// * `revision_ids` - 修订 ID 集合；为空时直接返回空集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的主体修订。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    async fn list_by_ids(
        &self,
        revision_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyRevision>>;

    /// 按法定名称或简称字面量模糊匹配修订所属主体 ID。
    ///
    /// 返回修订直属主体 ID；是否当前生效由调用方按 `current_revision_id` 回指判定，
    /// 不要在本方法隐含当前性语义（erp-party-012）。
    ///
    /// # 参数
    /// * `keyword` - 名称关键词
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重并按稳定 ID 排序的主体 ID。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    async fn matching_party_ids_by_name(
        &self,
        keyword: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyId>>;

    /// 返回指定主体下一修订序号。
    ///
    /// # 参数
    /// * `party_id` - 稳定主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 无历史时返回 `1`，否则返回最大修订号加一。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    async fn next_revision_no(&self, party_id: &PartyId, executor: &mut dyn Executor) -> Result<u32>;
}

impl PartyRevisionRepositoryExt for Repository<'_, PartyRevision> {
    async fn find_revisions_by_ids(
        &self,
        revision_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyRevision>> {
        self.list_by_ids(revision_ids, executor).await
    }

    async fn find_revision(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<PartyRevision>> {
        self.find_by_id(id, executor).await
    }

    async fn search_party_revisions(
        &self,
        filter: &PartyRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PartyRevisionRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending, &["created_at", "revision_no"]))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(party_revision_projection())
            .build();
        let collection = self.collection().clone_with_type::<PartyRevisionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }

    async fn list_revision_history(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyRevision>> {
        self.find_many_sorted(doc! { "party_id": party_id.to_string() }, doc! { "revision_no": 1 }, executor)
            .await
    }

    async fn list_by_ids(
        &self,
        revision_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyRevision>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": revision_ids } }, executor).await
    }

    async fn matching_party_ids_by_name(
        &self,
        keyword: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyId>> {
        let escaped = regex::escape(keyword);
        let revisions = self
            .find_many(
                doc! {
                    "$or": [
                        { "legal_name": { "$regex": &escaped, "$options": "i" } },
                        { "short_name": { "$regex": &escaped, "$options": "i" } },
                    ]
                },
                executor,
            )
            .await?;
        let mut ids: Vec<PartyId> = revisions.into_iter().map(|revision| revision.party_id).collect();
        ids.sort_by_key(ToString::to_string);
        ids.dedup();
        Ok(ids)
    }

    async fn next_revision_no(&self, party_id: &PartyId, executor: &mut dyn Executor) -> Result<u32> {
        let revisions = self.list_revision_history(party_id, executor).await?;
        PartyRevision::next_revision_no(party_id, &revisions)
            .map_err(|_| persistence_core::Error::EntityMetadataOutOfRange("revision_no"))
    }
}
/// 主体修订列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn party_revision_projection() -> Document {
    doc! {
        "id": 1,
        "party_id": 1,
        "revision_no": 1,
        "legal_name": 1,
        "short_name": 1,
        "change_reason": 1,
        "version": 1,
        "created_at": 1,
    }
}
