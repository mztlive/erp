//! 销售生命周期、审批与财务形式化流程。

use std::sync::Arc;

use erp_audit::execution::persist_log;

use crate::services::SalesServices;

mod adapter;
pub mod adapters;
mod authorization;
mod cancel_approval;
mod command;
mod draft_working_copy;
mod formalization_posting;
mod formalize;
mod handover;
mod inquiry;
mod procurement;
mod start_approval;

pub use adapter::sales_order_object_readable;
use erp_identity::AuthorizationService;
use erp_read_models::sales_center::order::SalesOrderReadService;
use erp_sales::entity::sales_order::BusinessType;
use erp_sales::service::sales_order::SalesOrderService;
use erp_workflow::entity::document_registry::DocumentType;
use mongodb::Database;

use crate::{Error, Result};

/// Sales lifecycle commands combining sales writes, provider checks, workflow and audit.
#[derive(Clone)]
pub struct SalesOrderCommandProcess {
    services: Arc<SalesServices>,
    db: Database,
    authorization: Arc<AuthorizationService>,
}
impl SalesOrderCommandProcess {
    /// 用共享授权源构造销售命令流程。
    ///
    /// # 参数
    /// * `db` - 业务数据库。
    /// * `authorization` - 共享授权源。
    /// * `services` - 应用已装配的领域服务、范围及审批能力。
    ///
    /// # 返回
    /// 返回已完整装配授权依赖的命令流程。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>, services: Arc<SalesServices>) -> Self {
        Self { db, services, authorization }
    }
    fn sales(&self) -> SalesOrderService {
        SalesOrderService::new(self.db.clone())
    }
    fn read_model(&self) -> SalesOrderReadService {
        SalesOrderReadService::new(self.db.clone(), self.authorization.clone())
    }
    fn catalog(&self) -> adapters::catalog::CatalogQualificationAdapter {
        adapters::catalog::CatalogQualificationAdapter::new(self.services.catalog.clone())
    }
}
fn document_type_of_sales_business(business_type: BusinessType) -> DocumentType {
    match business_type {
        BusinessType::GoodsService => DocumentType::SalesOrder,
        BusinessType::Voucher => DocumentType::VoucherSalesOrder,
    }
}
/// 按业务性质生成审批主体引用；种类无法映射时返回校验错误。
fn subject_ref_for_sales_business(business_type: BusinessType, id: &str) -> Result<bpm::SubjectRef> {
    erp_workflow::entity::approval_integration::subject_ref_for(
        document_type_of_sales_business(business_type),
        id,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 在审批运行时已有事务内取消销售审批状态。
///
/// 写销售单和审计之前，先按销售类型核对工作流动作。
///
/// # 参数
/// * `db` - 业务数据库。
/// * `id` - 销售单主键。
/// * `action` - 审批领域动作。
/// * `actor` - 审计操作人。
/// * `executor` - 审批运行时已有执行器。
///
/// # 返回
/// 销售单状态与审计都写入后返回。
///
/// # 错误
/// 销售单不存在、动作不属于该类型、状态不允许或仓储写入失败时返回错误。
pub async fn cancel_approval(
    db: &Database,
    id: &str,
    action: erp_workflow::service::approval::policy::ApprovalDomainAction,
    actor: &application_core::AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    use erp_audit::AuditActorLogs;
    use erp_sales::repository::SalesOrderExt;
    let mut order = db
        .sales_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在".into()))?;

    adapter::execute_sales_order_domain_action(&mut order, action, actor.id())?;
    SalesOrderService::new(db.clone()).persist_order(&mut order, executor).await?;

    let audit = actor.clone().resource_log("sales_order.cancel_approval", "sales_order", id.to_string())?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}
mod command_event;
