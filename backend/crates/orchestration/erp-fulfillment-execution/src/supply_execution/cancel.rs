use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::ids::SupplierOrderActionId;
use erp_integration::entity::integration_ops::{InboxMessage, InboxMessageStatus};
use erp_integration::repository::IntegrationOpsExt;
use erp_permissions::supplier_fulfillment;
use erp_supply::dto::supplier_fulfillment::{SubmitActionResultView, SubmitAfterSalesActionRequest};
use erp_supply::entity::supplier_api::{SupplierApiCapabilityCode, SupplierApiConnection};
use erp_supply::entity::supplier_fulfillment::{
    SupplierFulfillmentOrder, SupplierOrderAction, SupplierOrderActionLine, SupplierOrderActionType,
};
use erp_supply::repository::prelude::*;
use erp_supply::repository::{SupplierApiExt, SupplierFulfillmentExt};
use erp_supply::service::supplier_fulfillment::cancel::persist_after_sales;
use erp_supply::service::supplier_fulfillment::mapping::action_line_view;
use erp_supply::service::supplier_fulfillment::place::ensure_capability;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::SupplierFulfillmentProcess;
use super::place::build_action_message;
use crate::{Error, Result};

#[derive(Clone)]
struct AfterSalesIntent {
    order: SupplierFulfillmentOrder,
    action: SupplierOrderAction,
    lines: Vec<SupplierOrderActionLine>,
    message: InboxMessage,
    connection: SupplierApiConnection,
}

enum AfterSalesSubmission {
    Replayed(Box<SubmitActionResultView>),
    Dispatch(Box<AfterSalesIntent>),
}

impl SupplierFulfillmentProcess {
    /// 提交供应商取消（幂等键：「订单号 + CANCEL」，§6.19）。
    ///
    /// 同事务创建 `CANCEL` 动作头/行并把 `cancel_status` 推进到 `CANCEL_PENDING`；事务外派发供应商 API。重复提交（同一幂等键）返回原动作结果，不再次调用。
    ///
    /// `id` 为供应商子订单 ID；`req` 为取消动作提交请求。返回动作与动作后订单视图。`NotFound`：订单不存在。
    ///
    /// `BusinessLogicError`：动作范围非法或连接缺少取消能力。`ConflictError`：唯一键冲突（并发重复提交）。
    pub async fn submit_cancel(
        &self,
        id: &str,
        req: SubmitAfterSalesActionRequest,
        actor: &AuditActor,
    ) -> Result<SubmitActionResultView> {
        self.submit_after_sales_action(id, req, SupplierOrderActionType::Cancel, actor).await
    }

    /// 提交供应商退款（幂等键：「订单号 + REFUND」，§6.19）。
    ///
    /// 同事务创建 `REFUND` 动作头/行并把 `refund_status` 推进到 `REFUND_PENDING`；事务外派发供应商 API。重复提交（同一幂等键）返回原动作结果，不再次调用。
    ///
    /// `id` 为供应商子订单 ID；`req` 为退款动作提交请求。返回动作与动作后订单视图。`NotFound`：订单不存在。
    ///
    /// `BusinessLogicError`：动作范围非法或连接缺少退款能力。`ConflictError`：唯一键冲突（并发重复提交）。
    pub async fn submit_refund(
        &self,
        id: &str,
        req: SubmitAfterSalesActionRequest,
        actor: &AuditActor,
    ) -> Result<SubmitActionResultView> {
        self.submit_after_sales_action(id, req, SupplierOrderActionType::Refund, actor).await
    }

