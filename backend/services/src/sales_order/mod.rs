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
//! 幂等：提交入口按「同一工作副本已形成提交」去重，重复提交返回既有提交
//! （AGENTS.md 外部依赖容错）；建单按 `order_no` 唯一索引兜底（409）。

use mongodb::Database;

mod command;
mod draft_working_copy;
mod dto;
mod mapper;
mod pricing;
mod query;
mod status;

pub use self::dto::{
    CloseEligibilityView, CreateSalesOrderRequest, OpenProcurementRejectionView, PageView, RevisionView,
    SalesOrderCreateIntent, SalesOrderDetailView, SalesOrderDraftLineRequest, SalesOrderDraftRequest,
    SalesOrderLineView, SalesOrderListParams, SalesOrderStageSummary, SalesOrderView,
    SalesOrderWorkingCopyLineView, SaveWorkingCopyRequest, SubmissionView, SubmitSalesOrderRequest,
    VoidSalesOrderRequest, WorkingCopyView,
};

/// 销售单服务。
///
/// 提供销售单建单、草稿保存、提交、作废与查询编排。
pub struct SalesOrderService {
    db: Database,
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
        Self { db }
    }
}
