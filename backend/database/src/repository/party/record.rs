use entities::ids::PartyId;
use entities::party::{Party, PartyKind, PartyStatus};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::super::regex_filter::insert_literal_regex_filter;
use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::shared::sort_doc;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 主体列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyRow {
    /// 实体主键。
    pub id: String,
    /// 主体编号。
    pub party_no: String,
    /// 主体类型。
    pub party_kind: PartyKind,
    /// 统一社会信用代码（非空值规范化后全局唯一）。
    pub unified_credit_code: Option<String>,
    /// 启停状态。
    pub status: PartyStatus,
    /// 当前生效修订 ID。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 主体列表筛选条件。
#[derive(Debug, Clone)]
pub struct PartyFilter {
    /// 主体编号模糊匹配（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub keyword: Option<String>,
    /// 主体类型；`None` 表示不筛选。
    pub party_kind: Option<PartyKind>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<PartyStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PartyFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "party_no", self.keyword.as_deref());
        if let Some(party_kind) = self.party_kind {
            filter.insert("party_kind", party_kind.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for PartyFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, Party> {
    /// 按主体 ID 集合批量读取活跃主体。
    ///
    /// # 参数
    /// * `party_ids` - 主体 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的主体；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_parties_by_ids(
        &self,
        party_ids: &[PartyId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Party>> {
        if party_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = party_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    /// 按主体 ID 查找未删除 Party。
    ///
    /// # 参数
    /// * `id` - 稳定 Party ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除主体；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_party(&self, id: &PartyId, executor: &mut dyn Executor) -> Result<Option<Party>> {
        self.find_by_id(id.as_ref(), executor).await
    }

    /// 分页检索主体列表（投影查询）。
    ///
    /// 只返回 [`PartyRow`] 所需的列表字段，不加载整文档；排序字段经仓储
    /// 白名单校验，非法字段回落默认 `created_at`。
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
    pub async fn search_parties(
        &self,
        filter: &PartyFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PartyRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "party_no", "status"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(party_projection())
            .build();
        let collection = self.collection().clone_with_type::<PartyRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按主体编号查找主体，包含已软删除记录。
    ///
    /// 全局唯一索引包含软删除记录；编号占用校验必须使用本方法，避免
    /// 仅查未删除记录时误判为可用。
    ///
    /// # 参数
    /// * `party_no` - 主体编号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的主体（含已软删除）；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_party_no_including_deleted(
        &self,
        party_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Party>> {
        mongo_ops::find_one(&self.collection(), doc! { "party_no": party_no }, executor).await
    }

    /// 按统一社会信用代码查找主体，包含已软删除记录。
    ///
    /// 部分唯一索引 `uk_parties_credit_code` 仅约束非空代码且包含软删除；
    /// 信用代码占用校验必须使用本方法。
    ///
    /// # 参数
    /// * `unified_credit_code` - 已规范化的统一社会信用代码
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的主体（含已软删除）；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_unified_credit_code_including_deleted(
        &self,
        unified_credit_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Party>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "unified_credit_code": unified_credit_code },
            executor,
        )
        .await
    }

    /// 按主体 ID 集合批量读取未删除主体。
    ///
    /// # 参数
    /// * `party_ids` - 主体 ID 集合；为空时直接返回空集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的未删除主体。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_by_ids(
        &self,
        party_ids: &[PartyId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Party>> {
        if party_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = party_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }
}
/// 主体列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn party_projection() -> Document {
    doc! {
        "id": 1,
        "party_no": 1,
        "party_kind": 1,
        "unified_credit_code": 1,
        "status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{PartyFilter, QueryFilter};
    use entities::party::{PartyKind, PartyStatus};

    #[test]
    fn party_filter_applies_keyword_regex_and_status() {
        let filter = PartyFilter {
            keyword: Some("P-20".to_string()),
            party_kind: Some(PartyKind::Enterprise),
            status: Some(PartyStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("party_kind").unwrap(), "enterprise");
        assert_eq!(document.get_str("status").unwrap(), "active");
        let keyword = document.get_document("party_no").unwrap();
        assert_eq!(keyword.get_str("$regex").unwrap(), r"P\-20");
        assert_eq!(keyword.get_str("$options").unwrap(), "i");
    }
}
