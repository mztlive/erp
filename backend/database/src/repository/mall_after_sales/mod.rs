//! 域 D30 `mall_after_sales` 仓储：mall_after_sales_request(+_line)、mall_refund(+_line)、
//! mall_refund_allocation、mall_balance_restoration(+_allocation)。
//!
//! 售后申请头/行走 [`Repository`] 基类（状态推进与行派生使用乐观锁 CAS）；
//! 退款、余额恢复及其行/分配是正式事实（§4.5 不设业务软删除），**不提供软删除
//! 方法**：只暴露只读追加仓储。集合名常量统一从 `MallAfterSalesExt` 关联常量导入。
//!
//! 已知冻结实体缺陷（P1，待地基修订）：`MallAfterSalesRequest` 同时声明扁平化的
//! `BaseModel.created_at` 与同名域字段 `created_at: Instant`（商城申请时间），
//! serde 无法往返，`mall_after_sales_request` 头表暂不可持久化（实体修订前
//! `search_after_sales_requests` 仅可服务于既有数据，头表写入入口待修订后生效）。
//!
//! 筛选/行类型定义在本文件，经 `MallAfterSalesExt` 的关联类型对外暴露。
//!
//! - [`consumption_refund_limit_scope`]：原消费退款额度批量事实与历史净额（INT-R11）；
//! - [`restoration_limit_scope`]：余额恢复关联事实图与历史恢复净额（INT-R12）。

mod consumption_refund_limit_scope;
mod restoration_limit_scope;

// 供公共方法返回类型命名；当前调用方以类型推断消费，故允许未直接 use。
#[allow(unused_imports)]
pub use consumption_refund_limit_scope::ConsumptionRefundLimitScope;
#[allow(unused_imports)]
pub use restoration_limit_scope::RestorationLimitScope;

use entities::common::time::Instant;
use entities::mall_after_sales::{
    AfterSalesRequestStatus, AfterSalesRequestType, MallAfterSalesRequest, MallAfterSalesRequestLine,
    MallBalanceRestoration, MallBalanceRestorationAllocation, MallRefund, MallRefundAllocation,
    MallRefundLine,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::MallAfterSalesExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `mall_refund` 集合名（单一来源：`MallAfterSalesExt` 关联常量）。
pub(crate) const MALL_REFUNDS: &str = <mongodb::Database as MallAfterSalesExt>::MALL_REFUNDS;
/// `mall_refund_line` 集合名（单一来源：`MallAfterSalesExt` 关联常量）。
pub(crate) const MALL_REFUND_LINES: &str = <mongodb::Database as MallAfterSalesExt>::MALL_REFUND_LINES;
/// `mall_refund_allocation` 集合名（单一来源：`MallAfterSalesExt` 关联常量）。
pub(crate) const MALL_REFUND_ALLOCATIONS: &str =
    <mongodb::Database as MallAfterSalesExt>::MALL_REFUND_ALLOCATIONS;
/// `mall_balance_restoration` 集合名（单一来源：`MallAfterSalesExt` 关联常量）。
pub(crate) const MALL_BALANCE_RESTORATIONS: &str =
    <mongodb::Database as MallAfterSalesExt>::MALL_BALANCE_RESTORATIONS;
/// `mall_balance_restoration_allocation` 集合名（单一来源：`MallAfterSalesExt` 关联常量）。
pub(crate) const MALL_BALANCE_RESTORATION_ALLOCATIONS: &str =
    <mongodb::Database as MallAfterSalesExt>::MALL_BALANCE_RESTORATION_ALLOCATIONS;

/// 售后请求列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallAfterSalesRequestRow {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub mall_id: String,
    /// 商城售后请求稳定身份。
    pub external_request_id: String,
    /// 原商城订单。
    pub mall_order_id: entities::ids::MallOrderId,
    /// 取消或退款。
    pub request_type: AfterSalesRequestType,
    /// 请求状态。
    pub status: AfterSalesRequestStatus,
    /// 员工售后原因。
    pub reason: String,
    /// 商城申请时间。
    pub created_at: Instant,
    /// 乐观锁版本。
    pub version: u64,
}

