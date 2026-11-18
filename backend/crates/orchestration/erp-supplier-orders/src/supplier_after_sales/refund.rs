use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::ids::{SupplierFulfillmentOrderId, SupplierOrderActionId};
use erp_permissions::supplier_fulfillment;
use erp_supply::after_sales::{
    AfterSalesExt, RefundExecution, RefundExecutionState, refunds_due, refunds_for_order,
};
use erp_supply::api_order::ApiOrderExt;
use erp_supply::dto::supplier_fulfillment::{SubmitActionResultView, SubmitAfterSalesActionRequest};
use erp_supply::entity::supplier_fulfillment::{
    RefundStatus, SupplierFulfillmentItem, SupplierFulfillmentOrder, SupplierOrderAction,
    SupplierOrderActionData, SupplierOrderActionType,
};
use erp_supply::ports::connector::order::OrderReference;
use erp_supply::repository::{SupplierFulfillmentExt, SupplierFulfillmentItemRepositoryExt};
use erp_supply::service::supplier_api::SupplierApiService;
use erp_supply::service::supplier_fulfillment::{FulfillmentOrderAccess, SupplierFulfillmentService};
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::action_result::{ActionResult, read_action_result};
use super::refund_dispatch::RefundStore;
use super::refund_store::{MongoRefundStore, project};
use super::{SupplierAfterSalesProcess, refund_dispatch};
use crate::{Error, Result};

