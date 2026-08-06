//! 域 D29 `mall_order` 服务编排（W25 商城消费订单、W28 卡券消费台账）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 消费入账（§8.4 第 3 条：支付事实 + 唯一订单 + 明细 + 支付来源 + 分摊矩阵
//!   加消费事实、成本评估与 `cost_entry`/`cost_allocation` 原子写入）→
//!   `database::Transactional::with_transaction`；
//! - 取消/完成事实（事实 + 一对一扩展表）→ 同一事务；
//! - 列表/详情只读查询 → `&mut NoTransaction`。
//!
//! 幂等（§6.17）：`business_fact_key`/`inbox_message_id` 唯一，重复接收返回
//! 既有正式事实，不产生第二份事实、订单、消费或成本（§9.4）。
//!
//! 跨域协作只调对方 Repository（P3-service-api.md §2）：
//! - D28 `CardInstanceExt`：切换 `T`（履约链归属）与卡实例（卡券归集）；
//! - D20 `CostExt`：`cost_entry`/`cost_allocation`（消费成本评估，§8.4 第 7 条）。

use database::{AccessControlExt, CardInstanceExt, CostExt, MallOrderExt, NoTransaction, Transactional};
use entities::card_instance::{MallCardInstance, MallConsumptionCutover};
use entities::common::time::Instant;
use entities::cost::{
    CostAllocation, CostAllocationData, CostBasis as CostBasisEntry, CostEntry, CostEntryData, CostScope,
    CostStage, CostType,
};
use entities::ids::{
    CostAllocationId, CostEntryId, InboxMessageId, MallConsumptionCostAssessmentId, MallConsumptionEntryId,
    MallItemFundingAllocationId, MallOrderCancelFactId, MallOrderCompletionFactId, MallOrderFactId,
    MallOrderId, MallOrderItemId, MallPaymentSourceId,
};
use entities::mall_order::{
    AttributionStatus, ConsumptionDirection, CostBasis, DataSource, FactType, FulfillmentChain,
    MallConsumptionCostAssessment, MallConsumptionCostAssessmentData, MallConsumptionEntry,
    MallConsumptionEntryData, MallItemFundingAllocation, MallItemFundingAllocationData, MallOrder,
    MallOrderCancelFact, MallOrderCancelFactData, MallOrderCompletionFact, MallOrderCompletionFactData,
    MallOrderData, MallOrderFact, MallOrderFactData, MallOrderItem, MallOrderItemData, MallPaymentSource,
    MallPaymentSourceData, PaymentSourceType, ProcessingStatus,
};
use entities::money::{round_to_cent, Amount, Quantity, Rate, UnitPrice};
use id_generator::next_id;
use mongodb::Database;
use std::str::FromStr;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

pub use self::dto::{
    ConservationResultRow, ConservationView, ConsumptionEntryView, CostAssessmentView,
    CostBasisBreakdownItemView, FactSummaryItemView, FundingAllocationView, MallOrderAddressView,
    MallOrderAmountsView, MallOrderCustomerView, MallOrderDetailView, MallOrderFactListParams,
    MallOrderFactView, MallOrderFulfillmentView, MallOrderIdentityView, MallOrderItemView,
    MallOrderListParams, MallOrderListRow, PageView, PaymentCompositionView, PaymentSourceView,
    ReceiveMallOrderFactRequest, ReceivedFactView, SupplierOrderSummaryView, SupplierOrderView,
};

/// 商城订单列表筛选条件类型（经 `MallOrderExt` 关联类型跨 crate 可达）。
type MallOrderFilter = <mongodb::Database as MallOrderExt>::MallOrderFilter;
/// 关键事实列表筛选条件类型。
type MallOrderFactFilter = <mongodb::Database as MallOrderExt>::MallOrderFactFilter;
/// 商城订单域服务：事实接收、消费入账、订单与事实查询。
pub struct MallOrderService {
    db: Database,
}

impl MallOrderService {
    /// 创建服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询商城订单列表（W25 列表页）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`q`/`mall_id`/`external_order_no`/`customer_id`/
    ///   `fulfillment_chain`/`attribution_status`/`paid_at_from`/`paid_at_to` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn mall_order_list(&self, params: &MallOrderListParams) -> Result<PageView<MallOrderListRow>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = MallOrderFilter {
            mall_id: query.mall_id,
            external_order_no: query.external_order_no,
            customer_id: query
                .customer_id
                .as_deref()
                .map(entities::ids::CustomerAccountId::new),
            fulfillment_chain: query.fulfillment_chain,
            attribution_status: query.attribution_status,
            paid_at_from: query
                .paid_at_from
                .map(|secs| Instant::from_unix_secs(secs as i64)),
            paid_at_to: query.paid_at_to.map(|secs| Instant::from_unix_secs(secs as i64)),
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .mall_orders()
            .search_orders(&filter, &mut NoTransaction)
            .await?;
        // 行级聚合字段（事实摘要/支付构成/成本分项）按页内订单批量补齐：
        // 事实按（商城, 订单号）分组、支付来源按订单、消费事实沿支付来源取。
        let fact_map = self.facts_grouped_by_order(&filter.mall_id).await?;
        let mut rows = Vec::with_capacity(page.items.len());
        for row in page.items {
            rows.push(
                self.build_list_row(
                    OrderListRow {
                        id: row.id,
                        mall_id: row.mall_id,
                        external_order_no: row.external_order_no,
                        customer_id: row.customer_id,
                        paid_at: row.paid_at,
                        paid_amount: row.paid_amount,
                        fulfillment_chain: row.fulfillment_chain,
                        attribution_status: row.attribution_status,
                    },
                    &fact_map,
                )
                .await?,
            );
        }
        Ok(PageView {
            items: rows,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询商城订单详情（W25 对象中心）。
    ///
    /// # 参数
    /// * `id` - 商城订单 ID
    ///
    /// # 返回
    /// 返回订单详情视图（事实/明细/支付来源/分摊/守恒/消费/成本）。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn mall_order_detail(&self, id: &str) -> Result<MallOrderDetailView> {
        let order = self
            .db
            .mall_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商城订单不存在".to_string()))?;
        let order_id: MallOrderId = order.base.id.clone().into();
        let items = self
            .db
            .mall_order_items()
            .list_items_by_order(&order_id, &mut NoTransaction)
            .await?;
        let sources = self
            .db
            .mall_payment_sources()
            .list_by_order(&order_id, &mut NoTransaction)
            .await?;
        let item_ids: Vec<MallOrderItemId> = items.iter().map(|item| item.base.id.clone().into()).collect();
        let allocations = self
            .db
            .mall_item_funding_allocations()
            .list_by_items(&item_ids, &mut NoTransaction)
            .await?;
        let facts = self
            .load_facts_for_order(&order.mall_id, &order.external_order_no)
            .await?;
        let entries = self.load_entries_for_sources(&sources).await?;
        let assessments = self.load_current_assessments(&entries).await?;
        let cutover = self
            .db
            .mall_consumption_cutovers()
            .find_enabled_cutover_by_mall_id(&order.mall_id, &mut NoTransaction)
            .await?;

        Ok(self.build_detail_view(
            order,
            items,
            sources,
            allocations,
            facts,
            entries,
            assessments,
            cutover,
        ))
    }

    /// 分页查询关键事实列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`mall_id`/`fact_type`/`processing_status`/
    ///   `after_sales_request_id` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn mall_order_fact_list(
        &self,
        params: &MallOrderFactListParams,
    ) -> Result<PageView<MallOrderFactView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = MallOrderFactFilter {
            mall_id: query.mall_id,
            fact_type: query.fact_type,
            processing_status: query.processing_status,
            after_sales_request_id: query.after_sales_request_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .mall_order_facts()
            .search_facts(&filter, &mut NoTransaction)
            .await?;
        // 投影行不含扩展字段（售后请求/原支付），逐条加载完整事实后映射视图。
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            if let Some(fact) = self
                .db
                .mall_order_facts()
                .find_by_id(&row.id, &mut NoTransaction)
                .await?
            {
                items.push(fact_view(&fact));
            }
        }
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

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
        req.validate()?;
        if matches!(
            req.fact_type,
            FactType::RefundSucceeded | FactType::CardBalanceRestored
        ) {
            return Err(Error::BusinessLogicError(
                "退款与余额恢复事实由售后域接口接收".to_string(),
            ));
        }
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