/// 售后请求列表筛选条件。
#[derive(Debug, Clone)]
pub struct MallAfterSalesRequestFilter {
    /// 来源商城（字面量忽略大小写模糊匹配）；`None` 表示不筛选。
    pub mall_id: Option<String>,
    /// 商城售后请求稳定身份（字面量忽略大小写模糊匹配）；`None` 表示不筛选。
    pub external_request_id: Option<String>,
    /// 原商城订单；`None` 表示不筛选。
    pub mall_order_id: Option<entities::ids::MallOrderId>,
    /// 请求类型；`None` 表示不筛选。
    pub request_type: Option<AfterSalesRequestType>,
    /// 请求状态；`None` 表示不筛选。
    pub status: Option<AfterSalesRequestStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MallAfterSalesRequestFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "mall_id", self.mall_id.as_deref());
        insert_literal_regex_filter(
            &mut filter,
            "external_request_id",
            self.external_request_id.as_deref(),
        );
        if let Some(mall_order_id) = &self.mall_order_id {
            filter.insert("mall_order_id", mall_order_id.to_string());
        }
        if let Some(request_type) = self.request_type {
            filter.insert("request_type", request_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for MallAfterSalesRequestFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, MallAfterSalesRequest> {
    /// 分页检索售后请求列表（投影查询）。
    ///
    /// 只返回 [`MallAfterSalesRequestRow`] 所需的列表字段，不加载整文档；
    /// 排序字段按白名单映射（非法字段回落到 `created_at`），禁止透传任意字段名。
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
    pub async fn search_after_sales_requests(
        &self,
        filter: &MallAfterSalesRequestFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallAfterSalesRequestRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                &["created_at", "updated_at"],
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(after_sales_request_projection())
            .build();
        let collection = self.collection().clone_with_type::<MallAfterSalesRequestRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

impl<'a> Repository<'a, MallAfterSalesRequestLine> {}

/// `mall_refund` 只读追加仓储（退款头是不可变正式事实，§4.5 不设软删除）。
pub struct MallRefundRepository<'a> {
    db: &'a Database,
}

impl<'a> MallRefundRepository<'a> {
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

    /// 追加退款成功事实头。
    ///
    /// 事实不可变（只提供 `new()`）；`mall_order_fact_id` 一对一唯一与
    /// 「商城 + 退款单号 + 退款版本」唯一由唯一索引保证（§6.18），
    /// 重复写入透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `refund` - 待追加的退款头
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(&self, refund: &MallRefund, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), refund, executor).await
    }

    /// 按 ID 查找退款头。
    ///
    /// # 参数
    /// * `id` - 退款头主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的退款头；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<MallRefund>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按 `REFUND_SUCCEEDED` 事实查找退款头。
    ///
    /// 一对一唯一由 `uk_mall_refunds_fact` 唯一索引保证（§6.18）。
    ///
    /// # 参数
    /// * `mall_order_fact_id` - `REFUND_SUCCEEDED` 事实
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的退款头；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_fact_id(
        &self,
        mall_order_fact_id: &entities::ids::MallOrderFactId,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallRefund>> {
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

    /// 按商城退款身份查找退款头。
    ///
    /// 唯一性由 `uk_mall_refunds_identity` 唯一索引保证（§6.18），供幂等接收
    /// 判定使用，服务层不得「先查后插」。
    ///
    /// # 参数
    /// * `mall_id` - 来源商城
    /// * `external_refund_no` - 商城退款身份
    /// * `external_refund_version` - 商城退款版本
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的退款头；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_identity(
        &self,
        mall_id: &str,
        external_refund_no: &str,
        external_refund_version: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallRefund>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "mall_id": mall_id,
                "external_refund_no": external_refund_no,
                "external_refund_version": external_refund_version,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 按售后案件取退款头序列。
    ///
    /// # 参数
    /// * `after_sales_request_id` - 同一售后案件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该案件的退款头序列。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_after_sales_request(
        &self,
        after_sales_request_id: &entities::ids::MallAfterSalesRequestId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallRefund>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "after_sales_request_id": after_sales_request_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await
    }

    /// 按原订单取退款头序列。
    ///
    /// # 参数
    /// * `mall_order_id` - 原订单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该订单的退款头序列。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_order(
        &self,
        mall_order_id: &entities::ids::MallOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallRefund>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "mall_order_id": mall_order_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallRefund> {
        self.db.collection::<MallRefund>(MALL_REFUNDS)
    }
}

/// `mall_refund_line` 只读追加仓储（退款行是不可变正式事实，§4.5 不设软删除）。
pub struct MallRefundLineRepository<'a> {
    db: &'a Database,
}

impl<'a> MallRefundLineRepository<'a> {
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

    /// 追加退款行。
    ///
    /// 行不可变（只提供 `new()`）；`(mall_refund_id, line_no)` 与
    /// `(mall_refund_id, mall_order_item_id)` 唯一由唯一索引保证（§6.18），
    /// 重复写入透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `line` - 待追加的退款行
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(&self, line: &MallRefundLine, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), line, executor).await
    }

    /// 按 ID 查找退款行。
    ///
    /// # 参数
    /// * `id` - 退款行主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的退款行；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<MallRefundLine>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按退款头集合批量取退款行（`$in` 一次取回，避免 N+1）。
    ///
    /// # 参数
    /// * `mall_refund_ids` - 退款头 ID 集合；为空时返回空列表
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回这些退款头的全部退款行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_refunds(
        &self,
        mall_refund_ids: &[entities::ids::MallRefundId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallRefundLine>> {
        if mall_refund_ids.is_empty() {
            return Ok(Vec::new());
        }
        let refund_ids: Vec<String> = mall_refund_ids.iter().map(|id| id.to_string()).collect();
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "mall_refund_id": { "$in": refund_ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder().sort(doc! { "line_no": 1 }).build(),
            executor,
        )
        .await
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallRefundLine> {
        self.db.collection::<MallRefundLine>(MALL_REFUND_LINES)
    }
}

/// `mall_refund_allocation` 只读追加仓储（退款分配是不可变正式事实，§4.5 不设软删除）。
pub struct MallRefundAllocationRepository<'a> {
    db: &'a Database,
}

impl<'a> MallRefundAllocationRepository<'a> {
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

    /// 追加退款分配。
    ///
    /// 分配不可变（只提供 `new()`）；`(mall_refund_line_id, allocation_no)` 唯一
    /// 与「非空 `reverses_allocation_id` 唯一」由唯一索引保证（§6.18），
    /// 重复写入透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `allocation` - 待追加的退款分配
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(&self, allocation: &MallRefundAllocation, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), allocation, executor).await
    }

    /// 按 ID 查找退款分配。
    ///
    /// # 参数
    /// * `id` - 分配主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的分配；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallRefundAllocation>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按原消费事实取退款分配。
    ///
    /// 「同一原消费累计成功退款金额不得超过原消费金额」的净额校验依赖
    /// 本查询（§6.18，聚合校验由 P3 落实）。
    ///
    /// # 参数
    /// * `original_consumption_entry_id` - 原商品 × 原支付来源消费事实
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该原消费的全部分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_consumption(
        &self,
        original_consumption_entry_id: &entities::ids::MallConsumptionEntryId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallRefundAllocation>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "original_consumption_entry_id": original_consumption_entry_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallRefundAllocation> {
        self.db
            .collection::<MallRefundAllocation>(MALL_REFUND_ALLOCATIONS)
    }
}

/// `mall_balance_restoration` 只读追加仓储（恢复头是不可变正式事实，§4.5 不设软删除）。
pub struct MallBalanceRestorationRepository<'a> {
    db: &'a Database,
}

