//! 域 D17 `inventory` 查询与事务内库存写入。
//!
//! 跨域审批绑定、WorkItem 与审计编排由 `erp-fulfillment-execution::inventory_adjustment` 持有。

use std::sync::Arc;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::ports::{
    AdjustmentPeopleFactsPort, AuthorizationPort, CatalogFactsPort, FulfillmentFactsPort, InventoryAction,
    InventoryAuditPort, WarehouseFactsPort,
};

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
#[derive(Clone)]
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
    /// 在原命令执行器证明目标仓库的完整动作资格。
    /// 账号、同角色功能资格和仓库范围均成立时成功。
    /// actor、action、warehouse 为可信账号、固定动作和实际仓库；executor 为原事务。
    /// 对象、范围或功能资格不足拒绝；设施错误透传。
    pub async fn require_warehouse(
        &self,
        actor: &AuditActor,
        action: InventoryAction,
        warehouse: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization.require_warehouse(actor, action, warehouse, executor).await
    }

    /// 创建库存服务实例。
    /// `authorization` 为仓库范围授权端口；`warehouse_facts` 为仓库展示事实端口。
    /// `catalog_facts` 为 SKU 展示事实端口；`fulfillment_facts` 为入库单据号事实端口。
    /// `audit` 为审计持久化端口；`people_facts` 为调整单申请人与当前审批人事实验口。
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

// 只把普通对象拒绝转换为当前入口的不可见结果，设施错误继续失败。
async fn warehouse_allowed(
    port: &dyn AuthorizationPort,
    actor: &AuditActor,
    action: InventoryAction,
    warehouse: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    match port.require_warehouse(actor, action, warehouse, executor).await {
        Ok(()) => Ok(true),
        Err(crate::Error::Forbidden(_) | crate::Error::NotFound(_)) => Ok(false),
        Err(error) => Err(error),
    }
}
