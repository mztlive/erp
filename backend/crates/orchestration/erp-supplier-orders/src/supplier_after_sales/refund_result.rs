use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_core::ids::{InboxMessageId, SourceSystemId, SupplierFulfillmentOrderId, SupplierRefundFactId};
use erp_core::money::Amount;
use erp_integration::entity::integration_ops::{
    InboxMessage, InboxMessageData, InboxMessageStatus, MessageType,
};
use erp_integration::repository::IntegrationOpsExt;
use erp_permissions::supplier_fulfillment;
use erp_supply::after_sales::{AfterSalesExt, RefundExecution, RefundExecutionState, refunds_for_order};
use erp_supply::api_order::ApiOrderExt;
use erp_supply::dto::supplier_fulfillment::{RecordRefundResultRequest, SupplierRefundFactView};
use erp_supply::entity::supplier_fulfillment::{RefundStatus, SupplierFulfillmentOrder};
use erp_supply::repository::{
    SupplierFulfillmentExt, SupplierFulfillmentItemRepositoryExt, SupplierRefundAllocationRepositoryExt,
};
use erp_supply::service::supplier_fulfillment::mapping::refund_fact_view;
use erp_supply::service::supplier_fulfillment::refund_result::persist_refund_result;
use erp_supply::service::supplier_fulfillment::{FulfillmentOrderAccess, SupplierFulfillmentService};
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::SupplierAfterSalesProcess;
use super::refund_store::project;
use crate::{Error, Result};

impl SupplierAfterSalesProcess {
    /// 经既有退款权限登记 API 收据的明确财务分配，未验证收据不得人工覆盖。
    ///
    /// 原订单、结果命令、操作人与原数据范围。
    ///
    /// API 订单返回正式供应商退款事实，非 API 返回 None。
    ///
    /// 来源不一致、重复异载荷、超额及事务失败时拒绝。
    pub async fn record_refund_if_api(
        &self,
        id: &str,
        request: RecordRefundResultRequest,
        actor: &AuditActor,
        access: &FulfillmentOrderAccess,
    ) -> Result<Option<SupplierRefundFactView>> {
        let process = self.clone();
        let id = id.to_string();
        let actor = actor.clone();
        let access = access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let order = access
                        .require_order(&actor, &supplier_fulfillment::ORDER_REFUND, &id, executor)
                        .await?;
                    if process.db.supplier_api_order_executions().find_by_id(&id, executor).await?.is_none() {
                        return Ok(None);
                    }
                    request.validate()?;
                    process.apply_refund(&id, &request, &actor, order, executor).await.map(Some)
                })
            })
            .await
    }

    async fn apply_refund(
        &self,
        id: &str,
        request: &RecordRefundResultRequest,
        actor: &AuditActor,
        mut order: SupplierFulfillmentOrder,
        executor: &mut dyn Executor,
    ) -> Result<SupplierRefundFactView> {
        let rows = refunds_for_order(&self.db, id, executor).await?;
        if rows.len() > 1000 {
            return Err(Error::BusinessLogicError("退款记录超出核对上限".into()));
        }
        let mut row = rows
            .into_iter()
            .find(|row| row.base.id == request.source_event_id)
            .ok_or_else(|| Error::NotFound("原 API 退款来源动作不存在".into()))?;
        let hash = self.validate_refund_application(&mut row, &mut order, request, actor, executor).await?;
        if let Some(fact_id) = &row.fact_id {
            return self.existing_refund(fact_id, executor).await;
        }

        let financial = self
            .db
            .supplier_fulfillment()
            .refund_financial_snapshot(&SupplierFulfillmentOrderId::new(id), executor)
            .await?;
        let total = Amount::try_from(
            financial
                .refunded_total
                .to_decimal()
                .checked_add(request.refund_amount.to_decimal())
                .ok_or_else(|| Error::ValidationError("退款金额溢出".into()))?,
        )?;
        if total > financial.order_cost_gross {
            return Err(Error::BusinessLogicError("累计退款超过原订单成本".into()));
        }
        super::refund_validation::validate_finance(&self.db, &row, request, executor).await?;
        order.advance_refund(if total == financial.order_cost_gross {
            RefundStatus::Refunded
        } else {
            RefundStatus::Partial
        })?;

        let message = refund_message(&row.connection_id, &row.base.id)?;
        let (fact, allocations) = SupplierFulfillmentService::new(self.db.clone()).build_refund_fact(
            &order,
            request,
            &order.connection_id,
            &InboxMessageId::new(&message.base.id),
        )?;

        self.db.inbox_messages().create(&message, executor).await?;
        persist_refund_result(&self.db, &mut order, &fact, &allocations, executor).await?;
        self.complete_refund(&mut row, hash, &fact.base.id, actor, executor).await?;
        Ok(refund_fact_view(&fact, &allocations))
    }

    async fn validate_refund_application(
        &self,
        row: &mut RefundExecution,
        order: &mut SupplierFulfillmentOrder,
        request: &RecordRefundResultRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        row.ensure_order(order)?;
        if row.state == RefundExecutionState::Unsupported {
            row.confirm_manual(request, actor.id())?;
            if order.refund_status == RefundStatus::None {
                order.advance_refund(RefundStatus::RefundPending)?;
            }
        }

        let items = self
            .db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(&[SupplierFulfillmentOrderId::new(&order.base.id)], executor)
            .await?;
        self.ensure_refund_balance(row, &items, executor).await?;
        Ok(row.validate_application(request)?)
    }

    async fn complete_refund(
        &self,
        row: &mut RefundExecution,
        hash: String,
        fact_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        row.state = RefundExecutionState::Applied;
        row.application_hash = Some(hash);
        row.fact_id = Some(fact_id.to_string());
        row.blocker = Some("供应商退款事实及分配已登记；现金到账与应付核销仍按财务流程确认".into());
        self.db.api_refunds().update(row, executor).await?;

        let mut action = self
            .db
            .supplier_order_actions()
            .find_by_id(&row.base.id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原退款动作不存在".into()))?;
        project(row, &mut action)?;
        self.db.supplier_order_actions().update(&mut action, executor).await?;

        let audit = actor.clone().resource_log(
            "supplier_fulfillment.api_refund_apply",
            "supplier_refund_fact",
            fact_id.to_string(),
        )?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(())
    }

    async fn existing_refund(&self, id: &str, executor: &mut dyn Executor) -> Result<SupplierRefundFactView> {
        let fact = self
            .db
            .supplier_refund_facts()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("原退款事实不存在".into()))?;
        let allocations = self
            .db
            .supplier_refund_allocations()
            .find_allocations_by_fact_ids(&[SupplierRefundFactId::new(id)], executor)
            .await?;
        Ok(refund_fact_view(&fact, &allocations))
    }
}

fn refund_message(connection: &str, action: &str) -> Result<InboxMessage> {
    Ok(InboxMessage::new(
        InboxMessageId::new(format!("refund-{action}")),
        InboxMessageData {
            source_system_id: SourceSystemId::new(format!("supplier-api:{connection}")),
            source_event_id: action.into(),
            message_type: MessageType::SupplierCallback,
            business_fact_key: action.into(),
            payload_schema_version: "1.0".into(),
            payload_reference: Some(format!("supplier-api-refund:{action}")),
            status: InboxMessageStatus::Received,
            source_sent_at: None,
            received_at: Instant::now(),
            processed_at: None,
        },
    )?)
}
