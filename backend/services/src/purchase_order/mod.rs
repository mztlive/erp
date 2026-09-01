//! 域 D15 `purchase_order` 服务编排（页面：W08）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 依据/选源创建采购单并提交审批、保存草稿、提交启动审批、最终通过生效、撤回与采购变更提交
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

use database::{AccessControlExt, Executor, PurchaseOrderExt};
use mongodb::Database;

mod adapter;
mod allocation_maintenance;
mod approval_query;
mod authorization;
mod cancel_approval;
mod change;
mod change_adapter;
mod change_cancel;
mod change_start;
pub(crate) mod coverage;
mod create_submit;
mod creation_basis;
mod draft_edit;
mod dto;
mod formalization;
mod procurement_task_sync;
mod query;
mod review;
mod shared;
mod sourcing_create;
mod start_approval;
mod submission;
mod view_mapping;
mod void_order;

pub use self::adapter::purchase_order_object_readable;
pub use self::change_adapter::purchase_change_order_object_readable;
pub use self::dto::{
    CancelPurchaseChangeApprovalRequest, CancelPurchaseOrderApprovalRequest,
    CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderLineRequest, CreatePurchaseOrderResult,
    CreatePurchaseOrdersFromSourcingRequest, CreatePurchaseOrdersFromSourcingResult, CreationBasisLineView,
    CreationBasisListParams, CreationBasisView, DocumentApprovalView, EffectPurchaseChangeRequest,
    ExistingStockReservationResult, PageView, PurchaseActionBlockerView, PurchaseChangeEffectResult,
    PurchaseChangeOrderListParams, PurchaseChangeOrderView, PurchaseChangeSubmitResult,
    PurchaseOrderCenterView, PurchaseOrderLineView, PurchaseOrderListItemView, PurchaseOrderListParams,
    PurchaseReviewResult, PurchaseSalesAllocationView, SavePurchaseOrderDraftRequest,
    SavePurchaseOrderDraftResult, SavePurchaseOrderLine, SavePurchaseOrderLinePatch, SourcingLineAssignment,
    StartPurchaseChangeRequest, StartPurchaseChangeResult, SubmitPurchaseChangeRequest,
    SubmitPurchaseOrderRequest, SubmitPurchaseOrderResult, SupplySourceType, TotalsView,
    VoidPurchaseOrderRequest, VoidPurchaseOrderResult,
};
pub(crate) use self::procurement_task_sync::sync_procurement_tasks_for_sales_order;

use crate::approval::policy::ApprovalDomainAction;
use crate::audit::AuditActor;
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

    /// 读取采购单写命令所需的共享授权源。
    ///
    /// # 错误
    /// 未注入 RBAC 时返回内部错误，不得跳过绑定。
    pub(super) fn require_rbac(&self) -> Result<&SharedRbacService> {
        self.rbac
            .as_ref()
            .ok_or_else(|| Error::Internal("采购单写命令需要授权源".to_string()))
    }
}

/// 在审批运行时持有的事务内撤回采购单审批。
///
/// # 错误
/// 采购单不存在、提交快照缺失、状态迁移或 CAS 写入失败时返回错误。
pub(crate) async fn cancel_order_approval_in_transaction(
    db: &Database,
    id: &str,
    action: ApprovalDomainAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut order = db
        .purchase_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
    let submission_id = order
        .current_submission_id
        .clone()
        .ok_or_else(|| Error::ConflictError("采购单缺少当前审批提交".to_string()))?;
    adapter::execute_purchase_order_domain_action(&mut order, action, submission_id.as_ref(), actor.id())?;
    db.purchase_orders().update(&mut order, executor).await?;
    let audit =
        actor
            .clone()
            .resource_log("purchase_order.cancel_approval", "purchase_order", id.to_string())?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}

/// 在审批运行时持有的事务内撤回采购变更审批。
///
/// # 错误
/// 采购变更单不存在、状态迁移或 CAS 写入失败时返回错误。
pub(crate) async fn cancel_change_approval_in_transaction(
    db: &Database,
    id: &str,
    action: ApprovalDomainAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut change = db
        .purchase_change_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
    change_adapter::execute_purchase_change_domain_action(&mut change, action, actor.id())?;
    db.purchase_change_orders().update(&mut change, executor).await?;
    let audit = actor.clone().resource_log(
        "purchase_change_order.cancel_approval",
        "purchase_change_order",
        id.to_string(),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}
