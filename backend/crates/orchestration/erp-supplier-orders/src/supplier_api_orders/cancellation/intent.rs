use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierFulfillmentOrderId, SupplierOrderActionId};
use erp_permissions::supplier_fulfillment;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt};
use erp_supply::dto::supplier_fulfillment::SubmitAfterSalesActionRequest;
use erp_supply::entity::supplier_fulfillment::{
    CancelStatus, SupplierOrderAction, SupplierOrderActionData, SupplierOrderActionLine,
    SupplierOrderActionType,
};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_fulfillment::{FulfillmentOrderAccess, SupplierFulfillmentService};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::SupplierApiOrderProcess;
use crate::{Error, Result};

struct CancellationIntent {
    action: SupplierOrderAction,
    lines: Vec<SupplierOrderActionLine>,
}

impl SupplierApiOrderProcess {
    // 协议能力已经核实后，在写事务中登记原取消动作及逐行意图；提交成功前不交给后台对外发送。
    //
    // execution/request 为已经核实协议能力的冻结取消，actor/access 为原入口身份与共享授权。
    //
    // 原动作及逐行意图全部写入后成功。
    //
    // 授权拒绝、执行版本变化、订单状态冲突或数据库失败时拒绝整次写入。
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
        let intent = CancellationIntent { action, lines };
        client
            .with_transaction(move |executor| {
                Box::pin(async move { intent.persist(&db, &mut execution, &actor, &access, executor).await })
            })
            .await
    }
}

impl CancellationIntent {
    // 在取消意图的原事务中重验取消权限和执行版本，确认订单仍可进入或保持 `CancelPending` 后写入动作、行及审计。
    // 已有明确取消结果时拒绝写入，调用方应查询原结果。
    async fn persist(
        &self,
        db: &Database,
        execution: &mut ApiOrderExecution,
        actor: &AuditActor,
        access: &FulfillmentOrderAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut order = access
            .require_order(actor, &supplier_fulfillment::ORDER_CANCEL, &execution.base.id, executor)
            .await?;
        let current = db
            .supplier_api_order_executions()
            .find_by_id(&execution.base.id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原接口执行不存在".into()))?;
        if current.base.version != execution.base.version {
            return Err(Error::ConflictError("供应商订单状态已变化，请刷新并确认仍可取消后再操作".into()));
        }
        execution.ensure_cancellation_order(&order)?;
        if order.cancel_status == CancelStatus::None {
            order.advance_cancel(CancelStatus::CancelPending)?;
        } else if order.cancel_status != CancelStatus::CancelPending {
            return Err(Error::BusinessLogicError("原订单已有明确取消结果，请查询原结果".into()));
        }

        db.supplier_api_order_executions().update(execution, executor).await?;
        db.supplier_order_actions().create(&self.action, executor).await?;
        for line in &self.lines {
            db.supplier_order_action_lines().create(line, executor).await?;
        }
        db.supplier_fulfillment_orders().update(&mut order, executor).await?;

        let audit = actor.clone().resource_log(
            "supplier_fulfillment.api_cancel",
            "supplier_order_action",
            self.action.base.id.clone(),
        )?;
        persist_log(db, &audit, executor).await?;
        Ok(())
    }
}
