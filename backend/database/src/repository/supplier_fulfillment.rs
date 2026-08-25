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

use entities::ids::{
    MallAfterSalesRequestId, MallOrderId, MallOrderItemId, SupplierAccountId, SupplierApiConnectionId,
    SupplierFulfillmentOrderId, SupplierOfferingRevisionId, SupplierOrderActionId, SupplierRefundFactId,
};
use entities::mall_after_sales::MallAfterSalesRequestLine;
use entities::mall_order::MallOrderItem;
use entities::supplier_fulfillment::{
    CancelStatus, FulfillmentStatus, RefundStatus, SupplierFulfillmentItem, SupplierFulfillmentOrder,
    SupplierOrderAction, SupplierOrderActionLine, SupplierOrderStatusHistory, SupplierRefundAllocation,
    SupplierRefundFact,
};
use entities::supplier_offering::{SupplierOffering, SupplierOfferingRevision};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::{SupplierFulfillmentExt, SupplierOfferingExt};
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

/// D32 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `SupplierFulfillmentExt::supplier_fulfillment()` 访问。
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
