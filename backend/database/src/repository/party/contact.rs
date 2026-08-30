use entities::ids::PartyId;
use entities::party::{EffectiveRecordStatus, PartyContact};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::super::regex_filter::insert_literal_regex_filter;
use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::shared::{active_fact_filter, sort_doc};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 联系人列表投影行。
///
/// 手机号为敏感值（§4.5.5）：投影**不包含** `mobile_ciphertext` 与
/// `mobile_query_hmac`，列表只返回业务展示字段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyContactRow {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 联系人姓名。
    pub contact_name: String,
    /// 职务/用途。
    pub title: Option<String>,
    /// 电话。
    pub telephone: Option<String>,
    /// 手机号末四位；不包含可恢复明文。
    #[serde(default)]
    pub mobile_last4: String,
    /// 邮箱。
    pub email: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认联系人。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 联系人列表筛选条件。
#[derive(Debug, Clone)]
pub struct PartyContactFilter {
    /// 所属企业主体 ID；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 联系人姓名模糊匹配（字面量正则，忽略大小写）；`None` 表示不筛选。
    pub keyword: Option<String>,
    /// 手机号查询指纹（精确匹配，禁止明文查询）；`None` 表示不筛选。
    pub mobile_query_hmac: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EffectiveRecordStatus>,
    /// 默认标记；`None` 表示不筛选。
    pub is_default: Option<bool>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PartyContactFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
        }
        insert_literal_regex_filter(&mut filter, "contact_name", self.keyword.as_deref());
        if let Some(hmac) = &self.mobile_query_hmac {
            filter.insert("mobile_query_hmac", hmac);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(is_default) = self.is_default {
            filter.insert("is_default", is_default);
        }
        filter
    }
}

impl Pagination for PartyContactFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, PartyContact> {
    /// 分页检索联系人列表（投影查询，敏感字段不进投影）。
    ///
    /// 排序字段经仓储白名单校验（`created_at`/`contact_name`/`valid_from`），
    /// 非法字段回落默认 `created_at`。
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
    pub async fn search_party_contacts(
        &self,
        filter: &PartyContactFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PartyContactRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "contact_name", "valid_from"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(party_contact_projection())
            .build();
        let collection = self.collection().clone_with_type::<PartyContactRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按联系人 ID 查找未删除联系人事实。
    ///
    /// # 参数
    /// * `id` - 联系人事实 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配联系人；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_contact(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<PartyContact>> {
        self.find_by_id(id, executor).await
    }

    /// 读取指定日期生效的主体联系人。
    ///
    /// # 参数
    /// * `party_id` - 所属 Party ID
    /// * `as_of` - 业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该日期处于启用有效期的联系人。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_active_on(
        &self,
        party_id: &PartyId,
        as_of: entities::common::time::BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyContact>> {
        self.find_many(active_fact_filter(party_id, as_of), executor)
            .await
    }

    /// 按默认标记与创建时间读取指定日期生效的主体联系人。
    ///
    /// # 参数
    /// * `party_id` - 所属 Party ID
    /// * `as_of` - 业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回默认联系人优先、同组内最新创建优先的当前事实。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_current_on(
        &self,
        party_id: &PartyId,
        as_of: entities::common::time::BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyContact>> {
        self.find_many_sorted(
            active_fact_filter(party_id, as_of),
            doc! { "is_default": -1, "created_at": -1 },
            executor,
        )
        .await
    }

    /// 清除同一 Party 其他联系人的默认标记。
    ///
    /// 必须与主写入位于同一事务执行器中，避免并发或中途失败留下多个默认行。
    ///
    /// # 参数
    /// * `party_id` - 所属 Party ID
    /// * `exclude_id` - 保留默认标记的联系人 ID
    /// * `executor` - 数据访问执行器，必须位于调用方事务中
    ///
    /// # 返回
    /// 全部冲突默认标记清除后返回 `Ok(())`。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或 CAS 更新失败时返回错误。
    pub async fn clear_other_default_marks(
        &self,
        party_id: &PartyId,
        exclude_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let rows = self
            .find_many(
                doc! { "party_id": party_id.to_string(), "is_default": true },
                executor,
            )
            .await?;
        for mut row in rows {
            if exclude_id.is_some_and(|id| id == row.base.id) {
                continue;
            }
            row.is_default = false;
            self.update(&mut row, executor).await?;
        }
        Ok(())
    }

    /// 按默认标记和创建时间读取主体联系人。
    ///
    /// # 参数
    /// * `party_id` - 所属主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回默认联系人优先、同组内最新创建优先的完整实体。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_by_party(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyContact>> {
        self.find_many_sorted(
            doc! { "party_id": party_id.to_string() },
            doc! { "is_default": -1, "created_at": -1 },
            executor,
        )
        .await
    }
}
/// 联系人列表投影字段（不含敏感字段）。
///
/// # 返回
/// 返回投影条件文档。
fn party_contact_projection() -> Document {
    doc! {
        "id": 1,
        "party_id": 1,
        "contact_name": 1,
        "title": 1,
        "telephone": 1,
        "mobile_last4": 1,
        "email": 1,
        "valid_from": 1,
        "valid_to": 1,
        "is_default": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}
