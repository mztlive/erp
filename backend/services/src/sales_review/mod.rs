//! 域 D14 `sales_review` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - §8.1.1 采购确认通过：单个业务事务内同时写「销售版本 + 销售状态 + 应收
//!   原始分录 + 待办收尾/派发 + 审计」（P3 §2 跨域协作模板）；
//! - §8.1.2 卡券审批决定：审批运行时在唯一外层事务内调用本域 action port；
//!   领导决定追加不可变审批事实，运营通过同时形成销售版本、卡券期初应收、
//!   执行投影修订与待发送下发事实；
//! - §8.1.3（本批部分）销售变更生效：校验基准版本仍为当前版本，追加新版本与
//!   应收差额，不修改已发生事实；
//! - 驳回 / 保存分行 / 变更生命周期其余入口：跨集合事务或单集合直写。
//!
//! 跨域协作（P3 §2）：
//! - D13 `sales_order*` Repository：读取销售单/提交/提交行，形成正式版本
//!   （`formalize_submission`）；
//! - D18 `receivable*` Repository：应收往来子账 + 原始应收分录/差额分录
//!   （`create_receivable_with_entry`，无对应 Service 依赖）；
//! - D03 审批运行时：步骤、待办与实例仅在领域 action port 成功后推进；
//! - D27 执行投影：运营通过形成稳定投影、首个修订与待发送下发记录；
//! - D02 `audit_logs`：审计。
//!
//! 幂等：卡券审批由运行时命令收据与领域唯一键仲裁；重复调用先回读已提交结果，
//! 不重复产生审批记录、版本、分录、投影或待办。

use database::SalesReviewExt;
use mongodb::Database;

mod dto;
mod formalization;
mod low_margin_confirmation;
mod procurement_confirmation;
mod procurement_decision;
mod sales_change_mapping;
mod sales_change_order;
mod sales_change_review;
mod sales_order_review;
mod sourcing;

pub use self::dto::{
    ChangeReviewDecisionRequest, CompleteProcurementConfirmationCommand,
    CompleteProcurementConfirmationResult, CreateSalesChangeOrderRequest, PageView,
    ProcurementConfirmationActionBlockerView, ProcurementConfirmationAllowedAction,
    ProcurementConfirmationBusinessResult, ProcurementConfirmationDecision,
    ProcurementConfirmationDetailParams, ProcurementConfirmationDetailView, ProcurementConfirmationLineView,
    ProcurementConfirmationListParams, ProcurementConfirmationView, ProcurementRecommendationIssueView,
    ProcurementRecommendationLineView, ProcurementRecommendationOrderView, ProcurementRecommendationView,
    ProcurementSalesResolution, SalesChangeDraftRequest, SalesChangeLineRequest, SalesChangeOrderDetailView,
    SalesChangeOrderListParams, SalesChangeOrderView, SalesOrderReviewListParams, SalesOrderReviewView,
    SaveProcurementConfirmationAction, SaveProcurementConfirmationLinesRequest,
    SaveProcurementConfirmationResult, SubmitSalesChangeRequest, VoidSalesChangeOrderRequest,
};
pub use self::low_margin_confirmation::{
    CompleteLowMarginManagerConfirmationCommand, CompleteLowMarginManagerConfirmationResult,
    LowMarginManagerConfirmationBusinessResult, LowMarginManagerConfirmationDecision,
};
pub use self::sales_order_review::{
    CancelCardSalesApprovalCommand, CancelCardSalesApprovalResult, CardSalesApprovalActionPort,
    CardSalesApprovalBusinessResult, CardSalesApprovalCancelGuard, CardSalesApprovalCancelledBusinessResult,
    CardSalesApprovalDecision, CardSalesApprovalDecisionGuard, CardSalesExpectedReviewStatus,
    CardSalesReviewDecision, SubmitCardSalesApprovalDecisionCommand, SubmitCardSalesApprovalDecisionResult,
};

/// 审批记录列表筛选条件类型（经 `SalesReviewExt` 关联类型跨 crate 可达）。
type SalesOrderReviewFilter = <mongodb::Database as SalesReviewExt>::SalesOrderReviewFilter;
/// 采购确认列表筛选条件类型。
type ProcurementConfirmationFilter = <mongodb::Database as SalesReviewExt>::ProcurementConfirmationFilter;
/// 销售变更单列表筛选条件类型。
type SalesChangeOrderFilter = <mongodb::Database as SalesReviewExt>::SalesChangeOrderFilter;

/// 销售复核服务。
///
/// 提供采购二次确认、卡券销售审批、销售变更单的决策与生命周期编排。
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
