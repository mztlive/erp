//! 域 D29 `mall_order` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 两侧统一取
//! `<mongodb::Database as MallOrderExt>::MALL_ORDER_FACTS` 等值，禁止字面量重复。
//!
//! 五类关键事实及消费、成本评估是正式事实（§4.5 不设业务软删除），只暴露
//! 只读追加仓储（`MallOrderFactRepository` 等），不暴露带软删除方法的通用
//! `Repository`；`mall_order`/`mall_order_item`/`mall_payment_source`/
//! `mall_item_funding_allocation` 是订单追溯聚合，走通用 `Repository`。

use entities::mall_order::{MallItemFundingAllocation, MallOrder, MallOrderItem, MallPaymentSource};
use mongodb::Database;

use super::super::mall_order::{
    MallConsumptionCostAssessmentRepository, MallConsumptionEntryFilter, MallConsumptionEntryRepository,
    MallOrderCancelFactRepository, MallOrderCompletionFactRepository, MallOrderFactFilter,
    MallOrderFactRepository, MallOrderFilter, MallOrderRepository,
};
use crate::Repository;

/// 域 D29 仓储访问器。
pub trait MallOrderExt {
    /// `mall_order_fact` 集合名。
    const MALL_ORDER_FACTS: &'static str = "mall_order_facts";
    /// `mall_order_cancel_fact` 集合名。
    const MALL_ORDER_CANCEL_FACTS: &'static str = "mall_order_cancel_facts";
    /// `mall_order_completion_fact` 集合名。
    const MALL_ORDER_COMPLETION_FACTS: &'static str = "mall_order_completion_facts";
    /// `mall_order` 集合名。
    const MALL_ORDERS: &'static str = "mall_orders";
    /// `mall_order_item` 集合名。
    const MALL_ORDER_ITEMS: &'static str = "mall_order_items";
    /// `mall_payment_source` 集合名。
    const MALL_PAYMENT_SOURCES: &'static str = "mall_payment_sources";
    /// `mall_item_funding_allocation` 集合名。
    const MALL_ITEM_FUNDING_ALLOCATIONS: &'static str = "mall_item_funding_allocations";
    /// `mall_consumption_entry` 集合名。
    const MALL_CONSUMPTION_ENTRIES: &'static str = "mall_consumption_entries";
    /// `mall_consumption_cost_assessment` 集合名。
    const MALL_CONSUMPTION_COST_ASSESSMENTS: &'static str = "mall_consumption_cost_assessments";

    /// 关键事实列表筛选条件类型（定义见 `repository::mall_order`）。
    type MallOrderFactFilter;

    /// 商城订单列表筛选条件类型（定义见 `repository::mall_order`）。
    type MallOrderFilter;

    /// 消费事实列表筛选条件类型（定义见 `repository::mall_order`）。
    type MallConsumptionEntryFilter;

    /// 获取 `mall_order_fact` 集合的只读追加仓储。
    ///
    /// 关键事实是不可变正式事实（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `MallOrderFactRepository` 实例。
    fn mall_order_facts(&self) -> MallOrderFactRepository<'_>;

    /// 获取 `mall_order_cancel_fact` 集合的只读追加仓储。
    ///
    /// 取消事实是不可变正式事实（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `MallOrderCancelFactRepository` 实例。
    fn mall_order_cancel_facts(&self) -> MallOrderCancelFactRepository<'_>;

    /// 获取 `mall_order_completion_fact` 集合的只读追加仓储。
    ///
    /// 完成事实是不可变正式事实（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `MallOrderCompletionFactRepository` 实例。
    fn mall_order_completion_facts(&self) -> MallOrderCompletionFactRepository<'_>;

    /// 获取 `mall_order` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_order::MallOrder>`。
    fn mall_orders(&self) -> Repository<'_, MallOrder>;

    /// 获取 `mall_order_item` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_order::MallOrderItem>`。
    fn mall_order_items(&self) -> Repository<'_, MallOrderItem>;

    /// 获取 `mall_payment_source` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_order::MallPaymentSource>`。
    fn mall_payment_sources(&self) -> Repository<'_, MallPaymentSource>;

    /// 获取 `mall_item_funding_allocation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_order::MallItemFundingAllocation>`。
    fn mall_item_funding_allocations(&self) -> Repository<'_, MallItemFundingAllocation>;

    /// 获取 `mall_consumption_entry` 集合的只读追加仓储。
    ///
    /// 消费事实是不可变正式事实（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `MallConsumptionEntryRepository` 实例。
    fn mall_consumption_entries(&self) -> MallConsumptionEntryRepository<'_>;

    /// 获取 `mall_consumption_cost_assessment` 集合的只读追加仓储。
    ///
    /// 成本评估是不可变正式事实（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `MallConsumptionCostAssessmentRepository` 实例。
    fn mall_consumption_cost_assessments(&self) -> MallConsumptionCostAssessmentRepository<'_>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `MallOrderRepository` 实例。
    fn mall_order(&self) -> MallOrderRepository<'_>;
}

impl MallOrderExt for Database {
    type MallOrderFactFilter = MallOrderFactFilter;
    type MallOrderFilter = MallOrderFilter;
    type MallConsumptionEntryFilter = MallConsumptionEntryFilter;

    fn mall_order_facts(&self) -> MallOrderFactRepository<'_> {
        MallOrderFactRepository::new(self)
    }

    fn mall_order_cancel_facts(&self) -> MallOrderCancelFactRepository<'_> {
        MallOrderCancelFactRepository::new(self)
    }

    fn mall_order_completion_facts(&self) -> MallOrderCompletionFactRepository<'_> {
        MallOrderCompletionFactRepository::new(self)
    }

    fn mall_orders(&self) -> Repository<'_, MallOrder> {
        Repository::new(self, Self::MALL_ORDERS)
    }

    fn mall_order_items(&self) -> Repository<'_, MallOrderItem> {
        Repository::new(self, Self::MALL_ORDER_ITEMS)
    }

    fn mall_payment_sources(&self) -> Repository<'_, MallPaymentSource> {
        Repository::new(self, Self::MALL_PAYMENT_SOURCES)
    }

    fn mall_item_funding_allocations(&self) -> Repository<'_, MallItemFundingAllocation> {
        Repository::new(self, Self::MALL_ITEM_FUNDING_ALLOCATIONS)
    }

    fn mall_consumption_entries(&self) -> MallConsumptionEntryRepository<'_> {
        MallConsumptionEntryRepository::new(self)
    }

    fn mall_consumption_cost_assessments(&self) -> MallConsumptionCostAssessmentRepository<'_> {
        MallConsumptionCostAssessmentRepository::new(self)
    }

    fn mall_order(&self) -> MallOrderRepository<'_> {
        MallOrderRepository::new(self)
    }
}
