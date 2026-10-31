use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_core::AccountKind;
use erp_supply::after_sales::{AfterSalesExt, RefundExecution, RefundExecutionState};
use erp_supply::entity::supplier_fulfillment::{
    SupplierOrderAction, SupplierOrderActionStatus, SupplierOrderActionUpdate,
};
use erp_supply::repository::SupplierFulfillmentExt;
use persistence_core::Transactional;

use super::SupplierAfterSalesProcess;
use super::refund_dispatch::RefundStore;
use crate::audit::persist_log;
use crate::{Error, Result};

pub(super) struct MongoRefundStore<'a>(pub &'a SupplierAfterSalesProcess);
#[async_trait]
impl RefundStore for MongoRefundStore<'_> {
    async fn save(&self, row: &mut RefundExecution) -> Result<()> {
        let db = self.0.db.clone();
        let mut saved = row.clone();
        *row = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut action = db
                        .supplier_order_actions()
                        .find_by_id(&saved.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("退款动作不存在".into()))?;
                    if action.supplier_fulfillment_order_id.as_ref() != saved.request.order.merchant_order_no
                    {
                        return Err(Error::ConflictError("退款动作不属于原订单".into()));
                    }
                    let order = db
                        .supplier_fulfillment_orders()
                        .find_by_id(&saved.request.order.merchant_order_no, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("原退款订单不存在".into()))?;
                    saved.ensure_order(&order)?;
                    project(&saved, &mut action)?;
                    db.api_refunds().update(&mut saved, executor).await?;
                    db.supplier_order_actions().update(&mut action, executor).await?;
                    let actor = AuditActor::new(
                        "system:supplier-after-sales".into(),
                        "供应商售后处理器".into(),
                        AccountKind::Admin,
                    );
                    let audit = actor.resource_log(
                        "supplier_fulfillment.refund_observe",
                        "supplier_order_action",
                        saved.base.id.clone(),
                    )?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<_, Error>(saved)
                })
            })
            .await?;
        Ok(())
    }
}

pub(super) fn project(row: &RefundExecution, action: &mut SupplierOrderAction) -> Result<()> {
    let status = match row.state {
        RefundExecutionState::Ready => SupplierOrderActionStatus::Pending,
        RefundExecutionState::Unknown | RefundExecutionState::Accepted => {
            SupplierOrderActionStatus::ResultUnknown
        },
        RefundExecutionState::Unsupported | RefundExecutionState::Received => {
            SupplierOrderActionStatus::Manual
        },
        RefundExecutionState::Rejected => SupplierOrderActionStatus::Failed,
        RefundExecutionState::Applied => SupplierOrderActionStatus::Succeeded,
    };
    action.update(SupplierOrderActionUpdate {
        status: Some(status),
        response_summary: row.blocker.clone(),
        external_request_id: row.external_request_id.clone(),
        ..Default::default()
    })?;
    Ok(())
}
