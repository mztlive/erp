//! 库存调整流程：创建、提交、撤回与过账。

use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_inventory::InventoryService;
use erp_read_models::inventory_center::InventoryAdjustmentReadService;
use mongodb::Database;

use crate::workflow::FulfillmentWorkflow;

mod adapter;
pub(crate) mod approval_prepare;
pub mod cancel_approval;
pub(crate) mod cancel_authorization;
mod cancel_facts;
pub(super) mod cancel_persist;
pub(super) mod cancel_runtime;
mod create;
mod mapping;
mod persist;
mod post;
mod read_policy;
mod reject;
mod submit;

#[cfg(test)]
mod start_tests;

/// Cross-domain inventory adjustment process service.
///
/// Holds the root transaction for submit/cancel/create and reuses the caller
/// Executor for approval-runtime post and cancel actions.
#[derive(Clone)]
pub struct InventoryAdjustmentProcess {
    identity: Arc<AuthorizationService>,
    inventory_service: Arc<InventoryService>,
    workflow: Arc<dyn FulfillmentWorkflow>,
    db: Database,
}

impl InventoryAdjustmentProcess {
    /// 创建绑定到 `db` 的库存调整流程服务。
    ///
    /// # 参数
    /// * `db` - 各领域仓储共用的 MongoDB 数据库。
    /// * `identity` - 与工作流、库存能力同源的共享身份服务。
    /// * `workflow` - 应用装配的工作流绑定、材料和正式任务能力。
    /// * `inventory_service` - 已注入权威库存授权与事实 Port 的领域服务。
    ///
    /// # 返回
    /// 返回按命令复用执行器的流程服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        identity: Arc<AuthorizationService>,
        workflow: Arc<dyn FulfillmentWorkflow>,
        inventory_service: Arc<InventoryService>,
    ) -> Self {
        Self { identity, workflow, inventory_service, db }
    }

    /// 装配库存调整跨域读取与正式命令共用的动作政策。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回未执行 I/O 的读取模型。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn read_model(&self) -> InventoryAdjustmentReadService {
        InventoryAdjustmentReadService::new(
            self.db.clone(),
            self.inventory_service.clone(),
            Arc::new(read_policy::StockAdjustmentReadPolicy { workflow: self.workflow.clone() }),
        )
    }

    /// 库存领域服务，供流程读取调整单表头、明细与过账流水。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回绑定当前数据库及权威授权能力的库存领域服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn inventory(&self) -> &InventoryService {
        &self.inventory_service
    }
}

/// 流程模块名称。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回固定名称 `inventory_adjustment`。
///
/// # 错误
/// 不返回错误。
pub fn process_name() -> &'static str {
    "inventory_adjustment"
}
