//! 域 D29 `mall_order` 仓储：mall_order_fact(+cancel/completion)、mall_order、
//! mall_order_item、mall_payment_source、mall_item_funding_allocation、
//! mall_consumption_entry、mall_consumption_cost_assessment。
//!
//! 订单追溯聚合（`mall_order`/`mall_order_item`/`mall_payment_source`/
//! `mall_item_funding_allocation`）的 CRUD 与乐观锁复用 [`Repository`] 基类；
//! 五类关键事实、消费事实与成本评估是正式事实（§4.5 不设业务软删除），**不提供
//! 软删除方法**：只暴露只读追加仓储。集合名常量统一从 `MallOrderExt` 关联常量导入。
//!
//! ★ `business_fact_key` 去重只靠唯一索引（P2 计划 §5），本层不提供
//! 「先查后插」的查重入口；重复写入由
//! `uk_mall_order_facts_business_key` 唯一索引拒绝并透出
//! [`crate::Error::DuplicateKey`]。
//!
//! 筛选/行类型定义在本文件，经 `MallOrderExt` 的关联类型对外暴露。

mod order_read_batch;
mod payment_plan_persist;

use entities::common::time::Instant;
use entities::ids::{
    CustomerAccountId, MallAfterSalesRequestId, MallOrderFactId, MallOrderId, MallOrderItemId,
    MallPaymentSourceId, SalesOrderId,
};
use entities::mall_order::types::{
    AttributionStatus, DataSource, FactType, FulfillmentChain, ProcessingStatus,
};
use entities::mall_order::{
    ConsumptionDirection, MallConsumptionCostAssessment, MallConsumptionEntry, MallItemFundingAllocation,
    MallOrder, MallOrderCancelFact, MallOrderCompletionFact, MallOrderFact, MallOrderItem, MallPaymentSource,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::MallOrderExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `mall_order_fact` 集合名（单一来源：`MallOrderExt` 关联常量）。
const MALL_ORDER_FACTS: &str = <mongodb::Database as MallOrderExt>::MALL_ORDER_FACTS;
/// `mall_order_cancel_fact` 集合名（单一来源：`MallOrderExt` 关联常量）。
const MALL_ORDER_CANCEL_FACTS: &str = <mongodb::Database as MallOrderExt>::MALL_ORDER_CANCEL_FACTS;
/// `mall_order_completion_fact` 集合名（单一来源：`MallOrderExt` 关联常量）。
const MALL_ORDER_COMPLETION_FACTS: &str = <mongodb::Database as MallOrderExt>::MALL_ORDER_COMPLETION_FACTS;
/// `mall_order` 集合名（单一来源：`MallOrderExt` 关联常量）。
const MALL_ORDERS: &str = <mongodb::Database as MallOrderExt>::MALL_ORDERS;
/// `mall_consumption_entry` 集合名（单一来源：`MallOrderExt` 关联常量）。
const MALL_CONSUMPTION_ENTRIES: &str = <mongodb::Database as MallOrderExt>::MALL_CONSUMPTION_ENTRIES;
/// `mall_consumption_cost_assessment` 集合名（单一来源：`MallOrderExt` 关联常量）。
const MALL_CONSUMPTION_COST_ASSESSMENTS: &str =
    <mongodb::Database as MallOrderExt>::MALL_CONSUMPTION_COST_ASSESSMENTS;

/// 关键事实列表投影行（不投影加密原文引用 `raw_payload_reference`）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallOrderFactRow {
    /// 实体主键。
    pub id: String,
    /// 消息来源商城。
    pub mall_id: String,
    /// 事实类型。
    pub fact_type: FactType,
    /// 跨实时和回填的稳定事实键。
    pub business_fact_key: String,
    /// 商城订单号。
    pub external_order_no: String,
    /// 对应结果版本。
    pub external_order_version: String,
    /// 商城售后请求 ID。
    pub after_sales_request_id: Option<MallAfterSalesRequestId>,
    /// 原支付成功事实。
    pub original_payment_fact_id: Option<MallOrderFactId>,
    /// 事实发生时间。
    pub occurred_at: Instant,
    /// ERP 接收时间。
    pub received_at: Instant,
    /// 实时或历史回填。
    pub data_source: DataSource,
    /// 处理状态。
    pub processing_status: ProcessingStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 关键事实列表筛选条件。
#[derive(Debug, Clone)]
pub struct MallOrderFactFilter {
    /// 消息来源商城（字面量忽略大小写模糊匹配）；`None` 表示不筛选。
    pub mall_id: Option<String>,
    /// 事实类型；`None` 表示不筛选。
    pub fact_type: Option<FactType>,
    /// 处理状态；`None` 表示不筛选。
    pub processing_status: Option<ProcessingStatus>,
    /// 商城售后请求 ID；`None` 表示不筛选。
    pub after_sales_request_id: Option<MallAfterSalesRequestId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`occurred_at`/`received_at`/`created_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MallOrderFactFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "mall_id", self.mall_id.as_deref());
        if let Some(fact_type) = self.fact_type {
            filter.insert("fact_type", fact_type.as_str());
        }
        if let Some(status) = self.processing_status {
            filter.insert("processing_status", status.as_str());
        }
        if let Some(after_sales_request_id) = &self.after_sales_request_id {
            filter.insert("after_sales_request_id", after_sales_request_id.to_string());
        }
        filter
    }
}

impl Pagination for MallOrderFactFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 商城订单列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallOrderRow {
    /// 实体主键。
    pub id: String,
    /// 商城订单身份。
    pub mall_id: String,
    /// 商城订单号。
    pub external_order_no: String,
    /// 映射后的企业客户（待归集时为空）。
    pub customer_id: Option<CustomerAccountId>,
    /// 支付成功时间。
    pub paid_at: Instant,
    /// 实付快照（Decimal128 持久化）。
    pub paid_amount: entities::money::Amount,
    /// 履约链归属。
    pub fulfillment_chain: FulfillmentChain,
    /// 归集进度状态。
    pub attribution_status: AttributionStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 商城订单列表筛选条件。
#[derive(Debug, Clone)]
pub struct MallOrderFilter {
    /// 商城订单身份（字面量忽略大小写模糊匹配）；`None` 表示不筛选。
    pub mall_id: Option<String>,
    /// 商城订单号（字面量忽略大小写模糊匹配）；`None` 表示不筛选。
    pub external_order_no: Option<String>,
    /// 映射后的企业客户；`None` 表示不筛选。
    pub customer_id: Option<CustomerAccountId>,
    /// 履约链归属；`None` 表示不筛选。
    pub fulfillment_chain: Option<FulfillmentChain>,
    /// 归集进度状态；`None` 表示不筛选。
    pub attribution_status: Option<AttributionStatus>,
    /// 支付成功时间下界（含）；`None` 表示不设下界。
    pub paid_at_from: Option<Instant>,
    /// 支付成功时间上界（含）；`None` 表示不设上界。
    pub paid_at_to: Option<Instant>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`paid_at`/`ordered_at`/`created_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MallOrderFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "mall_id", self.mall_id.as_deref());
        insert_literal_regex_filter(
            &mut filter,
            "external_order_no",
            self.external_order_no.as_deref(),
        );
        if let Some(customer_id) = &self.customer_id {
            filter.insert("customer_id", customer_id.to_string());
        }
        if let Some(chain) = self.fulfillment_chain {
            filter.insert("fulfillment_chain", chain.as_str());
        }
        if let Some(status) = self.attribution_status {
            filter.insert("attribution_status", status.as_str());
        }
        if self.paid_at_from.is_some() || self.paid_at_to.is_some() {
            let mut paid_at = Document::new();
            if let Some(from) = self.paid_at_from {
                paid_at.insert("$gte", from.unix_secs());
            }
            if let Some(to) = self.paid_at_to {
                paid_at.insert("$lte", to.unix_secs());
            }
            filter.insert("paid_at", paid_at);
        }
        filter
    }
}

