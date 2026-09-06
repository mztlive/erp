//! 域 D13 `sales_order` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 建单（订单 + 稳定明细 + 工作副本 + 工作副本行 + 审计）：跨集合 →
//!   `persistence_core::Transactional::with_transaction`；
//! - 提交（提交快照 + 订单审核轨推进 + 审批记录/采购确认批次 + 待办 + 审计）：
//!   跨集合 → 同一事务模板；
//! - 保存草稿 / 作废：跨集合（工作副本行替换 + 头 CAS + 审计）→ 事务；
//! - 列表 / 详情：单集合无跨步骤原子性要求 → `&mut NoTransaction`。
//!
//! 跨域协作（P3 §2）：
//! - D08 `customer_accounts`：客户存在性校验；
//! - D12 `contracts`：合同存在性校验；
//! - D14 旧卡券/采购确认路径不得由新提交写入；`GoodsService` 与 `Voucher`
//!   均由提交命令启动统一审批；
//! - D03 `work_items`：待办派发；
//! - D02 `audit_logs`：审计。
//!
//! 幂等：提交入口先按服务端摘要后的业务幂等键读取事务收据，同键同载荷在
//! 工作副本版本校验之前返回原提交，同键异载荷冲突；建单按 `order_no` 唯一索引兜底（409）。

use crate::errors::{Error, Result};
use application_core::AuditActor;
use bpm::SubjectRef;
use database::SalesOrderExt;
use entities::sales_order::BusinessType;
use erp_audit::AuditActorLogs;
use erp_audit::AuditExt;
use erp_identity::SharedRbacService;
use erp_workflow::entity::approval_integration::SalesBusinessKind;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::service::approval::policy::ApprovalDomainAction;
use mongodb::Database;
use persistence_core::Executor;

mod adapter;
mod approval_query;
mod cancel_approval;
mod command;
mod draft_working_copy;
mod dto;
mod formalize;
mod mapper;
mod procurement;
mod progress;
mod query;
mod start_approval;
mod status;

pub use self::adapter::sales_order_object_readable;
pub use self::dto::{
    ActiveCardSalesApprovalView, CancelSalesOrderApprovalRequest, CardSalesApprovalAllowedAction,
    CloseEligibilityView, CreateSalesOrderRequest, DocumentApprovalView, PageView,
    PurchaseCreationAccessView, RevisionView, SalesOrderCreateIntent, SalesOrderDetailView,
    SalesOrderDraftLineRequest, SalesOrderDraftRequest, SalesOrderLineView, SalesOrderListParams,
    SalesOrderStageSummary, SalesOrderView, SalesOrderWorkingCopyLineView, SalesProcurementCoverageView,
    SaveWorkingCopyRequest, SubmissionView, SubmitSalesOrderRequest, VoidSalesOrderRequest, WorkingCopyView,
};
pub(crate) use self::progress::update_sales_order_money_progress;

fn sales_business_kind(business_type: BusinessType) -> SalesBusinessKind {
    match business_type {
        BusinessType::GoodsService => SalesBusinessKind::GoodsService,
        BusinessType::Voucher => SalesBusinessKind::Voucher,
    }
}

/// Map remaining-domain sales `BusinessType` onto the workflow document type.
pub(crate) fn document_type_of_sales_business(business_type: BusinessType) -> DocumentType {
    erp_workflow::entity::approval_integration::document_type_of_sales_business(sales_business_kind(
        business_type,
    ))
}

/// Build a workflow subject ref from remaining-domain sales `BusinessType`.
pub(crate) fn subject_ref_for_sales_business(
    business_type: BusinessType,
    business_object_id: &str,
) -> Result<SubjectRef> {
    erp_workflow::entity::approval_integration::subject_ref_for_sales_business(
        sales_business_kind(business_type),
        business_object_id,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 销售单服务。
///
/// 提供销售单建单、草稿保存、提交、作废与查询编排。
pub struct SalesOrderService {
    db: Database,
    rbac: Option<SharedRbacService>,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
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
        Self {
            db,
            rbac: None,
            object_read: std::sync::Arc::new(erp_workflow::FailClosedObjectReadPort),
        }
    }

    /// Inject composition-root object-read for approval binding.
    pub fn with_object_read(
        mut self,
        object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    ) -> Self {
        self.object_read = object_read;
        self
    }

    /// 创建可计算当前操作人审批动作的销售单服务。
    ///
    /// # 返回
    /// 返回同时绑定数据库和当前应用授权源的服务。
    pub fn with_rbac(db: Database, rbac: SharedRbacService) -> Self {
        Self {
            db,
            rbac: Some(rbac),
            object_read: std::sync::Arc::new(erp_workflow::FailClosedObjectReadPort),
        }
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

/// 在审批运行时持有的事务内撤回销售单审批提交。
///
/// # 错误
/// 单据不存在、动作与销售类型不匹配、状态迁移或 CAS 写入失败时返回错误。
pub async fn cancel_approval_in_transaction(
    db: &Database,
    id: &str,
    action: ApprovalDomainAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut order = db
        .sales_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
    adapter::execute_sales_order_domain_action(&mut order, action, actor.id())?;
    db.sales_orders().update(&mut order, executor).await?;
    let audit = actor
        .clone()
        .resource_log("sales_order.cancel_approval", "sales_order", id.to_string())?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}