impl<'a> MallBalanceRestorationRepository<'a> {
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

    /// 追加余额恢复事实头。
    ///
    /// 事实不可变（只提供 `new()`）；`mall_order_fact_id` 一对一唯一与
    /// 「商城 + 恢复单号 + 版本」唯一由唯一索引保证（§6.18），
    /// 重复写入透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `restoration` - 待追加的恢复头
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(
        &self,
        restoration: &MallBalanceRestoration,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), restoration, executor).await
    }

    /// 按 ID 查找恢复头。
    ///
    /// # 参数
    /// * `id` - 恢复头主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的恢复头；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallBalanceRestoration>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按 `CARD_BALANCE_RESTORED` 事实查找恢复头。
    ///
    /// 一对一唯一由 `uk_mall_balance_restorations_fact` 唯一索引保证（§6.18）。
    ///
    /// # 参数
    /// * `mall_order_fact_id` - `CARD_BALANCE_RESTORED` 事实
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的恢复头；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_fact_id(
        &self,
        mall_order_fact_id: &entities::ids::MallOrderFactId,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallBalanceRestoration>> {
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

    /// 按恢复身份查找恢复头。
    ///
    /// 唯一性由 `uk_mall_balance_restorations_identity` 唯一索引保证（§6.18），
    /// 供幂等接收判定使用。
    ///
    /// # 参数
    /// * `mall_id` - 来源商城
    /// * `external_restoration_no` - 恢复身份
    /// * `version` - 恢复身份版本
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的恢复头；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_identity(
        &self,
        mall_id: &str,
        external_restoration_no: &str,
        version: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallBalanceRestoration>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "mall_id": mall_id,
                "external_restoration_no": external_restoration_no,
                "version": version,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 按售后案件取恢复头序列。
    ///
    /// # 参数
    /// * `after_sales_request_id` - 同一售后案件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该案件的恢复头序列。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_after_sales_request(
        &self,
        after_sales_request_id: &entities::ids::MallAfterSalesRequestId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallBalanceRestoration>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "after_sales_request_id": after_sales_request_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallBalanceRestoration> {
        self.db
            .collection::<MallBalanceRestoration>(MALL_BALANCE_RESTORATIONS)
    }
}

/// `mall_balance_restoration_allocation` 只读追加仓储（恢复分配是不可变正式事实，§4.5 不设软删除）。
pub struct MallBalanceRestorationAllocationRepository<'a> {
    db: &'a Database,
}

impl<'a> MallBalanceRestorationAllocationRepository<'a> {
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

    /// 追加余额恢复分配。
    ///
    /// 分配不可变（只提供 `new()`）；`(mall_balance_restoration_id, allocation_no)`
    /// 唯一由唯一索引保证（§6.18），重复写入透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `allocation` - 待追加的恢复分配
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(
        &self,
        allocation: &MallBalanceRestorationAllocation,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), allocation, executor).await
    }

    /// 按 ID 查找恢复分配。
    ///
    /// # 参数
    /// * `id` - 分配主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的分配；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallBalanceRestorationAllocation>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按原 CARD 退款分配取恢复分配。
    ///
    /// 「每张卡累计恢复金额不得超过对应 CARD 退款净额」的校验依赖本查询
    /// （§6.18，聚合校验由 P3 落实）。
    ///
    /// # 参数
    /// * `mall_refund_allocation_id` - 原 CARD 退款资金分配
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回引用该退款分配的全部恢复分配。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_refund_allocation(
        &self,
        mall_refund_allocation_id: &entities::ids::MallRefundAllocationId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallBalanceRestorationAllocation>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "mall_refund_allocation_id": mall_refund_allocation_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallBalanceRestorationAllocation> {
        self.db
            .collection::<MallBalanceRestorationAllocation>(MALL_BALANCE_RESTORATION_ALLOCATIONS)
    }
}

