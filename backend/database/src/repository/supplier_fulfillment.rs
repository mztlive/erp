//! 域 D32 `supplier_fulfillment` 仓储：supplier_fulfillment_order、supplier_fulfillment_item、
//! supplier_order_action(+_line)、supplier_order_status_history、supplier_refund_fact、
//! supplier_refund_allocation（页面：W26）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本文件只补充域特有
//! 查询与跨集合多步骤写入入口。集合名常量统一取 `SupplierFulfillmentExt` 关联常量
//! （唯一权威来源，indexes 与 Repository 两侧共用）。
//!
//! 不可变正式事实（`supplier_order_status_history`、`supplier_refund_fact` 与
//! `supplier_refund_allocation`）只 `new` 不 `update`，本域不为它们提供软删除方法
//! （§4.5.1、§6.19）；履约订单是正式单据，仍走基类软删除/恢复语义。
//!
//! 筛选/行类型定义在本文件，经 `SupplierFulfillmentExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::common::time::Instant;
use std::collections::HashMap;
use std::str::FromStr;

use entities::ids::{
    MallAfterSalesRequestId, MallAfterSalesRequestLineId, MallOrderId, MallOrderItemId, SupplierAccountId,
    SupplierApiConnectionId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
    SupplierOfferingRevisionId, SupplierOrderActionId, SupplierRefundFactId,
};
use entities::mall_after_sales::MallAfterSalesRequestLine;
use entities::mall_order::MallOrderItem;
use entities::money::{Amount, Quantity};
use entities::supplier_fulfillment::{
    CancelStatus, FulfillmentStatus, RefundStatus, SupplierFulfillmentItem, SupplierFulfillmentOrder,
    SupplierOrderAction, SupplierOrderActionLine, SupplierOrderStatusHistory, SupplierRefundAllocation,
    SupplierRefundFact,
};
use entities::supplier_offering::{SupplierOffering, SupplierOfferingRevision};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::{MallAfterSalesExt, SupplierFulfillmentExt, SupplierOfferingExt};
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `supplier_fulfillment_order` 集合名（单一来源：`SupplierFulfillmentExt` 关联常量）。
const SUPPLIER_FULFILLMENT_ORDERS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS;
/// `supplier_fulfillment_item` 集合名（单一来源：`SupplierFulfillmentExt` 关联常量）。
const SUPPLIER_FULFILLMENT_ITEMS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ITEMS;
/// `supplier_order_action` 集合名（单一来源：`SupplierFulfillmentExt` 关联常量）。
const SUPPLIER_ORDER_ACTIONS: &str = <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTIONS;
/// `supplier_order_action_line` 集合名（单一来源：`SupplierFulfillmentExt` 关联常量）。
const SUPPLIER_ORDER_ACTION_LINES: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_ORDER_ACTION_LINES;
/// `supplier_offering` 集合名。
const SUPPLIER_OFFERINGS: &str = <mongodb::Database as SupplierOfferingExt>::SUPPLIER_OFFERINGS;
/// `supplier_offering_revision` 集合名。
const SUPPLIER_OFFERING_REVISIONS: &str =
    <mongodb::Database as SupplierOfferingExt>::SUPPLIER_OFFERING_REVISIONS;
/// `supplier_refund_fact` 集合名（单一来源：`SupplierFulfillmentExt` 关联常量）。
const SUPPLIER_REFUND_FACTS: &str = <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_FACTS;
/// `supplier_refund_allocation` 集合名（单一来源：`SupplierFulfillmentExt` 关联常量）。
const SUPPLIER_REFUND_ALLOCATIONS: &str =
    <mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_REFUND_ALLOCATIONS;
/// `mall_after_sales_request_line` 集合名（单一来源：`MallAfterSalesExt` 关联常量）。
const MALL_AFTER_SALES_REQUEST_LINES: &str =
    <mongodb::Database as MallAfterSalesExt>::MALL_AFTER_SALES_REQUEST_LINES;

/// 履约订单列表排序白名单（§6.19 查询索引支持的字段；白名单外一律回退 `created_at`）。
const ORDER_SORT_FIELDS: &[&str] = &["created_at", "submitted_at", "accepted_at", "completed_at"];

/// 供应商履约订单列表投影行。
///
/// 列表接口只取必要字段，禁止返回整文档；履约地址快照（加密值与查询指纹）是敏感值，
/// 一律不进列表投影（§4.5.5）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierFulfillmentOrderRow {
    /// 实体主键。
    pub id: String,
    /// ERP 供应商子订单号。
    pub fulfillment_order_no: String,
    /// 来源商城订单。
    pub mall_order_id: MallOrderId,
    /// 固定供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商 API 连接。
    pub connection_id: SupplierApiConnectionId,
    /// 确定性拆单序号。
    pub split_no: u32,
    /// 履约主线状态。
    pub fulfillment_status: FulfillmentStatus,
    /// 取消进度状态。
    pub cancel_status: CancelStatus,
    /// 退款进度状态。
    pub refund_status: RefundStatus,
    /// 供应商订单号。
    pub external_order_no: Option<String>,
    /// 提交给供应商的时间。
    pub submitted_at: Option<Instant>,
    /// 供应商接单时间。
    pub accepted_at: Option<Instant>,
    /// 履约完成时间。
    pub completed_at: Option<Instant>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商履约订单列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierFulfillmentOrderFilter {
    /// 固定供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 履约主线状态；`None` 表示不筛选。
    pub fulfillment_status: Option<FulfillmentStatus>,
    /// 供应商订单号（按字面量部分匹配，忽略大小写）；`None` 表示不筛选。
    pub external_order_no: Option<String>,
    /// 来源商城订单；`None` 表示不筛选。
    pub mall_order_id: Option<MallOrderId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内生效，白名单外回退 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierFulfillmentOrderFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(fulfillment_status) = self.fulfillment_status {
            filter.insert("fulfillment_status", fulfillment_status.as_str());
        }
        if let Some(mall_order_id) = &self.mall_order_id {
            filter.insert("mall_order_id", mall_order_id.to_string());
        }
        insert_literal_regex_filter(
            &mut filter,
            "external_order_no",
            self.external_order_no.as_deref(),
        );
        filter
    }
}

impl Pagination for SupplierFulfillmentOrderFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierFulfillmentOrder> {
    /// 分页检索供应商履约订单列表（投影查询）。
    ///
    /// 只返回 [`SupplierFulfillmentOrderRow`] 所需的列表字段，不加载整文档；
    /// 排序字段走白名单映射（`ORDER_SORT_FIELDS`），白名单外一律回退 `created_at`。
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
    pub async fn search_supplier_fulfillment_orders(
        &self,
        filter: &SupplierFulfillmentOrderFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierFulfillmentOrderRow>> {
        let options = FindOptions::builder()
            .sort(order_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_fulfillment_order_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierFulfillmentOrderRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按供应商批量读取全部未删除履约订单。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该供应商的履约订单集合。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_supplier_id(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierFulfillmentOrder>> {
        self.find_many(doc! { "supplier_id": supplier_id.to_string() }, executor)
            .await
    }

    /// 按 ERP 供应商子订单号查找唯一履约订单。
    ///
    /// 唯一性由 `uk_supplier_fulfillment_orders_order_no` 唯一索引保证；该方法用于
    /// 下单幂等判定，服务层不得做「先查后插」的重复性判断（§6.19）。
    ///
    /// # 参数
    /// * `fulfillment_order_no` - ERP 供应商子订单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除履约订单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_fulfillment_order_no(
        &self,
        fulfillment_order_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierFulfillmentOrder>> {
        self.find_one(doc! { "fulfillment_order_no": fulfillment_order_no }, executor)
            .await
    }
}

impl<'a> Repository<'a, SupplierFulfillmentItem> {
    /// 批量按供应商子订单查询履约明细（`$in` 一次取回，避免 N+1）。
    ///
    /// 明细随子订单同事务创建且创建后不可修改（§6.19），本方法供详情页与
    /// 结算/退款编排加载整单明细。
    ///
    /// # 参数
    /// * `order_ids` - 供应商子订单 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中集合对应的全部未删除履约明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_items_by_order_ids(
        &self,
        order_ids: &[SupplierFulfillmentOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierFulfillmentItem>> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut items = self
            .find_many(
                doc! { "supplier_fulfillment_order_id": { "$in": ids_to_strings(order_ids) } },
                executor,
            )
            .await?;
        items.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        Ok(items)
    }
}

impl<'a> Repository<'a, MallOrderItem> {
    /// 按商城订单明细主键批量读取明细。
    ///
    /// # 参数
    /// * `item_ids` - 商城订单明细主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的未删除商城订单明细。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_ids(
        &self,
        item_ids: &[MallOrderItemId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallOrderItem>> {
        if item_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": ids_to_strings(item_ids) } }, executor)
            .await
    }
}

impl<'a> Repository<'a, MallAfterSalesRequestLine> {
    /// 按商城售后申请读取全部申请行。
    ///
    /// # 参数
    /// * `request_id` - 商城售后申请主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回关联该申请的未删除申请行。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_request_id(
        &self,
        request_id: &MallAfterSalesRequestId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallAfterSalesRequestLine>> {
        self.find_many(
            doc! { "after_sales_request_id": request_id.to_string() },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierOrderAction> {
    /// 按履约订单读取动作，按创建时间和主键倒序排列。
    ///
    /// # 参数
    /// * `order_id` - 供应商履约订单主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新动作在前的动作集合。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_order_newest(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOrderAction>> {
        self.find_many_sorted(
            doc! { "supplier_fulfillment_order_id": order_id.to_string() },
            doc! { "created_at": -1, "id": -1 },
            executor,
        )
        .await
    }

    /// 按履约订单和动作类型读取动作，按创建时间和主键倒序排列。
    ///
    /// # 参数
    /// * `order_id` - 供应商履约订单主键
    /// * `action_type` - 供应商动作类型
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新动作在前的动作集合。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_order_and_type_newest(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        action_type: entities::supplier_fulfillment::SupplierOrderActionType,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOrderAction>> {
        self.find_many_sorted(
            doc! {
                "supplier_fulfillment_order_id": order_id.to_string(),
                "action_type": action_type.as_str(),
            },
            doc! { "created_at": -1, "id": -1 },
            executor,
        )
        .await
    }

    /// 读取履约订单最近一次指定类型动作。
    ///
    /// # 参数
    /// * `order_id` - 供应商履约订单主键
    /// * `action_type` - 供应商动作类型
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最近动作；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn latest_by_order_and_type(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        action_type: entities::supplier_fulfillment::SupplierOrderActionType,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOrderAction>> {
        Ok(self
            .list_by_order_and_type_newest(order_id, action_type, executor)
            .await?
            .into_iter()
            .next())
    }

    /// 按商城售后申请读取已提交供应商动作。
    ///
    /// # 参数
    /// * `request_id` - 商城售后申请主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回关联该售后申请的动作集合。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_after_sales_request(
        &self,
        request_id: &MallAfterSalesRequestId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOrderAction>> {
        self.find_many(
            doc! { "after_sales_request_id": request_id.to_string() },
            executor,
        )
        .await
    }

    /// 按对供应商动作幂等键查找唯一动作。
    ///
    /// 唯一性由 `uk_supplier_order_actions_idempotency_key` 唯一索引保证；人工重放
    /// 与网络超时恢复继续使用原幂等键（§6.19），本方法返回既有动作避免重复创建。
    ///
    /// # 参数
    /// * `idempotency_key` - 对供应商动作幂等键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除动作；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOrderAction>> {
        self.find_one(doc! { "idempotency_key": idempotency_key }, executor)
            .await
    }
}

impl<'a> Repository<'a, SupplierOrderActionLine> {
    /// 批量按动作头查询动作行（`$in` 一次取回，避免 N+1）。
    ///
    /// 动作行随动作头同事务创建且创建后不可修改（§6.19），本方法供详情页
    /// 加载一次取消/退款实际提交给供应商的范围。
    ///
    /// # 参数
    /// * `action_ids` - 供应商动作 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中集合对应的全部未删除动作行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_lines_by_action_ids(
        &self,
        action_ids: &[SupplierOrderActionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOrderActionLine>> {
        if action_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut lines = self
            .find_many(
                doc! { "supplier_order_action_id": { "$in": ids_to_strings(action_ids) } },
                executor,
            )
            .await?;
        lines.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        Ok(lines)
    }
}

impl<'a> Repository<'a, SupplierOrderStatusHistory> {
    /// 按履约订单读取状态历史，按发生时间和主键升序排列。
    ///
    /// # 参数
    /// * `order_id` - 供应商履约订单主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按业务发生顺序排列的状态历史。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_order_chronological(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOrderStatusHistory>> {
        self.find_many_sorted(
            doc! { "supplier_fulfillment_order_id": order_id.to_string() },
            doc! { "occurred_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 按「连接 + 外部事件 ID」查找状态历史（回调幂等判定）。
    ///
    /// 唯一性由 `uk_supplier_order_status_histories_connection_event` 唯一索引保证
    /// （§6.19：回调幂等唯一，避免同一供应商的不同连接或账号合法复用外部事件号）。
    ///
    /// # 参数
    /// * `connection_id` - 供应商 API 连接
    /// * `external_event_id` - 外部事件 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除状态历史；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_connection_and_event(
        &self,
        connection_id: &SupplierApiConnectionId,
        external_event_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOrderStatusHistory>> {
        self.find_one(
            doc! {
                "connection_id": connection_id.to_string(),
                "external_event_id": external_event_id,
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierRefundFact> {
    /// 按「连接 + 外部退款号 + 外部退款版本」查找退款事实头。
    ///
    /// 唯一性由 `uk_supplier_refund_facts_connection_refund` 唯一索引保证
    /// （§6.19：外部退款身份与版本组成幂等键）。
    ///
    /// # 参数
    /// * `connection_id` - 供应商 API 连接
    /// * `external_refund_no` - 外部退款号
    /// * `external_refund_version` - 外部退款版本
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除退款事实头；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_connection_and_refund(
        &self,
        connection_id: &SupplierApiConnectionId,
        external_refund_no: &str,
        external_refund_version: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierRefundFact>> {
        self.find_one(
            doc! {
                "connection_id": connection_id.to_string(),
                "external_refund_no": external_refund_no,
                "external_refund_version": external_refund_version,
            },
            executor,
        )
        .await
    }

    /// 批量按供应商子订单查询退款事实头（`$in` 一次取回，避免 N+1）。
    ///
    /// 退款事实是冲减供应商成本和应付的唯一事实（§6.19），订单详情页按子订单
    /// 聚合退款时使用本方法。
    ///
    /// # 参数
    /// * `order_ids` - 供应商子订单 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中集合对应的全部未删除退款事实头。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_refund_facts_by_order_ids(
        &self,
        order_ids: &[SupplierFulfillmentOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierRefundFact>> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut facts = self
            .find_many(
                doc! { "supplier_fulfillment_order_id": { "$in": ids_to_strings(order_ids) } },
                executor,
            )
            .await?;
        facts.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        Ok(facts)
    }
}

impl<'a> Repository<'a, SupplierRefundAllocation> {
    /// 批量按退款事实头查询退款分配行（`$in` 一次取回，避免 N+1）。
    ///
    /// 分配行是正式事实行，创建后不可修改（§6.19）；`validate_allocations` 与
    /// REVERSE 纠错编排在 P3 使用本方法加载全部分配行。
    ///
    /// # 参数
    /// * `fact_ids` - 供应商退款事实头 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中集合对应的全部未删除退款分配行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_fact_ids(
        &self,
        fact_ids: &[SupplierRefundFactId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierRefundAllocation>> {
        if fact_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut allocations = self
            .find_many(
                doc! { "supplier_refund_fact_id": { "$in": ids_to_strings(fact_ids) } },
                executor,
            )
            .await?;
        allocations.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        Ok(allocations)
    }
}

/// 售后申请行限额投影行（FUL-R03）。
///
/// 只投影售后动作净余额校验所需的申请行限额，不反序列化整实体。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AfterSalesRequestLineLimitRow {
    /// 商城售后申请行主键。
    pub id: MallAfterSalesRequestLineId,
    /// 本商品申请数量。
    pub requested_quantity: Quantity,
    /// 本商品申请金额。
    pub requested_amount: Amount,
}

/// 按商城售后申请行聚合的历史已提交动作行合计（FUL-R03）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmittedActionTotals {
    /// 历史已提交数量。
    pub quantity: Quantity,
    /// 历史已提交金额。
    pub amount: Amount,
}

/// 售后动作校验范围快照（FUL-R03）。
///
/// 一次取回服务层跨聚合校验所需的持久化事实：订单合法履约明细主键、
/// 售后申请行限额与按申请行聚合的历史已提交数量/金额；不携带 services
/// DTO 或授权结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AfterSalesActionScope {
    /// 订单合法履约明细主键集合。
    pub item_ids: Vec<SupplierFulfillmentItemId>,
    /// 售后申请全部未删除申请行限额。
    pub request_line_limits: Vec<AfterSalesRequestLineLimitRow>,
    /// 按申请行聚合的历史已提交数量/金额（同一申请下全部未软删除动作行，
    /// 不按动作状态过滤，六态均计入）。
    pub submitted_by_request_line: HashMap<MallAfterSalesRequestLineId, SubmittedActionTotals>,
}

/// 订单退款财务快照（FUL-R05）。
///
/// 只包含退款上限校验所需的两个持久化金额，不携带 services DTO 或授权
/// 结论；任一来源集合为空时对应金额为精确零。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefundFinancialSnapshot {
    /// 订单明细含税成本快照合计。
    pub order_cost_gross: Amount,
    /// 历史退款事实实际退款金额合计。
    pub refunded_total: Amount,
}

/// 履约明细主键投影行（FUL-R03）。
#[derive(Debug, Deserialize)]
struct FulfillmentItemIdRow {
    /// 履约明细主键。
    id: SupplierFulfillmentItemId,
}

/// 动作头主键投影行（FUL-R03）。
#[derive(Debug, Deserialize)]
struct OrderActionIdRow {
    /// 动作头主键。
    id: SupplierOrderActionId,
}

/// 动作行已提交合计聚合行（FUL-R03）。
#[derive(Debug, Deserialize)]
struct SubmittedActionTotalRow {
    /// 商城售后申请行主键（聚合分组键）。
    #[serde(rename = "_id")]
    after_sales_request_line_id: MallAfterSalesRequestLineId,
    /// 累计已提交数量（Decimal128 求和结果）。
    quantity: Quantity,
    /// 累计已提交金额（Decimal128 求和结果）。
    amount: Amount,
}

/// 订单金额合计聚合行（FUL-R05）。
#[derive(Debug, Deserialize)]
struct AmountTotalRow {
    /// 合计金额（Decimal128 求和结果）。
    total: Amount,
}

/// D32 域专用仓储：跨集合聚合写入与售后/退款读取范围。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口与跨集合只读范围快照（FUL-R03/FUL-R05），由
/// `SupplierFulfillmentExt::supplier_fulfillment()` 访问。
pub struct SupplierFulfillmentRepository<'a> {
    db: &'a Database,
}

impl<'a> SupplierFulfillmentRepository<'a> {
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

    /// 按供给修订主键批量加载对应供给稳定身份。
    ///
    /// 仓储负责修订与稳定身份的两段 `$in` 查询及关联；Service 只校验请求中的
    /// 供应商和连接关系，不接触 BSON 或通用查询方法。
    ///
    /// # 参数
    /// * `revision_ids` - 供给商业条款修订主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回以修订主键为键的供给稳定身份；缺失修订或供给时不生成对应条目。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn load_offerings_by_revision_ids(
        &self,
        revision_ids: &[SupplierOfferingRevisionId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SupplierOffering>> {
        if revision_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let revision_id_strings = revision_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let revisions = Repository::<SupplierOfferingRevision>::new(self.db, SUPPLIER_OFFERING_REVISIONS)
            .list_by_ids(&revision_id_strings, executor)
            .await?;
        let offering_ids = revisions
            .iter()
            .map(|revision| revision.supplier_offering_id.clone())
            .collect::<Vec<_>>();
        let offerings = Repository::<SupplierOffering>::new(self.db, SUPPLIER_OFFERINGS)
            .list_by_ids(&offering_ids, executor)
            .await?
            .into_iter()
            .map(|offering| (offering.base.id.clone(), offering))
            .collect::<HashMap<_, _>>();
        Ok(revisions
            .into_iter()
            .filter_map(|revision| {
                offerings
                    .get(revision.supplier_offering_id.as_ref())
                    .cloned()
                    .map(|offering| (revision.base.id, offering))
            })
            .collect())
    }

    /// 原子创建履约子订单、全部明细与首个 `PLACE` 动作。
    ///
    /// 依次写入 `supplier_fulfillment_orders`、`supplier_fulfillment_items` 与
    /// `supplier_order_actions`，保证「子订单、全部明细和首个 PLACE 动作」同事务
    /// 可见（§6.19）；唯一键冲突时服务层加载既有子订单继续原幂等动作。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时各笔写入各自自动提交，中途失败会留下只有订单没有明细的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `order` - 待写入的履约子订单
    /// * `items` - 待写入的全部履约明细
    /// * `action` - 待写入的首个 `PLACE` 动作
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为幂等命中）或 MongoDB 写入失败时返回错误。
    pub async fn create_fulfillment_with_items_and_place_action(
        &self,
        order: &SupplierFulfillmentOrder,
        items: &[SupplierFulfillmentItem],
        action: &SupplierOrderAction,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SupplierFulfillmentOrder>(SUPPLIER_FULFILLMENT_ORDERS),
            order,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SupplierFulfillmentItem>(SUPPLIER_FULFILLMENT_ITEMS),
            items.to_vec(),
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<SupplierOrderAction>(SUPPLIER_ORDER_ACTIONS),
            action,
            executor,
        )
        .await?;
        Ok(())
    }

    /// 原子创建供应商退款事实头与全部分配行。
    ///
    /// 依次写入 `supplier_refund_facts` 与 `supplier_refund_allocations`，保证
    /// 「退款头 + 分配行」同事务可见（§6.19：供应商退款成功是冲减供应商成本和
    /// 应付的唯一事实；纠错只追加成组 REVERSE 并引用原分配）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有事实头没有分配行的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `fact` - 待写入的退款事实头
    /// * `allocations` - 待写入的全部分配行
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_refund_fact_with_allocations(
        &self,
        fact: &SupplierRefundFact,
        allocations: &[SupplierRefundAllocation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SupplierRefundFact>(SUPPLIER_REFUND_FACTS),
            fact,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SupplierRefundAllocation>(SUPPLIER_REFUND_ALLOCATIONS),
            allocations.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }

    /// 读取售后动作校验范围（FUL-R03）。
    ///
    /// 一次取回订单合法履约明细主键、商城售后申请行限额，以及按申请行
    /// 聚合的历史已提交数量/金额。历史累计包含同一申请下全部未软删除
    /// 正式动作行，不按动作状态过滤（PENDING/SENDING/RESULT_UNKNOWN/
    /// SUCCEEDED/FAILED/MANUAL 六态均计入）；软删除动作头或动作行一律
    /// 排除。固定四次数据库访问，不随明细或动作行数增长；全部使用调用方
    /// 执行器，事务内调用看到同一事务的未提交写入，本方法不自行开启或
    /// 提交事务。跨聚合归属与净余额决定仍由 Service 承担。
    ///
    /// # 参数
    /// * `order_id` - 供应商子订单主键
    /// * `request_id` - 商城售后申请主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回订单合法明细主键、申请行限额与按申请行聚合的历史已提交合计；
    /// 无历史动作时提交合计映射为空，由 Service 按精确零处理。
    ///
    /// # 错误
    /// MongoDB 查询或聚合失败时返回错误；Decimal128 无法转换为
    /// `Quantity`/`Amount`（精度或上限越界）时返回错误而非 panic。
    pub async fn after_sales_action_scope(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        request_id: &MallAfterSalesRequestId,
        executor: &mut dyn Executor,
    ) -> Result<AfterSalesActionScope> {
        let item_ids = self.scope_fulfillment_item_ids(order_id, executor).await?;
        let request_line_limits = self.scope_request_line_limits(request_id, executor).await?;
        let submitted_by_request_line = self.scope_submitted_totals(request_id, executor).await?;
        Ok(AfterSalesActionScope {
            item_ids,
            request_line_limits,
            submitted_by_request_line,
        })
    }

    /// 读取订单退款财务快照（FUL-R05）。
    ///
    /// 在数据库内分别聚合未删除履约明细的含税成本快照合计
    /// （`order_cost_gross`）与未删除退款事实的实际退款金额合计
    /// （`refunded_total`），只返回两个金额，不反序列化明细或事实实体；
    /// 任一集合为空时对应金额为精确零。固定两次数据库访问，不随明细或
    /// 退款事实数增长；全部使用调用方执行器，事务内调用看到同一事务的
    /// 未提交写入，本方法不自行开启或提交事务。退款上限「历史累计 + 本次
    /// 退款不超过订单成本」的跨聚合决定仍由 Service 承担，本方法不返回
    /// services DTO 或授权结论。
    ///
    /// # 参数
    /// * `order_id` - 供应商子订单主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回订单含税成本合计与历史退款合计；无明细或无退款事实时对应
    /// 金额为精确零。
    ///
    /// # 错误
    /// MongoDB 聚合失败时返回错误；Decimal128 求和结果无法转换为
    /// `Amount`（精度或上限越界）时返回错误而非 panic。
    pub async fn refund_financial_snapshot(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        executor: &mut dyn Executor,
    ) -> Result<RefundFinancialSnapshot> {
        let order_cost_gross = aggregate_single_total(
            &self.db.collection::<AmountTotalRow>(SUPPLIER_FULFILLMENT_ITEMS),
            financial_total_pipeline(order_id, "cost_snapshot_total_gross"),
            executor,
        )
        .await?;
        let refunded_total = aggregate_single_total(
            &self.db.collection::<AmountTotalRow>(SUPPLIER_REFUND_FACTS),
            financial_total_pipeline(order_id, "refund_amount"),
            executor,
        )
        .await?;
        Ok(RefundFinancialSnapshot {
            order_cost_gross,
            refunded_total,
        })
    }

    /// 读取订单合法履约明细主键集合（FUL-R03）。
    ///
    /// 按订单主键过滤未删除明细并只投影主键。
    ///
    /// # 参数
    /// * `order_id` - 供应商子订单主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回订单全部未删除履约明细主键；无明细时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    async fn scope_fulfillment_item_ids(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierFulfillmentItemId>> {
        let rows = mongo_ops::find_many(
            &self
                .db
                .collection::<FulfillmentItemIdRow>(SUPPLIER_FULFILLMENT_ITEMS),
            doc! {
                "supplier_fulfillment_order_id": order_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder().projection(doc! { "id": 1 }).build(),
            executor,
        )
        .await?;
        Ok(rows.into_iter().map(|row| row.id).collect())
    }

    /// 读取售后申请全部未删除申请行限额（FUL-R03）。
    ///
    /// 按售后申请主键过滤未删除申请行并只投影限额字段。
    ///
    /// # 参数
    /// * `request_id` - 商城售后申请主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回申请全部未删除申请行限额；无申请行时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    async fn scope_request_line_limits(
        &self,
        request_id: &MallAfterSalesRequestId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AfterSalesRequestLineLimitRow>> {
        mongo_ops::find_many(
            &self
                .db
                .collection::<AfterSalesRequestLineLimitRow>(MALL_AFTER_SALES_REQUEST_LINES),
            doc! {
                "after_sales_request_id": request_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder()
                .projection(doc! { "id": 1, "requested_quantity": 1, "requested_amount": 1 })
                .build(),
            executor,
        )
        .await
    }

    /// 读取按申请行聚合的历史已提交合计（FUL-R03）。
    ///
    /// 先按售后申请主键读取全部未删除动作头主键（不按动作状态过滤），再
    /// 在动作行上按动作头主键 `$in` 过滤未删除行并聚合数量与金额。
    ///
    /// # 参数
    /// * `request_id` - 商城售后申请主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按申请行聚合的历史已提交合计；无历史动作时返回空映射。
    ///
    /// # 错误
    /// MongoDB 查询或聚合失败时返回错误；Decimal128 无法转换为
    /// `Quantity`/`Amount`（精度或上限越界）时返回错误而非 panic。
    async fn scope_submitted_totals(
        &self,
        request_id: &MallAfterSalesRequestId,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<MallAfterSalesRequestLineId, SubmittedActionTotals>> {
        let action_rows = mongo_ops::find_many(
            &self.db.collection::<OrderActionIdRow>(SUPPLIER_ORDER_ACTIONS),
            doc! {
                "after_sales_request_id": request_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder().projection(doc! { "id": 1 }).build(),
            executor,
        )
        .await?;
        let action_ids = action_rows
            .into_iter()
            .map(|row| row.id.to_string())
            .collect::<Vec<_>>();
        if action_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = aggregate_rows(
            &self
                .db
                .collection::<SubmittedActionTotalRow>(SUPPLIER_ORDER_ACTION_LINES),
            submitted_action_totals_pipeline(&action_ids),
            executor,
        )
        .await?;
        Ok(build_submitted_by_request_line(rows))
    }
}

/// 把动作行已提交合计聚合行归组为按申请行映射。
///
/// # 参数
/// * `rows` - 按 `after_sales_request_line_id` 分组的动作行合计行
///
/// # 返回
/// 返回按申请行主键映射的历史已提交合计；空集合返回空映射。
fn build_submitted_by_request_line(
    rows: Vec<SubmittedActionTotalRow>,
) -> HashMap<MallAfterSalesRequestLineId, SubmittedActionTotals> {
    rows.into_iter()
        .map(|row| {
            (
                row.after_sales_request_line_id,
                SubmittedActionTotals {
                    quantity: row.quantity,
                    amount: row.amount,
                },
            )
        })
        .collect()
}

/// 构建履约订单排序文档（白名单映射，禁止透传任意字段名）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单内时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn order_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|field| ORDER_SORT_FIELDS.contains(field))
        .unwrap_or("created_at");
    doc! { field: direction }
}

/// 按执行器语义执行聚合管道并收集全部结果行。
///
/// 带会话时使用 `SessionCursor` 逐条读取，与非会话游标返回同样的结果集合。
///
/// # 参数
/// * `collection` - 目标集合
/// * `pipeline` - 聚合管道
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 返回全部聚合结果行。
///
/// # 错误
/// 聚合执行、游标读取或结果行反序列化（含 Decimal128 精度/上限越界）
/// 失败时返回错误。
async fn aggregate_rows<T>(
    collection: &mongodb::Collection<T>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<T>>
where
    T: serde::de::DeserializeOwned + Send + Sync,
{
    match executor.session() {
        Some(session) => {
            collection
                .aggregate(pipeline)
                .with_type::<T>()
                .session(&mut *session)
                .await?
                .stream(session)
                .try_collect::<Vec<_>>()
                .await
        }
        None => {
            collection
                .aggregate(pipeline)
                .with_type::<T>()
                .await?
                .try_collect::<Vec<_>>()
                .await
        }
    }
    .map_err(crate::Error::from)
}

/// 执行订单金额合计管道并返回合计金额。
///
/// # 参数
/// * `collection` - 目标集合
/// * `pipeline` - 已构造的过滤与合计管道（`$match` + `$group` + `$project`）
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 返回合计金额；无匹配文档时返回精确零。
///
/// # 错误
/// 聚合执行、游标读取或结果反序列化（含 Decimal128 精度/上限越界）
/// 失败时返回错误。
async fn aggregate_single_total(
    collection: &mongodb::Collection<AmountTotalRow>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Amount> {
    let rows = aggregate_rows(collection, pipeline, executor).await?;
    Ok(first_total_or_zero(rows))
}

/// 返回金额合计的空集合兜底值（精确零）。
///
/// `Amount::from_str("0.00")` 对定点金额恒合法，集中为纯函数以便
/// 「无历史为精确零」维度直接单测。
///
/// # 返回
/// 返回精确零金额。
fn zero_total() -> Amount {
    Amount::from_str("0.00").expect("零是合法金额")
}

/// 把单行金额合计聚合结果映射为合计金额；无行时返回精确零。
///
/// # 参数
/// * `rows` - 金额合计聚合行（`$group` 管道至多一行）
///
/// # 返回
/// 返回首行合计金额；空集合返回 [`zero_total`]。
fn first_total_or_zero(rows: Vec<AmountTotalRow>) -> Amount {
    rows.into_iter()
        .next()
        .map(|row| row.total)
        .unwrap_or_else(zero_total)
}

/// 构造订单金额合计管道（FUL-R05）。
///
/// 按订单主键过滤未删除文档，对指定 Decimal128 金额字段求和，并移除
/// 分组键 `_id`。
///
/// # 参数
/// * `order_id` - 供应商子订单主键
/// * `amount_field` - 待求和金额字段名
///
/// # 返回
/// 返回三段聚合管道（`$match` + `$group` + `$project`）。
fn financial_total_pipeline(order_id: &SupplierFulfillmentOrderId, amount_field: &str) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                "supplier_fulfillment_order_id": order_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            }
        },
        doc! {
            "$group": {
                "_id": mongodb::bson::Bson::Null,
                "total": { "$sum": format!("${amount_field}") },
            }
        },
        doc! { "$project": { "_id": 0 } },
    ]
}

/// 构造动作行已提交合计聚合管道（FUL-R03）。
///
/// 按动作头主键 `$in` 过滤未删除动作行，并按 `after_sales_request_line_id`
/// 对数量与金额求和；不按动作状态过滤（六态均计入），软删除动作头已由
/// 调用方从 `action_ids` 排除。
///
/// # 参数
/// * `action_ids` - 同一售后申请下未删除动作头主键集合（非空）
///
/// # 返回
/// 返回两段聚合管道（`$match` + `$group`）。
fn submitted_action_totals_pipeline(action_ids: &[String]) -> Vec<Document> {
    vec![
        doc! {
            "$match": {
                "supplier_order_action_id": { "$in": action_ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            }
        },
        doc! {
            "$group": {
                "_id": "$after_sales_request_line_id",
                "quantity": { "$sum": "$quantity" },
                "amount": { "$sum": "$amount" },
            }
        },
    ]
}

/// 把 ID 集合转换为 BSON `$in` 需要的字符串集合。
///
/// # 参数
/// * `ids` - ID 值对象集合
///
/// # 返回
/// 返回 ID 的字符串形态（ID newtype 以字符串持久化）。
fn ids_to_strings<T: ToString>(ids: &[T]) -> Vec<String> {
    ids.iter().map(ToString::to_string).collect()
}

/// 供应商履约订单列表投影字段（不含敏感地址快照）。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_fulfillment_order_projection() -> Document {
    doc! {
        "id": 1,
        "fulfillment_order_no": 1,
        "mall_order_id": 1,
        "supplier_id": 1,
        "connection_id": 1,
        "split_no": 1,
        "fulfillment_status": 1,
        "cancel_status": 1,
        "refund_status": 1,
        "external_order_no": 1,
        "submitted_at": 1,
        "accepted_at": 1,
        "completed_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{order_sort_doc, QueryFilter, SupplierFulfillmentOrderFilter};
    use entities::ids::{MallOrderId, SupplierAccountId};
    use entities::supplier_fulfillment::FulfillmentStatus;
    use mongodb::bson::doc;

    #[test]
    fn order_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SupplierFulfillmentOrderFilter {
            supplier_id: Some(SupplierAccountId::new("supplier-1")),
            fulfillment_status: Some(FulfillmentStatus::Accepted),
            external_order_no: Some("SUP-1".to_string()),
            mall_order_id: Some(MallOrderId::new("mall-order-1")),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("supplier_id").unwrap(), "supplier-1");
        assert_eq!(document.get_str("fulfillment_status").unwrap(), "ACCEPTED");
        assert_eq!(document.get_str("mall_order_id").unwrap(), "mall-order-1");
        assert_eq!(
            document
                .get_document("external_order_no")
                .unwrap()
                .get_str("$regex")
                .unwrap(),
            r"SUP\-1"
        );
    }

    #[test]
    fn order_sort_doc_rejects_fields_outside_whitelist() {
        assert_eq!(order_sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(
            order_sort_doc(Some("fulfillment_status"), false),
            doc! { "created_at": -1 },
            "白名单外的排序字段必须回退 created_at"
        );
        assert_eq!(
            order_sort_doc(Some("submitted_at"), true),
            doc! { "submitted_at": 1 }
        );
        assert_eq!(
            order_sort_doc(Some("completed_at"), false),
            doc! { "completed_at": -1 }
        );
    }
}

#[cfg(test)]
mod snapshot_tests {
    use std::str::FromStr;

    use mongodb::bson::{doc, Bson};

    use super::{
        build_submitted_by_request_line, financial_total_pipeline, first_total_or_zero,
        submitted_action_totals_pipeline, zero_total, AmountTotalRow, SubmittedActionTotalRow,
    };
    use entities::ids::{MallAfterSalesRequestLineId, SupplierFulfillmentOrderId};
    use entities::money::{Amount, Quantity};

    /// FUL-R05 金额合计管道：按订单过滤未删除文档、单值求和并移除分组键。
    #[test]
    fn financial_total_pipeline_filters_undelted_documents_and_sums_single_total() {
        let pipeline = financial_total_pipeline(
            &SupplierFulfillmentOrderId::new("order-1"),
            "cost_snapshot_total_gross",
        );
        let match_stage = pipeline[0].get_document("$match").expect("过滤阶段");
        assert_eq!(
            match_stage.get_str("supplier_fulfillment_order_id").unwrap(),
            "order-1"
        );
        assert_eq!(match_stage.get_i64("deleted_at").expect("未删除条件"), 0);
        let group_stage = pipeline[1].get_document("$group").expect("分组阶段");
        assert!(matches!(group_stage.get("_id").expect("分组键"), Bson::Null));
        assert_eq!(
            group_stage
                .get_document("total")
                .expect("求和字段")
                .get_str("$sum")
                .expect("求和表达式"),
            "$cost_snapshot_total_gross"
        );
        let project_stage = pipeline[2].get_document("$project").expect("投影阶段");
        assert_eq!(project_stage.get_i32("_id").expect("移除分组键"), 0);
    }

    /// FUL-R05 求和字段随调用方指定，退款事实走 `refund_amount`。
    #[test]
    fn financial_total_pipeline_sums_refund_amount_field() {
        let pipeline = financial_total_pipeline(&SupplierFulfillmentOrderId::new("order-1"), "refund_amount");
        let group_stage = pipeline[1].get_document("$group").expect("分组阶段");
        assert_eq!(
            group_stage
                .get_document("total")
                .expect("求和字段")
                .get_str("$sum")
                .expect("求和表达式"),
            "$refund_amount"
        );
    }

    /// FUL-R05 Decimal128 合计结果可反序列化为 `Amount`。
    #[test]
    fn amount_total_row_deserializes_decimal128_total() {
        let document = doc! { "total": { "$numberDecimal": "1234.56" } };
        let row: AmountTotalRow =
            mongodb::bson::deserialize_from_document(document).expect("合法 Decimal128 必须成功");
        assert_eq!(row.total, Amount::from_str("1234.56").expect("合法金额"));
    }

    /// FUL-R05 超精度 Decimal128 必须返回反序列化错误而非 panic。
    #[test]
    fn amount_total_row_rejects_precision_overflow_without_panicking() {
        let document = doc! { "total": { "$numberDecimal": "1.235" } };
        let result: std::result::Result<AmountTotalRow, mongodb::bson::error::Error> =
            mongodb::bson::deserialize_from_document(document);
        assert!(result.is_err(), "超精度 Decimal128 必须失败而非 panic");
    }

    /// FUL-R03 动作行合计管道：只过滤未删除行并按申请行求和数量与金额，
    /// 不携带动作状态条件（六态均计入）。
    #[test]
    fn submitted_totals_pipeline_filters_undelted_lines_and_groups_by_request_line() {
        let pipeline = submitted_action_totals_pipeline(&["action-1".to_string(), "action-2".to_string()]);
        let match_stage = pipeline[0].get_document("$match").expect("过滤阶段");
        let action_ids = match_stage
            .get_document("supplier_order_action_id")
            .expect("动作头主键条件")
            .get_array("$in")
            .expect("动作头主键 $in 条件");
        assert_eq!(
            action_ids,
            &[
                Bson::String("action-1".to_string()),
                Bson::String("action-2".to_string())
            ]
        );
        assert_eq!(match_stage.get_i64("deleted_at").expect("未删除条件"), 0);
        assert!(!match_stage.contains_key("status"), "历史累计不得按动作状态过滤");
        let group_stage = pipeline[1].get_document("$group").expect("分组阶段");
        assert_eq!(
            group_stage.get_str("_id").expect("分组键"),
            "$after_sales_request_line_id"
        );
        assert_eq!(
            group_stage
                .get_document("quantity")
                .expect("数量求和字段")
                .get_str("$sum")
                .expect("数量求和表达式"),
            "$quantity"
        );
        assert_eq!(
            group_stage
                .get_document("amount")
                .expect("金额求和字段")
                .get_str("$sum")
                .expect("金额求和表达式"),
            "$amount"
        );
    }

    /// FUL-R03 Decimal128 聚合行可反序列化为数量与金额。
    #[test]
    fn submitted_total_row_deserializes_decimal128_totals() {
        let document = doc! {
            "_id": "request-line-1",
            "quantity": { "$numberDecimal": "2.500000" },
            "amount": { "$numberDecimal": "49.98" },
        };
        let row: SubmittedActionTotalRow =
            mongodb::bson::deserialize_from_document(document).expect("合法 Decimal128 必须成功");
        assert_eq!(row.after_sales_request_line_id.as_ref(), "request-line-1");
        assert_eq!(row.quantity, Quantity::from_str("2.5").expect("合法数量"));
        assert_eq!(row.amount, Amount::from_str("49.98").expect("合法金额"));
    }

    /// FUL-R03 超精度 Decimal128 必须返回反序列化错误而非 panic。
    #[test]
    fn submitted_total_row_rejects_precision_overflow_without_panicking() {
        let document = doc! {
            "_id": "request-line-2",
            "quantity": { "$numberDecimal": "1.0000001" },
            "amount": { "$numberDecimal": "1.00" },
        };
        let result: std::result::Result<SubmittedActionTotalRow, mongodb::bson::error::Error> =
            mongodb::bson::deserialize_from_document(document);
        assert!(result.is_err(), "超精度 Decimal128 必须失败而非 panic");
    }

    /// FUL-R05 无行聚合结果映射为精确零（「空集合为精确零」验收的单元级证据）。
    #[test]
    fn first_total_or_zero_maps_empty_rows_to_exact_zero() {
        assert_eq!(zero_total(), Amount::from_str("0.00").expect("合法金额"));
        assert_eq!(first_total_or_zero(Vec::new()), zero_total());

        let document = doc! { "total": { "$numberDecimal": "88.80" } };
        let row: AmountTotalRow =
            mongodb::bson::deserialize_from_document(document).expect("合法 Decimal128 必须成功");
        assert_eq!(
            first_total_or_zero(vec![row]),
            Amount::from_str("88.80").expect("合法金额")
        );
    }

    /// FUL-R03 空动作行集合映射为空映射（空 `action_ids` 提前返回分支的
    /// 纯映射证据；无历史端到端仍由 Mongo 集成测试覆盖）。
    #[test]
    fn build_submitted_by_request_line_maps_empty_rows_to_empty_map() {
        assert!(build_submitted_by_request_line(Vec::new()).is_empty());

        let document = doc! {
            "_id": "request-line-1",
            "quantity": { "$numberDecimal": "1.500000" },
            "amount": { "$numberDecimal": "20.00" },
        };
        let row: SubmittedActionTotalRow =
            mongodb::bson::deserialize_from_document(document).expect("合法 Decimal128 必须成功");
        let map = build_submitted_by_request_line(vec![row]);
        assert_eq!(map.len(), 1);
        let totals = map
            .get(&MallAfterSalesRequestLineId::new("request-line-1"))
            .expect("聚合行必须按申请行归组");
        assert_eq!(totals.quantity, Quantity::from_str("1.5").expect("合法数量"));
        assert_eq!(totals.amount, Amount::from_str("20.00").expect("合法金额"));
    }
}
