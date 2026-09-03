//! 域 D33 `supplier_settlement` 仓储：supplier_settlement_statement、
//! supplier_settlement_item、supplier_settlement_difference（页面：W27）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本文件只补充域特有
//! 查询与跨集合多步骤写入入口。集合名常量统一取 `SupplierSettlementExt` 关联常量
//! （唯一权威来源，indexes 与 Repository 两侧共用）。
//!
//! 结算明细是不可变正式结算事实行（只 `new` 不 `update`，§6.20），本域不为它提供
//! 软删除方法；结算单是正式单据，仍走基类软删除/恢复语义。
//!
//! 筛选/行类型定义在本文件，经 `SupplierSettlementExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    PayableAccountId, SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
    SupplierSettlementItemId,
};
use entities::supplier_fulfillment::{
    SupplierFulfillmentItem, SupplierFulfillmentOrder, SupplierRefundAllocation, SupplierRefundFact,
};
use entities::supplier_settlement::{
    SettlementDifferenceStatus, SettlementDifferenceType, SettlementPeriod, SettlementReviewResult,
    SettlementStatus, SupplierSettlementDifference, SupplierSettlementDifferenceEvidence,
    SupplierSettlementItem, SupplierSettlementSourceEvidence, SupplierSettlementStatement,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::extensions::{SupplierFulfillmentExt, SupplierSettlementExt};
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `supplier_settlement_statement` 集合名（单一来源：`SupplierSettlementExt` 关联常量）。
const SUPPLIER_SETTLEMENT_STATEMENTS: &str =
    <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS;
/// `supplier_settlement_item` 集合名（单一来源：`SupplierSettlementExt` 关联常量）。
const SUPPLIER_SETTLEMENT_ITEMS: &str =
    <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_ITEMS;
const SUPPLIER_SETTLEMENT_DIFFERENCES: &str =
    <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_DIFFERENCES;
const SUPPLIER_SETTLEMENT_DIFFERENCE_EVIDENCE: &str =
    <mongodb::Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_DIFFERENCE_EVIDENCE;
/// `supplier_fulfillment_order` 集合名（FUL-R06 结算来源范围读取）。
const SUPPLIER_FULFILLMENT_ORDERS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS;
/// `supplier_fulfillment_item` 集合名（FUL-R06 结算来源范围读取）。
const SUPPLIER_FULFILLMENT_ITEMS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS;
/// `supplier_refund_fact` 集合名（FUL-R06 结算来源范围读取）。
const SUPPLIER_REFUND_FACTS: &str = <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS;
/// `supplier_refund_allocation` 集合名（FUL-R06 结算来源范围读取）。
const SUPPLIER_REFUND_ALLOCATIONS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_ALLOCATIONS;

/// 结算单列表排序白名单（§6.20 查询索引支持的字段；白名单外一律回退 `created_at`）。
const STATEMENT_SORT_FIELDS: &[&str] = &["created_at", "period_start", "period_end", "confirmed_at"];
/// 结算明细列表排序白名单（白名单外一律回退 `created_at`）。
const ITEM_SORT_FIELDS: &[&str] = &["created_at", "erp_calculated_amount", "supplier_billed_amount"];
/// 结算差异列表排序白名单（白名单外一律回退 `created_at`）。
const DIFFERENCE_SORT_FIELDS: &[&str] = &["created_at", "difference_amount", "resolved_at"];

/// 供应商结算单列表投影行。
///
/// 列表接口只取必要字段，禁止返回整文档；金额以实体 `Amount`（Decimal128）原样
/// 透传，不做任何舍入或换算（P2 §2.4）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementStatementRow {
    /// 实体主键。
    pub id: String,
    /// ERP 结算单号。
    pub statement_no: String,
    /// 结算供应商。
    pub supplier_id: SupplierAccountId,
    /// 结算期间开始（含）。
    pub period_start: BusinessDate,
    /// 结算期间结束（含）。
    pub period_end: BusinessDate,
    /// 供应商结算期间策略。
    pub period_policy_id: String,
    /// 供应商结算期间策略版本。
    pub period_policy_version: String,
    /// 供应商结算期间策略时区。
    pub period_timezone: String,
    /// 供应商账单号。
    pub external_bill_no: Option<String>,
    /// 供应商账单版本。
    pub external_bill_version: Option<String>,
    /// ERP 金额。
    pub erp_amount: entities::money::Amount,
    /// 供应商金额。
    pub supplier_amount: entities::money::Amount,
    /// 双方金额差异（= 供应商金额 − ERP 金额）。
    pub difference_amount: entities::money::Amount,
    /// 结算状态。
    pub status: SettlementStatus,
    /// 正式复核主题摘要。
    pub subject_hash: String,
    /// 正式来源事实水位。
    pub source_as_of: Instant,
    /// 来源快照冻结时间。
    pub source_snapshot_at: Instant,
    /// 不可变来源快照摘要。
    pub source_snapshot_hash: String,
    /// 提交复核采用的刷新截止策略。
    pub refresh_cutoff_policy_id: String,
    /// 刷新截止策略冻结版本。
    pub refresh_cutoff_policy_version: String,
    /// 经办人。
    pub prepared_by: String,
    /// 复核人。
    pub reviewed_by: Option<String>,
    /// 最近一次正式复核决定。
    pub review_result: Option<SettlementReviewResult>,
    /// 最近一次驳回原因代码。
    pub review_reason_code: Option<String>,
    /// 最近一次复核说明。
    pub review_comment: Option<String>,
    /// 最近一次正式复核决定时间。
    pub reviewed_at: Option<Instant>,
    /// 确认时间。
    pub confirmed_at: Option<Instant>,
    /// 确认后形成的应付账户。
    pub payable_account_id: Option<PayableAccountId>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商结算单列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierSettlementStatementFilter {
    /// 结算单号（按字面量部分匹配，忽略大小写）；`None` 表示不筛选。
    pub statement_no: Option<String>,
    /// 结算供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 结算状态；`None` 表示不筛选。
    pub status: Option<SettlementStatus>,
    /// 结算期间开始下界（含）。
    pub period_from: Option<BusinessDate>,
    /// 结算期间结束上界（含）。
    pub period_to: Option<BusinessDate>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内生效，白名单外回退 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

/// 与结算单列表同一筛选水位计算的服务端汇总。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementStatementStatsRow {
    pub pending_reconciliation_count: i64,
    pub has_difference_count: i64,
    pub pending_review_count: i64,
    pub confirmed_amount: entities::money::Amount,
}

impl QueryFilter for SupplierSettlementStatementFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(period_from) = self.period_from {
            filter.insert("period_start", doc! { "$gte": period_from.to_string() });
        }
        if let Some(period_to) = self.period_to {
            filter.insert("period_end", doc! { "$lte": period_to.to_string() });
        }
        insert_literal_regex_filter(&mut filter, "statement_no", self.statement_no.as_deref());
        filter
    }
}

