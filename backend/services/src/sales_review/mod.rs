//! 域 D14 `sales_review` 服务编排。
//!
//! 销售变更单走统一审批启动/撤回/生效。采购二次确认、低毛利确认与卡券专用
//! 审批运行时已删除，不得回退旧责任动作或旧集合。

use database::SalesReviewExt;
use mongodb::Database;

mod adapter;
mod cancel_approval;
mod dto;
mod formalization;
mod sales_change_mapping;
mod sales_change_order;
mod sourcing;
mod start_approval;

pub use self::adapter::sales_change_order_object_readable;
pub use self::dto::{
    CancelSalesChangeApprovalRequest, ChangeReviewDecisionRequest, CreateSalesChangeOrderRequest,
    DocumentApprovalView, PageView, SalesChangeOrderDetailView, SalesChangeOrderListParams,
    SalesChangeOrderView, SubmitSalesChangeRequest, VoidSalesChangeOrderRequest,
};

/// 销售变更单列表筛选条件类型。
type SalesChangeOrderFilter = <mongodb::Database as SalesReviewExt>::SalesChangeOrderFilter;

/// 销售复核服务。
///
/// 提供销售变更单生命周期与统一审批编排，不再暴露采购确认或卡券专用审批入口。
pub struct SalesReviewService {
    db: Database,
}

impl SalesReviewService {
    /// 创建销售复核服务实例。
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

/// 合同 §4.4.4 目标端口名；落地于本域 `SalesReviewService`。
pub type SalesChangeOrderService = SalesReviewService;
