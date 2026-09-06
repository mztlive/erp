//! 域 D17 `inventory` 服务编排（页面：W10 库存台账）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 单集合无跨步骤原子性要求的 CRUD 传入 `&mut NoTransaction`；
//! - 表头+明细创建、状态迁移+审计、过账（调整单+流水+余额+预占）使用
//!   `persistence_core::Transactional::with_transaction`（§8.2 第 3 条跨集合原子性）。
//!
//! 跨域协作（P3-service-api §2：只调对方 Repository，不依赖对方 Service）：
//! - D11 `warehouse`：仓库代码 + 当前修订名称；
//! - D10 `catalog`：SKU 编号 + 当前修订名称/规格。
//!
//! 过账去重（§8.2）：状态守卫（仅 `IN_APPROVAL` 可由最终通过动作过账）+
//! `stock_movement` 的 `(source_document_id, source_line_id, movement_type)`
//! 唯一索引双重防护，重复过账返回 409，不产生第二条正式流水。

use mongodb::Database;

use crate::iam::SharedRbacService;

mod adapter;
mod adjustment_command;
mod adjustment_post;
mod adjustment_query;
mod adjustment_submit;
mod approval_query;
mod authorization;
mod balance;
mod cancel_approval;
mod dto;
mod movement;
mod reservation;
mod start_approval;

pub use self::dto::{
    CancelStockAdjustmentApprovalRequest, CancelStockAdjustmentApprovalTokenView,
    CreateStockAdjustmentRequest, DocumentApprovalView, ExpectedStockBalanceVersion, PageView,
    StockAdjustmentDetailView, StockAdjustmentLineInput, StockAdjustmentLineUpdateInput,
    StockAdjustmentLineView, StockAdjustmentListParams, StockAdjustmentSubmitResultQuery,
    StockAdjustmentView, StockBalanceDetailView, StockBalanceListParams, StockBalanceView,
    StockMovementListParams, StockMovementView, StockReservationListParams, StockReservationView,
    SubmitStockAdjustmentApprovalTokenView, SubmitStockAdjustmentRequest, UpdateStockAdjustmentRequest,
};

pub(crate) use self::cancel_approval::cancel_stock_adjustment_approval_in_transaction;

/// 库存服务。
///
/// 提供余额/流水/预占/调整单查询、创建绑定、提交启动与普通受控撤回。
/// 最终通过过账和管理员受阻取消由审批动作注册表复用运行时事务执行。
pub struct InventoryService {
    db: Database,
    rbac: SharedRbacService,
}

impl InventoryService {
    /// 创建库存服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `rbac` - 共享 RBAC，用于创建时绑定发布定义
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
}
