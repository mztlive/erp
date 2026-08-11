//! 域 D14 `sales_review` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - §8.1.1 采购确认通过：单个业务事务内同时写「销售版本 + 销售状态 + 应收
//!   原始分录 + 待办收尾/派发 + 审计」（P3 §2 跨域协作模板）；
//! - §8.1.2（本批部分）卡券运营审批通过：同事务形成销售版本、应收并推进销售
//!   状态（执行投影修订属 D27，本批不写）；
//! - §8.1.3（本批部分）销售变更生效：校验基准版本仍为当前版本，追加新版本与
//!   应收差额，不修改已发生事实；
//! - 驳回 / 保存分行 / 变更生命周期其余入口：跨集合事务或单集合直写。
//!
//! 跨域协作（P3 §2）：
//! - D13 `sales_order*` Repository：读取销售单/提交/提交行，形成正式版本
//!   （`formalize_submission`）；
//! - D18 `receivable*` Repository：应收往来子账 + 原始应收分录/差额分录
//!   （`create_receivable_with_entry`，无对应 Service 依赖）；
//! - D03 `work_items`：待办完成与派发；
//! - D02 `audit_logs`：审计。
//!
//! 幂等：通过/提交类入口按「状态机终态 + 业务对象查询」去重（重复通过返回既有
//! 结果，不重复产生版本/分录/待办）。

use database::SalesReviewExt;
use mongodb::Database;

mod dto;
mod formalization;
mod procurement_confirmation;
mod procurement_decision;
mod sales_change_mapping;
mod sales_change_order;
mod sales_change_review;
mod sales_order_review;
mod sourcing;

pub use self::dto::{
    ApproveProcurementConfirmationRequest, ChangeReviewDecisionRequest, CreateSalesChangeOrderRequest,
    GeneratedPurchaseOrderView, PageView, ProcurementConfirmationDecisionView,
    ProcurementConfirmationDetailView, ProcurementConfirmationLineView, ProcurementConfirmationListParams,
    ProcurementConfirmationView, ProcurementRecommendationIssueView, ProcurementRecommendationLineView,
    ProcurementRecommendationOrderView, ProcurementRecommendationView, RejectProcurementConfirmationRequest,
    ReviewDecisionRequest, SalesChangeDraftRequest, SalesChangeLineRequest, SalesChangeOrderDetailView,
    SalesChangeOrderListParams, SalesChangeOrderView, SalesOrderReviewListParams, SalesOrderReviewView,
    SaveProcurementConfirmationLinesRequest, SubmitSalesChangeRequest, VoidSalesChangeOrderRequest,
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
