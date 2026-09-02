//! 商城关键事实接收用例：消费入账、取消与完成登记（事务写入）。

use database::{AccessControlExt, CostExt, MallOrderExt, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    InboxMessageId, MallOrderCancelFactId, MallOrderCompletionFactId, MallOrderFactId, MallOrderId,
};
use entities::mall_order::{
    MallOrderCancelFact, MallOrderCancelFactData, MallOrderCompletionFact, MallOrderCompletionFactData,
    MallOrderFact, MallOrderFactData, ProcessingStatus,
};
use entities::money::{Amount, Quantity};
use id_generator::next_id;
use std::str::FromStr;

use super::dto;
use super::dto::{ReceiveMallOrderFactRequest, ReceivedFactView};
use super::query::fact_view;
use super::validated_fact_payload::ValidatedMallOrderFactPayload;
use super::MallOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl MallOrderService {
    /// 接收商城关键事实（消费入账/取消/完成）。
    ///
    /// `PAYMENT_SUCCEEDED` 触发完整消费入账（§8.4 第 3 条 + 第 7 条）；
    /// `ORDER_CANCELED`/`ORDER_COMPLETED` 只登记结果扩展事实。退款与余额恢复
    /// 事实由 D30 售后域接口接收。`business_fact_key`/`inbox_message_id`
    /// 幂等：重复提交返回既有正式事实。
    ///
    /// # 参数
    /// * `req` - 事实接收请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回事实接收结果视图（含幂等命中标记）。
    ///
    /// # 错误
    /// * `BusinessLogicError` - 事实类型载荷不匹配、原支付缺失/未归集、
    ///   金额守恒不成立、扩展表事实类型不匹配
    /// * `ConflictError` - 唯一键冲突或并发版本冲突
    /// * `ValidationError` - 请求体校验失败
    pub async fn receive_fact(
        &self,
        req: ReceiveMallOrderFactRequest,
        actor: &AuditActor,
    ) -> Result<ReceivedFactView> {
        let payload = ValidatedMallOrderFactPayload::try_from_request(&req)?;
        if let Some(existing) = self
            .db
            .mall_order_facts()
            .find_by_business_fact_key(&req.business_fact_key, &mut NoTransaction)
            .await?
        {
            return self.existing_received_view(existing).await;
        }
        if let Some(existing) = self
            .db
            .mall_order_facts()
            .find_by_inbox_message(
                &InboxMessageId::new(req.inbox_message_id.clone()),
                &mut NoTransaction,
            )
            .await?
        {
            return self.existing_received_view(existing).await;
        }

        let fact_id = MallOrderFactId::new(next_id());
        let fact = MallOrderFact::new(
            fact_id.clone(),
            MallOrderFactData {
                mall_id: req.mall_id.clone(),
                source_event_id: req.source_event_id.clone(),
                inbox_message_id: InboxMessageId::new(req.inbox_message_id.clone()),
                fact_type: req.fact_type,
                business_fact_key: req.business_fact_key.clone(),
                external_order_no: req.external_order_no.clone(),
                external_order_version: req.external_order_version.clone(),
                after_sales_request_id: req.after_sales_request_id.clone(),
                original_payment_fact_id: req.original_payment_fact_id.clone(),
                occurred_at: Instant::from_unix_secs(req.occurred_at as i64),
                received_at: Instant::from_unix_secs(req.received_at as i64),
                data_source: req.data_source,
                raw_payload_reference: req.raw_payload_reference.clone(),
            },
        )?;

        let view = match payload {
            ValidatedMallOrderFactPayload::Payment(payment) => {
                self.receive_payment(&req, payment, fact, fact_id, actor).await?
            }
            ValidatedMallOrderFactPayload::Cancel(cancel) => {
                self.receive_cancel(&req, cancel, fact, fact_id, actor).await?
            }
            ValidatedMallOrderFactPayload::Completion(completion) => {
                self.receive_completion(&req, completion, fact, fact_id, actor)
                    .await?
            }
        };
        Ok(view)
    }
}