impl Pagination for SupplierSettlementStatementFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 供应商结算明细列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementItemRow {
    /// 实体主键。
    pub id: String,
    /// 所属结算单。
    pub statement_id: entities::ids::SupplierSettlementStatementId,
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: entities::ids::SupplierFulfillmentOrderId,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: entities::ids::SupplierFulfillmentItemId,
    /// 来源快照冻结数量。
    pub quantity: entities::money::Quantity,
    /// 订单结算金额。
    pub order_amount: entities::money::Amount,
    /// 运费金额。
    pub freight_amount: entities::money::Amount,
    /// 服务费金额。
    pub service_fee_amount: entities::money::Amount,
    /// 供应商退款金额。
    pub refund_amount: entities::money::Amount,
    /// ERP 计算含税金额。
    pub erp_calculated_amount: entities::money::Amount,
    /// ERP 计算不含税金额。
    pub erp_calculated_net_amount: entities::money::Amount,
    /// ERP 计算税额。
    pub erp_calculated_tax_amount: entities::money::Amount,
    /// 供应商账单含税金额。
    pub supplier_billed_amount: entities::money::Amount,
    /// 供应商账单不含税金额。
    pub supplier_billed_net_amount: entities::money::Amount,
    /// 供应商账单税额。
    pub supplier_billed_tax_amount: entities::money::Amount,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商结算明细列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierSettlementItemFilter {
    /// 所属结算单；`None` 表示不筛选。
    pub statement_id: Option<entities::ids::SupplierSettlementStatementId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内生效，白名单外回退 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierSettlementItemFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(statement_id) = &self.statement_id {
            filter.insert("statement_id", statement_id.to_string());
        }
        filter
    }
}