impl Pagination for MallOrderFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 消费事实列表筛选条件。
#[derive(Debug, Clone)]
pub struct MallConsumptionEntryFilter {
    /// 卡券经营归属：原销售单；`None` 表示不筛选。
    pub origin_sales_order_id: Option<SalesOrderId>,
    /// 消费方向；`None` 表示不筛选。
    pub direction: Option<ConsumptionDirection>,
    /// 归集进度状态；`None` 表示不筛选。
    pub attribution_status: Option<AttributionStatus>,
    /// 业务发生时间下界（含）；`None` 表示不设下界。
    pub occurred_at_from: Option<Instant>,
    /// 业务发生时间上界（含）；`None` 表示不设上界。
    pub occurred_at_to: Option<Instant>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`occurred_at`/`created_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MallConsumptionEntryFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_id) = &self.origin_sales_order_id {
            filter.insert("origin_sales_order_id", sales_order_id.to_string());
        }
        if let Some(direction) = self.direction {
            filter.insert("direction", direction.as_str());
        }
        if let Some(status) = self.attribution_status {
            filter.insert("attribution_status", status.as_str());
        }
        if self.occurred_at_from.is_some() || self.occurred_at_to.is_some() {
            let mut occurred_at = Document::new();
            if let Some(from) = self.occurred_at_from {
                occurred_at.insert("$gte", from.unix_secs());
            }
            if let Some(to) = self.occurred_at_to {
                occurred_at.insert("$lte", to.unix_secs());
            }
            filter.insert("occurred_at", occurred_at);
        }
        filter
    }
}

impl Pagination for MallConsumptionEntryFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// `mall_order_fact` 只读追加仓储（关键事实是不可变正式事实，§4.5 不设软删除）。
pub struct MallOrderFactRepository<'a> {
    db: &'a Database,
}

impl<'a> MallOrderFactRepository<'a> {
    /// 创建仓储实例。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 追加关键事实。
    ///
    /// 事实不可变（只提供 `new()`）；`business_fact_key`/`inbox_message_id`/
    /// `(mall_id, source_event_id)` 的唯一性由唯一索引保证（§6.17），
    /// 重复写入透出 [`crate::Error::DuplicateKey`]，服务层不得先查后插。
    ///
    /// # 参数
    /// * `fact` - 待追加的关键事实
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(&self, fact: &MallOrderFact, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), fact, executor).await
    }

    /// 按 ID 查找关键事实。
    ///
    /// # 参数
    /// * `id` - 事实主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的事实；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<MallOrderFact>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按业务事实键查找关键事实。
    ///
    /// 唯一性由 `uk_mall_order_facts_business_key` 唯一索引保证（§6.17）；
    /// 供幂等处理与「后到版本保存为差异」判定使用，不得用于先查后插去重。
    ///
    /// # 参数
    /// * `business_fact_key` - 跨实时和回填的稳定事实键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的事实；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_business_fact_key(
        &self,
        business_fact_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallOrderFact>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "business_fact_key": business_fact_key,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 按共同信封查找关键事实。
    ///
    /// 唯一性由 `uk_mall_order_facts_inbox_message` 唯一索引保证（§6.17）；
    /// 用于 inbox 消费去重后的信封回溯。
    ///
    /// # 参数
    /// * `inbox_message_id` - 共同信封
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的事实；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_inbox_message(
        &self,
        inbox_message_id: &entities::ids::InboxMessageId,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallOrderFact>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "inbox_message_id": inbox_message_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 分页检索关键事实列表（投影查询）。
    ///
    /// 只返回 [`MallOrderFactRow`] 所需的列表字段，不加载整文档
    /// （加密原文引用 `raw_payload_reference` 不进列表投影）；排序字段按白名单
    /// 映射（非法字段回落到 `created_at`），禁止透传任意字段名。
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
    pub async fn search_facts(
        &self,
        filter: &MallOrderFactFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallOrderFactRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                &["occurred_at", "received_at", "created_at"],
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(order_fact_projection())
            .build();
        let collection = self.collection().clone_with_type::<MallOrderFactRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按商城与半开业务时间范围读取关键事实。
    ///
    /// 范围固定为 `[range_start, range_end)`，结果按发生时间与事实 ID 升序，
    /// 供历史回填在 Repository 内完成时间边界筛选，Service 不再分页后内存过滤。
    ///
    /// # 参数
    /// * `mall_id` - 来源商城
    /// * `range_start` - 发生时间起点（含）
    /// * `range_end` - 发生时间终点（不含）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回范围内全部未删除关键事实。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_mall_and_occurred_range(
        &self,
        mall_id: &str,
        range_start: Instant,
        range_end: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallOrderFact>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "mall_id": mall_id,
                "occurred_at": {
                    "$gte": range_start.unix_secs(),
                    "$lt": range_end.unix_secs(),
                },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder()
                .sort(doc! { "occurred_at": 1, "id": 1 })
                .build(),
            executor,
        )
        .await
    }

    /// 按精确 `(mall_id, external_order_no)` 读取全部关键事实实体。
    ///
    /// 一次返回该商城订单的全部未删除事实，避免按商城分页后再由 Service
    /// 过滤订单号导致前页零命中时提前中断、后续页事实永久遗漏（INT-R02）。
    ///
    /// # 参数
    /// * `mall_id` - 来源商城
    /// * `external_order_no` - 商城订单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该订单全部未删除关键事实；排序合同为 `occurred_at` 升序，同秒按稳定
    /// `id` 升序。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 精确过滤，不做分页截断；软删除事实排除；调用方事务可见性由 `executor`
    /// 决定。本方法不开事务、不返回 Service DTO。
    pub async fn list_by_mall_and_external_order_no(
        &self,
        mall_id: &str,
        external_order_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallOrderFact>> {
        mongo_ops::find_many(
            &self.collection(),
            facts_by_mall_and_external_order_filter(mall_id, external_order_no),
            FindOptions::builder()
                .sort(facts_by_mall_and_external_order_sort())
                .build(),
            executor,
        )
        .await
    }

    /// 按商城售后请求取全部关键事实。
    ///
    /// 取消、退款、余额恢复必须携带售后请求 ID（§6.17）；本方法一次取回
    /// 该请求的全部事实，供售后关闭条件派生使用。
    ///
    /// # 参数
    /// * `after_sales_request_id` - 商城售后请求 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该请求的全部未删除事实。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_after_sales_request(
        &self,
        after_sales_request_id: &MallAfterSalesRequestId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallOrderFact>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "after_sales_request_id": after_sales_request_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder().sort(doc! { "occurred_at": 1 }).build(),
            executor,
        )
        .await
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallOrderFact> {
        self.db.collection::<MallOrderFact>(MALL_ORDER_FACTS)
    }
}

/// 构造精确 `(mall_id, external_order_no)` 事实过滤（含软删除排除）。
///
/// # 参数
/// * `mall_id` - 来源商城
/// * `external_order_no` - 商城订单号
///
/// # 返回
/// 返回 MongoDB 过滤文档。
///
/// # 错误
/// 不返回错误。
fn facts_by_mall_and_external_order_filter(mall_id: &str, external_order_no: &str) -> Document {
    doc! {
        "mall_id": mall_id,
        "external_order_no": external_order_no,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 返回精确订单事实查询的稳定排序合同。
///
/// # 返回
/// 返回 `occurred_at` 升序、同秒按 `id` 升序的排序文档。
///
/// # 错误
/// 不返回错误。
fn facts_by_mall_and_external_order_sort() -> Document {
    doc! { "occurred_at": 1, "id": 1 }
}

/// `mall_order_cancel_fact` 只读追加仓储（取消事实是不可变正式事实，§4.5 不设软删除）。
pub struct MallOrderCancelFactRepository<'a> {
    db: &'a Database,
}

impl<'a> MallOrderCancelFactRepository<'a> {
    /// 创建仓储实例。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 追加订单取消事实扩展。
    ///
    /// `mall_order_fact_id` 一对一唯一由唯一索引保证（§6.17），重复写入
    /// 透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `fact` - 待追加的取消事实扩展
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(&self, fact: &MallOrderCancelFact, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), fact, executor).await
    }

    /// 按 ID 查找取消事实扩展。
    ///
    /// # 参数
    /// * `id` - 事实主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的扩展；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallOrderCancelFact>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按事件信封事实查找取消事实扩展。
    ///
    /// 一对一唯一由 `uk_mall_order_cancel_facts_fact` 唯一索引保证（§6.17）。
    ///
    /// # 参数
    /// * `mall_order_fact_id` - `ORDER_CANCELED` 事实
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的扩展；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_fact_id(
        &self,
        mall_order_fact_id: &entities::ids::MallOrderFactId,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallOrderCancelFact>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "mall_order_fact_id": mall_order_fact_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallOrderCancelFact> {
        self.db.collection::<MallOrderCancelFact>(MALL_ORDER_CANCEL_FACTS)
    }
}

/// `mall_order_completion_fact` 只读追加仓储（完成事实是不可变正式事实，§4.5 不设软删除）。
pub struct MallOrderCompletionFactRepository<'a> {
    db: &'a Database,
}

impl<'a> MallOrderCompletionFactRepository<'a> {
    /// 创建仓储实例。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 追加订单完成事实扩展。
    ///
    /// `mall_order_fact_id` 一对一唯一由唯一索引保证（§6.17），重复写入
    /// 透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `fact` - 待追加的完成事实扩展
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(&self, fact: &MallOrderCompletionFact, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), fact, executor).await
    }

    /// 按 ID 查找完成事实扩展。
    ///
    /// # 参数
    /// * `id` - 事实主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的扩展；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallOrderCompletionFact>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按事件信封事实查找完成事实扩展。
    ///
    /// 一对一唯一由 `uk_mall_order_completion_facts_fact` 唯一索引保证（§6.17）。
    ///
    /// # 参数
    /// * `mall_order_fact_id` - `ORDER_COMPLETED` 事实
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的扩展；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_fact_id(
        &self,
        mall_order_fact_id: &entities::ids::MallOrderFactId,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallOrderCompletionFact>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "mall_order_fact_id": mall_order_fact_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallOrderCompletionFact> {
        self.db
            .collection::<MallOrderCompletionFact>(MALL_ORDER_COMPLETION_FACTS)
    }
}

/// `mall_consumption_entry` 只读追加仓储（消费事实是不可变正式事实，§4.5 不设软删除）。
pub struct MallConsumptionEntryRepository<'a> {
    db: &'a Database,
}

impl<'a> MallConsumptionEntryRepository<'a> {
    /// 创建仓储实例。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 追加消费事实。
    ///
    /// 消费不可变（只提供 `new()`）；「同一业务事实、商品明细、支付来源和方向
    /// 唯一」由唯一索引保证（§6.17），重复写入透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `entry` - 待追加的消费事实
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(&self, entry: &MallConsumptionEntry, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), entry, executor).await
    }

    /// 按 ID 查找消费事实。
    ///
    /// # 参数
    /// * `id` - 消费主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的消费事实；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallConsumptionEntry>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按原支付来源取消费事实序列（按 `occurred_at` 升序）。
    ///
    /// 退款分配必须引用「原商品 × 原支付来源」消费事实（§6.18），
    /// 本方法供退款净额校验与差异处理取数。
    ///
    /// # 参数
    /// * `mall_payment_source_id` - 原卡券或微信来源
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按发生时间升序的消费事实序列。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_original_payment_source(
        &self,
        mall_payment_source_id: &MallPaymentSourceId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallConsumptionEntry>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "mall_payment_source_id": mall_payment_source_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder().sort(doc! { "occurred_at": 1 }).build(),
            executor,
        )
        .await
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallConsumptionEntry> {
        self.db
            .collection::<MallConsumptionEntry>(MALL_CONSUMPTION_ENTRIES)
    }
}

/// `mall_consumption_cost_assessment` 只读追加仓储（成本评估是不可变正式事实，§4.5 不设软删除）。
pub struct MallConsumptionCostAssessmentRepository<'a> {
    db: &'a Database,
}

impl<'a> MallConsumptionCostAssessmentRepository<'a> {
    /// 创建仓储实例。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 追加成本评估。
    ///
    /// 评估不可变（只提供 `new()`）；`(mall_consumption_entry_id, assessment_no)`
    /// 唯一与「非空 `supersedes_assessment_id` 唯一」由唯一索引保证（§6.17），
    /// 重复写入透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `assessment` - 待追加的成本评估
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(
        &self,
        assessment: &MallConsumptionCostAssessment,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), assessment, executor).await
    }

    /// 按 ID 查找成本评估。
    ///
    /// # 参数
    /// * `id` - 评估主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的评估；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallConsumptionCostAssessment>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按消费来源明细取评估链（按 `assessment_no` 升序）。
    ///
    /// 评估链只追加不覆盖（§6.17）；当前成本由「未被后续评估引用」的链尾派生，
    /// 链尾锁定由 P3 在追加前校验。
    ///
    /// # 参数
    /// * `mall_consumption_entry_id` - 消费来源明细
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按评估号升序的评估链。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_entry(
        &self,
        mall_consumption_entry_id: &entities::ids::MallConsumptionEntryId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallConsumptionCostAssessment>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "mall_consumption_entry_id": mall_consumption_entry_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder().sort(doc! { "assessment_no": 1 }).build(),
            executor,
        )
        .await
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallConsumptionCostAssessment> {
        self.db
            .collection::<MallConsumptionCostAssessment>(MALL_CONSUMPTION_COST_ASSESSMENTS)
    }
}

impl<'a> Repository<'a, MallOrder> {
    /// 按支付成功事实查找商城订单追溯对象。
    ///
    /// # 参数
    /// * `payment_fact_id` - 形成订单的支付成功事实
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该支付事实形成的商城订单；尚未形成时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_payment_fact(
        &self,
        payment_fact_id: &entities::ids::MallOrderFactId,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallOrder>> {
        self.find_one(doc! { "payment_fact_id": payment_fact_id.to_string() }, executor)
            .await
    }

    /// 分页检索商城订单列表（投影查询）。
    ///
    /// 只返回 [`MallOrderRow`] 所需的列表字段，不加载整文档
    /// （加密履约地址快照 `address_snapshot_encrypted` 不进列表投影）；
    /// 排序字段按白名单映射（非法字段回落到 `created_at`）。
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
    pub async fn search_orders(
        &self,
        filter: &MallOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallOrderRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                &["paid_at", "ordered_at", "created_at"],
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(mall_order_projection())
            .build();
        let collection = self.collection().clone_with_type::<MallOrderRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, MallOrderItem> {
    /// 按订单取商品明细（按来源明细 ID 升序）。
    ///
    /// # 参数
    /// * `mall_order_id` - 商城订单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该订单的全部未删除明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_items_by_order(
        &self,
        mall_order_id: &MallOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallOrderItem>> {
        self.find_many_sorted(
            doc! { "mall_order_id": mall_order_id.to_string() },
            doc! { "external_item_id": 1 },
            executor,
        )
        .await
    }

    /// 按商城订单明细主键批量读取明细（`$in` 一次取回，避免 N+1）。
    ///
    /// # 参数
    /// * `item_ids` - 商城订单明细主键集合；为空时返回空列表，不访问数据库
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的未删除商城订单明细；传入 ID 缺失时不补位，由调用方校验完整性。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只查询 `mall_order_items` 集合；不做分页截断，不返回 Service DTO。
    pub async fn list_by_ids(
        &self,
        item_ids: &[MallOrderItemId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallOrderItem>> {
        if item_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = item_ids.iter().map(|id| id.to_string()).collect();
        self.find_many(doc! { "id": { "$in": ids } }, executor).await
    }
}

impl<'a> Repository<'a, MallPaymentSource> {
    /// 按订单取支付来源（按来源序号升序）。
    ///
    /// # 参数
    /// * `mall_order_id` - 商城订单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该订单的全部未删除支付来源。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_order(
        &self,
        mall_order_id: &MallOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallPaymentSource>> {
        self.find_many_sorted(
            doc! { "mall_order_id": mall_order_id.to_string() },
            doc! { "source_no": 1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, MallItemFundingAllocation> {
    /// 按商品明细集合批量取分摊记录（`$in` 一次取回，避免 N+1）。
    ///
    /// # 参数
    /// * `mall_order_item_ids` - 商品明细 ID 集合；为空时返回空列表
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回这些明细的全部未删除分摊记录。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_items(
        &self,
        mall_order_item_ids: &[MallOrderItemId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallItemFundingAllocation>> {
        if mall_order_item_ids.is_empty() {
            return Ok(Vec::new());
        }
        let item_ids: Vec<String> = mall_order_item_ids.iter().map(|id| id.to_string()).collect();
        self.find_many(doc! { "mall_order_item_id": { "$in": item_ids } }, executor)
            .await
    }
}

/// D29 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类与各只读追加仓储；本类型只承载
/// 依赖事务的跨集合原子写入入口，由 `MallOrderExt::mall_order()` 访问。
pub struct MallOrderRepository<'a> {
    db: &'a Database,
}

impl<'a> MallOrderRepository<'a> {
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

    /// 建立支付成功事实并创建商城订单追溯对象（跨集合多步骤写入）。
    ///
    /// §6.17：第一份金额守恒且满足事实契约的支付事实以同一事务创建唯一
    /// `mall_order`，后到的不同支付版本保存为差异，不得再创建订单。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有事实没有订单的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `fact` - 待写入的 `PAYMENT_SUCCEEDED` 事实
    /// * `order` - 待写入的商城订单追溯对象
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_payment_fact_with_order(
        &self,
        fact: &MallOrderFact,
        order: &MallOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<MallOrderFact>(MALL_ORDER_FACTS),
            fact,
            executor,
        )
        .await?;
        mongo_ops::insert_one(&self.db.collection::<MallOrder>(MALL_ORDERS), order, executor).await?;
        Ok(())
    }
}

/// 构建排序文档（白名单映射）。
///
/// # 参数
/// * `sort_by` - 排序字段；不在白名单或为 `None` 时默认 `created_at`
/// * `allowed` - 允许的排序字段白名单
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, allowed: &[&str], sort_ascending: bool) -> Document {
    let field = sort_by
        .filter(|field| allowed.contains(field))
        .unwrap_or("created_at");
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction }
}

/// 关键事实列表投影字段（不含加密原文引用）。
///
/// # 返回
/// 返回投影条件文档。
fn order_fact_projection() -> Document {
    doc! {
        "id": 1,
        "mall_id": 1,
        "fact_type": 1,
        "business_fact_key": 1,
        "external_order_no": 1,
        "external_order_version": 1,
        "after_sales_request_id": 1,
        "original_payment_fact_id": 1,
        "occurred_at": 1,
        "received_at": 1,
        "data_source": 1,
        "processing_status": 1,
        "created_at": 1,
    }
}

/// 商城订单列表投影字段（不含加密履约地址快照）。
///
/// # 返回
/// 返回投影条件文档。
fn mall_order_projection() -> Document {
    doc! {
        "id": 1,
        "mall_id": 1,
        "external_order_no": 1,
        "customer_id": 1,
        "paid_at": 1,
        "paid_amount": 1,
        "fulfillment_chain": 1,
        "attribution_status": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use entities::common::time::Instant;
    use entities::mall_order::types::{AttributionStatus, FactType, FulfillmentChain, ProcessingStatus};
    use entities::mall_order::ConsumptionDirection;
    use mongodb::bson::doc;

    use super::{
        facts_by_mall_and_external_order_filter, facts_by_mall_and_external_order_sort,
        order_fact_projection, sort_doc, MallConsumptionEntryFilter, MallOrderFactFilter, MallOrderFilter,
        Pagination, QueryFilter,
    };

    #[test]
    fn fact_filter_applies_optional_fields_and_deleted_filter() {
        let filter = MallOrderFactFilter {
            mall_id: Some("mall-a".to_string()),
            fact_type: Some(FactType::PaymentSucceeded),
            processing_status: Some(ProcessingStatus::Attributed),
            after_sales_request_id: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("fact_type").unwrap(), "PAYMENT_SUCCEEDED");
        assert_eq!(document.get_str("processing_status").unwrap(), "attributed");
    }

    #[test]
    fn fact_projection_contains_complete_list_view_without_sensitive_payload() {
        let projection = order_fact_projection();

        for field in [
            "id",
            "fact_type",
            "business_fact_key",
            "external_order_version",
            "after_sales_request_id",
            "original_payment_fact_id",
            "occurred_at",
            "received_at",
            "data_source",
            "processing_status",
        ] {
            assert_eq!(projection.get_i32(field).unwrap(), 1, "列表字段 {field} 必须投影");
        }
        assert!(!projection.contains_key("raw_payload_reference"));
    }

    #[test]
    fn facts_by_mall_and_external_order_filter_is_exact_and_excludes_deleted() {
        let filter = facts_by_mall_and_external_order_filter("mall-a", "SO-TARGET");
        assert_eq!(filter.get_str("mall_id").unwrap(), "mall-a");
        assert_eq!(filter.get_str("external_order_no").unwrap(), "SO-TARGET");
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
        assert!(!filter.contains_key("fact_type"));
        assert!(!filter.contains_key("page"));
    }

    #[test]
    fn facts_by_mall_and_external_order_sort_is_occurred_at_then_stable_id() {
        assert_eq!(
            facts_by_mall_and_external_order_sort(),
            doc! { "occurred_at": 1, "id": 1 }
        );
    }

    #[test]
    fn order_filter_applies_time_range_and_pagination() {
        let filter = MallOrderFilter {
            mall_id: None,
            external_order_no: Some("SO-1".to_string()),
            customer_id: None,
            fulfillment_chain: Some(FulfillmentChain::ErpAutomated),
            attribution_status: Some(AttributionStatus::PendingAttribution),
            paid_at_from: Some(Instant::from_unix_secs(1_700_000_000)),
            paid_at_to: None,
            page: 2,
            page_size: 10,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("fulfillment_chain").unwrap(), "ERP_AUTOMATED");
        assert_eq!(
            document.get_document("paid_at").unwrap().get_i64("$gte").unwrap(),
            1_700_000_000
        );
        assert_eq!(filter.skip(), 10);
        assert_eq!(filter.limit(), 10);
    }

    #[test]
    fn consumption_entry_filter_maps_fields_and_status() {
        let filter = MallConsumptionEntryFilter {
            origin_sales_order_id: None,
            direction: Some(ConsumptionDirection::ConsumptionReversal),
            attribution_status: Some(AttributionStatus::Difference),
            occurred_at_from: None,
            occurred_at_to: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("direction").unwrap(), "consumption_reversal");
        assert_eq!(document.get_str("attribution_status").unwrap(), "difference");
    }

    #[test]
    fn sort_doc_maps_only_whitelisted_fields_and_defaults_to_created_at() {
        assert_eq!(
            sort_doc(None, &["occurred_at", "created_at"], false),
            doc! { "created_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("occurred_at"), &["occurred_at", "created_at"], true),
            doc! { "occurred_at": 1 }
        );
        assert_eq!(
            sort_doc(Some("malicious_field"), &["occurred_at", "created_at"], false),
            doc! { "created_at": -1 },
            "白名单外字段必须回落到默认排序"
        );
    }
}
