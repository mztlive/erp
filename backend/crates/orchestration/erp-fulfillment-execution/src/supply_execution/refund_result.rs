use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_core::ids::{
    InboxMessageId, SourceSystemId, SupplierApiConnectionId, SupplierFulfillmentOrderId, SupplierRefundFactId,
};
use erp_integration::entity::integration_ops::{
    InboxMessage, InboxMessageData, InboxMessageStatus, MessageType,
};
use erp_permissions::supplier_fulfillment;
use erp_supply::dto::supplier_fulfillment::{RecordRefundResultRequest, SupplierRefundFactView};
use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::repository::prelude::*;
use erp_supply::service::supplier_fulfillment::mapping::refund_fact_view;
use id_generator::next_id;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::SupplierFulfillmentProcess;
use crate::{Error, Result};

impl SupplierFulfillmentProcess {
    /// 登记供应商退款成功结果（幂等键 `(connection_id, external_refund_no,
    /// external_refund_version)`，§6.19）。
    ///
    /// 同事务写入 `inbox_message`、退款事实头与全部分配行并推进退款进度
    /// （累计等于订单成本余额时为 `REFUNDED`，否则为 `PARTIAL`）；累计净退款
    /// 不得超过订单成本余额（§6.19）。重复登记返回原退款事实。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 退款成功结果请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回退款事实视图（含分配行）。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    /// * `BusinessLogicError` - 退款金额超过订单成本净可退余额
    /// * `ConflictError` - 外部退款身份冲突（事务注入失败时全部不可见）
    pub async fn record_refund_result(
        &self,
        id: &str,
        req: RecordRefundResultRequest,
        actor: &AuditActor,
    ) -> Result<SupplierRefundFactView> {
        req.validate()?;
        let process = self.clone();
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.refund_result(&id, &req, &actor, executor).await })
            })
            .await
    }

    async fn refund_result(
        &self,
        id: &str,
        req: &RecordRefundResultRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierRefundFactView> {
        let mut order =
            self.require_legacy_order(id, actor, &supplier_fulfillment::ORDER_REFUND, executor).await?;
        let connection_id = order.connection_id.clone();
        if let Some(view) = self.replayed_refund_result(id, req, &connection_id, actor, executor).await? {
            return Ok(view);
        }
        self.domain().advance_refund_result(id, &mut order, req.refund_amount, executor).await?;
        let message = build_refund_message(&order, req, &connection_id, InboxMessageStatus::Received)?;
        let (fact, allocations) = self.domain().build_refund_fact(
            &order,
            req,
            &connection_id,
            &InboxMessageId::new(message.base.id.as_str()),
        )?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.refund_result",
            "supplier_refund_fact",
            fact.base.id.clone(),
        )?;
        super::refund_writes::persist(&self.db, &message, &mut order, &fact, &allocations, &audit, executor)
            .await?;
        Ok(refund_fact_view(&fact, &allocations))
    }

    async fn replayed_refund_result(
        &self,
        id: &str,
        req: &RecordRefundResultRequest,
        connection_id: &SupplierApiConnectionId,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierRefundFactView>> {
        if let Some(existing) = self
            .db
            .supplier_refund_facts()
            .find_by_connection_and_refund(
                connection_id,
                &req.external_refund_no,
                &req.external_refund_version,
                executor,
            )
            .await?
        {
            if !existing.matches_result(
                &SupplierFulfillmentOrderId::new(id),
                req.refund_amount,
                Instant::from_unix_secs(req.refunded_at),
                &req.source_event_id,
            ) {
                return Err(Error::ConflictError("外部退款身份已用于不同订单或退款结果".into()));
            }
            tracing::info!(account = %actor.id(), order_id = %id, "退款结果幂等命中");
            let fact_id = SupplierRefundFactId::new(existing.base.id.as_str());
            let allocations = self
                .db
                .supplier_refund_allocations()
                .find_allocations_by_fact_ids(&[fact_id], executor)
                .await?;
            return Ok(Some(refund_fact_view(&existing, &allocations)));
        }
        Ok(None)
    }
}

/// 构建退款结果 `inbox_message` 信封（来源事件取外部退款身份）。
///
/// # 参数
/// * `order` - 供应商子订单
/// * `req` - 退款成功结果请求
/// * `connection_id` - 供应商连接
/// * `status` - 初始消息状态（已接收）
///
/// # 返回
/// 返回消息实体。
///
/// # 错误
/// 实体构造校验失败时返回 `Logic`。
fn build_refund_message(
    order: &SupplierFulfillmentOrder,
    req: &RecordRefundResultRequest,
    connection_id: &SupplierApiConnectionId,
    status: InboxMessageStatus,
) -> Result<InboxMessage> {
    let event_key = format!("refund:{}:{}", req.external_refund_no, req.external_refund_version);
    Ok(InboxMessage::new(
        InboxMessageId::new(next_id()),
        InboxMessageData {
            source_system_id: SourceSystemId::new(format!("supplier-api:{connection_id}")),
            source_event_id: event_key.clone(),
            message_type: MessageType::SupplierCallback,
            business_fact_key: event_key,
            payload_schema_version: "1.0".to_string(),
            payload_reference: Some(format!("supplier-refund-order:{}", order.base.id)),
            status,
            source_sent_at: None,
            received_at: Instant::now(),
            processed_at: None,
        },
    )?)
}
