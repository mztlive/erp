use entities::ids::PartyId;
use entities::party::{EffectiveRecordStatus, PartyBankAccount};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::shared::{active_fact_filter, sort_doc};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 银行账户列表投影行。
///
/// 账号为敏感值（§4.5.5）：投影**不包含** `account_number_ciphertext` 与
/// `account_number_query_hmac`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyBankAccountRow {
    /// 实体主键。
    pub id: String,
    /// ERP 内部稳定账户编号。
    pub bank_account_no: String,
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 户名。
    pub account_name: String,
    /// 银行。
    pub bank_name: String,
    /// 账号末四位；不包含可恢复明文。
    #[serde(default)]
    pub account_number_last4: String,
    /// 支行。
    pub bank_branch_name: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认账户。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 银行账户列表筛选条件。
#[derive(Debug, Clone)]
pub struct PartyBankAccountFilter {
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

impl QueryFilter for PartyBankAccountFilter {
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

impl Pagination for PartyBankAccountFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, PartyBankAccount> {
    /// 分页检索银行账户列表（投影查询，敏感字段不进投影）。
    ///
    /// 排序字段经仓储白名单校验（`created_at`/`bank_account_no`/`valid_from`），
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
    pub async fn search_party_bank_accounts(
        &self,
        filter: &PartyBankAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PartyBankAccountRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["created_at", "bank_account_no", "valid_from"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(party_bank_account_projection())
            .build();
        let collection = self.collection().clone_with_type::<PartyBankAccountRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按银行账户 ID 查找未删除账户事实。
    ///
    /// # 参数
    /// * `id` - 银行账户事实 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配账户；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_bank_account(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PartyBankAccount>> {
        self.find_by_id(id, executor).await
    }

    /// 按银行账户 ID 集合批量取回未删除账户事实（FIN-R02，`$in` 一次取回）。
    ///
    /// 只返回实体事实；掩码与可见字段选择由 Service 集中完成，
    /// 本方法不执行脱敏策略。空集合直接返回空列表，不发送空 `$in`。
    ///
    /// # 参数
    /// * `ids` - 银行账户事实 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配账户事实。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_bank_accounts_by_ids(
        &self,
        ids: &[entities::ids::PartyBankAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyBankAccount>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }

    /// 读取指定日期生效的主体银行账户。
    ///
    /// # 参数
    /// * `party_id` - 所属 Party ID
    /// * `as_of` - 业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该日期处于启用有效期的银行账户。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_active_on(
        &self,
        party_id: &PartyId,
        as_of: entities::common::time::BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyBankAccount>> {
        self.find_many(active_fact_filter(party_id, as_of), executor)
            .await
    }

    /// 按默认标记与创建时间读取指定日期生效的主体银行账户。
    ///
    /// # 参数
    /// * `party_id` - 所属 Party ID
    /// * `as_of` - 业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回默认账户优先、同组内最新创建优先的当前事实。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_current_on(
        &self,
        party_id: &PartyId,
        as_of: entities::common::time::BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyBankAccount>> {
        self.find_many_sorted(
            active_fact_filter(party_id, as_of),
            doc! { "is_default": -1, "created_at": -1 },
            executor,
        )
        .await
    }

    /// 清除同一 Party 其他银行账户的默认标记。
    ///
    /// 必须与主写入位于同一事务执行器中，避免并发或中途失败留下多个默认行。
    ///
    /// # 参数
    /// * `party_id` - 所属 Party ID
    /// * `exclude_id` - 保留默认标记的银行账户 ID
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

    /// 按默认标记和创建时间读取主体银行账户。
    ///
    /// # 参数
    /// * `party_id` - 所属主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回默认账户优先、同组内最新创建优先的完整实体。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_by_party(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyBankAccount>> {
        self.find_many_sorted(
            doc! { "party_id": party_id.to_string() },
            doc! { "is_default": -1, "created_at": -1 },
            executor,
        )
        .await
    }
}
/// 银行账户列表投影字段（不含敏感字段）。
///
/// # 返回
/// 返回投影条件文档。
fn party_bank_account_projection() -> Document {
    doc! {
        "id": 1,
        "bank_account_no": 1,
        "party_id": 1,
        "account_name": 1,
        "bank_name": 1,
        "account_number_last4": 1,
        "bank_branch_name": 1,
        "valid_from": 1,
        "valid_to": 1,
        "is_default": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}
