//! 域 D32 `supplier_fulfillment` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS` 等值。

use entities::supplier_fulfillment::{
    SupplierFulfillmentItem, SupplierFulfillmentOrder, SupplierOrderAction, SupplierOrderActionLine,
    SupplierOrderStatusHistory, SupplierRefundAllocation, SupplierRefundFact,
};
use mongodb::Database;

use super::super::supplier_fulfillment::{SupplierFulfillmentOrderFilter, SupplierFulfillmentRepository};
use crate::Repository;

/// 域 D32 仓储访问器。
pub trait SupplierFulfillmentExt {
    /// `supplier_fulfillment_order` 集合名。
    const SUPPLIER_FULFILLMENT_ORDERS: &'static str = "supplier_fulfillment_orders";
    /// `supplier_fulfillment_item` 集合名。
    const SUPPLIER_FULFILLMENT_ITEMS: &'static str = "supplier_fulfillment_items";
    /// `supplier_order_action` 集合名。
    const SUPPLIER_ORDER_ACTIONS: &'static str = "supplier_order_actions";
    /// `supplier_order_action_line` 集合名。
    const SUPPLIER_ORDER_ACTION_LINES: &'static str = "supplier_order_action_lines";
    /// `supplier_order_status_history` 集合名。
    const SUPPLIER_ORDER_STATUS_HISTORIES: &'static str = "supplier_order_status_histories";
    /// `supplier_refund_fact` 集合名。
    const SUPPLIER_REFUND_FACTS: &'static str = "supplier_refund_facts";
    /// `supplier_refund_allocation` 集合名。
    const SUPPLIER_REFUND_ALLOCATIONS: &'static str = "supplier_refund_allocations";

    /// 供应商履约订单列表筛选条件类型（定义见 `repository::supplier_fulfillment`）。
    type SupplierFulfillmentOrderFilter;

    /// 获取 `supplier_fulfillment_order` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_fulfillment::SupplierFulfillmentOrder>`。
    fn supplier_fulfillment_orders(&self) -> Repository<'_, SupplierFulfillmentOrder>;

    /// 获取 `supplier_fulfillment_item` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_fulfillment::SupplierFulfillmentItem>`。
    fn supplier_fulfillment_items(&self) -> Repository<'_, SupplierFulfillmentItem>;

    /// 获取 `supplier_order_action` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_fulfillment::SupplierOrderAction>`。
    fn supplier_order_actions(&self) -> Repository<'_, SupplierOrderAction>;

    /// 获取 `supplier_order_action_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_fulfillment::SupplierOrderActionLine>`。
    fn supplier_order_action_lines(&self) -> Repository<'_, SupplierOrderActionLine>;

    /// 获取 `supplier_order_status_history` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_fulfillment::SupplierOrderStatusHistory>`。
    fn supplier_order_status_histories(&self) -> Repository<'_, SupplierOrderStatusHistory>;

    /// 获取 `supplier_refund_fact` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_fulfillment::SupplierRefundFact>`。
    fn supplier_refund_facts(&self) -> Repository<'_, SupplierRefundFact>;

    /// 获取 `supplier_refund_allocation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::supplier_fulfillment::SupplierRefundAllocation>`。
    fn supplier_refund_allocations(&self) -> Repository<'_, SupplierRefundAllocation>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `SupplierFulfillmentRepository` 实例。
    fn supplier_fulfillment(&self) -> SupplierFulfillmentRepository<'_>;
}

impl SupplierFulfillmentExt for Database {
    type SupplierFulfillmentOrderFilter = SupplierFulfillmentOrderFilter;

    fn supplier_fulfillment_orders(&self) -> Repository<'_, SupplierFulfillmentOrder> {
        Repository::new(self, Self::SUPPLIER_FULFILLMENT_ORDERS)
    }

    fn supplier_fulfillment_items(&self) -> Repository<'_, SupplierFulfillmentItem> {
        Repository::new(self, Self::SUPPLIER_FULFILLMENT_ITEMS)
    }

    fn supplier_order_actions(&self) -> Repository<'_, SupplierOrderAction> {
        Repository::new(self, Self::SUPPLIER_ORDER_ACTIONS)
    }

    fn supplier_order_action_lines(&self) -> Repository<'_, SupplierOrderActionLine> {
        Repository::new(self, Self::SUPPLIER_ORDER_ACTION_LINES)
    }

    fn supplier_order_status_histories(&self) -> Repository<'_, SupplierOrderStatusHistory> {
        Repository::new(self, Self::SUPPLIER_ORDER_STATUS_HISTORIES)
    }

    fn supplier_refund_facts(&self) -> Repository<'_, SupplierRefundFact> {
        Repository::new(self, Self::SUPPLIER_REFUND_FACTS)
    }

    fn supplier_refund_allocations(&self) -> Repository<'_, SupplierRefundAllocation> {
        Repository::new(self, Self::SUPPLIER_REFUND_ALLOCATIONS)
    }

    fn supplier_fulfillment(&self) -> SupplierFulfillmentRepository<'_> {
        SupplierFulfillmentRepository::new(self)
    }
}
