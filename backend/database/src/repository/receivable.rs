//! 域 D18 `receivable` 仓储：receivable_account、receivable_entry、
//! receivable_funds_review、receivable_entry_offset、customer_receipt、
//! receipt_allocation、invoice、sales_invoice_allocation。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类；本文件只补充域特有查询、
//! **条件核销进度更新**（原子写入口，不超额核销，P2 计划 §5）与跨集合多步骤
//! 事务写入入口。集合名常量统一从 `indexes::receivable` 导入。
//!
//! 正式事实集合（分录、复核、抵销、分配）过账后不可更新或删除，**不提供软删除
//! 方法**；`receivable_account` 与 `invoice` 是稳定主表类，可软删除与恢复。
//!
//! 筛选/行类型定义在本文件，经 `ReceivableExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::common::{stable::StableBase, time::BusinessDate, time::Instant};
use entities::ids::{
    CustomerAccountId, CustomerReceiptId, InvoiceId, PartyId, ReceivableAccountId, ReceivableEntryId,
};
use entities::money::Amount;
use entities::receivable::{
    AccountReviewStatus, CustomerReceipt, CustomerReceiptStatus, Invoice, InvoiceDirection, InvoiceKind,
    InvoiceStatus, ReceiptAllocation, ReceivableAccount, ReceivableAccountStatus, ReceivableEntry,
    ReceivableEntryOffset, ReceivableFundsReview, SalesInvoiceAllocation,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::ReceivableExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `receivable_entry` 集合名（单一来源：`ReceivableExt` 关联常量）。
const RECEIVABLE_ENTRIES: &str = <mongodb::Database as ReceivableExt>::RECEIVABLE_ENTRIES;

/// 应收账户最早到期日聚合行。
#[derive(Debug, Deserialize)]
struct AccountDueDateRow {
    /// 应收账户 ID。
    #[serde(rename = "_id")]
    account_id: String,
    /// 最早到期日。
    due_date: BusinessDate,
}
/// `receivable_funds_review` 集合名（单一来源：`ReceivableExt` 关联常量）。
const RECEIVABLE_FUNDS_REVIEWS: &str = <mongodb::Database as ReceivableExt>::RECEIVABLE_FUNDS_REVIEWS;
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

/// 应收往来子账列表筛选条件。
#[derive(Debug, Clone)]
pub struct ReceivableAccountFilter {
    /// 主键、销售单、客户或往来主体关键字；`None` 表示不筛选。
    pub keyword: Option<String>,
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
            let alternatives = ["id", "sales_order_id", "customer_id", "counterparty_party_id"]
                .into_iter()
                .map(|field| {
                    let mut alternative = Document::new();
                    insert_literal_regex_filter(&mut alternative, field, Some(keyword));
                    alternative
                })
                .collect::<Vec<_>>();
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

/// 客户回款单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerReceiptRow {
    /// 实体主键。
    pub id: String,
    /// 回款单状态。
    pub status: CustomerReceiptStatus,
    /// 回款单号。
    pub receipt_no: String,
    /// 实际付款往来主体。
    pub counterparty_party_id: String,
    /// 可选经营归属提示。
    pub customer_id: Option<String>,
    /// 实际到账时间（秒级时间戳）。
    pub received_at: Instant,
    /// 含税到账金额。
    pub amount: Amount,
    /// 银行流水或凭证引用。
    pub bank_reference: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 客户回款单列表筛选条件。
#[derive(Debug, Clone)]
pub struct CustomerReceiptFilter {
    /// 服务端关联投影解析出的回款单主键集合；`None` 表示不筛选。
    pub receipt_ids: Option<Vec<String>>,
    /// 回款单号模糊匹配；`None` 表示不筛选。
    pub receipt_no: Option<String>,
    /// 实际付款往来主体；`None` 表示不筛选。
    pub counterparty_party_id: Option<PartyId>,
    /// 回款单状态；`None` 表示不筛选。
    pub status: Option<CustomerReceiptStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for CustomerReceiptFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(receipt_ids) = &self.receipt_ids {
            filter.insert("id", doc! { "$in": receipt_ids });
        }
        insert_literal_regex_filter(&mut filter, "receipt_no", self.receipt_no.as_deref());
        if let Some(counterparty_party_id) = &self.counterparty_party_id {
            filter.insert("counterparty_party_id", counterparty_party_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for CustomerReceiptFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 发票列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceRow {
    /// 实体主键。
    pub id: String,
    /// 稳定公共字段（状态/版本归属/审计人）。
    #[serde(flatten)]
    pub stable: StableBase<InvoiceStatus>,
    /// 发票方向。
    pub invoice_direction: InvoiceDirection,
    /// 蓝红类型。
    pub invoice_kind: InvoiceKind,
    /// 客户或供应商。
    pub party_id: String,
    /// 发票代码。
    pub invoice_code: Option<String>,
    /// 发票号码。
    pub invoice_no: String,
    /// 开票日期（YYYY-MM-DD）。
    pub invoice_date: BusinessDate,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 发票尾差。
    pub rounding_adjustment_amount: Amount,
    /// 尾差原因。
    pub rounding_reason: Option<String>,
    /// 红票原蓝票。
    pub original_invoice_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 发票列表筛选条件。
#[derive(Debug, Clone)]
pub struct InvoiceFilter {
    /// 服务端关联投影解析出的发票主键集合；`None` 表示不筛选。
    pub invoice_ids: Option<Vec<String>>,
    /// 发票方向；`None` 表示不筛选。
    pub invoice_direction: Option<InvoiceDirection>,
    /// 蓝红类型；`None` 表示不筛选。
    pub invoice_kind: Option<InvoiceKind>,
    /// 客户或供应商；`None` 表示不筛选。
    pub party_id: Option<PartyId>,
    /// 发票号码模糊匹配；`None` 表示不筛选。
    pub invoice_no: Option<String>,
    /// 发票状态；`None` 表示不筛选。
    pub status: Option<InvoiceStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for InvoiceFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(invoice_ids) = &self.invoice_ids {
            filter.insert("id", doc! { "$in": invoice_ids });
        }
        if let Some(invoice_direction) = self.invoice_direction {
            filter.insert("invoice_direction", invoice_direction.as_str());
        }
        if let Some(invoice_kind) = self.invoice_kind {
            filter.insert("invoice_kind", invoice_kind.as_str());
        }
        if let Some(party_id) = &self.party_id {
            filter.insert("party_id", party_id.to_string());
        }
        insert_literal_regex_filter(&mut filter, "invoice_no", self.invoice_no.as_deref());
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for InvoiceFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ReceivableAccount> {
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
    async fn conditional_update(
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

impl<'a> Repository<'a, ReceivableAccount> {
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
}

impl<'a> Repository<'a, ReceivableEntry> {
    /// 按应收账户聚合最早正向分录到期日。
    ///
    /// # 参数
    /// * `account_ids` - 应收账户 ID；空集合不访问数据库
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 每个存在 Increase 分录的账户至多返回一个最早到期日；Decrease 与无分录账户不上表。
    ///
    /// # 错误
    /// MongoDB 聚合或反序列化失败时返回错误。
    pub async fn minimum_increase_due_dates_by_accounts(
        &self,
        account_ids: &[ReceivableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<std::collections::HashMap<String, BusinessDate>> {
        if account_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let ids = account_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let pipeline = minimum_due_dates_pipeline(ids);
        let rows = match executor.session() {
            Some(session) => {
                self.collection()
                    .aggregate(pipeline)
                    .with_type::<AccountDueDateRow>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            }
            None => {
                self.collection()
                    .aggregate(pipeline)
                    .with_type::<AccountDueDateRow>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            }
        };
        Ok(rows
            .into_iter()
            .map(|row| (row.account_id, row.due_date))
            .collect())
    }

    /// 批量按子账集合取回分录（`$in` 一次取回，禁止 N+1）。
    ///
    /// 用于账龄汇总与开票核销锁定；只返回未删除分录（事实类恒未删除）。
    ///
    /// # 参数
    /// * `account_ids` - 应收往来子账 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_entries_by_accounts(
        &self,
        account_ids: &[ReceivableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableEntry>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let account_ids: Vec<String> = account_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "receivable_account_id": { "$in": account_ids } }, executor)
            .await
    }

    /// 按子账取回全部分录（按来源序号升序）。
    ///
    /// # 参数
    /// * `account_id` - 应收往来子账 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `source_sequence` 升序的全部分录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_entries_by_account(
        &self,
        account_id: &ReceivableAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableEntry>> {
        self.find_many_sorted(
            doc! { "receivable_account_id": account_id.to_string() },
            doc! { "source_sequence": 1 },
            executor,
        )
        .await
    }
}

/// 构造应收正向分录最早到期日聚合管道。
fn minimum_due_dates_pipeline(account_ids: Vec<String>) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "receivable_account_id": { "$in": account_ids },
                "direction": entities::receivable::EntryDirection::Increase.as_str(),
            }
        },
        doc! {
            "$group": {
                "_id": "$receivable_account_id",
                "due_date": { "$min": "$due_date" },
            }
        },
        doc! { "$sort": { "_id": 1 } },
    ]
}

impl<'a> Repository<'a, ReceivableEntryOffset> {
    /// 按减少分录集合批量取回抵销记录。
    ///
    /// # 参数
    /// * `decrease_entry_ids` - 减少分录 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配抵销记录；调用方按分录分组后按抵销序号排序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_offsets_by_decreases(
        &self,
        decrease_entry_ids: &[ReceivableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableEntryOffset>> {
        if decrease_entry_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = decrease_entry_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        self.find_many(doc! { "decrease_entry_id": { "$in": ids } }, executor)
            .await
    }

    /// 按减少分录取回全部抵销（按抵销序号升序）。
    ///
    /// 用于校验「减少分录分配合计等于其金额」（数据模型 §6.8）。
    ///
    /// # 参数
    /// * `decrease_entry_id` - 减少分录 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `offset_sequence` 升序的抵销记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_offsets_by_decrease(
        &self,
        decrease_entry_id: &ReceivableEntryId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableEntryOffset>> {
        self.find_many_sorted(
            doc! { "decrease_entry_id": decrease_entry_id.to_string() },
            doc! { "offset_sequence": 1 },
            executor,
        )
        .await
    }

    /// 按增加分录取回被冲减的抵销集合。
    ///
    /// 用于校验「每笔增加分录累计净冲减不超过原增加金额」（数据模型 §6.8）。
    ///
    /// # 参数
    /// * `increase_entry_id` - 增加分录 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部引用该增加分录的抵销记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_offsets_by_increase(
        &self,
        increase_entry_id: &ReceivableEntryId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableEntryOffset>> {
        self.find_many(
            doc! { "increase_entry_id": increase_entry_id.to_string() },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, ReceivableFundsReview> {
    /// 按应收子账集合批量取回复核记录。
    ///
    /// # 参数
    /// * `account_ids` - 应收子账 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配复核记录；调用方按子账分组后按复核号排序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_reviews_by_accounts(
        &self,
        account_ids: &[ReceivableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableFundsReview>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = account_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many(doc! { "receivable_account_id": { "$in": ids } }, executor)
            .await
    }

    /// 按子账取回复核链全部记录（按复核号升序）。
    ///
    /// # 参数
    /// * `account_id` - 应收往来子账 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `review_no` 升序的复核记录；空链返回空集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_reviews_by_account(
        &self,
        account_id: &ReceivableAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceivableFundsReview>> {
        self.find_many_sorted(
            doc! { "receivable_account_id": account_id.to_string() },
            doc! { "review_no": 1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, CustomerReceipt> {
    /// 分页检索客户回款单列表（投影查询）。
    ///
    /// 只返回 [`CustomerReceiptRow`] 所需的列表字段；回款单号支持字面量
    /// 模糊匹配（复用 `regex_filter`，禁止自拼正则）。
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
    pub async fn search_customer_receipts(
        &self,
        filter: &CustomerReceiptFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<CustomerReceiptRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["received_at", "amount", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(customer_receipt_projection())
            .build();
        let collection = self.collection().clone_with_type::<CustomerReceiptRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按回款单号精确查找（单号全局唯一，`uk_customer_receipts_no` 保证）。
    ///
    /// # 参数
    /// * `receipt_no` - 回款单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的回款单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_receipt_no(
        &self,
        receipt_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<CustomerReceipt>> {
        self.find_one_by_field("receipt_no", receipt_no, executor).await
    }

    /// 批量按回款单 ID 读取活跃回款事实。
    ///
    /// # 参数
    /// * `receipt_ids` - 回款单 ID 字符串集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的回款单；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_receipts_by_ids(
        &self,
        receipt_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<CustomerReceipt>> {
        if receipt_ids.is_empty() {
            return Ok(Vec::new());
        }

        self.find_many(doc! { "id": { "$in": receipt_ids } }, executor)
            .await
    }
}

impl<'a> Repository<'a, ReceiptAllocation> {
    /// 批量按回款单集合取回核销分配（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `receipt_ids` - 回款单 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_receipts(
        &self,
        receipt_ids: &[CustomerReceiptId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceiptAllocation>> {
        if receipt_ids.is_empty() {
            return Ok(Vec::new());
        }
        let receipt_ids: Vec<String> = receipt_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "customer_receipt_id": { "$in": receipt_ids } }, executor)
            .await
    }

    /// 批量按应收分录集合取回核销分配（`$in`，用于反向核销锁定）。
    ///
    /// # 参数
    /// * `entry_ids` - 应收分录 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_entries(
        &self,
        entry_ids: &[ReceivableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ReceiptAllocation>> {
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }
        let entry_ids: Vec<String> = entry_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "receivable_entry_id": { "$in": entry_ids } }, executor)
            .await
    }
}

impl<'a> Repository<'a, Invoice> {
    /// 分页检索发票列表（投影查询）。
    ///
    /// 只返回 [`InvoiceRow`] 所需的列表字段；发票号码支持字面量模糊匹配。
    /// D19 的进项发票查询经本方法按 `invoice_direction = Purchase` 复用。
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
    pub async fn search_invoices(
        &self,
        filter: &InvoiceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<InvoiceRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["invoice_date", "gross_amount", "net_amount", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(invoice_projection())
            .build();
        let collection = self.collection().clone_with_type::<InvoiceRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「方向 + 规范化号码」查找发票（无代码数电票唯一键）。
    ///
    /// 唯一性由 `uk_invoices_uncoded` 部分唯一索引保证（有代码发票走
    /// `uk_invoices_coded`）；本方法用于登记前幂等判定与 D19 进项发票引用，
    /// 服务层不得做「先查后插」的重复性判断。
    ///
    /// # 参数
    /// * `invoice_direction` - 发票方向
    /// * `normalized_no` - 规范化发票号码（去空白转大写）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的发票；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_direction_and_normalized_no(
        &self,
        invoice_direction: InvoiceDirection,
        normalized_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Invoice>> {
        self.find_one(
            doc! {
                "invoice_direction": invoice_direction.as_str(),
                "normalized_no": normalized_no,
            },
            executor,
        )
        .await
    }

    /// 批量按发票 ID 读取活跃发票事实。
    ///
    /// # 参数
    /// * `invoice_ids` - 发票 ID 字符串集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的发票；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_invoices_by_ids(
        &self,
        invoice_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Invoice>> {
        if invoice_ids.is_empty() {
            return Ok(Vec::new());
        }

        self.find_many(doc! { "id": { "$in": invoice_ids } }, executor)
            .await
    }
}

impl<'a> Repository<'a, SalesInvoiceAllocation> {
    /// 批量按发票集合取回销项发票分配（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `invoice_ids` - 发票 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_invoices(
        &self,
        invoice_ids: &[InvoiceId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesInvoiceAllocation>> {
        if invoice_ids.is_empty() {
            return Ok(Vec::new());
        }
        let invoice_ids: Vec<String> = invoice_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "invoice_id": { "$in": invoice_ids } }, executor)
            .await
    }

    /// 批量按应收子账集合取回销项发票分配（`$in`，用于开票进度校验）。
    ///
    /// # 参数
    /// * `account_ids` - 应收往来子账 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_accounts(
        &self,
        account_ids: &[ReceivableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesInvoiceAllocation>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let account_ids: Vec<String> = account_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "receivable_account_id": { "$in": account_ids } }, executor)
            .await
    }
}

/// D18 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `ReceivableExt::receivable()` 访问。
pub struct ReceivableRepository<'a> {
    db: &'a Database,
}

impl<'a> ReceivableRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 建立应收往来子账与原始应收分录（跨集合多步骤写入）。
    ///
    /// 依次写入 `receivable_accounts` 与 `receivable_entries`，保证「子账 + 原始
    /// 应收」原子可见（数据模型 §6.8 销售单生效后才形成原始应收）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 两笔写入各自自动提交，第二笔失败会留下只有子账没有分录的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `account` - 待写入的应收往来子账
    /// * `entry` - 待写入的原始应收分录
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_receivable_with_entry(
        &self,
        account: &ReceivableAccount,
        entry: &ReceivableEntry,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<ReceivableAccount>(<mongodb::Database as ReceivableExt>::RECEIVABLE_ACCOUNTS),
            account,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<ReceivableEntry>(RECEIVABLE_ENTRIES),
            entry,
            executor,
        )
        .await?;
        Ok(())
    }

    /// 追加卡券票款正式复核（复核链尾锁定，跨集合读后写）。
    ///
    /// 复核链按数据模型 §6.8 逐号递增：`review_no = 1` 必须是链头（此前无任何
    /// 复核），`review_no > 1` 必须引用当前链尾且复核号连续。方法先读当前链尾
    /// （同子账最大 `review_no`）再插入新记录；链尾已被其他并发复核占用时
    /// 返回 [`crate::Error::OptimisticLockingError`]（链尾锁定失败），
    /// 并发写同号复核由 `uk_receivable_funds_reviews_account_no` 唯一索引兜底。
    /// **必须收到事务执行器**：读后写构成两步骤，传入 `NoTransaction` 时
    /// 链尾判定与插入各自自动提交，并发场景下可能读出旧链尾后插入失败留下
    /// 半个复核；Service 必须传入事务会话。
    ///
    /// # 参数
    /// * `review` - 待写入的复核记录（含链尾引用）
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 链尾不匹配时返回 [`crate::Error::OptimisticLockingError`]；
    /// 复核号重复时返回 [`crate::Error::DuplicateKey`]。
    pub async fn append_funds_review(
        &self,
        review: &ReceivableFundsReview,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let collection = self
            .db
            .collection::<ReceivableFundsReview>(RECEIVABLE_FUNDS_REVIEWS);
        let options = FindOptions::builder()
            .sort(doc! { "review_no": -1 })
            .limit(1)
            .build();
        let mut tail = mongo_ops::find_many(
            &collection,
            doc! { "receivable_account_id": review.receivable_account_id.to_string() },
            options,
            executor,
        )
        .await?;
        let tail = tail.pop();

        let chain_locked = match (&tail, review.review_no) {
            (None, 1) => true,
            (Some(tail), no) if no > 1 => {
                review.supersedes_review_id.as_ref().map(ToString::to_string) == Some(tail.base.id.clone())
                    && tail.review_no + 1 == no
            }
            _ => false,
        };
        if !chain_locked {
            return Err(crate::Error::OptimisticLockingError);
        }
        mongo_ops::insert_one(&collection, review, executor).await
    }

    /// 批量创建销项发票分配（`insert_many`，调用方事务内原子写入，FIN-R10）。
    ///
    /// 唯一键冲突，由 Service 转译并整体回滚。
    /// **必须收到事务执行器**：本方法不构成原子边界，Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `allocations` - 待持久化的销项发票分配
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 返回
    /// 全部写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_sales_invoice_allocations_many(
        &self,
        allocations: &[SalesInvoiceAllocation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(
            &self.db.collection::<SalesInvoiceAllocation>(
                <mongodb::Database as ReceivableExt>::SALES_INVOICE_ALLOCATIONS,
            ),
            allocations.to_vec(),
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
fn amount_bson(amount: &Amount) -> Result<Bson> {
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

/// 构建排序文档：字段名经白名单映射，未命中回退 `created_at` 降序。
///
/// # 参数
/// * `sort_by` - 排序字段（白名单内有效）
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段名集合（防止透传任意字段名）
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|name| allowed.contains(name))
        .unwrap_or("created_at");
    doc! { field: direction }
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
fn progress_pipeline(
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

/// 客户回款单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn customer_receipt_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "receipt_no": 1,
        "counterparty_party_id": 1,
        "customer_id": 1,
        "received_at": 1,
        "amount": 1,
        "bank_reference": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 发票列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn invoice_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "current_revision_id": 1,
        "created_by": 1,
        "updated_by": 1,
        "invoice_direction": 1,
        "invoice_kind": 1,
        "party_id": 1,
        "invoice_code": 1,
        "invoice_no": 1,
        "invoice_date": 1,
        "gross_amount": 1,
        "net_amount": 1,
        "tax_amount": 1,
        "rounding_adjustment_amount": 1,
        "rounding_reason": 1,
        "original_invoice_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        minimum_due_dates_pipeline, progress_pipeline, sort_doc, CustomerReceiptFilter, QueryFilter,
        ReceivableAccountFilter,
    };
    use entities::ids::{CustomerAccountId, PartyId};
    use entities::money::Amount;
    use entities::receivable::ReceivableAccountStatus;
    use mongodb::bson::doc;
    use mongodb::bson::Bson;
    use std::str::FromStr;

    #[test]
    fn minimum_due_date_pipeline_excludes_decrease_entries() {
        let pipeline = minimum_due_dates_pipeline(vec!["ra-1".to_string()]);
        let matched = pipeline[0].get_document("$match").unwrap();
        assert_eq!(matched.get_str("direction").unwrap(), "increase");
        let group = pipeline[1].get_document("$group").unwrap();
        assert_eq!(group.get_str("_id").unwrap(), "$receivable_account_id");
        assert_eq!(
            group.get_document("due_date").unwrap(),
            &doc! { "$min": "$due_date" }
        );
    }

    #[test]
    fn account_filter_applies_optional_fields_and_deleted_filter() {
        let filter = ReceivableAccountFilter {
            keyword: None,
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
    }

    #[test]
    fn receipt_filter_escapes_regex_literals() {
        let filter = CustomerReceiptFilter {
            receipt_ids: None,
            receipt_no: Some("RC-1.2".to_string()),
            counterparty_party_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        let regex = document.get_document("receipt_no").unwrap();
        assert_eq!(regex.get_str("$regex").unwrap(), r"RC\-1\.2");
        assert_eq!(regex.get_str("$options").unwrap(), "i");
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
            &super::amount_bson(&amount).unwrap(),
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
            &super::amount_bson(&amount).unwrap(),
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
            &super::amount_bson(&amount).unwrap(),
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
}
