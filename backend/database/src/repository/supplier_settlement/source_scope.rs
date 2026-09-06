use crate::repository::owned::{
    SupplierFulfillmentItemRepository, SupplierFulfillmentOrderRepository,
    SupplierRefundAllocationRepository, SupplierRefundFactRepository,
};
use std::collections::BTreeSet;

use entities::supplier_fulfillment::{
    SupplierFulfillmentItem, SupplierFulfillmentOrder, SupplierRefundAllocation, SupplierRefundFact,
};
use entities::supplier_settlement::SettlementPeriod;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId};
use mongodb::bson::{doc, Document};

use super::{
    SupplierSettlementRepository, SUPPLIER_FULFILLMENT_ITEMS, SUPPLIER_FULFILLMENT_ORDERS,
    SUPPLIER_REFUND_ALLOCATIONS, SUPPLIER_REFUND_FACTS,
};
use persistence_core::Executor;
use persistence_core::Result;

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

impl<'a> SupplierSettlementRepository<'a> {
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
        let refund_facts: Vec<SupplierRefundFact> =
            SupplierRefundFactRepository::new(self.db, SUPPLIER_REFUND_FACTS)
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
            SupplierFulfillmentOrderRepository::new(self.db, SUPPLIER_FULFILLMENT_ORDERS)
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
        let items: Vec<SupplierFulfillmentItem> =
            SupplierFulfillmentItemRepository::new(self.db, SUPPLIER_FULFILLMENT_ITEMS)
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
            let extra = SupplierFulfillmentOrderRepository::new(self.db, SUPPLIER_FULFILLMENT_ORDERS)
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
            SupplierRefundAllocationRepository::new(self.db, SUPPLIER_REFUND_ALLOCATIONS)
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
pub(super) fn refund_fact_scope_filter(
    supplier_id: &SupplierAccountId,
    start_secs: i64,
    end_secs: i64,
) -> Document {
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
pub(super) fn order_scope_filter(
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
pub(super) fn item_scope_filter(
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