        let view = match req.fact_type {
            FactType::PaymentSucceeded => {
                let payment = req
                    .payment
                    .clone()
                    .ok_or_else(|| Error::BusinessLogicError("支付事实必须携带付款载荷".to_string()))?;
                self.receive_payment(&req, payment, fact, fact_id, actor).await?
            }
            FactType::OrderCanceled => {
                let cancel = req
                    .cancel
                    .clone()
                    .ok_or_else(|| Error::BusinessLogicError("取消事实必须携带取消载荷".to_string()))?;
                self.receive_cancel(&req, cancel, fact, fact_id, actor).await?
            }
            FactType::OrderCompleted => {
                let completion = req
                    .completion
                    .clone()
                    .ok_or_else(|| Error::BusinessLogicError("完成事实必须携带完成载荷".to_string()))?;
                self.receive_completion(&req, completion, fact, fact_id, actor)
                    .await?
            }
            FactType::RefundSucceeded | FactType::CardBalanceRestored => unreachable!(),
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
        if original.fact_type != FactType::PaymentSucceeded {
            return Err(Error::BusinessLogicError("原事实不是支付成功事实".to_string()));
        }
        if original.mall_id != req.mall_id || original.external_order_no != req.external_order_no {
            return Err(Error::BusinessLogicError(
                "原支付事实与本次事实的商城或订单不一致".to_string(),
            ));
        }
        if original.processing_status != ProcessingStatus::Attributed {
            return Err(Error::BusinessLogicError("原支付事实尚未正式归集".to_string()));
        }
        Ok(())
    }

