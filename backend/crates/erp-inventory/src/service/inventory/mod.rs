//! 域 D17 `inventory` 查询与事务内库存写入。
//!
//! 跨域审批绑定、WorkItem 与审计编排由 `erp-processes::inventory_adjustment` 持有。

use std::sync::Arc;

use mongodb::Database;

use crate::ports::{
    AdjustmentPeopleFactsPort, AuthorizationPort, CatalogFactsPort, FulfillmentFactsPort, InventoryAuditPort,
    WarehouseFactsPort,
};

mod adjustment_query;
mod balance;
mod mapping;
pub(super) mod movement;
mod reservation;
mod search;
mod stock_write;
mod update;

pub use mapping::build_adjustment_line_updates;
pub use stock_write::apply_posted_adjustment;

/// 库存查询与本域事务内命令。
pub struct InventoryService {
    pub(crate) db: Database,
    pub(crate) authorization: Arc<dyn AuthorizationPort>,
    pub(crate) warehouse_facts: Arc<dyn WarehouseFactsPort>,
    pub(crate) catalog_facts: Arc<dyn CatalogFactsPort>,
    pub(crate) fulfillment_facts: Arc<dyn FulfillmentFactsPort>,
    pub(crate) audit: Arc<dyn InventoryAuditPort>,
    pub(crate) people_facts: Arc<dyn AdjustmentPeopleFactsPort>,
}

impl InventoryService {
    /// 创建库存服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `authorization` - 仓库范围授权端口
    /// * `warehouse_facts` - 仓库展示事实端口
    /// * `catalog_facts` - SKU 展示事实端口
    /// * `fulfillment_facts` - 入库单据号事实端口
    /// * `audit` - 审计持久化端口
    /// * `people_facts` - 调整单申请人与当前审批人事实验口
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(
        db: Database,
        authorization: Arc<dyn AuthorizationPort>,
        warehouse_facts: Arc<dyn WarehouseFactsPort>,
        catalog_facts: Arc<dyn CatalogFactsPort>,
        fulfillment_facts: Arc<dyn FulfillmentFactsPort>,
        audit: Arc<dyn InventoryAuditPort>,
        people_facts: Arc<dyn AdjustmentPeopleFactsPort>,
    ) -> Self {
        Self { db, authorization, warehouse_facts, catalog_facts, fulfillment_facts, audit, people_facts }
    }
}