/// D30 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类与各只读追加仓储；本类型只承载
/// 依赖事务的跨集合原子写入入口，由 `MallAfterSalesExt::mall_after_sales()` 访问。
pub struct MallAfterSalesRepository<'a> {
    db: &'a Database,
}

impl<'a> MallAfterSalesRepository<'a> {
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

    /// 建立退款聚合：头、行与初始 `APPLY` 分配同事务写入（跨集合多步骤写入）。
    ///
    /// §6.18/§8.4 第 3 条：退款头、行、初始 `APPLY` 分配（消费反向由 P3 在
    /// 同事务追加 D29 消费冲减）在同一事务写入，过账后不可更新或删除。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时各笔写入各自自动提交，中途失败会留下只有头没有行或分配的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `refund` - 待写入的退款头
    /// * `lines` - 待写入的退款行
    /// * `allocations` - 待写入的初始退款分配（须引用 `lines` 的行）
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入失败
    /// 时返回错误。
    pub async fn create_refund_with_lines_and_allocations(
        &self,
        refund: &MallRefund,
        lines: &[MallRefundLine],
        allocations: &[MallRefundAllocation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<MallRefund>(MALL_REFUNDS), refund, executor).await?;
        mongo_ops::insert_many(
            &self.db.collection::<MallRefundLine>(MALL_REFUND_LINES),
            lines.to_vec(),
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<MallRefundAllocation>(MALL_REFUND_ALLOCATIONS),
            allocations.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }

    /// 建立余额恢复聚合：恢复头与分配同事务写入（跨集合多步骤写入）。
    ///
    /// §6.18：余额恢复头与按原 CARD 退款资金分配的恢复分配原子可见。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有头没有分配的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `restoration` - 待写入的余额恢复头
    /// * `allocations` - 待写入的恢复分配
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入失败
    /// 时返回错误。
    pub async fn create_balance_restoration_with_allocations(
        &self,
        restoration: &MallBalanceRestoration,
        allocations: &[MallBalanceRestorationAllocation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<MallBalanceRestoration>(MALL_BALANCE_RESTORATIONS),
            restoration,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<MallBalanceRestorationAllocation>(MALL_BALANCE_RESTORATION_ALLOCATIONS),
            allocations.to_vec(),
            executor,
        )
        .await?;
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

/// 售后请求列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn after_sales_request_projection() -> Document {
    doc! {
        "id": 1,
        "mall_id": 1,
        "external_request_id": 1,
        "mall_order_id": 1,
        "request_type": 1,
        "status": 1,
        "reason": 1,
        "created_at": 1,
        "version": 1,
    }
}

#[cfg(test)]
mod tests {
    use entities::mall_after_sales::{AfterSalesRequestStatus, AfterSalesRequestType};
    use mongodb::bson::doc;

    use super::{sort_doc, MallAfterSalesRequestFilter, QueryFilter};

    #[test]
    fn after_sales_request_filter_applies_optional_fields_and_deleted_filter() {
        let filter = MallAfterSalesRequestFilter {
            mall_id: Some("mall-a".to_string()),
            external_request_id: None,
            mall_order_id: None,
            request_type: Some(AfterSalesRequestType::Refund),
            status: Some(AfterSalesRequestStatus::Received),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("request_type").unwrap(), "refund");
        assert_eq!(document.get_str("status").unwrap(), "received");
    }

    #[test]
    fn sort_doc_maps_only_whitelisted_fields_and_defaults_to_created_at() {
        assert_eq!(
            sort_doc(None, &["created_at", "updated_at"], false),
            doc! { "created_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("updated_at"), &["created_at", "updated_at"], true),
            doc! { "updated_at": 1 }
        );
        assert_eq!(
            sort_doc(Some("malicious_field"), &["created_at", "updated_at"], false),
            doc! { "created_at": -1 },
            "白名单外字段必须回落到默认排序"
        );
    }
}