    /// 幂等命中：返回既有事实视图（含订单 ID 与命中标记）。
    ///
    /// # 参数
    /// * `fact` - 既有事实实体
    ///
    /// # 返回
    /// 返回事实接收结果视图。
    async fn existing_received_view(&self, fact: MallOrderFact) -> Result<ReceivedFactView> {
        let order_id = if fact.fact_type == FactType::PaymentSucceeded {
            self.db
                .mall_orders()
                .find_one(
                    mongodb::bson::doc! { "payment_fact_id": fact.base.id.clone() },
                    &mut NoTransaction,
                )
                .await?
                .map(|order: MallOrder| order.base.id)
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

/// 消费入账写入计划（事务闭包内全部实体的不可变快照）。
#[derive(Debug, Clone)]
struct PaymentPlan {
    /// 商城订单追溯对象。
    order: MallOrder,
    /// 商品明细。
    items: Vec<MallOrderItem>,
    /// 支付来源。
    sources: Vec<MallPaymentSource>,
    /// 商品 × 支付来源分摊。
    allocations: Vec<MallItemFundingAllocation>,
    /// 消费事实（每行分摊一条）。
    entries: Vec<MallConsumptionEntry>,
    /// 成本评估（每行消费一条链首评估）。
    assessments: Vec<MallConsumptionCostAssessment>,
    /// 实际成本事实（D20）。
    cost_entries: Vec<CostEntry>,
    /// 实际成本分配（D20）。
    cost_allocations: Vec<CostAllocation>,
}

impl MallOrderService {
    /// 构建消费入账写入计划（金额守恒校验 + 归集 + 成本评估）。
    ///
    /// # 参数
    /// * `req` - 事实接收请求
    /// * `payment` - 付款载荷
    /// * `fact_id` - 事实 ID
    /// * `fact` - 待写入的支付事实（归集结果写入处理状态）
    /// * `order_id` - 订单 ID
    ///
    /// # 返回
    /// 返回全部待写实体。
    ///
    /// # 错误
    /// 金额守恒不成立、明细行/来源引用缺失或实体校验失败时返回错误。
    async fn build_payment_plan(
        &self,
        req: &ReceiveMallOrderFactRequest,
        payment: dto::PaymentFactData,
        fact_id: MallOrderFactId,
        fact: &mut MallOrderFact,
        order_id: &MallOrderId,
        actor: &AuditActor,
    ) -> Result<PaymentPlan> {
        let occurred = Instant::from_unix_secs(req.occurred_at as i64);
        let cutover = self
            .db
            .mall_consumption_cutovers()
            .find_enabled_cutover_by_mall_id(&req.mall_id, &mut NoTransaction)
            .await?;
        let chain = match cutover.as_ref().and_then(|c| c.enabled_at) {
            Some(t) if occurred >= t => FulfillmentChain::ErpAutomated,
            _ => FulfillmentChain::LegacyManual,
        };

        let mut items: Vec<MallOrderItem> = Vec::with_capacity(payment.items.len());
        for line in &payment.items {
            let quantity = Quantity::from_str(&line.quantity)?;
            let unit_price = UnitPrice::from_str(&line.unit_price_gross)?;
            let line_gross =
                Amount::try_from(round_to_cent(quantity.to_decimal() * unit_price.to_decimal()))?;
            let paid = line_gross
                .checked_sub(Amount::from_str(&line.allocated_discount_amount)?)
                .checked_add(Amount::from_str(&line.allocated_freight_amount)?);
            items.push(MallOrderItem::new(
                MallOrderItemId::new(next_id()),
                MallOrderItemData {
                    mall_order_id: order_id.clone(),
                    external_item_id: line.external_item_id.clone(),
                    sku_id: line.sku_id.clone().map(entities::ids::SkuId::new),
                    product_publication_revision_id: line
                        .product_publication_revision_id
                        .clone()
                        .map(entities::ids::ProductPublicationRevisionId::new),
                    supplier_offering_revision_id: line
                        .supplier_offering_revision_id
                        .clone()
                        .map(entities::ids::SupplierOfferingRevisionId::new),
                    name_snapshot: line.name_snapshot.clone(),
                    spec_snapshot: line.spec_snapshot.clone(),
                    quantity,
                    unit_price_gross: unit_price,
                    line_gross_amount: line_gross,
                    allocated_discount_amount: Amount::from_str(&line.allocated_discount_amount)?,
                    allocated_freight_amount: Amount::from_str(&line.allocated_freight_amount)?,
                    paid_amount: paid,
                    sales_tax_rate: Rate::from_str(&line.sales_tax_rate)?,
                    unit_cost_snapshot: line
                        .unit_cost_snapshot
                        .as_deref()
                        .map(UnitPrice::from_str)
                        .transpose()?,
                    cost_snapshot_total: line
                        .cost_snapshot_total
                        .as_deref()
                        .map(Amount::from_str)
                        .transpose()?,
                    cost_tax_inclusion: line.cost_tax_inclusion,
                    cost_input_tax_rate: line
                        .cost_input_tax_rate
                        .as_deref()
                        .map(Rate::from_str)
                        .transpose()?,
                },
            )?);
        }

        let mut sources: Vec<MallPaymentSource> = Vec::with_capacity(payment.payment_sources.len());
        for source in &payment.payment_sources {
            let card_instance = match source.source_type {
                PaymentSourceType::Card => {
                    self.db
                        .mall_card_instances()
                        .find_by_identity(
                            &req.mall_id,
                            source.source_card_instance_ref.as_deref().unwrap_or_default(),
                            &mut NoTransaction,
                        )
                        .await?
                }
                PaymentSourceType::Wechat => None,
            };
            sources.push(MallPaymentSource::new(
                MallPaymentSourceId::new(next_id()),
                MallPaymentSourceData {
                    mall_order_id: order_id.clone(),
                    source_no: source.source_no,
                    source_type: source.source_type,
                    amount: Amount::from_str(&source.amount)?,
                    source_card_instance_ref: source.source_card_instance_ref.clone(),
                    mall_card_instance_id: card_instance.as_ref().map(|c| c.base.id.clone().into()),
                    wechat_payment_ref: source.wechat_payment_ref.clone(),
                    attribution_status: attribution_for(source.source_type, &card_instance),
                },
            )?);
        }

        let mut allocations: Vec<MallItemFundingAllocation> =
            Vec::with_capacity(payment.funding_allocations.len());
        for allocation in &payment.funding_allocations {
            let item = items
                .iter()
                .find(|item| item.external_item_id == allocation.external_item_id)
                .ok_or_else(|| {
                    Error::BusinessLogicError(format!(
                        "分摊引用的商品明细不存在: {}",
                        allocation.external_item_id
                    ))
                })?;
            let source = sources
                .iter()
                .find(|source| source.source_no == allocation.source_no)
                .ok_or_else(|| {
                    Error::BusinessLogicError(format!("分摊引用的支付来源不存在: {}", allocation.source_no))
                })?;
            allocations.push(MallItemFundingAllocation::new(
                MallItemFundingAllocationId::new(next_id()),
                MallItemFundingAllocationData {
                    mall_order_item_id: item.base.id.clone().into(),
                    mall_payment_source_id: source.base.id.clone().into(),
                    allocated_payment_amount: Amount::from_str(&allocation.allocated_payment_amount)?,
                },
            )?);
        }
        self.ensure_conservation(&payment, &items, &sources, &allocations)?;

        let all_attributed = sources
            .iter()
            .all(|source| source.attribution_status == AttributionStatus::Attributed);
        let order_attribution = if all_attributed {
            AttributionStatus::Attributed
        } else {
            AttributionStatus::PendingAttribution
        };
        let order = MallOrder::new(
            order_id.clone(),
            MallOrderData {
                mall_id: req.mall_id.clone(),
                external_order_no: req.external_order_no.clone(),
                payment_fact_id: fact_id.clone(),
                mall_user_ref: payment.mall_user_ref.clone(),
                source_customer_ref: payment.source_customer_ref.clone(),
                customer_id: payment
                    .customer_id
                    .as_deref()
                    .map(entities::ids::CustomerAccountId::new),
                ordered_at: Instant::from_unix_secs(payment.ordered_at as i64),
                paid_at: occurred,
                gross_amount: Amount::from_str(&payment.gross_amount)?,
                discount_amount: Amount::from_str(&payment.discount_amount)?,
                freight_amount: Amount::from_str(&payment.freight_amount)?,
                paid_amount: Amount::from_str(&payment.paid_amount)?,
                fulfillment_chain: chain,
                attribution_status: order_attribution,
                address_snapshot_encrypted: payment.address_snapshot_encrypted.clone(),
            },
        )?;

        let mut entries: Vec<MallConsumptionEntry> = Vec::with_capacity(allocations.len());
        for allocation in &allocations {
            let source = sources
                .iter()
                .find(|source| source.base.id == allocation.mall_payment_source_id.as_ref())
                .expect("分摊来源已校验存在");
            let item = items
                .iter()
                .find(|item| item.base.id == allocation.mall_order_item_id.as_ref())
                .expect("分摊明细已校验存在");
            let origin_sales_order_id = match source.mall_card_instance_id.as_ref() {
                Some(card_id) => self
                    .db
                    .mall_card_instances()
                    .find_by_id(card_id, &mut NoTransaction)
                    .await?
                    .map(|instance| instance.origin_sales_order_id),
                None => None,
            };
            entries.push(MallConsumptionEntry::new(
                MallConsumptionEntryId::new(next_id()),
                MallConsumptionEntryData {
                    mall_order_fact_id: fact_id.clone(),
                    mall_order_item_id: allocation.mall_order_item_id.clone(),
                    mall_payment_source_id: allocation.mall_payment_source_id.clone(),
                    direction: ConsumptionDirection::Consumption,
                    amount: allocation.allocated_payment_amount,
                    customer_id: None,
                    origin_sales_order_id,
                    sales_order_line_id: None,
                    occurred_at: occurred,
                    attribution_status: source.attribution_status,
                    reverses_consumption_entry_id: None,
                },
            )?);
            let _ = item;
        }
        let (assessments, cost_entries, cost_allocations) = self.build_cost_assessments(
            &items,
            &sources,
            &allocations,
            &entries,
            occurred,
            actor.id().to_string(),
        );

        let processing = if all_attributed {
            ProcessingStatus::Attributed
        } else {
            ProcessingStatus::PendingAttribution
        };
        fact.update_processing_status(processing)?;

        Ok(PaymentPlan {
            order,
            items,
            sources,
            allocations,
            entries,
            assessments,
            cost_entries,
            cost_allocations,
        })
    }

    /// 校验分摊矩阵守恒（§6.17）：行合计 = 明细实付、列合计 = 来源金额、
    /// 订单汇总恒等。任一不成立即拒绝接收。
    ///
    /// # 参数
    /// * `payment` - 付款载荷
    /// * `items` - 商品明细
    /// * `sources` - 支付来源
    /// * `allocations` - 分摊记录
    ///
    /// # 返回
    /// 守恒成立返回 `Ok(())`。
    ///
    /// # 错误
    /// 守恒不成立返回 `BusinessLogicError`。
    fn ensure_conservation(
        &self,
        payment: &dto::PaymentFactData,
        items: &[MallOrderItem],
        sources: &[MallPaymentSource],
        allocations: &[MallItemFundingAllocation],
    ) -> Result<()> {
        let zero = Amount::from_str("0.00")?;
        for item in items {
            let allocated = allocations
                .iter()
                .filter(|allocation| allocation.mall_order_item_id.as_ref() == item.base.id)
                .fold(zero, |acc, allocation| {
                    acc.checked_add(allocation.allocated_payment_amount)
                });
            if allocated.to_decimal() != item.paid_amount.to_decimal() {
                return Err(Error::BusinessLogicError(format!(
                    "商品明细 {} 分摊合计与实付不一致",
                    item.external_item_id
                )));
            }
        }
        for source in sources {
            let allocated = allocations
                .iter()
                .filter(|allocation| allocation.mall_payment_source_id.as_ref() == source.base.id)
                .fold(zero, |acc, allocation| {
                    acc.checked_add(allocation.allocated_payment_amount)
                });
            if allocated.to_decimal() != source.amount.to_decimal() {
                return Err(Error::BusinessLogicError(format!(
                    "支付来源 {} 分摊合计与支付金额不一致",
                    source.source_no
                )));
            }
        }
        let totals = items.iter().fold((zero, zero, zero), |acc, item| {
            (
                acc.0.checked_add(item.line_gross_amount),
                acc.1.checked_add(item.allocated_discount_amount),
                acc.2.checked_add(item.paid_amount),
            )
        });
        if totals.0.to_decimal() != Amount::from_str(&payment.gross_amount)?.to_decimal()
            || totals.1.to_decimal() != Amount::from_str(&payment.discount_amount)?.to_decimal()
            || totals.2.to_decimal() != Amount::from_str(&payment.paid_amount)?.to_decimal()
        {
            return Err(Error::BusinessLogicError(
                "商品明细汇总与订单金额不一致".to_string(),
            ));
        }
        Ok(())
    }
}

impl MallOrderService {
    /// 构建消费成本评估（§8.4 第 7 条，P3 只落 `ACTUAL`/`NONE` 两级）。
    ///
    /// `ACTUAL`：明细商城成本快照含完整税额标识与进项税率；按支付来源金额
    /// 比例分摊，尾差计入最后一个来源。`NONE`：成本数据不全（`STANDARD`
    /// 依赖 D24 供给版本查询，未授予 D29，属闭环缺口）。
    ///
    /// # 参数
    /// * `items` - 商品明细
    /// * `sources` - 支付来源
    /// * `allocations` - 分摊记录
    /// * `entries` - 消费事实（与分摊一一对应）
    /// * `occurred` - 事实发生时间
    /// * `assessed_by` - 评估人
    ///
    /// # 返回
    /// 返回 `(评估, 成本事实, 成本分配)` 三元组。
    fn build_cost_assessments(
        &self,
        items: &[MallOrderItem],
        sources: &[MallPaymentSource],
        allocations: &[MallItemFundingAllocation],
        entries: &[MallConsumptionEntry],
        occurred: Instant,
        assessed_by: String,
    ) -> (
        Vec<MallConsumptionCostAssessment>,
        Vec<CostEntry>,
        Vec<CostAllocation>,
    ) {
        let mut assessments = Vec::new();
        let mut cost_entries = Vec::new();
        let mut cost_allocations = Vec::new();
        for item in items {
            // 同明细的分摊按来源序号稳定排序，成本尾差计入最后一个来源。
            let mut item_allocations: Vec<&MallItemFundingAllocation> = allocations
                .iter()
                .filter(|allocation| allocation.mall_order_item_id.as_ref() == item.base.id)
                .collect();
            item_allocations.sort_by_key(|allocation| {
                sources
                    .iter()
                    .find(|source| source.base.id == allocation.mall_payment_source_id.as_ref())
                    .map(|source| source.source_no)
                    .unwrap_or_default()
            });
            let entry_of = |allocation: &MallItemFundingAllocation| -> &MallConsumptionEntry {
                entries
                    .iter()
                    .find(|entry| {
                        entry.mall_order_item_id.as_ref() == allocation.mall_order_item_id.as_ref()
                            && entry.mall_payment_source_id.as_ref()
                                == allocation.mall_payment_source_id.as_ref()
                    })
                    .expect("分摊与消费事实一一对应")
            };
            let source_of = |allocation: &MallItemFundingAllocation| -> &MallPaymentSource {
                sources
                    .iter()
                    .find(|source| source.base.id == allocation.mall_payment_source_id.as_ref())
                    .expect("分摊来源已校验存在")
            };
            let has_actual = item.cost_snapshot_total.is_some()
                && item.cost_tax_inclusion.is_some()
                && (!item.cost_tax_inclusion.unwrap_or(false) || item.cost_input_tax_rate.is_some());
            if !has_actual {
                for allocation in &item_allocations {
                    assessments.push(self.none_assessment(entry_of(allocation), occurred, &assessed_by));
                }
                continue;
            }
            let cost_total = item.cost_snapshot_total.expect("已校验存在");
            let paid = item.paid_amount;
            let mut accrued = Amount::from_str("0.00").expect("零常量可解析");
            let count = item_allocations.len();
            for (index, allocation) in item_allocations.iter().enumerate() {
                let entry = entry_of(allocation);
                let is_last = index + 1 == count;
                let gross = if is_last {
                    cost_total.checked_sub(accrued)
                } else {
                    let share = round_to_cent(
                        cost_total.to_decimal() * allocation.allocated_payment_amount.to_decimal()
                            / paid.to_decimal(),
                    );
                    Amount::try_from(share).expect("舍入后金额合法")
                };
                accrued = accrued.checked_add(gross);
                let (net, tax, input_rate) = match item.cost_tax_inclusion {
                    Some(true) => {
                        let rate = item.cost_input_tax_rate.expect("含税成本已校验税率");
                        let tax = Amount::try_from(round_to_cent(gross.to_decimal() * rate.to_decimal()))
                            .expect("舍入后金额合法");
                        (gross.checked_sub(tax), tax, Some(rate))
                    }
                    _ => (gross, Amount::from_str("0.00").expect("零常量可解析"), None),
                };
                let assessment = self.actual_assessment(
                    entry,
                    gross,
                    net,
                    tax,
                    input_rate,
                    item,
                    allocation,
                    occurred,
                    &assessed_by,
                );
                let cost_entry = CostEntry::new(
                    CostEntryId::new(next_id()),
                    CostEntryData {
                        cost_type: CostType::Product,
                        cost_stage: CostStage::Actual,
                        cost_scope: if source_of(allocation).source_type == PaymentSourceType::Card {
                            CostScope::MallConsumption
                        } else {
                            CostScope::WechatCost
                        },
                        cost_basis: Some(CostBasisEntry::Actual),
                        supplier_id: None,
                        gross_amount: gross,
                        net_amount: net,
                        tax_amount: tax,
                        tax_inclusion: item.cost_tax_inclusion.unwrap_or(false),
                        input_tax_rate: input_rate
                            .unwrap_or_else(|| Rate::from_str("0").expect("税率可解析")),
                        occurred_at: occurred,
                        source_fact_type: "mall_consumption_entry".to_string(),
                        source_document_id: entry.base.id.clone(),
                        source_line_id: item.base.id.clone(),
                        source_version: "1".to_string(),
                        adjusts_cost_entry_id: None,
                        evidence_attachment_id: None,
                    },
                )
                .expect("成本事实内容已校验");
                let cost_allocation = CostAllocation::new(
                    CostAllocationId::new(next_id()),
                    CostAllocationData {
                        cost_entry_id: cost_entry.base.id.clone().into(),
                        sales_order_id: None,
                        sales_order_line_id: None,
                        mall_consumption_entry_id: Some(entry.base.id.clone().into()),
                        mall_payment_source_id: Some(allocation.mall_payment_source_id.clone()),
                        allocated_gross_amount: gross,
                        allocated_net_amount: net,
                        rounding_residual_flag: is_last,
                    },
                )
                .expect("成本分配内容已校验");
                assessments.push(assessment);
                cost_entries.push(cost_entry);
                cost_allocations.push(cost_allocation);
            }
        }
        (assessments, cost_entries, cost_allocations)
    }

    /// 构造 `NONE` 成本评估（无来源依据、金额与税字段）。
    ///
    /// # 参数
    /// * `entry` - 消费事实
    /// * `occurred` - 评估时间（= 事实发生时间）
    /// * `assessed_by` - 评估人
    ///
    /// # 返回
    /// 返回链首 `NONE` 评估。
    fn none_assessment(
        &self,
        entry: &MallConsumptionEntry,
        occurred: Instant,
        assessed_by: &str,
    ) -> MallConsumptionCostAssessment {
        MallConsumptionCostAssessment::new(
            MallConsumptionCostAssessmentId::new(next_id()),
            MallConsumptionCostAssessmentData {
                mall_consumption_entry_id: entry.base.id.clone().into(),
                assessment_no: 1,
                cost_basis: CostBasis::None,
                basis_source_type: None,
                basis_source_id: None,
                basis_source_line_id: None,
                basis_source_version: None,
                source_snapshot_hash: None,
                gross_amount: None,
                net_amount: None,
                tax_amount: None,
                tax_inclusion: None,
                input_tax_rate: None,
                delta_cost_entry_id: None,
                supersedes_assessment_id: None,
                assessed_at: occurred,
                assessed_by: assessed_by.to_string(),
            },
        )
        .expect("NONE 评估内容已校验")
    }

    /// 构造 `ACTUAL` 成本评估（商城成本快照来源，§12.1 第 5 项）。
    ///
    /// # 参数
    /// * `entry` - 消费事实
    /// * `gross` - 分摊含税成本
    /// * `net` - 分摊不含税成本
    /// * `tax` - 分摊税额
    /// * `input_rate` - 进项税率（不含税成本时为空）
    /// * `item` - 商品明细
    /// * `allocation` - 分摊记录
    /// * `occurred` - 评估时间
    /// * `assessed_by` - 评估人
    ///
    /// # 返回
    /// 返回链首 `ACTUAL` 评估。
    #[allow(clippy::too_many_arguments)]
    fn actual_assessment(
        &self,
        entry: &MallConsumptionEntry,
        gross: Amount,
        net: Amount,
        tax: Amount,
        input_rate: Option<Rate>,
        item: &MallOrderItem,
        allocation: &MallItemFundingAllocation,
        occurred: Instant,
        assessed_by: &str,
    ) -> MallConsumptionCostAssessment {
        MallConsumptionCostAssessment::new(
            MallConsumptionCostAssessmentId::new(next_id()),
            MallConsumptionCostAssessmentData {
                mall_consumption_entry_id: entry.base.id.clone().into(),
                assessment_no: 1,
                cost_basis: CostBasis::Actual,
                basis_source_type: Some(entities::mall_order::CostBasisSourceType::MallCostSnapshot),
                basis_source_id: Some(item.base.id.clone()),
                basis_source_line_id: Some(allocation.mall_payment_source_id.to_string()),
                basis_source_version: Some("1".to_string()),
                source_snapshot_hash: Some(format!(
                    "mall_item:{}:{}",
                    item.base.id,
                    item.cost_snapshot_total
                        .map(|amount| amount.to_string())
                        .unwrap_or_default()
                )),
                gross_amount: Some(gross),
                net_amount: Some(net),
                tax_amount: Some(tax),
                tax_inclusion: Some(item.cost_tax_inclusion.unwrap_or(false)),
                input_tax_rate: input_rate,
                delta_cost_entry_id: None,
                supersedes_assessment_id: None,
                assessed_at: occurred,
                assessed_by: assessed_by.to_string(),
            },
        )
        .expect("ACTUAL 评估内容已校验")
    }

    /// 分页加载指定商城的全部关键事实并按（商城, 订单号）分组。
    ///
    /// # 参数
    /// * `mall_id` - 商城筛选（`None` 表示全部商城）
    ///
    /// # 返回
    /// 返回 `(mall_id, external_order_no)` → 事实摘要 `(类型, 发生时间, 来源)` 映射。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn facts_grouped_by_order(&self, mall_id: &Option<String>) -> Result<OrderFactMap> {
        let mut grouped = std::collections::HashMap::new();
        let mut page = 1u64;
        loop {
            let filter = MallOrderFactFilter {
                mall_id: mall_id.clone(),
                fact_type: None,
                processing_status: None,
                after_sales_request_id: None,
                page,
                page_size: 100,
                sort_by: Some("occurred_at".to_string()),
                sort_ascending: true,
            };
            let result = self
                .db
                .mall_order_facts()
                .search_facts(&filter, &mut NoTransaction)
                .await?;
            if result.items.is_empty() {
                break;
            }
            for row in result.items {
                grouped
                    .entry((row.mall_id.clone(), row.external_order_no.clone()))
                    .or_insert_with(Vec::new)
                    .push((row.fact_type, row.occurred_at, row.data_source));
            }
            if (result.total as u64) <= page * 100 {
                break;
            }
            page += 1;
        }
        Ok(grouped)
    }

    /// 加载指定（商城, 订单号）的全部关键事实实体（按发生时间升序）。
    ///
    /// # 参数
    /// * `mall_id` - 商城
    /// * `external_order_no` - 商城订单号
    ///
    /// # 返回
    /// 返回按发生时间升序的事实实体。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn load_facts_for_order(
        &self,
        mall_id: &str,
        external_order_no: &str,
    ) -> Result<Vec<MallOrderFact>> {
        let mut facts = Vec::new();
        let mut page = 1u64;
        loop {
            let filter = MallOrderFactFilter {
                mall_id: Some(mall_id.to_string()),
                fact_type: None,
                processing_status: None,
                after_sales_request_id: None,
                page,
                page_size: 100,
                sort_by: Some("occurred_at".to_string()),
                sort_ascending: true,
            };
            let result = self
                .db
                .mall_order_facts()
                .search_facts(&filter, &mut NoTransaction)
                .await?;
            let mut hit = false;
            for row in result.items {
                if row.external_order_no != external_order_no {
                    continue;
                }
                hit = true;
                if let Some(fact) = self
                    .db
                    .mall_order_facts()
                    .find_by_id(&row.id, &mut NoTransaction)
                    .await?
                {
                    facts.push(fact);
                }
            }
            if !hit || (result.total as u64) <= page * 100 {
                break;
            }
            page += 1;
        }
        facts.sort_by_key(|fact| (fact.occurred_at, fact.base.id.clone()));
        Ok(facts)
    }

    /// 沿支付来源加载消费事实（去重后按发生时间升序）。
    ///
    /// # 参数
    /// * `sources` - 支付来源
    ///
    /// # 返回
    /// 返回消费事实列表。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn load_entries_for_sources(
        &self,
        sources: &[MallPaymentSource],
    ) -> Result<Vec<MallConsumptionEntry>> {
        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for source in sources {
            for entry in self
                .db
                .mall_consumption_entries()
                .list_by_original_payment_source(&source.base.id.clone().into(), &mut NoTransaction)
                .await?
            {
                if seen.insert(entry.base.id.clone()) {
                    entries.push(entry);
                }
            }
        }
        entries.sort_by_key(|entry| (entry.occurred_at, entry.base.id.clone()));
        Ok(entries)
    }