impl Pagination for SupplierSettlementItemFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 供应商结算差异列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementDifferenceRow {
    /// 实体主键。
    pub id: String,
    /// 所属结算明细。
    pub statement_item_id: SupplierSettlementItemId,
    /// 差异类型。
    pub difference_type: SettlementDifferenceType,
    /// 差异金额。
    pub difference_amount: entities::money::Amount,
    /// 差异状态。
    pub status: SettlementDifferenceStatus,
    /// 处理结果文本。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间。
    pub resolved_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商结算差异列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierSettlementDifferenceFilter {
    /// 所属结算明细；`None` 表示不筛选。
    pub statement_item_id: Option<SupplierSettlementItemId>,
    /// 差异状态；`None` 表示不筛选。
    pub status: Option<SettlementDifferenceStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内生效，白名单外回退 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierSettlementDifferenceFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(statement_item_id) = &self.statement_item_id {
            filter.insert("statement_item_id", statement_item_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SupplierSettlementDifferenceFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierSettlementStatement> {
    /// 按结算单 ID 集合批量读取结算单。
    ///
    /// # 参数
    /// * `statement_ids` - 结算单 ID 字符串集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的结算单；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_statements_by_ids(
        &self,
        statement_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementStatement>> {
        if statement_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": statement_ids } }, executor)
            .await
    }

    /// 分页检索供应商结算单列表（投影查询）。
    ///
    /// 只返回 [`SupplierSettlementStatementRow`] 所需的列表字段，不加载整文档；
    /// 排序字段走白名单映射（`STATEMENT_SORT_FIELDS`），白名单外一律回退 `created_at`。
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
    pub async fn search_supplier_settlement_statements(
        &self,
        filter: &SupplierSettlementStatementFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierSettlementStatementRow>> {
        let options = FindOptions::builder()
            .sort(statement_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_settlement_statement_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<SupplierSettlementStatementRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按 ERP 结算单号查找唯一结算单。
    ///
    /// 唯一性由 `uk_supplier_settlement_statements_statement_no` 唯一索引保证；
    /// 该方法用于结算单号幂等判定与外部账单回填定位，服务层不得做
    /// 「先查后插」的重复性判断（§6.20）。
    ///
    /// # 参数
    /// * `statement_no` - ERP 结算单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除结算单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_statement_no(
        &self,
        statement_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementStatement>> {
        self.find_one(doc! { "statement_no": statement_no }, executor)
            .await
    }

    /// 按稳定 ID 读取供应商结算岗位分离事实。
    ///
    /// 工作项入口的历史名称；纯主键读取，直接委托基类单条查询。
    ///
    /// # 参数
    /// * `id` - 供应商结算单 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回未删除结算单；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的结算单集合，不访问结算明细集合。
    pub async fn find_work_item_supplier_settlement(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementStatement>> {
        self.find_by_id(id, executor).await
    }

    /// 按列表完全相同的过滤条件计算跨页状态和确认金额汇总。
    pub async fn aggregate_supplier_settlement_statement_stats(
        &self,
        filter: &SupplierSettlementStatementFilter,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementStatementStatsRow>> {
        let pipeline = vec![
            doc! { "$match": filter.to_doc() },
            doc! {
                "$group": {
                    "_id": mongodb::bson::Bson::Null,
                    "pending_reconciliation_count": {
                        "$sum": { "$cond": [{ "$eq": ["$status", "PENDING_RECONCILIATION"] }, 1, 0] }
                    },
                    "has_difference_count": {
                        "$sum": { "$cond": [{ "$eq": ["$status", "HAS_DIFFERENCE"] }, 1, 0] }
                    },
                    "pending_review_count": {
                        "$sum": { "$cond": [{ "$eq": ["$status", "PENDING_REVIEW"] }, 1, 0] }
                    },
                    "confirmed_amount": {
                        "$sum": {
                            "$toDecimal": {
                                "$cond": [{ "$eq": ["$status", "CONFIRMED"] }, "$erp_amount", "0.00"]
                            }
                        }
                    }
                }
            },
            doc! { "$project": { "_id": 0 } },
        ];
        let collection = self.collection();
        let rows = match executor.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<SupplierSettlementStatementStatsRow>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            }
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<SupplierSettlementStatementStatsRow>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            }
        };
        Ok(rows.into_iter().next())
    }
}

impl<'a> Repository<'a, SupplierSettlementSourceEvidence> {
    /// 按稳定请求 ID 查找不可变来源证据批次。
    pub async fn find_by_request_id(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementSourceEvidence>> {
        self.find_one(doc! { "request_id": request_id }, executor).await
    }

    /// 读取供应商、周期与策略版本下最新的完整来源证据批次。
    pub async fn latest_for_period(
        &self,
        supplier_id: &SupplierAccountId,
        period_start: BusinessDate,
        period_end: BusinessDate,
        period_policy_id: &str,
        period_policy_version: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementSourceEvidence>> {
        let options = FindOptions::builder()
            .sort(doc! { "source_version": -1, "created_at": -1, "id": -1 })
            .limit(1)
            .build();
        let mut values = mongo_ops::find_many(
            &self.collection(),
            doc! {
                "supplier_id": supplier_id.to_string(),
                "period_start": period_start.to_string(),
                "period_end": period_end.to_string(),
                "period_policy_id": period_policy_id,
                "period_policy_version": period_policy_version,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await?;
        Ok(values.pop())
    }

    /// 读取供应商与周期下最近登记的完整来源证据，用于创建前服务端预检。
    pub async fn latest_for_scope(
        &self,
        supplier_id: &SupplierAccountId,
        period_start: BusinessDate,
        period_end: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementSourceEvidence>> {
        let options = FindOptions::builder()
            .sort(doc! { "created_at": -1, "source_version": -1, "id": -1 })
            .limit(1)
            .build();
        let mut values = mongo_ops::find_many(
            &self.collection(),
            doc! {
                "supplier_id": supplier_id.to_string(),
                "period_start": period_start.to_string(),
                "period_end": period_end.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await?;
        Ok(values.pop())
    }

    /// 按冻结来源摘要批量读取不可变来源证据。
    ///
    /// # 参数
    /// * `source_hashes` - 结算单持有的来源快照摘要
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回匹配且未软删除的来源证据批次。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_by_source_hashes(
        &self,
        source_hashes: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementSourceEvidence>> {
        if source_hashes.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many_sorted(
            doc! { "source_hash": { "$in": source_hashes } },
            doc! { "source_version": -1, "id": 1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierSettlementDifferenceEvidence> {
    /// 按稳定请求 ID 查找不可变差异补证。
    pub async fn find_by_request_id(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementDifferenceEvidence>> {
        self.find_one(doc! { "request_id": request_id }, executor).await
    }

    /// 批量读取差异对应的全部补证，避免详情 N+1。
    pub async fn find_by_difference_ids(
        &self,
        difference_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementDifferenceEvidence>> {
        if difference_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many_sorted(
            doc! { "difference_id": { "$in": difference_ids } },
            doc! { "provided_at": 1, "id": 1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierSettlementItem> {
    /// 按结算单读取全部冻结明细，按创建时间和主键升序排列。
    ///
    /// # 参数
    /// * `statement_id` - 供应商结算单主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该结算单的全部未删除冻结明细。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_statement(
        &self,
        statement_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementItem>> {
        self.find_many_sorted(
            doc! { "statement_id": statement_id },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 按结算单主键批量读取全部冻结明细。
    ///
    /// # 参数
    /// * `statement_ids` - 已授权结算单主键集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按结算单、创建时间和主键稳定排序的冻结明细。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_by_statement_ids(
        &self,
        statement_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementItem>> {
        if statement_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many_sorted(
            doc! { "statement_id": { "$in": statement_ids } },
            doc! { "statement_id": 1, "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 分页检索供应商结算明细列表（投影查询）。
    ///
    /// 只返回 [`SupplierSettlementItemRow`] 所需的列表字段，不加载整文档；
    /// 排序字段走白名单映射（`ITEM_SORT_FIELDS`），白名单外一律回退 `created_at`。
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
    pub async fn search_supplier_settlement_items(
        &self,
        filter: &SupplierSettlementItemFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierSettlementItemRow>> {
        let options = FindOptions::builder()
            .sort(item_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_settlement_item_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierSettlementItemRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, SupplierSettlementDifference> {
    /// 按结算明细批量读取差异，按创建时间和主键升序排列。
    ///
    /// # 参数
    /// * `statement_item_ids` - 结算明细主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回关联这些结算明细的全部未删除差异。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_statement_item_ids(
        &self,
        statement_item_ids: &[SupplierSettlementItemId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementDifference>> {
        if statement_item_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many_sorted(
            doc! {
                "statement_item_id": {
                    "$in": statement_item_ids.iter().map(ToString::to_string).collect::<Vec<_>>()
                }
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 分页检索供应商结算差异列表（投影查询）。
    ///
    /// 只返回 [`SupplierSettlementDifferenceRow`] 所需的列表字段，不加载整文档；
    /// 排序字段走白名单映射（`DIFFERENCE_SORT_FIELDS`），白名单外一律回退 `created_at`。
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
    pub async fn search_supplier_settlement_differences(
        &self,
        filter: &SupplierSettlementDifferenceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierSettlementDifferenceRow>> {
        let options = FindOptions::builder()
            .sort(difference_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_settlement_difference_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<SupplierSettlementDifferenceRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 供应商结算来源范围的最小事实快照。
///
/// 只包含请求显式引用的订单/明细、期间内服务端可枚举的完成订单与退款事实及其
/// 分配（FUL-R06）；全部按主键稳定排序，不承诺跨集合关联顺序。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupplierSettlementSourceScope {
    /// 范围订单（请求引用 ∪ 期间内完成 ∪ 期间内退款事实所属）。
    pub orders: Vec<SupplierFulfillmentOrder>,
    /// 范围订单的明细与请求显式引用的明细。
    pub items: Vec<SupplierFulfillmentItem>,
    /// 期间内退款事实头。
    pub refund_facts: Vec<SupplierRefundFact>,
    /// 期间内退款事实头的全部分配。
    pub refund_allocations: Vec<SupplierRefundAllocation>,
}

/// D33 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `SupplierSettlementExt::supplier_settlement()` 访问。
pub struct SupplierSettlementRepository<'a> {
    db: &'a Database,
}

impl<'a> SupplierSettlementRepository<'a> {
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

    /// 原子创建结算单与全部结算明细。
    ///
    /// 依次写入 `supplier_settlement_statements` 与 `supplier_settlement_items`，
    /// 保证「结算单 + 明细」同事务可见（§6.20：完成、取消和退款事实均参与结算，
    /// 结算单确认与成本差额、应付账户及原始应付分录在同一事务完成，P3 编排）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有结算单没有明细的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `statement` - 待写入的结算单
    /// * `items` - 待写入的全部结算明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_statement_with_items(
        &self,
        statement: &SupplierSettlementStatement,
        items: &[SupplierSettlementItem],
        differences: &[SupplierSettlementDifference],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SupplierSettlementStatement>(SUPPLIER_SETTLEMENT_STATEMENTS),
            statement,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SupplierSettlementItem>(SUPPLIER_SETTLEMENT_ITEMS),
            items.to_vec(),
            executor,
        )
        .await?;
        if !differences.is_empty() {
            mongo_ops::insert_many(
                &self
                    .db
                    .collection::<SupplierSettlementDifference>(SUPPLIER_SETTLEMENT_DIFFERENCES),
                differences.to_vec(),
                executor,
            )
            .await?;
        }
        Ok(())
    }

    /// 原子替换尚未提交复核的草稿快照。
    ///
    /// 旧明细与差异仅属于可变草稿试算；服务层在事务内重验版本和状态后物理替换，
    /// 旧差异补证随其草稿差异一并移除，新快照及审计同时可见。已提交复核或终态
    /// 不得调用。
    pub async fn replace_draft_snapshot(
        &self,
        statement: &mut SupplierSettlementStatement,
        old_item_ids: &[String],
        old_difference_ids: &[String],
        items: &[SupplierSettlementItem],
        differences: &[SupplierSettlementDifference],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !old_difference_ids.is_empty() {
            mongo_ops::delete_many(
                &self.db.collection::<SupplierSettlementDifferenceEvidence>(
                    SUPPLIER_SETTLEMENT_DIFFERENCE_EVIDENCE,
                ),
                doc! { "difference_id": { "$in": old_difference_ids } },
                executor,
            )
            .await?;
        }
        if !old_item_ids.is_empty() {
            mongo_ops::delete_many(
                &self
                    .db
                    .collection::<SupplierSettlementDifference>(SUPPLIER_SETTLEMENT_DIFFERENCES),
                doc! { "statement_item_id": { "$in": old_item_ids } },
                executor,
            )
            .await?;
        }
        mongo_ops::delete_many(
            &self
                .db
                .collection::<SupplierSettlementItem>(SUPPLIER_SETTLEMENT_ITEMS),
            doc! { "statement_id": &statement.base.id },
            executor,
        )
        .await?;
        super::Repository::new(self.db, SUPPLIER_SETTLEMENT_STATEMENTS)
            .update(statement, executor)
            .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SupplierSettlementItem>(SUPPLIER_SETTLEMENT_ITEMS),
            items.to_vec(),
            executor,
        )
        .await?;
        if !differences.is_empty() {
            mongo_ops::insert_many(
                &self
                    .db
                    .collection::<SupplierSettlementDifference>(SUPPLIER_SETTLEMENT_DIFFERENCES),
                differences.to_vec(),
                executor,
            )
            .await?;
        }
        Ok(())
    }

    /// 读取供应商结算来源范围的最小事实快照。
    ///
    /// 来源证据核验只需三类正式事实：请求显式引用的订单与明细、期间内服务端可
    /// 枚举的完成订单、期间内退款事实及其分配。本方法把这三类读取下沉为有界
    /// 查询，不再按供应商全量历史订单/明细/退款做无界读取；完成与退款边界由
    /// 领域 `SettlementPeriod::secs_bounds` 唯一提供（与 `contains` 同口径：
    /// 开始日 `00:00`（`Asia/Shanghai`）含，结束日次日 `00:00` 不含），本层
    /// 不再复制第二份边界计算。软删除过滤由基类自动追加，与本域其他读取一致。
    ///
    /// # 参数
    /// * `supplier_id` - 结算供应商
    /// * `period_start` - 结算期间开始（含）
    /// * `period_end` - 结算期间结束（含）
    /// * `requested_order_ids` - 来源命令显式引用的履约订单主键
    /// * `requested_item_ids` - 来源命令显式引用的履约明细主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按主键稳定排序的订单、明细、退款事实与分配快照。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn settlement_source_scope(
        &self,
        supplier_id: &SupplierAccountId,
        period_start: BusinessDate,
        period_end: BusinessDate,
        requested_order_ids: &[SupplierFulfillmentOrderId],
        requested_item_ids: &[SupplierFulfillmentItemId],
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementSourceScope> {
        let (start_secs, end_secs) = SettlementPeriod::secs_bounds(period_start, period_end);
        let refund_facts: Vec<SupplierRefundFact> = super::Repository::new(self.db, SUPPLIER_REFUND_FACTS)
            .find_many_sorted(
                refund_fact_scope_filter(supplier_id, start_secs, end_secs),
                doc! { "id": 1 },
                executor,
            )
            .await?;
        let mut order_ids = requested_order_ids
            .iter()
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        order_ids.extend(
            refund_facts
                .iter()
                .map(|fact| fact.supplier_fulfillment_order_id.to_string()),
        );
        let mut orders: Vec<SupplierFulfillmentOrder> =
            super::Repository::new(self.db, SUPPLIER_FULFILLMENT_ORDERS)
                .find_many_sorted(
                    order_scope_filter(supplier_id, &order_ids, start_secs, end_secs),
                    doc! { "id": 1 },
                    executor,
                )
                .await?;
        let fetched_order_ids = orders
            .iter()
            .map(|order| order.base.id.clone())
            .collect::<BTreeSet<_>>();
        let items: Vec<SupplierFulfillmentItem> = super::Repository::new(self.db, SUPPLIER_FULFILLMENT_ITEMS)
            .find_many_sorted(
                item_scope_filter(&fetched_order_ids, requested_item_ids),
                doc! { "id": 1 },
                executor,
            )
            .await?;
        // 请求按明细主键引用、但订单不在范围集合内的明细（例如只按明细引用
        // 供应商既有订单），其订单仍需取回，保证 Service 的订单归属与来源行
        // 派生可以完成；跨供应商或不存在的订单在此查询下取不回，由 Service
        // 的完整性检查 fail-closed。
        let missing_order_ids = items
            .iter()
            .map(|item| item.supplier_fulfillment_order_id.to_string())
            .filter(|order_id| !fetched_order_ids.contains(order_id))
            .collect::<BTreeSet<_>>();
        if !missing_order_ids.is_empty() {
            let extra = super::Repository::new(self.db, SUPPLIER_FULFILLMENT_ORDERS)
                .find_many_sorted(
                    doc! {
                        "supplier_id": supplier_id.to_string(),
                        "id": { "$in": missing_order_ids.into_iter().collect::<Vec<_>>() },
                    },
                    doc! { "id": 1 },
                    executor,
                )
                .await?;
            orders.extend(extra);
            orders.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        }
        let fact_ids = refund_facts
            .iter()
            .map(|fact| fact.base.id.clone())
            .collect::<Vec<_>>();
        let refund_allocations = if fact_ids.is_empty() {
            Vec::new()
        } else {
            super::Repository::new(self.db, SUPPLIER_REFUND_ALLOCATIONS)
                .find_many_sorted(
                    doc! { "supplier_refund_fact_id": { "$in": fact_ids } },
                    doc! { "id": 1 },
                    executor,
                )
                .await?
        };
        Ok(SupplierSettlementSourceScope {
            orders,
            items,
            refund_facts,
            refund_allocations,
        })
    }
}

/// 构建期间内退款事实的查询条件。
///
/// 只按供应商与退款时间过滤；软删除过滤由基类自动追加。
///
/// # 参数
/// * `supplier_id` - 结算供应商
/// * `start_secs` - 期间开始秒级时间戳（含）
/// * `end_secs` - 期间结束次日零点的秒级时间戳（不含）
///
/// # 返回
/// 返回退款事实查询条件文档。
fn refund_fact_scope_filter(supplier_id: &SupplierAccountId, start_secs: i64, end_secs: i64) -> Document {
    doc! {
        "supplier_id": supplier_id.to_string(),
        "refunded_at": { "$gte": start_secs, "$lt": end_secs },
    }
}

/// 构建结算来源范围的履约订单查询条件。
///
/// 取「请求显式引用 ∪ 期间内完成 ∪ 期间内退款事实所属」的订单并集；订单归属
/// 校验保留给 Service。第三分支只命中「已完成但缺少完成时间」的损坏行：正常
/// 写入路径由实体构造器排除该形态，只有直接改库才能产生；若不纳入，其明细会
/// 静默漏出完整性枚举（fail-open）。纳入后 Service 的 `confirmed_completed_at`
/// 校验会对这类订单 fail-closed。
///
/// # 参数
/// * `supplier_id` - 结算供应商
/// * `order_ids` - 请求引用与期间内退款事实推导出的订单主键集合
/// * `start_secs` - 期间开始秒级时间戳（含）
/// * `end_secs` - 期间结束次日零点的秒级时间戳（不含）
///
/// # 返回
/// 返回履约订单查询条件文档。
fn order_scope_filter(
    supplier_id: &SupplierAccountId,
    order_ids: &BTreeSet<String>,
    start_secs: i64,
    end_secs: i64,
) -> Document {
    doc! {
        "supplier_id": supplier_id.to_string(),
        "$or": [
            { "id": { "$in": order_ids.iter().cloned().collect::<Vec<_>>() } },
            { "completed_at": { "$gte": start_secs, "$lt": end_secs } },
            { "fulfillment_status": "COMPLETED", "completed_at": null },
        ],
    }
}

/// 构建结算来源范围的履约明细查询条件。
///
/// 取「范围订单的全部明细 ∪ 请求显式引用的明细」；明细归属校验保留给 Service。
///
/// # 参数
/// * `order_ids` - 已取回订单的主键集合
/// * `requested_item_ids` - 来源命令显式引用的履约明细主键
///
/// # 返回
/// 返回履约明细查询条件文档。
fn item_scope_filter(
    order_ids: &BTreeSet<String>,
    requested_item_ids: &[SupplierFulfillmentItemId],
) -> Document {
    doc! {
        "$or": [
            { "supplier_fulfillment_order_id": { "$in": order_ids.iter().cloned().collect::<Vec<_>>() } },
            { "id": { "$in": requested_item_ids.iter().map(ToString::to_string).collect::<Vec<_>>() } },
        ],
    }
}

/// 构建结算单排序文档（白名单映射，禁止透传任意字段名）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn statement_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    sort_doc(STATEMENT_SORT_FIELDS, sort_by, sort_ascending)
}

/// 构建结算明细排序文档（白名单映射，禁止透传任意字段名）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn item_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    sort_doc(ITEM_SORT_FIELDS, sort_by, sort_ascending)
}

/// 构建结算差异排序文档（白名单映射，禁止透传任意字段名）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn difference_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    sort_doc(DIFFERENCE_SORT_FIELDS, sort_by, sort_ascending)
}

/// 构建白名单排序文档。
///
/// # 参数
/// * `whitelist` - 允许的排序字段集合
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(whitelist: &[&str], sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|field| whitelist.contains(field))
        .unwrap_or("created_at");
    doc! { field: direction }
}

/// 供应商结算单列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_settlement_statement_projection() -> Document {
    doc! {
        "id": 1,
        "statement_no": 1,
        "supplier_id": 1,
        "period_start": 1,
        "period_end": 1,
        "period_policy_id": 1,
        "period_policy_version": 1,
        "period_timezone": 1,
        "external_bill_no": 1,
        "external_bill_version": 1,
        "erp_amount": 1,
        "supplier_amount": 1,
        "difference_amount": 1,
        "status": 1,
        "subject_hash": 1,
        "source_as_of": 1,
        "source_snapshot_at": 1,
        "source_snapshot_hash": 1,
        "refresh_cutoff_policy_id": 1,
        "refresh_cutoff_policy_version": 1,
        "prepared_by": 1,
        "reviewed_by": 1,
        "review_result": 1,
        "review_reason_code": 1,
        "review_comment": 1,
        "reviewed_at": 1,
        "confirmed_at": 1,
        "payable_account_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 供应商结算明细列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_settlement_item_projection() -> Document {
    doc! {
        "id": 1,
        "statement_id": 1,
        "supplier_fulfillment_order_id": 1,
        "supplier_fulfillment_item_id": 1,
        "quantity": 1,
        "order_amount": 1,
        "freight_amount": 1,
        "service_fee_amount": 1,
        "refund_amount": 1,
        "erp_calculated_amount": 1,
        "erp_calculated_net_amount": 1,
        "erp_calculated_tax_amount": 1,
        "supplier_billed_amount": 1,
        "supplier_billed_net_amount": 1,
        "supplier_billed_tax_amount": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 供应商结算差异列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_settlement_difference_projection() -> Document {
    doc! {
        "id": 1,
        "statement_item_id": 1,
        "difference_type": 1,
        "difference_amount": 1,
        "status": 1,
        "resolution": 1,
        "resolved_by": 1,
        "resolved_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{
        order_scope_filter, sort_doc, supplier_settlement_statement_projection, QueryFilter,
        SupplierSettlementDifferenceFilter, SupplierSettlementStatementFilter,
    };
    use entities::ids::SupplierSettlementItemId;
    use entities::supplier_settlement::{SettlementDifferenceStatus, SettlementPeriod, SettlementStatus};
    use mongodb::bson::doc;

    #[test]
    fn statement_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SupplierSettlementStatementFilter {
            statement_no: Some("ST-2026".to_string()),
            supplier_id: None,
            status: Some(SettlementStatus::Confirmed),
            period_from: None,
            period_to: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("status").unwrap(), "CONFIRMED");
        assert_eq!(
            document
                .get_document("statement_no")
                .unwrap()
                .get_str("$regex")
                .unwrap(),
            r"ST\-2026"
        );
    }

    #[test]
    fn difference_filter_applies_statement_item_and_status() {
        let filter = SupplierSettlementDifferenceFilter {
            statement_item_id: Some(SupplierSettlementItemId::new("settlement-item-1")),
            status: Some(SettlementDifferenceStatus::Pending),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(
            document.get_str("statement_item_id").unwrap(),
            "settlement-item-1"
        );
        assert_eq!(document.get_str("status").unwrap(), "PENDING");
    }

    #[test]
    fn settlement_sort_doc_rejects_fields_outside_whitelist() {
        let whitelist = ["created_at", "period_start", "confirmed_at"];
        assert_eq!(sort_doc(&whitelist, None, false), doc! { "created_at": -1 });
        assert_eq!(
            sort_doc(&whitelist, Some("status"), false),
            doc! { "created_at": -1 },
            "白名单外的排序字段必须回退 created_at"
        );
        assert_eq!(
            sort_doc(&whitelist, Some("period_start"), true),
            doc! { "period_start": 1 }
        );
        assert_eq!(
            sort_doc(&whitelist, Some("confirmed_at"), false),
            doc! { "confirmed_at": -1 }
        );
    }

    #[test]
    fn statement_projection_contains_frozen_review_and_audit_facts() {
        let projection = supplier_settlement_statement_projection();
        for field in [
            "subject_hash",
            "source_as_of",
            "source_snapshot_at",
            "source_snapshot_hash",
            "refresh_cutoff_policy_id",
            "refresh_cutoff_policy_version",
            "review_result",
            "review_reason_code",
            "review_comment",
            "reviewed_at",
        ] {
            assert_eq!(projection.get_i32(field).unwrap(), 1, "缺少字段 {field}");
        }
    }

    #[test]
    fn settlement_period_bounds_use_shanghai_inclusive_start_exclusive_end() {
        use chrono::DateTime;
        use entities::common::time::BusinessDate;

        // 边界口径必须来自领域 SettlementPeriod::secs_bounds（与 contains 同源），
        // 仓储不再复制第二份计算。
        for (start_text, end_text, first_secs_text, last_secs_text) in [
            (
                "2026-07-01",
                "2026-07-31",
                "2026-07-01T00:00:00+08:00",
                "2026-08-01T00:00:00+08:00",
            ),
            (
                "2025-12-01",
                "2026-02-28",
                "2025-12-01T00:00:00+08:00",
                "2026-03-01T00:00:00+08:00",
            ),
            (
                "2028-02-01",
                "2028-02-29",
                "2028-02-01T00:00:00+08:00",
                "2028-03-01T00:00:00+08:00",
            ),
        ] {
            let start = BusinessDate::from_str(start_text).unwrap();
            let end = BusinessDate::from_str(end_text).unwrap();
            let (start_secs, end_secs) = SettlementPeriod::secs_bounds(start, end);
            assert_eq!(
                start_secs,
                DateTime::parse_from_rfc3339(first_secs_text).unwrap().timestamp(),
                "{start_text} 开始边界错误"
            );
            assert_eq!(
                end_secs,
                DateTime::parse_from_rfc3339(last_secs_text).unwrap().timestamp(),
                "{start_text} 结束边界错误"
            );
        }
        let start = BusinessDate::from_ymd(2026, 7, 1).unwrap();
        let end = BusinessDate::from_ymd(2026, 7, 31).unwrap();
        let (start_secs, end_secs) = SettlementPeriod::secs_bounds(start, end);
        let last_second = DateTime::parse_from_rfc3339("2026-07-31T23:59:59+08:00")
            .unwrap()
            .timestamp();
        assert!(
            (start_secs..end_secs).contains(&last_second),
            "结束日 23:59:59 +08:00 必须落在期间内"
        );
    }

    #[test]
    fn settlement_scope_filters_cover_supplier_period_and_requested_ids() {
        use entities::ids::{SupplierAccountId, SupplierFulfillmentItemId};
        use std::collections::BTreeSet;

        use super::{item_scope_filter, refund_fact_scope_filter};

        let supplier = SupplierAccountId::new("supplier-1");
        let facts = refund_fact_scope_filter(&supplier, 100, 200);
        assert_eq!(facts.get_str("supplier_id").unwrap(), "supplier-1");
        let range = facts.get_document("refunded_at").unwrap();
        assert_eq!(range.get_i64("$gte").unwrap(), 100);
        assert_eq!(range.get_i64("$lt").unwrap(), 200);

        let order_ids = BTreeSet::from(["order-1".to_string()]);
        let orders = order_scope_filter(&supplier, &order_ids, 100, 200);
        assert_eq!(orders.get_str("supplier_id").unwrap(), "supplier-1");
        let branches = orders.get_array("$or").unwrap();
        assert_eq!(branches.len(), 3);
        let id_branch = branches[0].as_document().unwrap();
        assert_eq!(
            id_branch
                .get_document("id")
                .unwrap()
                .get_array("$in")
                .expect("id 分支必须是 $in")
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["order-1"]
        );
        let completed_branch = branches[1].as_document().unwrap();
        assert_eq!(
            completed_branch
                .get_document("completed_at")
                .unwrap()
                .get_i64("$gte")
                .unwrap(),
            100
        );
        assert_eq!(
            completed_branch
                .get_document("completed_at")
                .unwrap()
                .get_i64("$lt")
                .unwrap(),
            200
        );
        // 损坏行分支：已完成但缺少完成时间（仅直接改库可产生）必须纳入，
        // 使 Service 的 confirmed_completed_at 校验 fail-closed。
        let tampered_branch = branches[2].as_document().unwrap();
        assert_eq!(
            tampered_branch.get_str("fulfillment_status").unwrap(),
            "COMPLETED"
        );
        assert!(matches!(
            tampered_branch.get("completed_at").unwrap(),
            mongodb::bson::Bson::Null
        ));
        let item_ids = vec![SupplierFulfillmentItemId::new("item-1")];
        let items = item_scope_filter(&order_ids, &item_ids);
        let branches = items.get_array("$or").unwrap();
        assert_eq!(branches.len(), 2);
        assert_eq!(
            branches[0]
                .as_document()
                .unwrap()
                .get_document("supplier_fulfillment_order_id")
                .unwrap()
                .get_array("$in")
                .expect("订单分支必须是 $in")
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["order-1"]
        );
        assert_eq!(
            branches[1]
                .as_document()
                .unwrap()
                .get_document("id")
                .unwrap()
                .get_array("$in")
                .expect("明细分支必须是 $in")
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["item-1"]
        );
    }
}
