//! 域 D13 `sales_order` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 建单（订单 + 稳定明细 + 工作副本 + 工作副本行 + 审计）：跨集合 →
//!   `database::Transactional::with_transaction`；
//! - 提交（提交快照 + 订单审核轨推进 + 审批记录/采购确认批次 + 待办 + 审计）：
//!   跨集合 → 同一事务模板；
//! - 保存草稿 / 作废：跨集合（工作副本行替换 + 头 CAS + 审计）→ 事务；
//! - 列表 / 详情：单集合无跨步骤原子性要求 → `&mut NoTransaction`。
//!
//! 跨域协作（P3 §2）：
//! - D08 `customer_accounts`：客户存在性校验；
//! - D12 `contracts`：合同存在性校验；
//! - D14 `sales_order_reviews` / `procurement_confirmations`：提交时按业务性质
//!   派发审批记录（卡券 → 销售领导审批）或采购二次确认批次（实物及服务）；
//! - D03 `work_items`：待办派发；
//! - D02 `audit_logs`：审计。
//!
//! 幂等：提交入口先按服务端摘要后的业务幂等键读取事务收据，同键同载荷在
//! 工作副本版本校验之前返回原提交，同键异载荷冲突；建单按 `order_no` 唯一索引兜底（409）。

use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use mongodb::Database;

mod adapter;
mod command;
mod draft_working_copy;
mod dto;
mod formalize;
mod mapper;
mod pricing;
mod procurement_rejection;
mod query;
mod start_approval;
mod status;

pub use self::adapter::sales_order_object_readable;
pub use self::dto::{
    ActiveCardSalesApprovalView, ActiveLowMarginManagerConfirmationView, CancelSalesOrderApprovalRequest,
    CardSalesApprovalAllowedAction, CloseEligibilityView, CreateSalesOrderRequest, DocumentApprovalView,
    LowMarginManagerAllowedAction, OpenProcurementRejectionView, PageView, ProcurementRejectionAllowedAction,
    RevisionView, SalesOrderCreateIntent, SalesOrderDetailView, SalesOrderDraftLineRequest,
    SalesOrderDraftRequest, SalesOrderLineView, SalesOrderListParams, SalesOrderStageSummary, SalesOrderView,
    SalesOrderWorkingCopyLineView, SaveWorkingCopyRequest, SubmissionView, SubmitSalesOrderRequest,
    VoidSalesOrderRequest, WorkingCopyView,
};
pub use self::procurement_rejection::{
    ProcurementRejectionBusinessResult, ResolveProcurementRejectionCommand, ResolveProcurementRejectionResult,
};

/// 销售单服务。
///
/// 提供销售单建单、草稿保存、提交、作废与查询编排。
pub struct SalesOrderService {
    db: Database,
    rbac: Option<SharedRbacService>,
}

impl SalesOrderService {
    /// 创建销售单服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db, rbac: None }
    }

    /// 创建可计算当前操作人审批动作的销售单服务。
    ///
    /// # 返回
    /// 返回同时绑定数据库和当前应用授权源的服务。
    pub fn with_rbac(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac: Some(rbac) }
    }

    /// 读取创建绑定所需的授权源。
    ///
    /// # 错误
    /// 未注入 RBAC 时返回内部错误，不得跳过绑定。
    pub(super) fn require_rbac(&self) -> Result<&SharedRbacService> {
        self.rbac
            .as_ref()
            .ok_or_else(|| Error::Internal("销售单审批绑定需要授权源".to_string()))
    }
}