    /// 取每条消费的当前成本评估（链尾，即最大评估号）。
    ///
    /// # 参数
    /// * `entries` - 消费事实
    ///
    /// # 返回
    /// 返回 `消费ID → 当前评估` 映射。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn load_current_assessments(
        &self,
        entries: &[MallConsumptionEntry],
    ) -> Result<std::collections::HashMap<String, MallConsumptionCostAssessment>> {
        let mut current = std::collections::HashMap::new();
        for entry in entries {
            let chain = self
                .db
                .mall_consumption_cost_assessments()
                .list_by_entry(&entry.base.id.clone().into(), &mut NoTransaction)
                .await?;
            if let Some(tail) = chain
                .into_iter()
                .max_by_key(|assessment| assessment.assessment_no)
            {
                current.insert(entry.base.id.clone(), tail);
            }
        }
        Ok(current)
    }
}

impl MallOrderService {
    /// 构建列表行视图（事实摘要/支付构成/成本分项聚合）。
    ///
    /// # 参数
    /// * `row` - 订单投影行（已按列表投影字段提取）
    /// * `fact_map` - （商城, 订单号）→ 事实摘要映射
    ///
    /// # 返回
    /// 返回列表行视图。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn build_list_row(&self, row: OrderListRow, fact_map: &OrderFactMap) -> Result<MallOrderListRow> {
        let order_id: MallOrderId = row.id.clone().into();
        let sources = self
            .db
            .mall_payment_sources()
            .list_by_order(&order_id, &mut NoTransaction)
            .await?;
        let entries = self.load_entries_for_sources(&sources).await?;
        let assessments = self.load_current_assessments(&entries).await?;
        let facts = fact_map
            .get(&(row.mall_id.clone(), row.external_order_no.clone()))
            .cloned()
            .unwrap_or_default();
        let facts = facts
            .into_iter()
            .map(|(fact_type, occurred_at, data_source)| OrderFactSummary {
                fact_type,
                occurred_at,
                data_source,
            })
            .collect::<Vec<_>>();