impl MallOrderService {
    /// 接收支付成功事实：完整消费入账（§8.4 第 3、7 条）。
    ///
    /// 一个事务内写入：支付事实 + 唯一订单 + 明细 + 支付来源 + 分摊矩阵 +
    /// 消费事实 + 成本评估 + `cost_entry`/`cost_allocation` + 审计。
    ///
    /// # 参数
    /// * `req` - 事实接收请求
    /// * `payment` - 付款载荷
    /// * `fact` - 待写入的支付事实（状态由归集结果推进）
    /// * `fact_id` - 事实 ID
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回事实接收结果视图。
    ///
    /// # 错误
    /// 见 [`MallOrderService::receive_fact`]。
    async fn receive_payment(
        &self,
        req: &ReceiveMallOrderFactRequest,
        payment: dto::PaymentFactData,
        mut fact: MallOrderFact,
        fact_id: MallOrderFactId,
        actor: &AuditActor,
    ) -> Result<ReceivedFactView> {
        let order_id = MallOrderId::new(next_id());
        let plan = self
            .build_payment_plan(req, payment, fact_id.clone(), &mut fact, &order_id, actor)
            .await?;
        let audit = actor.clone().resource_log(
            "mall_order_fact.payment_received",
            "mall_order_fact",
            fact.base.id.clone(),
        )?;
        let plan_for_tx = plan.clone();
        let fact_for_tx = fact.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.mall_order()
                        .create_payment_fact_with_order(&fact_for_tx, &plan_for_tx.order, session)
                        .await?;
                    for item in &plan_for_tx.items {
                        db.mall_order_items().create(item, session).await?;
                    }
                    for source in &plan_for_tx.sources {
                        db.mall_payment_sources().create(source, session).await?;
                    }
                    for allocation in &plan_for_tx.allocations {
                        db.mall_item_funding_allocations()
                            .create(allocation, session)
                            .await?;
                    }
                    for entry in &plan_for_tx.entries {
                        db.mall_consumption_entries().create(entry, session).await?;
                    }
                    for assessment in &plan_for_tx.assessments {
                        db.mall_consumption_cost_assessments()
                            .create(assessment, session)
                            .await?;
                    }
                    for cost_entry in &plan_for_tx.cost_entries {
                        db.cost_entries().create(cost_entry, session).await?;
                    }
                    for allocation in &plan_for_tx.cost_allocations {
                        db.cost_allocations().create(allocation, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ReceivedFactView {
            fact: fact_view(&fact),
            mall_order_id: Some(order_id.to_string()),
            idempotent_hit: false,
        })
    }

    /// 接收订单取消事实（事实 + 一对一取消扩展，原子写入）。
    ///
    /// 取消只记录结果；发生资金退回时仍须另有 `REFUND_SUCCEEDED`（§6.17）。
    ///
    /// # 参数
    /// * `req` - 事实接收请求
    /// * `cancel` - 取消载荷
    /// * `fact` - 待写入的取消事实
    /// * `fact_id` - 事实 ID
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回事实接收结果视图。
    async fn receive_cancel(
        &self,
        req: &ReceiveMallOrderFactRequest,
        cancel: dto::CancelFactData,
        mut fact: MallOrderFact,
        fact_id: MallOrderFactId,
        actor: &AuditActor,
    ) -> Result<ReceivedFactView> {
        self.ensure_original_payment(req).await?;
        let extension = MallOrderCancelFact::new(
            MallOrderCancelFactId::new(next_id()),
            MallOrderCancelFactData {
                mall_order_fact_id: fact_id,
                cancel_version: cancel.cancel_version,
                cancel_scope: cancel.cancel_scope,
                actual_canceled_quantity: Quantity::from_str(&cancel.actual_canceled_quantity)?,
                actual_canceled_amount: Amount::from_str(&cancel.actual_canceled_amount)?,
                reason: cancel.reason,
            },
        )?;
        fact.update_processing_status(ProcessingStatus::Attributed)?;
        let audit = actor.clone().resource_log(
            "mall_order_fact.order_canceled",
            "mall_order_fact",
            fact.base.id.clone(),
        )?;
        let fact_for_tx = fact.clone();
        let extension_for_tx = extension.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.mall_order_facts().create(&fact_for_tx, session).await?;
                    db.mall_order_cancel_facts()
                        .create(&extension_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ReceivedFactView {
            fact: fact_view(&fact),
            mall_order_id: None,
            idempotent_hit: false,
        })
    }

    /// 接收订单完成事实（事实 + 一对一完成扩展，原子写入）。
    ///
    /// # 参数
    /// * `req` - 事实接收请求
    /// * `completion` - 完成载荷
    /// * `fact` - 待写入的完成事实
    /// * `fact_id` - 事实 ID
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回事实接收结果视图。
    async fn receive_completion(
        &self,
        req: &ReceiveMallOrderFactRequest,
        completion: dto::CompletionFactData,
        mut fact: MallOrderFact,
        fact_id: MallOrderFactId,
        actor: &AuditActor,
    ) -> Result<ReceivedFactView> {
        self.ensure_original_payment(req).await?;
        let extension = MallOrderCompletionFact::new(
            MallOrderCompletionFactId::new(next_id()),
            MallOrderCompletionFactData {
                mall_order_fact_id: fact_id,
                completion_version: completion.completion_version,
                completed_at: Instant::from_unix_secs(completion.completed_at as i64),
            },
        )?;
        fact.update_processing_status(ProcessingStatus::Attributed)?;
        let audit = actor.clone().resource_log(
            "mall_order_fact.order_completed",
            "mall_order_fact",
            fact.base.id.clone(),
        )?;
        let fact_for_tx = fact.clone();
        let extension_for_tx = extension.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.mall_order_facts().create(&fact_for_tx, session).await?;
                    db.mall_order_completion_facts()
                        .create(&extension_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ReceivedFactView {
            fact: fact_view(&fact),
            mall_order_id: None,
            idempotent_hit: false,
        })
    }

    /// 校验取消/完成事实关联的原支付：存在、同商城同订单、已正式归集。
    ///
    /// # 参数
    /// * `req` - 事实接收请求
    ///
    /// # 返回
    /// 校验通过返回 `Ok(())`。
    ///
    /// # 错误
    /// 原支付缺失、类型不符、商城/订单不一致或未归集时返回 `BusinessLogicError`。
    async fn ensure_original_payment(&self, req: &ReceiveMallOrderFactRequest) -> Result<()> {
        let original_id = req
            .original_payment_fact_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("取消/完成事实必须关联原支付事实".to_string()))?;
        let original = self
            .db
            .mall_order_facts()
            .find_by_id(&original_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("原支付事实不存在".to_string()))?;
        Ok(original.ensure_attributed_payment_for(&req.mall_id, &req.external_order_no)?)
    }

    /// 幂等命中：返回既有事实视图（含订单 ID 与命中标记）。
    ///
    /// # 参数
    /// * `fact` - 既有事实实体
    ///
    /// # 返回
    /// 返回事实接收结果视图。
    async fn existing_received_view(&self, fact: MallOrderFact) -> Result<ReceivedFactView> {
        let order_id = if fact.fact_type.is_payment_succeeded() {
            self.db
                .mall_orders()
                .find_by_payment_fact(&MallOrderFactId::new(fact.base.id.clone()), &mut NoTransaction)
                .await?
                .map(|order| order.base.id)
        } else {
            None
        };
        Ok(ReceivedFactView {
            fact: fact_view(&fact),
            mall_order_id: order_id,
            idempotent_hit: true,
        })
    }
}
