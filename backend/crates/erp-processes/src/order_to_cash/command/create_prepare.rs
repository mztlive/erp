//! 销售创建命令的前置关系、内容与稳定对象准备，保持原验证顺序。
use application_core::AuditActor;
use erp_core::ids::{BusinessDocumentId, SalesOrderId};
use erp_sales::dto::sales_order::CreateSalesOrderRequest;
use erp_sales::entity::sales_order::{
    SalesOrder, SalesOrderLine, SalesOrderWorkingCopy, SalesOrderWorkingCopyLine,
};
use erp_sales::service::sales_order::SalesOrderService;
use erp_sales::service::sales_order::mapper::{build_stable_lines, build_working_copy};
use erp_workflow::entity::document_registry::{BusinessDocument, BusinessDocumentData};
use persistence_core::NoTransaction;

use super::super::SalesOrderCommandProcess;
use super::super::authorization::SalesCommandAccess;
use crate::Result;
use crate::business_ownership::required_business_org;
use crate::order_to_cash::document_type_of_sales_business;

/// 创建事务需要的稳定对象和已验证的初始工作副本。
pub(super) struct PreparedSalesCreation {
    pub order: SalesOrder,
    pub document: BusinessDocument,
    pub stable_lines: Vec<SalesOrderLine>,
    pub working_copy: SalesOrderWorkingCopy,
    pub working_copy_lines: Vec<SalesOrderWorkingCopyLine>,
}

impl SalesOrderCommandProcess {
    /// 在命令重放检查之后按原顺序准备创建对象。
    ///
    /// # 参数
    /// * `req` / `actor` / `access` - 当前完整命令及其认证授权
    /// # 返回
    /// 返回保持相同行顺序的稳定销售、单据注册及工作副本。
    /// # 错误
    /// 关系、行资格、业务组织或构造规则失败时按原顺序返回。
    pub(super) async fn prepare_sales_creation(
        &self,
        req: &CreateSalesOrderRequest,
        actor: &AuditActor,
        access: &SalesCommandAccess,
    ) -> Result<PreparedSalesCreation> {
        let (customer_id, settlement_party_id, draft) = self
            .resolve_sales_command_draft(
                access,
                &req.contract_id,
                &req.customer_id,
                req.draft.clone(),
                &mut NoTransaction,
            )
            .await?;
        self.sales().ensure_sellable_draft_lines(&draft.lines, &self.catalog()).await?;
        let organization = required_business_org(&self.db, actor.id(), &mut NoTransaction).await?;
        let order =
            SalesOrderService::prepare_order(req, customer_id, settlement_party_id, actor, organization)?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let document = BusinessDocument::new(
            BusinessDocumentId::new(order.base.id.clone()),
            BusinessDocumentData {
                document_type: document_type_of_sales_business(req.business_type),
                document_no: order.order_no.clone(),
            },
        )?;
        let stable_lines = build_stable_lines(&order_id, &draft.lines)?;
        let (working_copy, working_copy_lines) = build_working_copy(&order, &stable_lines, &draft, 1, actor)?;
        Ok(PreparedSalesCreation { order, document, stable_lines, working_copy, working_copy_lines })
    }
}
