use erp_core::common::time::Instant;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, ApiOrderStage, CancellationActionState};
use erp_supply::entity::supplier_fulfillment::{
    SupplierOrderAction, SupplierOrderActionStatus, SupplierOrderActionUpdate,
};
use erp_supply::order_tracking::repository::OrderTrackingExt;
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_api::SupplierApiService;
use persistence_core::{Executor, Transactional};

use super::SupplierApiOrderProcess;
use crate::{Error, Result};

pub(super) enum CancellationWrite {
    SendIntent,
    Progress,
}

impl SupplierApiOrderProcess {
    pub(super) async fn persist_cancellation(
        &self,
        execution: &mut ApiOrderExecution,
        write: CancellationWrite,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let process = self.clone();
        let mut saved = execution.clone();
        *execution = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    if matches!(write, CancellationWrite::SendIntent) {
                        process.ensure_cancel_send(&saved, executor).await?;
                    }

                    let action_id = saved
                        .cancellation_action_id
                        .as_ref()
                        .ok_or_else(|| Error::NotFound("原取消动作身份缺失".into()))?;
                    let mut action = db
                        .supplier_order_actions()
                        .find_by_id(action_id, executor)
                        .await?
                        .filter(|action| action.supplier_fulfillment_order_id.as_ref() == saved.base.id)
                        .ok_or_else(|| Error::NotFound("原取消动作不属于当前执行".into()))?;
                    project_cancellation_action(&saved, &mut action)?;
                    db.supplier_api_order_executions().update(&mut saved, executor).await?;
                    db.supplier_order_actions().update(&mut action, executor).await?;

                    if let Some(mut tracking) =
                        db.supplier_order_tracking().find_by_id(&saved.base.id, executor).await?
                    {
                        tracking.poll_after = Instant::now();
                        db.supplier_order_tracking().update(&mut tracking, executor).await?;
                    }
                    Ok::<_, Error>(saved)
                })
            })
            .await?;
        Ok(())
    }

    async fn ensure_cancel_send(
        &self,
        execution: &ApiOrderExecution,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, executor)
            .await?;
        if connection.supplier_id.as_ref() != execution.supplier_id {
            return Err(Error::ConflictError("原取消连接归属已变化".into()));
        }
        let connector = self
            .runtime
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        if connector.cancellations().is_none() {
            return Err(Error::BusinessLogicError("当前协议未实现原订单取消".into()));
        }

        let order = self
            .db
            .supplier_fulfillment_orders()
            .find_by_id(&execution.base.id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原供应商订单不存在".into()))?;
        Ok(execution.ensure_cancellation_order(&order)?)
    }
}

fn project_cancellation_action(
    execution: &ApiOrderExecution,
    action: &mut SupplierOrderAction,
) -> Result<()> {
    let states: Vec<_> = execution.cancellation_actions.values().map(|item| item.state).collect();
    let unknown = execution.stage == ApiOrderStage::CancellationUnknown
        || states.iter().any(|state| {
            matches!(state, CancellationActionState::SentUnknown | CancellationActionState::Accepted)
        });
    let ready = states.contains(&CancellationActionState::Ready);
    let status = if execution.all_cancellations_confirmed() {
        SupplierOrderActionStatus::Succeeded
    } else if unknown {
        SupplierOrderActionStatus::ResultUnknown
    } else if ready {
        SupplierOrderActionStatus::Pending
    } else {
        SupplierOrderActionStatus::Manual
    };

    let canceled = states.iter().filter(|state| **state == CancellationActionState::Canceled).count();
    let accepted = states.iter().filter(|state| **state == CancellationActionState::Accepted).count();
    action.update(SupplierOrderActionUpdate {
        status: Some(status),
        response_summary: Some(format!(
            "当前子单{}个，已核实取消{}个，受理待核实{}个；取消不代表资金已返还",
            states.len(),
            canceled,
            accepted
        )),
        ..Default::default()
    })?;
    Ok(())
}
