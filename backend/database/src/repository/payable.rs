//! 域 D19 `payable` 仓储：payable_account、payable_entry、payable_entry_offset、
//! supplier_payment、payment_allocation、purchase_invoice_allocation。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类；本文件只补充域特有查询、
//! **条件核销进度更新**（原子写入口，不超额核销，P2 计划 §5）与跨集合多步骤
//! 事务写入入口。集合名常量统一从 `indexes::payable` 导入。
//!
//! 正式事实集合（分录、抵销、分配）过账后不可更新或删除，**不提供软删除方法**；
//! `payable_account` 是稳定主表类，可软删除与恢复。`invoice` 由 D18 拥有，
//! 本域只通过 `ReceivableExt::invoices()` 在 P3 复用。
//!
//! 筛选/行类型定义在本文件，经 `PayableExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::common::stable::StableBase;
use entities::ids::{PayableAccountId, PayableEntryId, SupplierAccountId};
use entities::money::Amount;
use entities::payable::{
    PayableAccount, PayableAccountStatus, PayableEntry, PayableEntryOffset, PayableSourceType,
    PaymentAllocation, PurchaseInvoiceAllocation, SupplierPayment, SupplierPaymentStatus,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::ser::SerializerOptions;
use mongodb::bson::{doc, to_bson_with_options, Bson, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::PayableExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `payable_entry` 集合名（单一来源：`PayableExt` 关联常量）。
const PAYABLE_ENTRIES: &str = <mongodb::Database as PayableExt>::PAYABLE_ENTRIES;

/// 应付往来子账列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayableAccountRow {
    /// 实体主键。
    pub id: String,
    /// 稳定公共字段（状态/版本归属/审计人）。
    #[serde(flatten)]
    pub stable: StableBase<PayableAccountStatus>,
    /// 来源单据 ID。
    pub source_document_id: String,
    /// 往来供应商。
    pub supplier_id: String,
    /// 来源类型。
    pub source_type: PayableSourceType,
    /// 含税应付总额。
    pub gross_total: Amount,
    /// 已核销含税总额。
    pub settled_total: Amount,
    /// 剩余开放含税余额。
    pub open_total: Amount,
    /// 可收票含税总额。
    pub invoiceable_total: Amount,
    /// 净已收票含税总额。
    pub invoiced_total: Amount,
    /// 剩余可收票含税额度。
    pub open_invoiceable_total: Amount,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 应付往来子账列表筛选条件。
#[derive(Debug, Clone)]
pub struct PayableAccountFilter {
    /// 往来供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 来源类型；`None` 表示不筛选。
    pub source_type: Option<PayableSourceType>,
    /// 子账状态；`None` 表示不筛选。
    pub status: Option<PayableAccountStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PayableAccountFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(source_type) = self.source_type {
            filter.insert("source_type", source_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for PayableAccountFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 供应商付款单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierPaymentRow {
    /// 实体主键。
    pub id: String,
    /// 付款单状态。
    pub status: SupplierPaymentStatus,
    /// 付款单号。
    pub payment_no: String,
    /// 收款供应商。
    pub supplier_id: String,
    /// 实际付款时间（秒级时间戳）。
    pub paid_at: u64,
    /// 含税付款金额。
    pub amount: Amount,
    /// 付款凭证引用。
    pub bank_reference: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商付款单列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierPaymentFilter {
    /// 付款单号模糊匹配；`None` 表示不筛选。
    pub payment_no: Option<String>,
    /// 收款供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 付款单状态；`None` 表示不筛选。
    pub status: Option<SupplierPaymentStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierPaymentFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "payment_no", self.payment_no.as_deref());
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SupplierPaymentFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, PayableAccount> {
    /// 分页检索应付往来子账列表（投影查询）。
    ///
    /// 只返回 [`PayableAccountRow`] 所需的列表字段，不加载整文档；
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
    pub async fn search_payable_accounts(
        &self,
        filter: &PayableAccountFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<PayableAccountRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &[
                    "gross_total",
                    "settled_total",
                    "open_total",
                    "open_invoiceable_total",
                    "created_at",
                ],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(payable_account_projection())
            .build();
        let collection = self.collection().clone_with_type::<PayableAccountRow>();
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
    /// * `id` - 应付往来子账 ID
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
    /// * `id` - 应付往来子账 ID
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

    /// 条件收票：增加净已收票进度（不超过可收票额度）。
    ///
    /// 进项蓝票 `APPLY` 的原子写入口：以写条件保证
    /// `invoiced_total + 本次收票 <= invoiceable_total`，不满足时整个更新不生效，
    /// 返回 `false`。同时重算 `open_invoiceable_total`，不会产生负可收票余额。
    ///
    /// # 参数
    /// * `id` - 应付往来子账 ID
    /// * `amount` - 本次收票含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 收票在额度内并已生效时返回 `true`；超过剩余可收票额度被拒绝时返回 `false`。
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

    /// 条件收票冲减：减少净已收票进度（不产生负已收票）。
    ///
    /// 进项红票 `REVERSE` 的原子写入口：以写条件保证 `本次红冲 <= invoiced_total`，
    /// 不满足时整个更新不生效，返回 `false`。累计红冲由 P3 登记事务结合
    /// `reverses_allocation_id` 校验，本方法防止已收票进度被冲成负数。
    ///
    /// # 参数
    /// * `id` - 应付往来子账 ID
    /// * `amount` - 本次红冲含税金额（正数）
    /// * `updated_by` - 本次更新执行人
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 红冲在已收票额度内并已生效时返回 `true`；超过已收票金额被拒绝时返回 `false`。
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

impl<'a> Repository<'a, PayableEntry> {
    /// 批量按子账集合取回分录（`$in` 一次取回，禁止 N+1）。
    ///
    /// 用于账龄汇总与付款核销锁定；只返回未删除分录（事实类恒未删除）。
    ///
    /// # 参数
    /// * `account_ids` - 应付往来子账 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_entries_by_accounts(
        &self,
        account_ids: &[PayableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableEntry>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let account_ids: Vec<String> = account_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "payable_account_id": { "$in": account_ids } }, executor)
            .await
    }

    /// 按子账取回全部分录（按来源序号升序）。
    ///
    /// # 参数
    /// * `account_id` - 应付往来子账 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `source_sequence` 升序的全部分录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_entries_by_account(
        &self,
        account_id: &PayableAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableEntry>> {
        self.find_many_sorted(
            doc! { "payable_account_id": account_id.to_string() },
            doc! { "source_sequence": 1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, PayableEntryOffset> {
    /// 按减少分录取回全部抵销（按抵销序号升序）。
    ///
    /// 用于校验「减少分录分配合计等于其金额」（数据模型 §6.9）。
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
        decrease_entry_id: &PayableEntryId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableEntryOffset>> {
        self.find_many_sorted(
            doc! { "decrease_entry_id": decrease_entry_id.to_string() },
            doc! { "offset_sequence": 1 },
            executor,
        )
        .await
    }

    /// 按增加分录取回被冲减的抵销集合。
    ///
    /// 用于校验「累计冲减不得超额」（数据模型 §6.9）。
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
        increase_entry_id: &PayableEntryId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PayableEntryOffset>> {
        self.find_many(
            doc! { "increase_entry_id": increase_entry_id.to_string() },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierPayment> {
    /// 分页检索供应商付款单列表（投影查询）。
    ///
    /// 只返回 [`SupplierPaymentRow`] 所需的列表字段；付款单号支持字面量
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
    pub async fn search_supplier_payments(
        &self,
        filter: &SupplierPaymentFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierPaymentRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
                &["paid_at", "amount", "created_at"],
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_payment_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierPaymentRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按付款单号精确查找（单号全局唯一，`uk_supplier_payments_no` 保证）。
    ///
    /// # 参数
    /// * `payment_no` - 付款单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的付款单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_payment_no(
        &self,
        payment_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierPayment>> {
        self.find_one_by_field("payment_no", payment_no, executor).await
    }
}

impl<'a> Repository<'a, PaymentAllocation> {
    /// 批量按付款单集合取回核销分配（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `payment_ids` - 付款单 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_payments(
        &self,
        payment_ids: &[entities::ids::SupplierPaymentId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentAllocation>> {
        if payment_ids.is_empty() {
            return Ok(Vec::new());
        }
        let payment_ids: Vec<String> = payment_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "supplier_payment_id": { "$in": payment_ids } }, executor)
            .await
    }

    /// 批量按应付分录集合取回核销分配（`$in`，用于反向核销锁定）。
    ///
    /// # 参数
    /// * `entry_ids` - 应付分录 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_entries(
        &self,
        entry_ids: &[PayableEntryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentAllocation>> {
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }
        let entry_ids: Vec<String> = entry_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "payable_entry_id": { "$in": entry_ids } }, executor)
            .await
    }
}

impl<'a> Repository<'a, PurchaseInvoiceAllocation> {
    /// 批量按发票集合取回进项发票分配（`$in` 一次取回，禁止 N+1）。
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
        invoice_ids: &[entities::ids::InvoiceId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseInvoiceAllocation>> {
        if invoice_ids.is_empty() {
            return Ok(Vec::new());
        }
        let invoice_ids: Vec<String> = invoice_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "invoice_id": { "$in": invoice_ids } }, executor)
            .await
    }

    /// 批量按应付子账集合取回进项发票分配（`$in`，用于收票进度校验）。
    ///
    /// # 参数
    /// * `account_ids` - 应付往来子账 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_accounts(
        &self,
        account_ids: &[PayableAccountId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseInvoiceAllocation>> {
        if account_ids.is_empty() {
            return Ok(Vec::new());
        }
        let account_ids: Vec<String> = account_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "payable_account_id": { "$in": account_ids } }, executor)
            .await
    }
}

/// D19 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `PayableExt::payable()` 访问。
pub struct PayableRepository<'a> {
    db: &'a Database,
}

impl<'a> PayableRepository<'a> {
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

    /// 建立应付往来子账与原始应付分录（跨集合多步骤写入）。
    ///
    /// 依次写入 `payable_accounts` 与 `payable_entries`，保证「子账 + 原始
    /// 应付」原子可见（数据模型 §6.9 采购单财务审核通过或供应商结算单确认后
    /// 才形成原始应付）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 两笔写入各自自动提交，第二笔失败会留下只有子账没有分录的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `account` - 待写入的应付往来子账
    /// * `entry` - 待写入的原始应付分录
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_payable_with_entry(
        &self,
        account: &PayableAccount,
        entry: &PayableEntry,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<PayableAccount>(<mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS),
            account,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<PayableEntry>(PAYABLE_ENTRIES),
            entry,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 将金额按 BSON Decimal128 形态转换（仓储层禁止任何舍入或换算）。
///
/// `bson::to_bson` 默认走 human-readable 字符串形态，与实体持久化的 Decimal128
/// 形态不一致，`$add`/`$lte` 等表达式会因类型不匹配而失败；这里显式关闭
/// human-readable 以保持与库内金额完全一致。
///
/// # 参数
/// * `amount` - 定点金额
///
/// # 返回
/// 返回 Decimal128 形态的 BSON 值。
///
/// # 错误
/// 金额序列化失败时返回错误。
#[allow(deprecated)] // bson 2.15 `human_readable` 设置项仍有效，替代方案 serde_helpers::HumanReadable 只适用于包装类型
fn amount_bson(amount: &Amount) -> Result<Bson> {
    let options = SerializerOptions::builder().human_readable(false).build();
    Ok(to_bson_with_options(amount, options)?)
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

/// 构建核销/收票进度条件更新管道。
///
/// 在单条 MongoDB 原子更新内重算进度字段、开放余额与派生状态：
/// 增加方向 `progress = progress + amount`、`balance = total - progress`；
/// 减少方向 `progress = progress - amount`、`balance = total - progress`。
/// 状态仅由开放余额派生：增加后开放余额归零为 `settled`，减少后已核销归零为
/// `open`，其余为 `partially_settled`；收票进度不派生状态。
///
/// # 参数
/// * `progress_field` - 进度字段名（`settled_total` 或 `invoiced_total`）
/// * `balance_field` - 开放余额字段名（`open_total` 或 `open_invoiceable_total`）
/// * `amount` - 本次金额（正数，Decimal128 形态）
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

/// 应付往来子账列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn payable_account_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "current_revision_id": 1,
        "created_by": 1,
        "updated_by": 1,
        "source_document_id": 1,
        "supplier_id": 1,
        "source_type": 1,
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

/// 供应商付款单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_payment_projection() -> Document {
    doc! {
        "id": 1,
        "status": 1,
        "payment_no": 1,
        "supplier_id": 1,
        "paid_at": 1,
        "amount": 1,
        "bank_reference": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        amount_bson, progress_pipeline, sort_doc, PayableAccountFilter, QueryFilter, SupplierPaymentFilter,
    };
    use entities::ids::SupplierAccountId;
    use entities::money::Amount;
    use entities::payable::{PayableAccountStatus, PayableSourceType};
    use mongodb::bson::{doc, Bson};
    use std::str::FromStr;

    #[test]
    fn account_filter_applies_optional_fields_and_deleted_filter() {
        let filter = PayableAccountFilter {
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            source_type: Some(PayableSourceType::PurchaseOrder),
            status: Some(PayableAccountStatus::Open),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("supplier_id").unwrap(), "sup-1");
        assert_eq!(document.get_str("source_type").unwrap(), "purchase_order");
        assert_eq!(document.get_str("status").unwrap(), "open");
    }

    #[test]
    fn payment_filter_escapes_regex_literals() {
        let filter = SupplierPaymentFilter {
            payment_no: Some("PAY-9.9".to_string()),
            supplier_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        let regex = document.get_document("payment_no").unwrap();
        assert_eq!(regex.get_str("$regex").unwrap(), r"PAY\-9\.9");
    }

    #[test]
    fn sort_doc_maps_whitelisted_fields_and_falls_back() {
        assert_eq!(
            sort_doc(Some("open_total"), true, &["open_total", "gross_total"]),
            doc! { "open_total": 1 }
        );
        assert_eq!(
            sort_doc(Some("status"), false, &["gross_total"]),
            doc! { "created_at": -1 }
        );
    }

    #[test]
    fn amount_bson_keeps_decimal128_fidelity() {
        let amount = Amount::from_str("1234.56").unwrap();
        assert!(matches!(amount_bson(&amount).unwrap(), Bson::Decimal128(_)));
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
        assert!(!set.contains_key("status"), "收票进度不派生状态");
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
}
