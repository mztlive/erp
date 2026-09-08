use crate::entity::receivable::{AccountReviewStatus, ReceivableAccount, ReceivableAccountStatus};
use crate::repository::owned::ReceivableAccountRepository;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::stable::StableBase;
use erp_core::ids::{CustomerAccountId, PartyId, ReceivableAccountId, SalesOrderId, SalesOrderRevisionId};
use erp_core::money::Amount;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use super::sort_doc;
use persistence_core::insert_literal_regex_filter;
use persistence_core::Executor;
use persistence_core::{mongo_ops, Result};
use persistence_core::{PageResult, Pagination, QueryFilter};

/// 应收往来子账列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivableAccountRow {
    /// 实体主键。
    pub id: String,
    /// 稳定公共字段（状态/版本归属/审计人）。
    #[serde(flatten)]
    pub stable: StableBase<ReceivableAccountStatus>,
    /// 来源销售单。
    pub sales_order_id: String,
    /// 往来子账序号。
    pub account_seq: u32,
    /// 企业客户经营归属。
    pub customer_id: String,
    /// 收款和开票往来主体。
    pub counterparty_party_id: String,
    /// 卡券票款复核状态缓存。
    pub review_status: AccountReviewStatus,
    /// 含税应收总额。
    pub gross_total: Amount,
    /// 已核销含税总额。
    pub settled_total: Amount,
    /// 剩余开放含税余额。
    pub open_total: Amount,
    /// 可开票含税总额。
    pub invoiceable_total: Amount,
    /// 净已开含税总额。
    pub invoiced_total: Amount,
    /// 剩余可开票含税额度。
    pub open_invoiceable_total: Amount,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 批量条件开票结果：按账户逐个报告命中情况，由 Service 转译业务错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoicingBatchResult {
    /// 条件命中并完成开票的账户（输入顺序）。
    pub applied: Vec<ReceivableAccountId>,
    /// 条件未命中（超过剩余可开票额度）被拒绝的账户（输入顺序）。
    pub rejected: Vec<ReceivableAccountId>,
}

/// 批量条件核销结果：按账户逐个报告命中情况，由 Service 转译业务错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementBatchResult {
    /// 条件命中并完成核销的账户（输入顺序）。
    pub applied: Vec<ReceivableAccountId>,
    /// 条件未命中（超过剩余开放余额）被拒绝的账户（输入顺序）。
    pub rejected: Vec<ReceivableAccountId>,
}