        let card_amount = sources
            .iter()
            .filter(|source| source.source_type == PaymentSourceType::Card)
            .fold(Amount::from_str("0.00")?, |acc, source| {
                acc.checked_add(source.amount)
            });
        let wechat_amount = sources
            .iter()
            .filter(|source| source.source_type == PaymentSourceType::Wechat)
            .fold(Amount::from_str("0.00")?, |acc, source| {
                acc.checked_add(source.amount)
            });
        let mut fact_summary = Vec::new();
        for fact_type in [
            FactType::PaymentSucceeded,
            FactType::OrderCanceled,
            FactType::RefundSucceeded,
            FactType::OrderCompleted,
            FactType::CardBalanceRestored,
        ] {
            let matched: Vec<_> = facts.iter().filter(|fact| fact.fact_type == fact_type).collect();
            if !matched.is_empty() {
                fact_summary.push(FactSummaryItemView {
                    fact_type,
                    latest_occurred_at: matched
                        .iter()
                        .map(|fact| fact.occurred_at.unix_secs())
                        .max()
                        .unwrap_or_default() as u64,
                    count: matched.len() as u64,
                });
            }
        }
        let data_source = facts
            .iter()
            .max_by_key(|fact| fact.occurred_at)
            .map(|fact| fact.data_source)
            .unwrap_or(DataSource::Realtime);