impl SupplierAfterSalesProcess {
    /// 将正式 API 订单退款接入公共处理器，原非 API 订单仍由原入口处理。
    /// # 参数
    /// 原履约订单、申请、已认证操作人及原数据范围授权。
    /// # 返回
    /// 已持久化动作；缺少能力返回待人工动作而非伪成功。
    /// # 错误
    /// 越权、范围、金额或幂等冲突时拒绝，不执行外部调用。
    pub async fn submit_refund_if_api(
        &self,
        id: &str,
        request: SubmitAfterSalesActionRequest,
        actor: &AuditActor,
        access: &FulfillmentOrderAccess,
    ) -> Result<Option<SubmitActionResultView>> {
        let process = self.clone();
        let order_id = id.to_string();
        let command_actor = actor.clone();
        let command_access = access.clone();
        let action_id = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let order = command_access
                        .require_order(
                            &command_actor,
                            &supplier_fulfillment::ORDER_REFUND,
                            &order_id,
                            executor,
                        )
                        .await?;
                    if process
                        .db
                        .supplier_api_order_executions()
                        .find_by_id(&order_id, executor)
                        .await?
                        .is_none()
                    {
                        return Ok(None);
                    }
                    request.validate()?;
                    process
                        .enqueue_refund(&order_id, &request, &command_actor, order, executor)
                        .await
                        .map(Some)
                })
            })
            .await?;
        match action_id {
            Some(action_id) => {
                read_action_result(&self.db, access, actor, id, ActionResult::Refund(action_id))
                    .await
                    .map(Some)
            },
            None => Ok(None),
        }
    }

    async fn enqueue_refund(
        &self,
        id: &str,
        request: &SubmitAfterSalesActionRequest,
        actor: &AuditActor,
        mut order: SupplierFulfillmentOrder,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let execution = self
            .db
            .supplier_api_order_executions()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原 API 执行不存在".into()))?;
        if order.connection_id.as_ref() != execution.connection_id
            || order.supplier_id.as_ref() != execution.supplier_id
        {
            return Err(Error::ConflictError("退款原订单与执行身份不一致".into()));
        }
        let items = self
            .db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(&[SupplierFulfillmentOrderId::new(id)], executor)
            .await?;
        let reference = OrderReference {
            merchant_order_no: id.into(),
            external_order_no: order
                .external_order_no
                .clone()
                .ok_or_else(|| Error::BusinessLogicError("原订单尚无已核实供应商订单号，不能退款".into()))?,
        };
        let mut row = RefundExecution::prepare(
            &execution.connection_id,
            &execution.supplier_id,
            reference,
            request,
            &items,
        )?;
        if let Some(existing) = self.db.api_refunds().find_by_id(&row.base.id, executor).await? {
            existing.ensure_replay(&row)?;
            return Ok(existing.base.id);
        }
        self.ensure_refund_balance(&row, &items, executor).await?;
        if !self.supports_refund(&row, executor).await? {
            row.unsupported();
        } else if order.refund_status == RefundStatus::None {
            order.advance_refund(RefundStatus::RefundPending)?;
        }
        self.persist_refund_intent(&row, request, &mut order, actor, executor).await?;
        Ok(row.base.id)
    }

    async fn supports_refund(&self, row: &RefundExecution, executor: &mut dyn Executor) -> Result<bool> {
        let connection =
            SupplierApiService::new(self.db.clone()).load_connection(&row.connection_id, executor).await?;
        if connection.supplier_id.as_ref() != row.supplier_id {
            return Err(Error::Forbidden("退款连接归属不一致".into()));
        }
        let connector = self
            .registry
            .after_sales_connector(&connection)
            .map_err(|e| Error::BusinessLogicError(e.summary))?;
        Ok(connector.refunds().is_some())
    }

    pub(super) async fn ensure_refund_balance(
        &self,
        row: &RefundExecution,
        items: &[SupplierFulfillmentItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let previous = refunds_for_order(&self.db, &row.request.order.merchant_order_no, executor).await?;
        if previous.len() > 1000
            || previous.len() == 1000 && !previous.iter().any(|old| old.base.id == row.base.id)
        {
            return Err(Error::BusinessLogicError("原订单退款申请过多，请核对原动作".into()));
        }
        Ok(row.ensure_remaining(&previous, items)?)
    }

    async fn persist_refund_intent(
        &self,
        row: &RefundExecution,
        request: &SubmitAfterSalesActionRequest,
        order: &mut SupplierFulfillmentOrder,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.create_refund_action(row, request, executor).await?;
        self.db.api_refunds().create(row, executor).await?;
        self.db.supplier_fulfillment_orders().update(order, executor).await?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.api_refund",
            "supplier_order_action",
            row.base.id.clone(),
        )?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(())
    }

    async fn create_refund_action(
        &self,
        row: &RefundExecution,
        input: &SubmitAfterSalesActionRequest,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut action = SupplierOrderAction::new(
            SupplierOrderActionId::new(&row.base.id),
            SupplierOrderActionData::manual_adjustment(
                SupplierFulfillmentOrderId::new(&row.request.order.merchant_order_no),
                SupplierOrderActionType::Refund,
                &row.base.id,
                Some(&row.request.reason_code),
            ),
        )?;
        project(row, &mut action)?;
        let lines = SupplierFulfillmentService::new(self.db.clone()).build_action_lines(&action, input)?;
        self.db.supplier_order_actions().create(&action, executor).await?;
        for line in lines {
            self.db.supplier_order_action_lines().create(&line, executor).await?;
        }
        Ok(())
    }

    /// 有界恢复退款动作；失败不重放已发送请求。
    /// # 参数
    /// 无。
    /// # 返回
    /// 本轮检查的动作数量。
    /// # 错误
    /// 队列读取失败时返回错误；单动作错误保留原状态。
    pub async fn run_refunds_once(&self) -> Result<usize> {
        let rows = refunds_due(&self.db, &mut NoTransaction).await?;
        let count = rows.len();
        for mut row in rows {
            let result = match self.connector(&row.connection_id, &row.supplier_id).await {
                Ok(connector) => {
                    refund_dispatch::process(&MongoRefundStore(self), connector.refunds(), &mut row).await
                },
                Err(error) => Err(error),
            };
            if result.is_err() {
                tracing::warn!(action_id = %row.base.id, "退款处理受阻，保留原动作状态等待核实");
                self.defer_refund(&row.base.id).await;
            }
        }
        Ok(count)
    }
    async fn defer_refund(&self, id: &str) {
        if let Ok(Some(mut current)) = self.db.api_refunds().find_by_id(id, &mut NoTransaction).await
            && matches!(
                current.state,
                RefundExecutionState::Ready | RefundExecutionState::Unknown | RefundExecutionState::Accepted
            )
            && current.next_run_at.is_some()
        {
            current.defer("退款处理或结果保存受阻，保留原动作等待核实，不重新发送未知请求");
            if MongoRefundStore(self).save(&mut current).await.is_err() {
                tracing::warn!(action_id = %id, "退款阻断原因尚未保存，下轮仍读取原状态");
            }
        }
    }
}