/// 应收往来子账列表筛选条件。
#[derive(Debug, Clone)]
pub struct ReceivableAccountFilter {
    /// 主键、销售单、客户或往来主体关键字；`None` 表示不筛选。
    pub keyword: Option<String>,
    /// 关键词命中的来源销售单身份；由组合查询解析。
    pub keyword_sales_order_ids: Vec<SalesOrderId>,
    /// 关键词命中的往来主体身份；由组合查询解析。
    pub keyword_party_ids: Vec<PartyId>,
    /// 子账主键；`None` 表示不筛选。
    pub account_id: Option<ReceivableAccountId>,
    /// 企业客户经营归属；`None` 表示不筛选。
    pub customer_id: Option<CustomerAccountId>,
    /// 收款和开票往来主体；`None` 表示不筛选。
    pub counterparty_party_id: Option<PartyId>,
    /// 子账状态；`None` 表示不筛选。
    pub status: Option<ReceivableAccountStatus>,
    /// 来源销售单；`None` 表示不筛选。
    pub sales_order_id: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ReceivableAccountFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(keyword) = self.keyword.as_deref() {
            let mut alternatives = ["id", "sales_order_id", "customer_id", "counterparty_party_id"]
                .into_iter()
                .map(|field| {
                    let mut alternative = Document::new();
                    insert_literal_regex_filter(&mut alternative, field, Some(keyword));
                    alternative
                })
                .collect::<Vec<_>>();
            if !self.keyword_sales_order_ids.is_empty() {
                alternatives.push(doc! { "sales_order_id": { "$in": self.keyword_sales_order_ids.iter().map(ToString::to_string).collect::<Vec<_>>() } });
            }
            if !self.keyword_party_ids.is_empty() {
                alternatives.push(doc! { "counterparty_party_id": { "$in": self.keyword_party_ids.iter().map(ToString::to_string).collect::<Vec<_>>() } });
            }
            filter.insert("$or", alternatives);
        }
        if let Some(account_id) = &self.account_id {
            filter.insert("id", account_id.to_string());
        }
        if let Some(customer_id) = &self.customer_id {
            filter.insert("customer_id", customer_id.to_string());
        }
        if let Some(counterparty_party_id) = &self.counterparty_party_id {
            filter.insert("counterparty_party_id", counterparty_party_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        filter
    }
}

impl Pagination for ReceivableAccountFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> ReceivableAccountRepository<'a> {
    /// 分页检索应收往来子账列表（投影查询）。
    ///
    /// 只返回 [`ReceivableAccountRow`] 所需的列表字段，不加载整文档；
    /// 排序字段经白名单映射，未命中回退 `created_at` 降序。
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
    pub async fn search_receivable_accounts(
        &self,
        filter: &ReceivableAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ReceivableAccountRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &[
                    "account_seq",
                    "gross_total",
                    "settled_total",
                    "open_total",
                    "open_invoiceable_total",
                    "created_at",
                ],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(receivable_account_projection())
            .build();
        let collection = self.collection().clone_with_type::<ReceivableAccountRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 条件核销：增加已核销进度（不超额核销）。
    ///
    /// 原子写入口（P2 计划 §5）：以写条件而非读后判断保证
    /// `settled_total + 本次核销 <= gross_total`，不满足时**整个更新不生效**
    /// （matched 为 0），返回 `false` 且金额与状态均不变。核销进度同时重算
    /// `open_total` 与派生状态，全部在同一条件更新内完成，不会产生负开放余额。
    /// 单文档更新本身原子，可在 Service 的过账事务内参与回滚。
    ///
    /// # 参数
    /// * `id` - 应收往来子账 ID
    /// * `amount` - 本次核销含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 核销在额度内并已生效时返回 `true`；超过剩余开放余额被拒绝时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    pub async fn apply_settlement(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let amount = amount_bson(amount)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "$expr": {
                "$lte": [
                    { "$add": ["$settled_total", &amount] },
                    "$gross_total",
                ],
            },
        };
        self.conditional_update(
            filter,
            progress_pipeline("settled_total", "open_total", &amount, true, updated_by),
            executor,
        )
        .await
    }

    /// 条件核销冲减：减少已核销进度（不产生负已核销）。
    ///
    /// 反向核销（`REVERSE` 分配）的原子写入口：以写条件保证
    /// `本次冲减 <= settled_total`，不满足时整个更新不生效，返回 `false`。
    /// 用于冲正/退款时追加反向核销，防止冲减超过已核销金额。
    ///
    /// # 参数
    /// * `id` - 应收往来子账 ID
    /// * `amount` - 本次冲减含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 冲减在已核销额度内并已生效时返回 `true`；超过已核销金额被拒绝时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    pub async fn revert_settlement(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let amount = amount_bson(amount)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "$expr": {
                "$gte": ["$settled_total", &amount],
            },
        };
        self.conditional_update(
            filter,
            progress_pipeline("settled_total", "open_total", &amount, false, updated_by),
            executor,
        )
        .await
    }

    /// 条件开票：增加净已开票进度（不超过可开票额度）。
    ///
    /// 销项蓝票 `APPLY` 的原子写入口：以写条件保证
    /// `invoiced_total + 本次开票 <= invoiceable_total`，不满足时整个更新不生效，
    /// 返回 `false`。同时重算 `open_invoiceable_total`，不会产生负可开票余额。
    ///
    /// # 参数
    /// * `id` - 应收往来子账 ID
    /// * `amount` - 本次开票含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 开票在额度内并已生效时返回 `true`；超过剩余可开票额度被拒绝时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    pub async fn apply_invoicing(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let amount = amount_bson(amount)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "$expr": {
                "$lte": [
                    { "$add": ["$invoiced_total", &amount] },
                    "$invoiceable_total",
                ],
            },
        };
        self.conditional_update(
            filter,
            progress_pipeline(
                "invoiced_total",
                "open_invoiceable_total",
                &amount,
                true,
                updated_by,
            ),
            executor,
        )
        .await
    }

    /// 批量条件开票：按子账聚合增量原子更新开票进度（FIN-R10）。
    ///
    /// 对已去重并按账户聚合的 `deltas` 逐账户执行条件更新（`invoicing_guard`
    /// 保证 `invoiced_total + delta <= invoiceable_total`），并按输入顺序
    /// 报告每个账户的命中情况；调用方（Service）负责将 `rejected` 转译为
    /// 业务错误，失败时整个事务回滚，不产生部分写入。
    ///
    /// # 参数
    /// * `deltas` - 按子账聚合的开票增量（已去重、首次出现顺序，同一账户只出现一次）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按账户报告命中情况的 [`InvoicingBatchResult`]。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    ///
    /// # 约束
    /// 聚合口径（同一账户增量求和、同一账户只更新一次）由 Service 经领域
    /// 计划保证；本方法不自行开启事务、不决定跨账户业务结论。
    pub async fn apply_invoicings_many(
        &self,
        deltas: &[(ReceivableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoicingBatchResult> {
        let mut applied = Vec::new();
        let mut rejected = Vec::new();
        for (id, amount) in deltas {
            let amount = amount_bson(amount)?;
            let filter = invoicing_guard(id.as_ref(), &amount);
            let hit = self
                .conditional_update(
                    filter,
                    progress_pipeline(
                        "invoiced_total",
                        "open_invoiceable_total",
                        &amount,
                        true,
                        updated_by,
                    ),
                    executor,
                )
                .await?;
            if hit {
                applied.push(id.clone());
            } else {
                rejected.push(id.clone());
            }
        }
        Ok(InvoicingBatchResult { applied, rejected })
    }

    /// 条件开票冲减：减少净已开票进度（不产生负已开票）。
    ///
    /// 销项红票 `REVERSE` 的原子写入口：以写条件保证 `本次红冲 <= invoiced_total`，
    /// 不满足时整个更新不生效，返回 `false`。累计红冲由 P3 登记事务结合
    /// `reverses_allocation_id` 校验，本方法防止已开票进度被冲成负数。
    ///
    /// # 参数
    /// * `id` - 应收往来子账 ID
    /// * `amount` - 本次红冲含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 红冲在已开票额度内并已生效时返回 `true`；超过已开票金额被拒绝时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    pub async fn revert_invoicing(
        &self,
        id: &str,
        amount: &Amount,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let amount = amount_bson(amount)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "$expr": {
                "$gte": ["$invoiced_total", &amount],
            },
        };
        self.conditional_update(
            filter,
            progress_pipeline(
                "invoiced_total",
                "open_invoiceable_total",
                &amount,
                false,
                updated_by,
            ),
            executor,
        )
        .await
    }

    /// 批量条件开票冲减：按子账聚合增量原子回退开票进度（FIN-R11）。
    ///
    /// 对已去重并按账户聚合的 `deltas` 逐账户执行条件更新（写条件保证
    /// `本次红冲 <= invoiced_total`），并按输入顺序报告每个账户的命中情况；
    /// 调用方（Service）负责将 `rejected` 转译为业务错误，失败时整个事务回滚，
    /// 不产生部分写入。本方法只执行计划，不判断红票业务资格。
    ///
    /// # 参数
    /// * `deltas` - 按子账聚合的红冲增量（已去重、首次出现顺序，同一账户只出现一次）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按账户报告命中情况的 [`InvoicingBatchResult`]（`applied` 为命中，
    /// `rejected` 为超过已开票进度被拒绝）。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    ///
    /// # 约束
    /// 聚合口径（同一账户增量求和、同一账户只更新一次）由 Service 经领域
    /// 计划保证；本方法不自行开启事务、不决定跨账户业务结论。
    pub async fn revert_invoicings_many(
        &self,
        deltas: &[(ReceivableAccountId, Amount)],
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<InvoicingBatchResult> {
        let mut applied = Vec::new();
        let mut rejected = Vec::new();
        for (id, amount) in deltas {
            let amount = amount_bson(amount)?;
            let filter = doc! {
                "id": id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "$expr": {
                    "$gte": ["$invoiced_total", &amount],
                },
            };
            let hit = self
                .conditional_update(
                    filter,
                    progress_pipeline(
                        "invoiced_total",
                        "open_invoiceable_total",
                        &amount,
                        false,
                        updated_by,
                    ),
                    executor,
                )
                .await?;
            if hit {
                applied.push(id.clone());
            } else {
                rejected.push(id.clone());
            }
        }
        Ok(InvoicingBatchResult { applied, rejected })
    }

    /// 执行单文档条件更新（管道形态）。
    ///
    /// 直接按执行器会话语义执行：带会话时加入调用方事务，否则自动提交；
    /// 仓储不自行开启或提交事务。
    ///
    /// # 参数
    /// * `filter` - 更新条件（含核销进度守卫）
    /// * `pipeline` - 聚合管道更新（重算进度与派生状态）
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 条件命中并完成更新时返回 `true`。
    ///
    /// # 错误
    /// 当 MongoDB 更新失败时返回错误。
    pub async fn conditional_update(
        &self,
        filter: Document,
        pipeline: Vec<Document>,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let result = match executor.session() {
            Some(session) => {
                self.collection()
                    .update_one(filter, pipeline)
                    .session(session)
                    .await?
            }
            None => self.collection().update_one(filter, pipeline).await?,
        };
        Ok(result.matched_count == 1)
    }
}

impl<'a> ReceivableAccountRepository<'a> {
    /// 按销售单读取全部活跃应收子账，供服务端关联列表投影使用。
    pub async fn find_accounts_by_sales_order_id(
        &self,
        sales_order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableAccount>> {
        self.find_many(doc! { "sales_order_id": sales_order_id }, executor)
            .await
    }

    /// 批量按应收子账 ID 读取活跃账户。
    ///
    /// # 参数
    /// * `account_ids` - 应收子账 ID 字符串集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的应收子账；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_accounts_by_ids(
        &self,
        account_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableAccount>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }

        self.find_many(doc! { "id": { "$in": account_ids } }, executor)
            .await
    }

    /// 列出销售单的全部应收子账。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按子账序号升序排列的应收子账。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `receivable_accounts` 集合，不访问销售单集合。
    pub async fn list_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableAccount>> {
        self.find_many_sorted(
            doc! { "sales_order_id": sales_order_id.to_string() },
            doc! { "account_seq": 1 },
            executor,
        )
        .await
    }

    /// 查找销售单的首个应收子账。
    ///
    /// # 参数
    /// * `sales_order_id` - 来源销售单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回 `account_seq = 1` 的应收子账；尚未形成时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `receivable_accounts` 集合，不访问销售单集合。
    pub async fn find_primary_by_sales_order(
        &self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReceivableAccount>> {
        self.find_one(
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "account_seq": 1,
            },
            executor,
        )
        .await
    }

    /// 按稳定 ID 读取票款复核任务使用的应收子账。
    ///
    /// 工作项入口的历史名称；纯主键读取，直接委托基类单条查询。
    ///
    /// # 参数
    /// * `id` - 应收子账 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回未删除应收子账；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `receivable_accounts` 集合，不访问销售单集合。
    pub async fn find_work_item_receivable_account(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReceivableAccount>> {
        self.find_by_id(id, executor).await
    }

    /// 按来源销售版本查找应收结果。
    ///
    /// # 参数
    /// * `revision_id` - 来源销售单修订 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回引用该销售版本的应收账户；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `receivable_accounts` 集合，按来源版本引用过滤，不访问销售单集合。
    pub async fn find_by_source_sales_order_revision(
        &self,
        revision_id: &SalesOrderRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReceivableAccount>> {
        self.find_one(
            doc! { "source_sales_order_revision_id": revision_id.to_string() },
            executor,
        )
        .await
    }
}

/// 将金额按 BSON Decimal128 形态转换（仓储层禁止任何舍入或换算）。
///
/// `bson::serialize_to_bson` 默认走 human-readable 字符串形态，与实体持久化的
/// Decimal128 形态不一致；这里直接构造 Decimal128，确保 `$add`/`$lte`
/// 等表达式与库内金额类型一致。
///
/// # 参数
/// * `amount` - 定点金额
///
/// # 返回
/// 返回 Decimal128 形态的 BSON 值。
///
/// # 错误
/// 金额无法表示为 Decimal128 时返回错误。
pub(super) fn amount_bson(amount: &Amount) -> Result<Bson> {
    Ok(Bson::Decimal128(amount.to_string().parse()?))
}

/// 构造条件开票的写前置条件（不超额开票）。
///
/// 以写条件而非读后判断保证 `invoiced_total + 本次开票 <= invoiceable_total`，
/// 不满足时整个更新不生效（matched 为 0）。
///
/// # 参数
/// * `id` - 应收往来子账 ID
/// * `amount` - 本次开票含税金额（已转为 Decimal128 形态）
///
/// # 返回
/// 返回未删除账户的开票额度守卫文档。
fn invoicing_guard(id: &str, amount: &Bson) -> Document {
    doc! {
        "id": id,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "$expr": {
            "$lte": [
                { "$add": ["$invoiced_total", amount] },
                "$invoiceable_total",
            ],
        },
    }
}

/// 构建核销/开票进度条件更新管道。
///
/// 在单条 MongoDB 原子更新内重算进度字段、开放余额与派生状态：
/// 增加方向 `progress = progress + amount`、`balance = total - progress`；
/// 减少方向 `progress = progress - amount`、`balance = total - progress`。
/// 状态仅由开放余额派生：增加后开放余额归零为 `settled`，减少后已核销归零为
/// `open`，其余为 `partially_settled`；开票进度不派生状态。
///
/// # 参数
/// * `progress_field` - 进度字段名（`settled_total` 或 `invoiced_total`）
/// * `balance_field` - 开放余额字段名（`open_total` 或 `open_invoiceable_total`）
/// * `amount` - 本次金额（正数）
/// * `increase` - `true` 为增加进度，`false` 为冲减进度
/// * `updated_by` - 本次更新执行人
///
/// # 返回
/// 返回聚合管道更新文档。
pub(super) fn progress_pipeline(
    progress_field: &str,
    balance_field: &str,
    amount: &Bson,
    increase: bool,
    updated_by: &str,
) -> Vec<Document> {
    let total_field = if progress_field == "settled_total" {
        "gross_total"
    } else {
        "invoiceable_total"
    };
    let new_progress = if increase {
        doc! { "$add": ["$" .to_owned() + progress_field, amount] }
    } else {
        doc! { "$subtract": ["$" .to_owned() + progress_field, amount] }
    };
    let new_balance = doc! { "$subtract": ["$" .to_owned() + total_field, &new_progress] };
    let mut set = doc! {
        progress_field: &new_progress,
        balance_field: &new_balance,
        "updated_by": updated_by,
        "version": { "$add": ["$version", 1] },
        "updated_at": chrono::Local::now().timestamp(),
    };
    if progress_field == "settled_total" {
        set.insert(
            "status",
            doc! {
                "$cond": [
                    { "$eq": [&new_balance, { "$toDecimal": "0" }] },
                    "settled",
                    {
                        "$cond": [
                            { "$eq": [&new_progress, { "$toDecimal": "0" }] },
                            "open",
                            "partially_settled",
                        ]
                    },
                ],
            },
        );
    }
    vec![doc! { "$set": set }]
}

/// 应收往来子账列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn receivable_account_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "current_revision_id": 1,
        "created_by": 1,
        "updated_by": 1,
        "sales_order_id": 1,
        "account_seq": 1,
        "customer_id": 1,
        "counterparty_party_id": 1,
        "review_status": 1,
        "gross_total": 1,
        "settled_total": 1,
        "open_total": 1,
        "invoiceable_total": 1,
        "invoiced_total": 1,
        "open_invoiceable_total": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{amount_bson, progress_pipeline, sort_doc, ReceivableAccountFilter};
    use crate::entity::receivable::ReceivableAccountStatus;
    use crate::repository::owned::ReceivableAccountRepository;
    use crate::repository::ReceivableExt;
    use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId};
    use erp_core::money::Amount;
    use mongodb::bson::{doc, Bson};
    use persistence_core::NoTransaction;
    use persistence_core::QueryFilter;
    use std::str::FromStr;

    #[test]
    fn account_filter_applies_optional_fields_and_deleted_filter() {
        let mut filter = ReceivableAccountFilter {
            keyword: None,
            keyword_sales_order_ids: Vec::new(),
            keyword_party_ids: Vec::new(),
            account_id: None,
            customer_id: Some(CustomerAccountId::new("cust-1")),
            counterparty_party_id: Some(PartyId::new("party-1")),
            status: Some(ReceivableAccountStatus::Open),
            sales_order_id: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("customer_id").unwrap(), "cust-1");
        assert_eq!(document.get_str("counterparty_party_id").unwrap(), "party-1");
        assert_eq!(document.get_str("status").unwrap(), "open");

        filter.keyword = Some("XS.1".to_string());
        filter.keyword_sales_order_ids = vec![SalesOrderId::new("sales-1")];
        filter.keyword_party_ids = vec![PartyId::new("party-1")];
        let searched = filter.to_doc();
        let alternatives = searched.get_array("$or").unwrap();
        assert_eq!(alternatives.len(), 6);
        assert_eq!(
            alternatives[4],
            Bson::Document(doc! { "sales_order_id": { "$in": ["sales-1"] } })
        );
        assert_eq!(
            alternatives[5],
            Bson::Document(doc! { "counterparty_party_id": { "$in": ["party-1"] } })
        );
        assert_eq!(searched.get_str("customer_id").unwrap(), "cust-1");
        assert_eq!(searched.get_str("status").unwrap(), "open");
        assert_eq!(searched.get_i64("deleted_at").unwrap(), 0);
    }

    #[test]
    fn sort_doc_maps_whitelisted_fields_and_falls_back() {
        assert_eq!(
            sort_doc(Some("amount"), true, &["amount", "received_at"]),
            doc! { "amount": 1 }
        );
        assert_eq!(
            sort_doc(Some("$where"), false, &["amount"]),
            doc! { "created_at": -1 }
        );
        assert_eq!(sort_doc(None, true, &[]), doc! { "created_at": 1 });
    }

    #[test]
    fn apply_pipeline_guards_status_and_keeps_decimal_fidelity() {
        let amount = Amount::from_str("100.50").unwrap();
        let pipeline = progress_pipeline(
            "settled_total",
            "open_total",
            &amount_bson(&amount).unwrap(),
            true,
            "admin-1",
        );

        let set = pipeline[0].get_document("$set").unwrap();
        let add = set
            .get_document("settled_total")
            .unwrap()
            .get_array("$add")
            .unwrap();
        assert_eq!(add[0], Bson::String("$settled_total".to_string()));
        assert!(matches!(add[1], Bson::Decimal128(_)));
        assert!(set.get_document("status").unwrap().get("$cond").is_some());
    }

    #[test]
    fn revert_pipeline_reduces_progress_without_status_cond_misuse() {
        let amount = Amount::from_str("50.00").unwrap();
        let pipeline = progress_pipeline(
            "invoiced_total",
            "open_invoiceable_total",
            &amount_bson(&amount).unwrap(),
            false,
            "sys",
        );

        let set = pipeline[0].get_document("$set").unwrap();
        assert!(set.contains_key("invoiced_total"));
        assert!(set.contains_key("open_invoiceable_total"));
        assert!(!set.contains_key("status"), "开票进度不派生状态");
    }

    #[test]
    fn revert_pipeline_derives_open_when_progress_reaches_zero() {
        let amount = Amount::from_str("1000.00").unwrap();
        let pipeline = progress_pipeline(
            "settled_total",
            "open_total",
            &amount_bson(&amount).unwrap(),
            false,
            "sys",
        );

        let set = pipeline[0].get_document("$set").unwrap();
        let cond = set.get_document("status").unwrap().get_array("$cond").unwrap();
        assert!(cond[0].as_document().unwrap().get_array("$eq").is_ok());
        assert_eq!(
            cond[1],
            Bson::String("settled".to_string()),
            "开放余额归零为已结清"
        );
        let nested = cond[2].as_document().unwrap().get_array("$cond").unwrap();
        assert!(nested[0].as_document().unwrap().get_array("$eq").is_ok());
        assert_eq!(nested[1], Bson::String("open".to_string()), "已核销归零为未结");
        assert_eq!(nested[2], Bson::String("partially_settled".to_string()));
    }

    /// 空输入批量回退直接成功且不访问数据库。
    #[tokio::test]
    async fn revert_invoicings_many_empty_input_returns_empty_without_db() {
        let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1")
            .await
            .expect("客户端句柄创建失败");
        let database = client.database("unused");
        let repository = ReceivableAccountRepository::new(
            &database,
            <mongodb::Database as ReceivableExt>::RECEIVABLE_ACCOUNTS,
        );
        let repository: ReceivableAccountRepository<'_> = repository;
        let result = repository
            .revert_invoicings_many(&[], "tester", &mut NoTransaction)
            .await
            .expect("空输入批量红冲必须成功");
        assert!(result.applied.is_empty());
        assert!(result.rejected.is_empty());
    }
}