        let mut breakdown: Vec<CostBasisBreakdownItemView> = Vec::new();
        let mut distinct_bases: Vec<CostBasis> = Vec::new();
        for assessment in assessments.values() {
            if !distinct_bases.contains(&assessment.cost_basis) {
                distinct_bases.push(assessment.cost_basis);
            }
            let bucket = breakdown
                .iter_mut()
                .find(|item| item.basis == assessment.cost_basis);
            match bucket {
                Some(item) => {
                    item.line_count += 1;
                    if let Some(cost) = assessment.cost_amount_string() {
                        let current = item
                            .cost_amount
                            .as_deref()
                            .map(Amount::from_str)
                            .transpose()
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| Amount::from_str("0.00").expect("零常量可解析"));
                        item.cost_amount = Some(current.checked_add(cost).to_string());
                    }
                }
                None => breakdown.push(CostBasisBreakdownItemView {
                    basis: assessment.cost_basis,
                    line_count: 1,
                    cost_amount: assessment.cost_amount_string().map(|amount| amount.to_string()),
                }),
            }
        }
        let normalized_cost_basis = match distinct_bases.len() {
            0 => None,
            1 => distinct_bases
                .into_iter()
                .next()
                .map(|basis| basis.as_str().to_string()),
            _ => Some("MIXED".to_string()),
        };

        let customer_id_label = row.customer_id.map(|id| id.to_string());
        Ok(MallOrderListRow {
            mall_order_id: row.id,
            mall_id: row.mall_id.clone(),
            mall_name: row.mall_id,
            external_order_no: row.external_order_no,
            customer_id: customer_id_label.clone(),
            customer_label: customer_id_label,
            paid_at: row.paid_at.unix_secs() as u64,
            paid_amount: row.paid_amount.to_string(),
            payment_composition: PaymentCompositionView {
                card_amount: card_amount.to_string(),
                wechat_amount: wechat_amount.to_string(),
                source_count: sources.len() as u32,
            },
            fact_summary,
            fulfillment_chain: row.fulfillment_chain,
            supplier_order_summary: SupplierOrderSummaryView {
                total: 0,
                statuses: Vec::new(),
                has_exception: false,
            },
            attribution_status: row.attribution_status,
            cost_basis_breakdown: breakdown,
            data_source,
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
            cost_basis_policy_state: "CONFIGURED".to_string(),
            normalized_cost_basis,
        })
    }

    /// 组装订单详情视图（W25 §8.2 对象中心）。
    ///
    /// # 参数
    /// * `order` - 订单实体
    /// * `items` - 商品明细
    /// * `sources` - 支付来源
    /// * `allocations` - 分摊记录
    /// * `facts` - 关键事实
    /// * `entries` - 消费事实
    /// * `assessments` - 消费 → 当前评估映射
    /// * `cutover` - 该商城已启用的切换记录
    ///
    /// # 返回
    /// 返回订单详情视图。
    #[allow(clippy::too_many_arguments)]
    fn build_detail_view(
        &self,
        order: MallOrder,
        items: Vec<MallOrderItem>,
        sources: Vec<MallPaymentSource>,
        allocations: Vec<MallItemFundingAllocation>,
        facts: Vec<MallOrderFact>,
        entries: Vec<MallConsumptionEntry>,
        assessments: std::collections::HashMap<String, MallConsumptionCostAssessment>,
        cutover: Option<MallConsumptionCutover>,
    ) -> MallOrderDetailView {
        let source_views: Vec<PaymentSourceView> = sources
            .iter()
            .map(|source| PaymentSourceView {
                payment_source_id: source.base.id.clone(),
                source_no: source.source_no,
                source_type: source.source_type,
                amount: source.amount.to_string(),
                source_reference: source
                    .source_card_instance_ref
                    .clone()
                    .or_else(|| source.wechat_payment_ref.clone())
                    .map(|reference| mask_reference(&reference))
                    .unwrap_or_else(|| "已加密存储".to_string()),
                mall_card_instance_id: source.mall_card_instance_id.as_ref().map(|id| id.to_string()),
                attribution_status: source.attribution_status,
                origin: None,
            })
            .collect();
        let conservation = self.build_conservation(&order, &items, &sources, &allocations);
        let item_attribution = if sources
            .iter()
            .any(|source| source.attribution_status == AttributionStatus::PendingAttribution)
        {
            AttributionStatus::PendingAttribution
        } else {
            AttributionStatus::Attributed
        };
        let entry_views = entries
            .iter()
            .map(|entry| ConsumptionEntryView {
                consumption_entry_id: entry.base.id.clone(),
                fact_id: entry.mall_order_fact_id.to_string(),
                item_id: entry.mall_order_item_id.to_string(),
                payment_source_id: entry.mall_payment_source_id.to_string(),
                direction: entry.direction,
                amount: entry.amount.to_string(),
                occurred_at: entry.occurred_at.unix_secs() as u64,
                attribution_status: entry.attribution_status,
                origin_sales_order_id: entry.origin_sales_order_id.as_ref().map(|id| id.to_string()),
                reverses_consumption_entry_id: entry
                    .reverses_consumption_entry_id
                    .as_ref()
                    .map(|id| id.to_string()),
                current_cost_assessment: assessments.get(&entry.base.id).map(cost_assessment_view),
            })
            .collect();

        MallOrderDetailView {
            identity: MallOrderIdentityView {
                mall_order_id: order.base.id.clone(),
                mall_id: order.mall_id.clone(),
                mall_name: order.mall_id.clone(),
                external_order_no: order.external_order_no.clone(),
                payment_fact_id: order.payment_fact_id.to_string(),
            },
            customer: MallOrderCustomerView {
                source_customer_ref: order.source_customer_ref.clone(),
                customer_id: order.customer_id.as_ref().map(|id| id.to_string()),
                customer_label: order.customer_id.as_ref().map(|id| id.to_string()),
                attribution_status: order.attribution_status,
            },
            ordered_at: order.ordered_at.unix_secs() as u64,
            paid_at: order.paid_at.unix_secs() as u64,
            amounts: MallOrderAmountsView {
                gross: order.gross_amount.to_string(),
                discount: order.discount_amount.to_string(),
                freight: order.freight_amount.to_string(),
                paid: order.paid_amount.to_string(),
                conservation_status: if conservation.order_total.valid {
                    "VALID".to_string()
                } else {
                    "DIFFERENCE".to_string()
                },
            },
            fulfillment: MallOrderFulfillmentView {
                chain: order.fulfillment_chain,
                cutover_id: cutover.as_ref().map(|record| record.base.id.clone()),
                cutover_at: cutover
                    .as_ref()
                    .and_then(|record| record.enabled_at.map(|t| t.unix_secs() as u64)),
                decided_by_occurred_at: order.paid_at.unix_secs() as u64,
            },
            facts: facts.iter().map(fact_view).collect(),
            items: items
                .iter()
                .map(|item| MallOrderItemView {
                    mall_order_item_id: item.base.id.clone(),
                    external_item_id: item.external_item_id.clone(),
                    sku_id: item.sku_id.as_ref().map(|id| id.to_string()),
                    product_publication_revision_id: item
                        .product_publication_revision_id
                        .as_ref()
                        .map(|id| id.to_string()),
                    supplier_offering_revision_id: item
                        .supplier_offering_revision_id
                        .as_ref()
                        .map(|id| id.to_string()),
                    name_snapshot: item.name_snapshot.clone(),
                    spec_snapshot: item.spec_snapshot.clone(),
                    quantity: item.quantity.to_string(),
                    unit_price_gross: item.unit_price_gross.to_string(),
                    line_gross_amount: item.line_gross_amount.to_string(),
                    allocated_discount_amount: item.allocated_discount_amount.to_string(),
                    allocated_freight_amount: item.allocated_freight_amount.to_string(),
                    paid_amount: item.paid_amount.to_string(),
                    sales_tax_rate: item.sales_tax_rate.to_string(),
                    unit_cost_snapshot: item.unit_cost_snapshot.map(|value| value.to_string()),
                    cost_snapshot_total: item.cost_snapshot_total.map(|value| value.to_string()),
                    cost_tax_inclusion: item.cost_tax_inclusion,
                    cost_input_tax_rate: item.cost_input_tax_rate.map(|value| value.to_string()),
                    attribution_status: item_attribution,
                })
                .collect(),
            payment_sources: source_views,
            funding_allocations: allocations
                .iter()
                .map(|allocation| FundingAllocationView {
                    mall_order_item_id: allocation.mall_order_item_id.to_string(),
                    payment_source_id: allocation.mall_payment_source_id.to_string(),
                    allocated_payment_amount: allocation.allocated_payment_amount.to_string(),
                })
                .collect(),
            conservation,
            consumption_entries: entry_views,
            supplier_orders: Vec::new(),
            address: MallOrderAddressView {
                masked_summary: if order.address_snapshot_encrypted.is_some() {
                    "已加密存储，需受控揭示".to_string()
                } else {
                    "未记录".to_string()
                },
                reveal_allowed: false,
            },
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
        }
    }

    /// 计算分摊矩阵守恒校验（§6.17 行/列守恒 + 订单总额）。
    ///
    /// # 参数
    /// * `order` - 订单实体
    /// * `items` - 商品明细
    /// * `sources` - 支付来源
    /// * `allocations` - 分摊记录
    ///
    /// # 返回
    /// 返回守恒校验视图。
    fn build_conservation(
        &self,
        order: &MallOrder,
        items: &[MallOrderItem],
        sources: &[MallPaymentSource],
        allocations: &[MallItemFundingAllocation],
    ) -> ConservationView {
        let zero = Amount::from_str("0.00").expect("零常量可解析");
        let item_rows = items
            .iter()
            .map(|item| {
                let actual = allocations
                    .iter()
                    .filter(|allocation| allocation.mall_order_item_id.as_ref() == item.base.id)
                    .fold(zero, |acc, allocation| {
                        acc.checked_add(allocation.allocated_payment_amount)
                    });
                ConservationResultRow {
                    id: item.base.id.clone(),
                    expected: item.paid_amount.to_string(),
                    actual: actual.to_string(),
                    valid: actual.to_decimal() == item.paid_amount.to_decimal(),
                }
            })
            .collect();
        let source_columns = sources
            .iter()
            .map(|source| {
                let actual = allocations
                    .iter()
                    .filter(|allocation| allocation.mall_payment_source_id.as_ref() == source.base.id)
                    .fold(zero, |acc, allocation| {
                        acc.checked_add(allocation.allocated_payment_amount)
                    });
                ConservationResultRow {
                    id: source.base.id.clone(),
                    expected: source.amount.to_string(),
                    actual: actual.to_string(),
                    valid: actual.to_decimal() == source.amount.to_decimal(),
                }
            })
            .collect();
        let actual_paid = allocations.iter().fold(zero, |acc, allocation| {
            acc.checked_add(allocation.allocated_payment_amount)
        });
        ConservationView {
            item_row_results: item_rows,
            source_column_results: source_columns,
            order_total: ConservationResultRow {
                id: order.base.id.clone(),
                expected: order.paid_amount.to_string(),
                actual: actual_paid.to_string(),
                valid: actual_paid.to_decimal() == order.paid_amount.to_decimal(),
            },
        }
    }
}

