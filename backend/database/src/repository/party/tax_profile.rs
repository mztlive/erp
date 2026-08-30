use entities::ids::PartyId;
use entities::party::{EffectiveRecordStatus, PartyTaxProfile};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::shared::{active_fact_filter, active_fact_window_filter, sort_doc};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 税务资料列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyTaxProfileRow {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 纳税人识别号。
    pub tax_no: String,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认税务资料。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 税务资料列表筛选条件。
#[derive(Debug, Clone)]
pub struct PartyTaxProfileFilter {
    /// 所属企业主体 ID；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
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

impl QueryFilter for PartyTaxProfileFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
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

impl Pagination for PartyTaxProfileFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, PartyTaxProfile> {
    /// 分页检索税务资料列表（投影查询）。
    ///
    /// 排序字段经仓储白名单校验（`created_at`/`tax_no`/`valid_from`），
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
    pub async fn search_party_tax_profiles(
        &self,
        filter: &PartyTaxProfileFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PartyTaxProfileRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "tax_no", "valid_from"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(party_tax_profile_projection())
            .build();
        let collection = self.collection().clone_with_type::<PartyTaxProfileRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 读取指定日期生效的主体税务资料。
    ///
    /// # 参数
    /// * `party_id` - 所属 Party ID
    /// * `as_of` - 业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该日期处于启用有效期的税务资料。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_active_on(
        &self,
        party_id: &PartyId,
        as_of: entities::common::time::BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyTaxProfile>> {
        self.find_many(active_fact_filter(party_id, as_of), executor)
            .await
    }

    /// 按默认标记与创建时间读取指定日期生效的主体税务资料。
    ///
    /// # 参数
    /// * `party_id` - 所属 Party ID
    /// * `as_of` - 业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回默认税务资料优先、同组内最新创建优先的当前事实。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_current_on(
        &self,
        party_id: &PartyId,
        as_of: entities::common::time::BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyTaxProfile>> {
        self.find_many_sorted(
            active_fact_filter(party_id, as_of),
            doc! { "is_default": -1, "created_at": -1 },
            executor,
        )
        .await
    }

    /// 批量读取多个主体在指定日期生效的税务资料。
    ///
    /// # 参数
    /// * `party_ids` - 往来主体 ID 集合
    /// * `as_of` - 业务日期
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按主体、默认标记和创建时间稳定排序的启用税务资料。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_current_for_parties_on(
        &self,
        party_ids: &[PartyId],
        as_of: entities::common::time::BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyTaxProfile>> {
        if party_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut filter = active_fact_window_filter(as_of);
        filter.insert(
            "party_id",
            doc! { "$in": party_ids.iter().map(ToString::to_string).collect::<Vec<_>>() },
        );
        self.find_many_sorted(
            filter,
            doc! { "party_id": 1, "is_default": -1, "created_at": -1, "id": -1 },
            executor,
        )
        .await
    }

    /// 清除同一 Party 其他税务资料的默认标记。
    ///
    /// 必须与主写入位于同一事务执行器中，避免并发或中途失败留下多个默认行。
    ///
    /// # 参数
    /// * `party_id` - 所属 Party ID
    /// * `exclude_id` - 保留默认标记的税务资料 ID
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

    /// 按默认标记和创建时间读取主体税务资料。
    ///
    /// # 参数
    /// * `party_id` - 所属主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回默认税务资料优先、同组内最新创建优先的完整实体。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_by_party(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyTaxProfile>> {
        self.find_many_sorted(
            doc! { "party_id": party_id.to_string() },
            doc! { "is_default": -1, "created_at": -1 },
            executor,
        )
        .await
    }
}
/// 税务资料列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn party_tax_profile_projection() -> Document {
    doc! {
        "id": 1,
        "party_id": 1,
        "tax_no": 1,
        "valid_from": 1,
        "valid_to": 1,
        "is_default": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}
