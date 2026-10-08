//! 库存调整流程：创建、提交、撤回与过账。

use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_workflow::ApprovalObjectReadPort;
use mongodb::Database;

mod adapter;
mod approval_prepare;
mod approval_query;
pub mod cancel_approval;
mod cancel_facts;
pub(super) mod cancel_persist;
pub(super) mod cancel_runtime;
mod create;
mod mapping;
mod persist;
mod post;
mod query;
mod submit;

#[cfg(test)]
mod start_tests;

/// Cross-domain inventory adjustment process service.
///
/// Holds the root transaction for submit/cancel/create and reuses the caller
/// Executor for approval-runtime post and cancel actions.
pub struct InventoryAdjustmentService {
    db: Database,
    rbac: SharedRbacService,
    object_read: Arc<dyn ApprovalObjectReadPort>,
}

impl InventoryAdjustmentService {
    /// 创建绑定到 `db` 的库存调整流程服务。
    ///
    /// # 参数
    /// * `db` - 各领域仓储共用的 MongoDB 数据库。
    /// * `rbac` - 审批绑定与授权使用的共享 RBAC。
    ///
    /// # 返回
    /// 返回按命令复用执行器的流程服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac, object_read: Arc::new(erp_workflow::FailClosedObjectReadPort) }
    }

    /// 注入组合根的对象读取端口，供审批绑定使用。
    ///
    /// 消耗 `self` 并替换对象读取实现。
    ///
    /// # 参数
    /// * `object_read` - 组合根提供的审批对象读取端口。
    ///
    /// # 返回
    /// 返回替换对象读取端口后的流程服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_object_read(mut self, object_read: Arc<dyn ApprovalObjectReadPort>) -> Self {
        self.object_read = object_read;
        self
    }

    /// 库存领域服务，供流程读取调整单表头、明细与过账流水。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回绑定当前数据库与 RBAC 的库存领域服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn inventory(&self) -> erp_inventory::InventoryService {
        crate::adapters::inventory_service(self.db.clone(), self.rbac.clone())
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