/// 从事实实体构造响应视图。
///
/// # 参数
/// * `fact` - 关键事实实体
///
/// # 返回
/// 返回响应视图。
fn fact_view(fact: &MallOrderFact) -> MallOrderFactView {
    MallOrderFactView {
        fact_id: fact.base.id.clone(),
        fact_type: fact.fact_type,
        business_fact_key: fact.business_fact_key.clone(),
        external_order_version: fact.external_order_version.clone(),
        after_sales_request_id: fact.after_sales_request_id.as_ref().map(|id| id.to_string()),
        original_payment_fact_id: fact.original_payment_fact_id.as_ref().map(|id| id.to_string()),
        occurred_at: fact.occurred_at.unix_secs() as u64,
        received_at: fact.received_at.unix_secs() as u64,
        data_source: fact.data_source,
        processing_status: fact.processing_status,
    }
}

/// 从成本评估实体构造响应视图。
///
/// # 参数
/// * `assessment` - 成本评估实体
///
/// # 返回
/// 返回响应视图。
fn cost_assessment_view(assessment: &MallConsumptionCostAssessment) -> CostAssessmentView {
    CostAssessmentView {
        assessment_id: assessment.base.id.clone(),
        assessment_no: assessment.assessment_no,
        cost_basis: assessment.cost_basis,
        basis_source_label: assessment
            .basis_source_type
            .map(|source| source.label().to_string())
            .unwrap_or_else(|| "无可用成本来源".to_string()),
        gross_amount: assessment.gross_amount.map(|amount| amount.to_string()),
        net_amount: assessment.net_amount.map(|amount| amount.to_string()),
        tax_amount: assessment.tax_amount.map(|amount| amount.to_string()),
        tax_inclusion: assessment.tax_inclusion,
        input_tax_rate: assessment.input_tax_rate.map(|rate| rate.to_string()),
        assessed_at: assessment.assessed_at.unix_secs() as u64,
    }
}