    // 提交供应商取消/退款动作的公共编排。
    //
    // 完成幂等命中、售后申请存在性、连接能力、动作行净余额校验后，同事务写入动作头/行与订单状态推进，事务外派发供应商 API。
    //
    // `id` 为供应商子订单 ID；`req` 为动作提交请求；`action_type` 为动作类型（`Cancel` 或 `Refund`）。返回动作与动作后订单视图。
    //
    // 同 [`Self::submit_cancel`]。
    async fn submit_after_sales_action(
        &self,
        id: &str,
        req: SubmitAfterSalesActionRequest,
        action_type: SupplierOrderActionType,
        actor: &AuditActor,
    ) -> Result<SubmitActionResultView> {
        req.validate()?;
        let process = self.clone();
        let id = id.to_string();
        let actor_for_tx = actor.clone();
        let submission = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.prepare_submission(&id, &req, action_type, &actor_for_tx, executor).await
                })
            })
            .await?;

        let mut intent = match submission {
            AfterSalesSubmission::Replayed(result) => return Ok(*result),
            AfterSalesSubmission::Dispatch(intent) => *intent,
        };

        self.settle_dispatch(
            &mut intent.order,
            &mut intent.action,
            &mut intent.message,
            &intent.connection,
            actor,
        )
        .await?;
        Ok(SubmitActionResultView {
            action: intent.action.into(),
            lines: intent.lines.into_iter().map(action_line_view).collect(),
            order: intent.order.into(),
        })
    }

    async fn prepare_submission(
        &self,
        id: &str,
        req: &SubmitAfterSalesActionRequest,
        action_type: SupplierOrderActionType,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<AfterSalesSubmission> {
        let permission = match action_type {
            SupplierOrderActionType::Cancel => &supplier_fulfillment::ORDER_CANCEL,
            SupplierOrderActionType::Refund => &supplier_fulfillment::ORDER_REFUND,
            _ => return Err(Error::ValidationError("不支持的售后动作类型".into())),
        };
        let order = self.require_legacy_order(id, actor, permission, executor).await?;
        let idempotency_key = format!("{}+{}", order.fulfillment_order_no, action_type.as_str());

        if let Some(existing) =
            self.db.supplier_order_actions().find_by_idempotency_key(&idempotency_key, executor).await?
        {
            return Ok(AfterSalesSubmission::Replayed(Box::new(
                self.replayed_after_sales_result(order, existing, executor).await?,
            )));
        }

        let mut intent =
            self.prepare_after_sales_intent(order, req, &idempotency_key, action_type, executor).await?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.after_sales_action",
            "supplier_order_action",
            intent.action.base.id.clone(),
        )?;
        intent.persist(&self.db, executor).await?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(AfterSalesSubmission::Dispatch(Box::new(intent)))
    }

    async fn replayed_after_sales_result(
        &self,
        order: SupplierFulfillmentOrder,
        action: SupplierOrderAction,
        executor: &mut dyn Executor,
    ) -> Result<SubmitActionResultView> {
        let lines = self
            .db
            .supplier_order_action_lines()
            .find_lines_by_action_ids(&[SupplierOrderActionId::new(action.base.id.as_str())], executor)
            .await?;
        Ok(SubmitActionResultView {
            action: action.into(),
            lines: lines.into_iter().map(action_line_view).collect(),
            order: order.into(),
        })
    }

    async fn prepare_after_sales_intent(
        &self,
        mut order: SupplierFulfillmentOrder,
        req: &SubmitAfterSalesActionRequest,
        idempotency_key: &str,
        action_type: SupplierOrderActionType,
        executor: &mut dyn Executor,
    ) -> Result<AfterSalesIntent> {
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&order.connection_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商连接不存在".to_string()))?;
        let capabilities = self.domain().capabilities(&connection)?;
        let needed = match action_type {
            SupplierOrderActionType::Cancel => SupplierApiCapabilityCode::Cancel,
            SupplierOrderActionType::Refund => SupplierApiCapabilityCode::Refund,
            _ => return Err(Error::BusinessLogicError("不支持的售后动作类型".to_string())),
        };
        ensure_capability(&capabilities, needed)?;

        self.domain().ensure_action_lines(&order, req, executor).await?;
        let action = self.domain().build_after_sales_action(&order, req, idempotency_key, action_type)?;
        let lines = self.domain().build_action_lines(&action, req)?;
        self.domain().advance_after_sales(&mut order, action_type)?;
        let message = build_action_message(&action, &connection, InboxMessageStatus::Received)?;
        Ok(AfterSalesIntent { order, action, lines, message, connection })
    }
}

impl AfterSalesIntent {
    async fn persist(&mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        persist_after_sales(db, &self.action, &self.lines, &mut self.order, executor).await?;
        db.inbox_messages().create(&self.message, executor).await?;
        Ok(())
    }
}
