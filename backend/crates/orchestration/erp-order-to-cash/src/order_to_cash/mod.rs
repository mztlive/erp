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
    /// 装配销售单命令流程，使销售写入、授权范围、商品资格和审批能力由同一个用例协调。
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
// 按实物及服务、卡券两类销售单生成对应的审批主体引用，避免流程关联到另一种单据。无效单据身份返回校验错误。
fn subject_ref_for_sales_business(business_type: BusinessType, id: &str) -> Result<bpm::SubjectRef> {
    erp_workflow::entity::approval_integration::subject_ref_for(
        document_type_of_sales_business(business_type),
        id,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

/// 审批运行时撤回销售单时，按销售类型核对撤回动作，再在当前审批事务更新销售单并写入审计。
///
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