/// 对敏感引用做脱敏展示（保留前后缀，中间以 `****` 掩盖）。
///
/// # 参数
/// * `reference` - 原始引用
///
/// # 返回
/// 返回脱敏后的展示串。
fn mask_reference(reference: &str) -> String {
    if reference.len() <= 8 {
        "****".to_string()
    } else {
        format!("{}****{}", &reference[..4], &reference[reference.len() - 4..])
    }
}

/// 归集状态判定：卡券来源映射到卡实例为已归集，否则待归集；微信恒为已归集。
///
/// # 参数
/// * `source_type` - 来源类型
/// * `card_instance` - 映射到的卡实例
///
/// # 返回
/// 返回归集状态。
fn attribution_for(
    source_type: PaymentSourceType,
    card_instance: &Option<MallCardInstance>,
) -> AttributionStatus {
    match source_type {
        PaymentSourceType::Card if card_instance.is_some() => AttributionStatus::Attributed,
        PaymentSourceType::Card => AttributionStatus::PendingAttribution,
        PaymentSourceType::Wechat => AttributionStatus::Attributed,
    }
}

/// 从评估实体派生成本金额合计（`NONE` 为空）。
trait AssessmentAmountString {
    /// 返回成本金额展示串（`NONE` 为 `None`）。
    fn cost_amount_string(&self) -> Option<Amount>;
}

impl AssessmentAmountString for MallConsumptionCostAssessment {
    fn cost_amount_string(&self) -> Option<Amount> {
        if self.cost_basis == CostBasis::None {
            None
        } else {
            self.gross_amount
        }
    }
}

/// （商城, 订单号）→ 事实摘要列表的映射类型（列表行聚合用）。
type OrderFactMap = std::collections::HashMap<(String, String), Vec<(FactType, Instant, DataSource)>>;

/// 商城订单列表投影行（Service 内私有，避免依赖仓储私有子树类型名）。
#[derive(Debug, Clone)]
struct OrderListRow {
    /// 实体主键。
    id: String,
    /// 商城订单身份。
    mall_id: String,
    /// 商城订单号。
    external_order_no: String,
    /// 映射后的企业客户。
    customer_id: Option<entities::ids::CustomerAccountId>,
    /// 支付成功时间。
    paid_at: Instant,
    /// 实付快照。
    paid_amount: Amount,
    /// 履约链归属。
    fulfillment_chain: FulfillmentChain,
    /// 归集进度状态。
    attribution_status: AttributionStatus,
}

/// 事实摘要（列表行聚合用）。
#[derive(Debug, Clone, Copy)]
struct OrderFactSummary {
    /// 事实类型。
    fact_type: FactType,
    /// 发生时间。
    occurred_at: Instant,
    /// 数据来源。
    data_source: DataSource,
}
