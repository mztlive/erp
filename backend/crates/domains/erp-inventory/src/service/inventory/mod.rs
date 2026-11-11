//! 域 D17 `inventory` 查询与事务内库存写入。
//!
//! 跨域审批绑定、WorkItem 与审计编排由 `erp-fulfillment-execution::inventory_adjustment` 持有。

use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use crate::ports::{
    AdjustmentPeopleFactsPort, AuthorizationPort, CatalogFactsPort, FulfillmentFactsPort, InventoryAction,
    InventoryAuditPort, WarehouseFactsPort,
};
use crate::{InventoryAuthorization, Result};

mod adjustment_display;
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
    /// 在调用方执行器内解析本服务已经装配的库存动作资格。
    ///
    /// # 参数
    /// `actor` 为当前操作人，`actions` 为所需动作，`executor` 为当前业务命令的执行器。
    /// # 返回
    /// 返回本服务的权威库存授权事实。
    /// # 错误
    /// 授权来源或事实查询失败时返回原库存错误。
    pub async fn authorize(
        &self,
        actor: &AuditActor,
        actions: &[InventoryAction],
        executor: &mut dyn Executor,
    ) -> Result<InventoryAuthorization> {
        self.authorization.authorize(actor, actions, executor).await
    }

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
    ///
    /// # 错误
    /// 不返回错误。
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
