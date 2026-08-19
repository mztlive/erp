//! 域 D15 `purchase_order` 服务编排（页面：W08）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 依据创建采购单、保存草稿、提交启动审批、最终通过生效、撤回与采购变更提交
//!   均为跨集合写入 → `database::Transactional::with_transaction`；
//! - 最终通过（§8.1.4）在单事务内：锁定提交 → 逐行复验采购确认来源 →
//!   复制为生效版本与版本行 → 形成销售分配 → 推进采购状态与版本指针 →
//!   形成应付原始分录与 `CONFIRMED` 成本事实 → 审计；
//! - 采购变更生效（§8.1.3 采购部分）在单事务内：基准版本校验 → 新版本/版本行/
//!   分配 → 应付与成本差额 → 当前版本指针推进，不修改已发生事实。
//!
//! 跨域协作（只经 DatabaseExt 调对方 Repository，禁止 Service 依赖 Service）：
//! - D09 `supplier`：供应商角色与商务结算版本（提交快照）；
//! - D14 `sales_review`：采购二次确认及其分行（创建依据）；
//! - D13 `sales_order`：销售提交行快照（商品名/规格/单位/SKU）与销售版本行
//!   （销售分配指向）——D13 不在 domains.md 声明清单，属越界读，见最终报告；
//! - D07 `party`：主体名称（供应商名称快照）——同上，报告协调人；
//! - D19 `payable`：应付子账与原始应付分录（审核通过、变更差额）；
//! - D20 `cost`：`CONFIRMED` 成本事实（审核通过、变更差额）；
//! - D03 `work_item`：采购审核待办（提交创建、审核完成）。

use mongodb::Database;

mod adapter;
mod cancel_approval;
mod change;
mod change_adapter;
mod change_cancel;
mod change_start;
mod creation_basis;
mod draft_edit;
mod draft_from_confirmation;
mod dto;
mod formalization;
mod query;
mod review;
mod shared;
mod start_approval;
mod submission;
mod view_mapping;

pub use self::adapter::purchase_order_object_readable;
pub use self::change_adapter::purchase_change_order_object_readable;
pub(crate) use self::draft_from_confirmation::create_drafts_from_confirmation_lines;
pub use self::dto::{
    CancelPurchaseChangeApprovalRequest, CancelPurchaseOrderApprovalRequest,
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderResult, CreationBasisLineView, CreationBasisView,
    DocumentApprovalView, EffectPurchaseChangeRequest, PageView, PurchaseActionBlockerView,
    PurchaseChangeEffectResult, PurchaseChangeOrderListParams, PurchaseChangeOrderView,
    PurchaseChangeSubmitResult, PurchaseOrderCenterView, PurchaseOrderLineView, PurchaseOrderListItemView,
    PurchaseOrderListParams, PurchaseOrderReviewDecisionCommand, PurchaseOrderReviewDecisionResult,
    PurchaseReviewDomainAction, PurchaseReviewResult, PurchaseReviewWorkItemView,
    PurchaseSalesAllocationView, ReviewPurchaseOrderCommand, SavePurchaseOrderDraftRequest,
    SavePurchaseOrderDraftResult, SavePurchaseOrderLine, StartPurchaseChangeRequest,
    StartPurchaseChangeResult, SubmitPurchaseChangeRequest, SubmitPurchaseOrderRequest,
    SubmitPurchaseOrderResult, TotalsView,
};

use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

/// 采购单服务。
///
/// 提供采购单从依据创建绑定、草稿保存、提交启动审批、最终通过生效与撤回编排。
pub struct PurchaseOrderService {
    db: Database,
    rbac: Option<SharedRbacService>,
}

impl PurchaseOrderService {
    /// 创建采购单服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db, rbac: None }
    }

    /// 创建可绑定发布定义的采购单服务。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `rbac` - 共享授权源
    ///
    /// # 返回
    /// 返回同时绑定数据库和授权源的服务。
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
            .ok_or_else(|| Error::Internal("采购单审批绑定需要授权源".to_string()))
    }
}
