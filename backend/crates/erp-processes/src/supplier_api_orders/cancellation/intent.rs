use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierFulfillmentOrderId, SupplierOrderActionId};
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt};
use erp_supply::dto::supplier_fulfillment::SubmitAfterSalesActionRequest;
use erp_supply::entity::supplier_fulfillment::{
    CancelStatus, SupplierOrderAction, SupplierOrderActionData, SupplierOrderActionType,
};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_fulfillment::{FulfillmentOrderAccess, SupplierFulfillmentService};
use persistence_core::Transactional;

use super::SupplierApiOrderProcess;
use crate::audit::persist_log;
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    pub(super) async fn persist_cancel_intent(
        &self,
        mut execution: ApiOrderExecution,
        request: &SubmitAfterSalesActionRequest,
        actor: &AuditActor,
        access: &FulfillmentOrderAccess,
    ) -> Result<()> {
        let action_id = execution
            .cancellation_action_id
            .as_ref()
            .ok_or_else(|| Error::ValidationError("取消动作身份缺失".into()))?;
        let action = SupplierOrderAction::new(
            SupplierOrderActionId::new(action_id.clone()),
            SupplierOrderActionData::manual_adjustment(
                SupplierFulfillmentOrderId::new(&execution.base.id),
                SupplierOrderActionType::Cancel,
                action_id,
                request.reason_code.as_deref(),
            ),
        )?;
        let lines = SupplierFulfillmentService::new(self.db.clone()).build_action_lines(&action, request)?;
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let access = access.clone();
        execution.next_run_at = Instant::now();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut order =
                        access.require_order(&actor, "cancel", &execution.base.id, None, executor).await?;
                    execution.ensure_cancellation_order(&order)?;
                    if order.cancel_status == CancelStatus::None {
                        order.advance_cancel(CancelStatus::CancelPending)?;
                    } else if order.cancel_status != CancelStatus::CancelPending {
                        return Err(Error::BusinessLogicError("原订单已有明确取消结果，请查询原结果".into()));
                    }
                    db.supplier_api_order_executions().update(&mut execution, executor).await?;
                    db.supplier_order_actions().create(&action, executor).await?;
                    for line in &lines {
                        db.supplier_order_action_lines().create(line, executor).await?;
                    }
                    db.supplier_fulfillment_orders().update(&mut order, executor).await?;
                    let audit = actor.resource_log(
                        "supplier_fulfillment.api_cancel",
                        "supplier_order_action",
                        action.base.id.clone(),
                    )?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<_, Error>(())
                })
            })
            .await
    }
}
