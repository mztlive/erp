//! 域 D32 `supplier_fulfillment` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 下单：`supplier_fulfillment_orders` + `supplier_fulfillment_items` +
//!   首个 `PLACE` 动作 + `inbox_message` + 审计同事务（§6.19）；
//! - 取消/退款：动作头 + 动作行 + 订单状态推进 + `inbox_message` 同事务；
//! - 拒单结果：订单状态 + 状态历史 + 动作结果 + 审计同事务（§6.19 回调幂等）；
//! - 退款成功结果：退款事实头 + 分配行 + 订单退款进度同事务（§8.4 第 5 条
//!   实体可判定部分，`create_refund_fact_with_allocations` 要求事务执行器）。
//!
//! 供应商 API 调用在事务之外完成（P3 §7）：先事务内落 `inbox_message`，事务外经
//! [`SupplierGateway`] 派发（超时/重试上限/错误分类由网关实现承担），结果经
//! `inbox_message` + `integration_error_task` 承接（D34 仓储），失败降级为
//! 可观测错误并记录 `account` 上下文。
//!
//! 跨域协作只经 DatabaseExt 调对方域 Repository（P3 §2）：D25 `supplier_api`
//! （连接与能力）、D29 `mall_order`（商城订单与明细）、D24 `supplier_offering`
//! （供给修订）、D30 `mall_after_sales`（售后申请与行，§6.19 净余额校验）、
//! D34 `integration_ops`（inbox_message / integration_error_task）。
//!
//! 资金/状态机入口一律幂等（§6.19）：下单键为 `fulfillment_order_no`，
//! 取消/退款键为「ERP 供应商订单号 + 动作类型 + 商城售后请求 ID」（本域拼装），
//! 拒单键为 `(connection_id, external_event_id)`，退款结果键为
//! `(connection_id, external_refund_no, external_refund_version)`；
//! 重复提交只返回原结果，不产生第二条正式事实。

use std::collections::HashMap;
use std::sync::Arc;

use database::{
    AccessControlExt, IntegrationOpsExt, MallAfterSalesExt, MallOrderExt, NoTransaction, SupplierApiExt,
    SupplierFulfillmentExt, SupplierOfferingExt, Transactional,
};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::ids::{
    InboxMessageId, SourceSystemId, SupplierOrderActionId, SupplierOrderActionLineId,
    SupplierOrderStatusHistoryId, SupplierRefundAllocationId, SupplierRefundFactId,
};
use entities::integration_ops::{
    ErrorClass, InboxMessage, InboxMessageData, InboxMessageStatus, InboxMessageUpdate, IntegrationErrorTask,
    IntegrationErrorTaskData, IntegrationErrorTaskId, MessageType,
};
use entities::money::{round_to_cent, Amount, Quantity};
use entities::supplier_api::{SupplierApiCapability, SupplierApiCapabilityCode, SupplierApiConnection};
use entities::supplier_fulfillment::{
    CancelStatus, FulfillmentStatus, RefundStatus, SupplierFulfillmentItem, SupplierFulfillmentItemData,
    SupplierFulfillmentItemId, SupplierFulfillmentOrder, SupplierFulfillmentOrderData,
    SupplierFulfillmentOrderId, SupplierFulfillmentOrderUpdate, SupplierOrderAction, SupplierOrderActionData,
    SupplierOrderActionLine, SupplierOrderActionLineData, SupplierOrderActionStatus, SupplierOrderActionType,
    SupplierOrderActionUpdate, SupplierOrderStatusHistory, SupplierOrderStatusHistoryData,
    SupplierRefundAllocation, SupplierRefundAllocationData, SupplierRefundFact, SupplierRefundFactData,
};
use id_generator::next_id;
use mongodb::{bson::doc, Database};
use std::str::FromStr;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

pub(crate) mod dto;
mod gateway;

use self::dto::SortDir;

pub use self::dto::{
    AfterSalesActionLineRequest, PageView, PlaceFulfillmentOrderRequest, RecordRefundResultRequest,
    RecordSupplierRejectRequest, SubmitActionResultView, SubmitAfterSalesActionRequest,
    SupplierFulfillmentOrderDetailView, SupplierFulfillmentOrderListParams, SupplierFulfillmentOrderView,
    SupplierOrderActionLineView, SupplierOrderActionView, SupplierOrderStatusHistoryView,
    SupplierRefundAllocationView, SupplierRefundFactView,
};
pub use self::gateway::{DispatchOutcome, SimulatedSupplierGateway, SupplierGateway};

/// 履约订单列表筛选条件类型（经 `SupplierFulfillmentExt` 关联类型跨 crate 可达）。
type FulfillmentOrderFilter = <mongodb::Database as SupplierFulfillmentExt>::SupplierFulfillmentOrderFilter;

/// 返回零金额（成本余额累加起点）。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("零是合法金额")
}

/// 返回零数量（售后申请行净余额累加起点）。
fn zero_quantity() -> Quantity {
    Quantity::from_str("0.000000").expect("零是合法数量")
}

/// 数量相加（小数位受类型约束，恒合法）。
///
/// # 参数
/// * `left` - 加数
/// * `right` - 加数
///
/// # 返回
/// 返回相加结果。
fn qty_add(left: Quantity, right: Quantity) -> Quantity {
    Quantity::try_from(left.to_decimal() + right.to_decimal()).expect("数量小数位合法")
}

/// 数量相减（小数位受类型约束，恒合法）。
///
/// # 参数
/// * `left` - 被减数
/// * `right` - 减数
///
/// # 返回
/// 返回相减结果。
fn qty_sub(left: Quantity, right: Quantity) -> Quantity {
    Quantity::try_from(left.to_decimal() - right.to_decimal()).expect("数量小数位合法")
}

/// 金额相减（小数位受类型约束，恒合法）。
///
/// # 参数
/// * `left` - 被减数
/// * `right` - 减数
///
/// # 返回
/// 返回相减结果。
fn amount_sub(left: Amount, right: Amount) -> Amount {
    Amount::try_from(left.to_decimal() - right.to_decimal()).expect("金额小数位合法")
}

/// 供应商履约服务。
///
/// 提供供应商子订单的下单、查询、取消/退款动作提交与外部结果登记编排。
pub struct SupplierFulfillmentService {
    db: Database,
    gateway: Arc<dyn SupplierGateway>,
}

impl SupplierFulfillmentService {
    /// 创建供应商履约服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `gateway` - 供应商动作派发网关（真实 Connector 接入点，只在事务外调用）
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, gateway: Arc<dyn SupplierGateway>) -> Self {
        Self { db, gateway }
    }

    /// 分页查询供应商履约订单列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数（`supplier_id`/三条状态/`external_order_no` 等扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_fulfillment_order_list(
        &self,
        params: &SupplierFulfillmentOrderListParams,
    ) -> Result<PageView<SupplierFulfillmentOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = FulfillmentOrderFilter {
            supplier_id: query.supplier_id,
            fulfillment_status: query.fulfillment_status,
            external_order_no: query.external_order_no,
            mall_order_id: query.mall_order_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_fulfillment_orders()
            .search_supplier_fulfillment_orders(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierFulfillmentOrderView {
                id: row.id,
                fulfillment_order_no: row.fulfillment_order_no,
                mall_order_id: row.mall_order_id.to_string(),
                supplier_id: row.supplier_id.to_string(),
                connection_id: row.connection_id.to_string(),
                split_no: row.split_no,
                fulfillment_status: row.fulfillment_status,
                cancel_status: row.cancel_status,
                refund_status: row.refund_status,
                external_order_no: row.external_order_no,
                submitted_at: row.submitted_at.map(|t| t.unix_secs()),
                accepted_at: row.accepted_at.map(|t| t.unix_secs()),
                completed_at: row.completed_at.map(|t| t.unix_secs()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询供应商履约订单详情（订单 + 明细 + 状态历史 + 动作 + 退款事实）。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_fulfillment_order_detail(
        &self,
        id: &str,
    ) -> Result<SupplierFulfillmentOrderDetailView> {
        let order = self.load_order(id).await?;
        let order_id = SupplierFulfillmentOrderId::new(id);
        let items = self
            .db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(std::slice::from_ref(&order_id), &mut NoTransaction)
            .await?;
        let actions = self
            .db
            .supplier_order_actions()
            .find_many_sorted(
                doc! { "supplier_fulfillment_order_id": id },
                doc! { "created_at": -1 },
                &mut NoTransaction,
            )
            .await?;
        let histories = self
            .db
            .supplier_order_status_histories()
            .find_many_sorted(
                doc! { "supplier_fulfillment_order_id": id },
                doc! { "occurred_at": 1 },
                &mut NoTransaction,
            )
            .await?;
        let refund_views = self.refund_views_for_order(&order_id).await?;

        Ok(SupplierFulfillmentOrderDetailView {
            order: order.into(),
            items: items.into_iter().map(item_view).collect(),
            status_history: histories.into_iter().map(Into::into).collect(),
            actions: actions.into_iter().map(Into::into).collect(),
            refund_facts: refund_views,
        })
    }

    /// 供应商下单（幂等键：`fulfillment_order_no`，§6.19）。
    ///
    /// 同事务创建子订单、全部明细、首个 `PLACE` 动作与 `inbox_message`；
    /// 事务外经网关派发供应商 API（P3 §7），结果经 `inbox_message` +
    /// `integration_error_task` 承接。重复提交（同一订单号）返回原订单当前视图，
    /// 不重复下单、不生成新单号。
    ///
    /// # 参数
    /// * `req` - 下单请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回下单后订单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 连接/商城订单/明细/供给修订不存在
    /// * `BusinessLogicError` - 连接未启用或缺少下单能力
    /// * `ConflictError` - 唯一键冲突（并发重复下单）
    /// * `OutcomeUnknown` - 提交结果无法确认
    pub async fn submit_place(
        &self,
        req: PlaceFulfillmentOrderRequest,
        actor: &AuditActor,
    ) -> Result<SupplierFulfillmentOrderView> {
        req.validate()?;
        if let Some(existing) = self
            .db
            .supplier_fulfillment_orders()
            .find_by_fulfillment_order_no(&req.fulfillment_order_no, &mut NoTransaction)
            .await?
        {
            tracing::info!(account = %actor.id(), order_no = %req.fulfillment_order_no, "下单幂等命中，返回原订单");
            return Ok(existing.into());
        }
        let (connection, offerings) = self.ensure_placeable(&req).await?;
        let (mut order, items, mut action) = self.build_place_facts(&req, &offerings)?;
        let mut message = build_action_message(&action, &connection, InboxMessageStatus::Received)?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.submit",
            "supplier_fulfillment_order",
            order.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let items_for_tx = items.clone();
        let action_for_tx = action.clone();
        let message_for_tx = message.clone();
        let audit_for_tx = audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_fulfillment()
                        .create_fulfillment_with_items_and_place_action(
                            &order_for_tx,
                            &items_for_tx,
                            &action_for_tx,
                            session,
                        )
                        .await?;
                    db.inbox_messages().create(&message_for_tx, session).await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        tracing::info!(account = %actor.id(), order_id = %order.base.id, "下单事务已提交，开始事务外供应商派发");
        self.settle_dispatch(&mut order, &mut action, &mut message, &connection, actor)
            .await?;
        Ok(order.into())
    }

    /// 提交供应商取消（幂等键：「订单号 + CANCEL + 售后申请 ID」，§6.19）。
    ///
    /// 同事务创建 `CANCEL` 动作头/行并把 `cancel_status` 推进到 `CANCEL_PENDING`；
    /// 事务外派发供应商 API。重复提交（同一幂等键）返回原动作结果，不再次调用。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 取消动作提交请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回动作与动作后订单视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单/售后申请/申请行不存在
    /// * `BusinessLogicError` - 动作范围非法或连接缺少取消能力
    /// * `ConflictError` - 唯一键冲突（并发重复提交）
    pub async fn submit_cancel(
        &self,
        id: &str,
        req: SubmitAfterSalesActionRequest,
        actor: &AuditActor,
    ) -> Result<SubmitActionResultView> {
        self.submit_after_sales_action(id, req, SupplierOrderActionType::Cancel, actor)
            .await
    }

    /// 提交供应商退款（幂等键：「订单号 + REFUND + 售后申请 ID」，§6.19）。
    ///
    /// 同事务创建 `REFUND` 动作头/行并把 `refund_status` 推进到 `REFUND_PENDING`；
    /// 事务外派发供应商 API。重复提交（同一幂等键）返回原动作结果，不再次调用。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 退款动作提交请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回动作与动作后订单视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单/售后申请/申请行不存在
    /// * `BusinessLogicError` - 动作范围非法或连接缺少退款能力
    /// * `ConflictError` - 唯一键冲突（并发重复提交）
    pub async fn submit_refund(
        &self,
        id: &str,
        req: SubmitAfterSalesActionRequest,
        actor: &AuditActor,
    ) -> Result<SubmitActionResultView> {
        self.submit_after_sales_action(id, req, SupplierOrderActionType::Refund, actor)
            .await
    }

    /// 登记供应商拒单结果（回调幂等键 `(connection_id, external_event_id)`，§6.19）。
    ///
    /// 同事务推进履约主线到 `REJECTED`、追加状态历史并把原 `PLACE` 动作标记为
    /// 明确失败。重复回调（同一事件 ID）返回原状态历史，不重复推进。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 拒单结果请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新增的状态历史视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    /// * `BusinessLogicError` - 当前履约状态不可拒单
    /// * `ConflictError` - 事件 ID 冲突（事务注入失败时全部不可见）
    pub async fn record_reject(
        &self,
        id: &str,
        req: RecordSupplierRejectRequest,
        actor: &AuditActor,
    ) -> Result<SupplierOrderStatusHistoryView> {
        req.validate()?;
        let mut order = self.load_order(id).await?;
        let connection_id = order.connection_id.clone();
        if let Some(existing) = self
            .db
            .supplier_order_status_histories()
            .find_by_connection_and_event(&connection_id, &req.external_event_id, &mut NoTransaction)
            .await?
        {
            tracing::info!(account = %actor.id(), order_id = %id, "拒单回调幂等命中");
            return Ok(existing.into());
        }
        let previous = order.fulfillment_status;
        order.advance_fulfillment(FulfillmentStatus::Rejected)?;
        let history = SupplierOrderStatusHistory::new(
            SupplierOrderStatusHistoryId::new(next_id()),
            SupplierOrderStatusHistoryData {
                connection_id,
                previous_status: previous,
                new_status: FulfillmentStatus::Rejected,
                supplier_status_version: req.supplier_status_version.clone(),
                occurred_at: Instant::from_unix_secs(req.occurred_at),
                received_at: Instant::now(),
                external_event_id: req.external_event_id.clone(),
                source_type: SourceType::SupplierCallback,
            },
        )?;
        let mut action = self.latest_place_action(id).await?;
        action.update(SupplierOrderActionUpdate {
            status: Some(SupplierOrderActionStatus::Failed),
            response_summary: Some("供应商明确拒单（回调登记）".to_string()),
            ..Default::default()
        })?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.reject",
            "supplier_fulfillment_order",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mut order_for_tx = order.clone();
        let history_for_tx = history.clone();
        let mut action_for_tx = action.clone();
        let audit_for_tx = audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_fulfillment_orders()
                        .update(&mut order_for_tx, session)
                        .await?;
                    db.supplier_order_status_histories()
                        .create(&history_for_tx, session)
                        .await?;
                    db.supplier_order_actions()
                        .update(&mut action_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(history.into())
    }

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
        let mut order = self.load_order(id).await?;
        let connection_id = order.connection_id.clone();
        if let Some(existing) = self
            .db
            .supplier_refund_facts()
            .find_by_connection_and_refund(
                &connection_id,
                &req.external_refund_no,
                &req.external_refund_version,
                &mut NoTransaction,
            )
            .await?
        {
            tracing::info!(account = %actor.id(), order_id = %id, "退款结果幂等命中");
            let fact_id = SupplierRefundFactId::new(existing.base.id.as_str());
            let allocations = self
                .db
                .supplier_refund_allocations()
                .find_allocations_by_fact_ids(&[fact_id], &mut NoTransaction)
                .await?;
            return Ok(refund_fact_view(&existing, &allocations));
        }
        let order_total = self.order_total_cost(id).await?;
        let refunded_total = self
            .db
            .supplier_refund_facts()
            .find_refund_facts_by_order_ids(&[SupplierFulfillmentOrderId::new(id)], &mut NoTransaction)
            .await?
            .iter()
            .fold(zero_amount(), |acc, fact| acc.checked_add(fact.refund_amount));
        let total_after = refunded_total.checked_add(req.refund_amount);
        if total_after > order_total {
            return Err(Error::BusinessLogicError(
                "累计净退款金额不得超过订单成本余额".to_string(),
            ));
        }
        order.advance_refund(if total_after == order_total {
            RefundStatus::Refunded
        } else {
            RefundStatus::Partial
        })?;
        let message = build_refund_message(&order, &req, &connection_id, InboxMessageStatus::Received)?;
        let (fact, allocations) = self.build_refund_fact(&order, &req, &connection_id, &message)?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.refund_result",
            "supplier_refund_fact",
            fact.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mut order_for_tx = order.clone();
        let fact_for_tx = fact.clone();
        let allocations_for_tx = allocations.clone();
        let message_for_tx = message.clone();
        let audit_for_tx = audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.inbox_messages().create(&message_for_tx, session).await?;
                    db.supplier_fulfillment_orders()
                        .update(&mut order_for_tx, session)
                        .await?;
                    db.supplier_fulfillment()
                        .create_refund_fact_with_allocations(&fact_for_tx, &allocations_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(refund_fact_view(&fact, &allocations))
    }

    /// 提交供应商取消/退款动作的公共编排。
    ///
    /// 完成幂等命中、售后申请存在性、连接能力、动作行净余额校验后，同事务写入
    /// 动作头/行与订单状态推进，事务外派发供应商 API。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 动作提交请求
    /// * `action_type` - 动作类型（`Cancel` 或 `Refund`）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回动作与动作后订单视图。
    ///
    /// # 错误
    /// 同 [`Self::submit_cancel`]。
    async fn submit_after_sales_action(
        &self,
        id: &str,
        req: SubmitAfterSalesActionRequest,
        action_type: SupplierOrderActionType,
        actor: &AuditActor,
    ) -> Result<SubmitActionResultView> {
        req.validate()?;
        let mut order = self.load_order(id).await?;
        let idempotency_key = format!(
            "{}+{}+{}",
            order.fulfillment_order_no,
            action_type.as_str(),
            req.after_sales_request_id
        );
        if let Some(existing) = self
            .db
            .supplier_order_actions()
            .find_by_idempotency_key(&idempotency_key, &mut NoTransaction)
            .await?
        {
            tracing::info!(account = %actor.id(), order_id = %id, action_type = %action_type.as_str(), "售后动作幂等命中");
            let lines = self
                .db
                .supplier_order_action_lines()
                .find_lines_by_action_ids(
                    &[SupplierOrderActionId::new(existing.base.id.as_str())],
                    &mut NoTransaction,
                )
                .await?;
            return Ok(SubmitActionResultView {
                action: existing.into(),
                lines: lines.into_iter().map(action_line_view).collect(),
                order: order.into(),
            });
        }
        self.db
            .mall_after_sales_requests()
            .find_by_id(&req.after_sales_request_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商城售后申请不存在".to_string()))?;
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&order.connection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商连接不存在".to_string()))?;
        let capabilities = self
            .db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(&order.connection_id, &mut NoTransaction)
            .await?;
        let needed = match action_type {
            SupplierOrderActionType::Cancel => SupplierApiCapabilityCode::Cancel,
            SupplierOrderActionType::Refund => SupplierApiCapabilityCode::Refund,
            _ => return Err(Error::BusinessLogicError("不支持的售后动作类型".to_string())),
        };
        ensure_capability(&capabilities, needed)?;
        self.ensure_action_lines(&order, &req).await?;
        let mut action = self.build_after_sales_action(&order, &req, &idempotency_key, action_type)?;
        let lines = self.build_action_lines(&action, &req)?;
        match action_type {
            SupplierOrderActionType::Cancel => order.advance_cancel(CancelStatus::CancelPending)?,
            SupplierOrderActionType::Refund => order.advance_refund(RefundStatus::RefundPending)?,
            _ => {}
        }
        let mut message = build_action_message(&action, &connection, InboxMessageStatus::Received)?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.after_sales_action",
            "supplier_order_action",
            action.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mut order_for_tx = order.clone();
        let action_for_tx = action.clone();
        let lines_for_tx = lines.clone();
        let message_for_tx = message.clone();
        let audit_for_tx = audit.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_order_actions()
                        .create(&action_for_tx, session)
                        .await?;
                    for line in &lines_for_tx {
                        db.supplier_order_action_lines().create(line, session).await?;
                    }
                    db.supplier_fulfillment_orders()
                        .update(&mut order_for_tx, session)
                        .await?;
                    db.inbox_messages().create(&message_for_tx, session).await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        tracing::info!(account = %actor.id(), action_id = %action.base.id, "售后动作事务已提交，开始事务外供应商派发");
        self.settle_dispatch(&mut order, &mut action, &mut message, &connection, actor)
            .await?;
        Ok(SubmitActionResultView {
            action: action.into(),
            lines: lines.into_iter().map(action_line_view).collect(),
            order: order.into(),
        })
    }
}

impl SupplierFulfillmentService {
    /// 校验下单前置条件（跨域读取，P3 §2）并返回连接实体。
    ///
    /// D25 连接存在且启用并声明 `order` 能力；D29 商城订单与全部明细存在且归属一致；
    /// D24 全部供给修订存在。
    ///
    /// # 参数
    /// * `req` - 下单请求
    ///
    /// # 返回
    /// 返回已校验的供应商连接实体。
    ///
    /// # 错误
    /// * `NotFound` - 连接/商城订单/明细/供给修订不存在
    /// * `BusinessLogicError` - 连接未启用、缺少下单能力或明细归属不一致
    async fn ensure_placeable(
        &self,
        req: &PlaceFulfillmentOrderRequest,
    ) -> Result<(
        SupplierApiConnection,
        HashMap<String, entities::supplier_offering::SupplierOffering>,
    )> {
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&req.connection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商连接不存在".to_string()))?;
        if !connection.is_active() {
            return Err(Error::BusinessLogicError("供应商连接未启用".to_string()));
        }
        if connection.supplier_id != req.supplier_id {
            return Err(Error::BusinessLogicError(
                "供应商连接不属于下单供应商".to_string(),
            ));
        }
        let capabilities = self
            .db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(&req.connection_id, &mut NoTransaction)
            .await?;
        ensure_capability(&capabilities, SupplierApiCapabilityCode::Order)?;
        self.db
            .mall_orders()
            .find_by_id(&req.mall_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源商城订单不存在".to_string()))?;
        self.ensure_mall_items(req).await?;
        let revision_ids = req
            .items
            .iter()
            .map(|item| item.supplier_offering_revision_id.to_string())
            .collect::<std::collections::HashSet<_>>();
        let revisions = self
            .db
            .supplier_offering_revisions()
            .find_many(
                doc! { "id": { "$in": revision_ids.iter().cloned().collect::<Vec<_>>() } },
                &mut NoTransaction,
            )
            .await?;
        if revisions.len() != revision_ids.len() {
            return Err(Error::NotFound("供应商供给修订不存在".to_string()));
        }
        let offering_ids = revisions
            .iter()
            .map(|revision| revision.supplier_offering_id.to_string())
            .collect::<std::collections::HashSet<_>>();
        let offerings = self
            .db
            .supplier_offerings()
            .find_many(
                doc! { "id": { "$in": offering_ids.into_iter().collect::<Vec<_>>() } },
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .map(|offering| (offering.base.id.clone(), offering))
            .collect::<HashMap<_, _>>();
        let mut by_revision = HashMap::with_capacity(revisions.len());
        for revision in revisions {
            let offering = offerings
                .get(revision.supplier_offering_id.as_ref())
                .ok_or_else(|| Error::NotFound("供应商供给不存在".to_string()))?;
            let connection_matches = offering
                .source_connection_id
                .as_ref()
                .is_none_or(|id| id == &req.connection_id);
            if offering.supplier_id != req.supplier_id || !connection_matches {
                return Err(Error::BusinessLogicError(
                    "供给不属于下单供应商或供应商连接不匹配".to_string(),
                ));
            }
            by_revision.insert(revision.base.id, offering.clone());
        }
        Ok((connection, by_revision))
    }

    /// 校验商城订单明细全部存在且归属该商城订单（D29 跨域读取）。
    ///
    /// # 参数
    /// * `req` - 下单请求
    ///
    /// # 错误
    /// * `NotFound` - 商城订单明细不存在
    /// * `BusinessLogicError` - 明细不属于该商城订单
    async fn ensure_mall_items(&self, req: &PlaceFulfillmentOrderRequest) -> Result<()> {
        let item_ids: Vec<String> = req
            .items
            .iter()
            .map(|item| item.mall_order_item_id.to_string())
            .collect();
        let items = self
            .db
            .mall_order_items()
            .find_many(doc! { "id": { "$in": item_ids } }, &mut NoTransaction)
            .await?;
        if items.len() != req.items.len() {
            return Err(Error::NotFound("商城订单明细不存在".to_string()));
        }
        let expected = req.mall_order_id.to_string();
        if items
            .iter()
            .any(|item| item.mall_order_id.to_string() != expected)
        {
            return Err(Error::BusinessLogicError(
                "商城订单明细不属于该商城订单".to_string(),
            ));
        }
        Ok(())
    }

    /// 构建下单事实（子订单 + 明细 + 首个 `PLACE` 动作）。
    ///
    /// 明细含税成本快照由单位成本 × 数量按分舍入派生（§4.2 铁律 1）；
    /// `PLACE` 动作幂等键为 ERP 供应商子订单号（§6.19）。
    ///
    /// # 参数
    /// * `req` - 下单请求
    ///
    /// # 返回
    /// 返回 `(子订单, 明细, 动作)` 三元组。
    ///
    /// # 错误
    /// 实体构造校验失败时返回 `LogicError` 或 `ValidationError`。
    fn build_place_facts(
        &self,
        req: &PlaceFulfillmentOrderRequest,
        offerings: &HashMap<String, entities::supplier_offering::SupplierOffering>,
    ) -> Result<(
        SupplierFulfillmentOrder,
        Vec<SupplierFulfillmentItem>,
        SupplierOrderAction,
    )> {
        let order_id = SupplierFulfillmentOrderId::new(next_id());
        let order = SupplierFulfillmentOrder::new(
            order_id.clone(),
            SupplierFulfillmentOrderData {
                fulfillment_order_no: req.fulfillment_order_no.clone(),
                mall_order_id: req.mall_order_id.clone(),
                supplier_id: req.supplier_id.clone(),
                connection_id: req.connection_id.clone(),
                split_no: req.split_no,
                fulfillment_status: FulfillmentStatus::Submitting,
                cancel_status: CancelStatus::None,
                refund_status: RefundStatus::None,
                external_order_no: None,
                submitted_at: Some(Instant::now()),
                accepted_at: None,
                completed_at: None,
                address_snapshot_encrypted: req.address_snapshot_encrypted.clone(),
                address_snapshot_fingerprint: req.address_snapshot_fingerprint.clone(),
            },
        )?;
        let items = self.build_place_items(&order_id, req, offerings)?;
        let action = SupplierOrderAction::new(
            SupplierOrderActionId::new(next_id()),
            SupplierOrderActionData {
                supplier_fulfillment_order_id: order_id,
                action_type: SupplierOrderActionType::Place,
                after_sales_request_id: None,
                idempotency_key: req.fulfillment_order_no.clone(),
                status: SupplierOrderActionStatus::Pending,
                external_request_id: None,
                request_summary: Some(format!(
                    "下单 {} 明细 {} 行",
                    req.fulfillment_order_no,
                    items.len()
                )),
                response_summary: None,
                attempt_count: 0,
                next_attempt_at: None,
            },
        )?;
        Ok((order, items, action))
    }

    /// 构建下单明细（含税成本快照派生）。
    ///
    /// # 参数
    /// * `order_id` - 子订单 ID
    /// * `req` - 下单请求
    ///
    /// # 返回
    /// 返回明细集合。
    ///
    /// # 错误
    /// 数量/成本快照恒等校验失败时返回 `LogicError`。
    fn build_place_items(
        &self,
        order_id: &SupplierFulfillmentOrderId,
        req: &PlaceFulfillmentOrderRequest,
        offerings: &HashMap<String, entities::supplier_offering::SupplierOffering>,
    ) -> Result<Vec<SupplierFulfillmentItem>> {
        req.items
            .iter()
            .map(|item| {
                let offering = offerings
                    .get(item.supplier_offering_revision_id.as_ref())
                    .ok_or_else(|| Error::NotFound("供应商供给不存在".to_string()))?;
                let total = Amount::try_from(round_to_cent(
                    item.unit_cost_snapshot_gross.to_decimal() * item.quantity.to_decimal(),
                ))
                .map_err(|_| Error::from(entities::Error::from("明细成本快照金额无效")))?;
                SupplierFulfillmentItem::new(
                    SupplierFulfillmentItemId::new(next_id()),
                    SupplierFulfillmentItemData {
                        supplier_fulfillment_order_id: order_id.clone(),
                        mall_order_item_id: item.mall_order_item_id.clone(),
                        supplier_offering_revision_id: item.supplier_offering_revision_id.clone(),
                        supplier_sku_code_snapshot: offering.supplier_sku_code.clone(),
                        supplier_product_code_snapshot: offering.supplier_product_code.clone(),
                        quantity: item.quantity,
                        unit_cost_snapshot_gross: item.unit_cost_snapshot_gross,
                        cost_snapshot_total_gross: total,
                        input_tax_rate: item.input_tax_rate,
                    },
                )
                .map_err(Error::from)
            })
            .collect()
    }

    /// 构建取消/退款动作头。
    ///
    /// # 参数
    /// * `order` - 供应商子订单
    /// * `req` - 动作提交请求
    /// * `idempotency_key` - 「订单号 + 动作类型 + 售后申请 ID」
    /// * `action_type` - `Cancel` 或 `Refund`
    ///
    /// # 返回
    /// 返回动作实体。
    ///
    /// # 错误
    /// 实体构造校验失败时返回 `LogicError`。
    fn build_after_sales_action(
        &self,
        order: &SupplierFulfillmentOrder,
        req: &SubmitAfterSalesActionRequest,
        idempotency_key: &str,
        action_type: SupplierOrderActionType,
    ) -> Result<SupplierOrderAction> {
        let request_summary = req
            .reason_code
            .as_ref()
            .map(|code| {
                format!(
                    "{} 售后申请 {} 原因 {}",
                    action_type.label(),
                    req.after_sales_request_id,
                    code
                )
            })
            .unwrap_or_else(|| format!("{} 售后申请 {}", action_type.label(), req.after_sales_request_id));
        SupplierOrderAction::new(
            SupplierOrderActionId::new(next_id()),
            SupplierOrderActionData {
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                action_type,
                after_sales_request_id: Some(req.after_sales_request_id.clone()),
                idempotency_key: idempotency_key.to_string(),
                status: SupplierOrderActionStatus::Pending,
                external_request_id: None,
                request_summary: Some(request_summary),
                response_summary: None,
                attempt_count: 0,
                next_attempt_at: None,
            },
        )
        .map_err(Into::into)
    }

    /// 构建取消/退款动作行（行号从 1 起，冻结实际提交范围）。
    ///
    /// # 参数
    /// * `action` - 动作头
    /// * `req` - 动作提交请求
    ///
    /// # 返回
    /// 返回动作行集合。
    ///
    /// # 错误
    /// 数量/金额非正的实体校验失败时返回 `LogicError`。
    fn build_action_lines(
        &self,
        action: &SupplierOrderAction,
        req: &SubmitAfterSalesActionRequest,
    ) -> Result<Vec<SupplierOrderActionLine>> {
        req.lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                SupplierOrderActionLine::new(
                    SupplierOrderActionLineId::new(next_id()),
                    SupplierOrderActionLineData {
                        supplier_order_action_id: SupplierOrderActionId::new(action.base.id.as_str()),
                        line_no: (index + 1) as u32,
                        after_sales_request_line_id: line.after_sales_request_line_id.clone(),
                        supplier_fulfillment_item_id: line.supplier_fulfillment_item_id.clone(),
                        quantity: line.quantity,
                        amount: line.amount,
                    },
                )
            })
            .collect::<std::result::Result<Vec<_>, entities::Error>>()
            .map_err(crate::errors::Error::from)
    }

    /// 校验动作行范围（§6.19）：行明细必须属于该子订单；数量/金额不得超过
    /// 对应售后申请行尚未提交的净余额。
    ///
    /// # 参数
    /// * `order` - 供应商子订单
    /// * `req` - 动作提交请求
    ///
    /// # 错误
    /// * `NotFound` - 售后申请行不存在
    /// * `BusinessLogicError` - 明细归属或净余额超限
    async fn ensure_action_lines(
        &self,
        order: &SupplierFulfillmentOrder,
        req: &SubmitAfterSalesActionRequest,
    ) -> Result<()> {
        let order_items = self
            .db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(
                &[SupplierFulfillmentOrderId::new(order.base.id.as_str())],
                &mut NoTransaction,
            )
            .await?;
        let item_ids: std::collections::HashSet<String> =
            order_items.iter().map(|item| item.base.id.clone()).collect();
        let request_lines = self
            .db
            .mall_after_sales_request_lines()
            .find_many(
                doc! { "after_sales_request_id": req.after_sales_request_id.to_string() },
                &mut NoTransaction,
            )
            .await?;
        let prior_actions = self
            .db
            .supplier_order_actions()
            .find_many(
                doc! { "after_sales_request_id": req.after_sales_request_id.to_string() },
                &mut NoTransaction,
            )
            .await?;
        let prior_action_ids: Vec<SupplierOrderActionId> = prior_actions
            .iter()
            .map(|action| SupplierOrderActionId::new(action.base.id.as_str()))
            .collect();
        let prior_lines = self
            .db
            .supplier_order_action_lines()
            .find_lines_by_action_ids(&prior_action_ids, &mut NoTransaction)
            .await?;
        let mut submitted: HashMap<String, (Quantity, Amount)> = HashMap::new();
        for line in &prior_lines {
            let entry = submitted
                .entry(line.after_sales_request_line_id.to_string())
                .or_insert((zero_quantity(), zero_amount()));
            entry.0 = qty_add(entry.0, line.quantity);
            entry.1 = entry.1.checked_add(line.amount);
        }
        for line in &req.lines {
            if !item_ids.contains(line.supplier_fulfillment_item_id.as_ref()) {
                return Err(Error::BusinessLogicError(
                    "动作行不属于该供应商子订单".to_string(),
                ));
            }
            let request_line = request_lines
                .iter()
                .find(|request_line| request_line.base.id == *line.after_sales_request_line_id.as_ref())
                .ok_or_else(|| Error::NotFound("商城售后申请行不存在".to_string()))?;
            let (submitted_qty, submitted_amount) = submitted
                .get(line.after_sales_request_line_id.as_ref())
                .copied()
                .unwrap_or((zero_quantity(), zero_amount()));
            if line.quantity.to_decimal()
                > qty_sub(request_line.requested_quantity, submitted_qty).to_decimal()
                || line.amount.to_decimal()
                    > amount_sub(request_line.requested_amount, submitted_amount).to_decimal()
            {
                return Err(Error::BusinessLogicError(
                    "动作行数量或金额超过售后申请行尚未提交的净余额".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// 构建退款事实头与全部分配行，并校验 APPLY 合计恒等（§6.19）。
    ///
    /// # 参数
    /// * `order` - 供应商子订单
    /// * `req` - 退款成功结果请求
    /// * `connection_id` - 供应商连接
    /// * `message` - 同事务创建的 `inbox_message` 信封
    ///
    /// # 返回
    /// 返回 `(事实头, 分配行)`。
    ///
    /// # 错误
    /// 金额恒等或实体校验失败时返回 `LogicError`。
    fn build_refund_fact(
        &self,
        order: &SupplierFulfillmentOrder,
        req: &RecordRefundResultRequest,
        connection_id: &entities::ids::SupplierApiConnectionId,
        message: &InboxMessage,
    ) -> Result<(SupplierRefundFact, Vec<SupplierRefundAllocation>)> {
        let fact = SupplierRefundFact::new(
            SupplierRefundFactId::new(next_id()),
            SupplierRefundFactData {
                supplier_id: order.supplier_id.clone(),
                connection_id: connection_id.clone(),
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                external_refund_no: req.external_refund_no.clone(),
                external_refund_version: req.external_refund_version.clone(),
                refund_amount: req.refund_amount,
                refunded_at: Instant::from_unix_secs(req.refunded_at),
                source_event_id: req.source_event_id.clone(),
                inbox_message_id: InboxMessageId::new(message.base.id.as_str()),
            },
        )?;
        let allocations = req
            .allocations
            .iter()
            .enumerate()
            .map(|(index, allocation)| {
                SupplierRefundAllocation::new(
                    SupplierRefundAllocationId::new(next_id()),
                    SupplierRefundAllocationData {
                        supplier_refund_fact_id: SupplierRefundFactId::new(fact.base.id.as_str()),
                        allocation_no: (index + 1) as u32,
                        supplier_fulfillment_item_id: allocation.supplier_fulfillment_item_id.clone(),
                        original_cost_entry_id: allocation.original_cost_entry_id.clone(),
                        original_cost_allocation_id: allocation.original_cost_allocation_id.clone(),
                        original_payable_entry_id: allocation.original_payable_entry_id.clone(),
                        original_payment_allocation_id: allocation.original_payment_allocation_id.clone(),
                        refund_quantity: allocation.refund_quantity,
                        gross_amount: allocation.gross_amount,
                        net_amount: allocation.net_amount,
                        tax_amount: allocation.tax_amount,
                        payable_reduction_amount: allocation.payable_reduction_amount,
                        cash_refund_amount: allocation.cash_refund_amount,
                        cash_supplier_refund_id: None,
                        allocation_action: entities::supplier_fulfillment::AllocationAction::Apply,
                        reverses_allocation_id: None,
                    },
                )
            })
            .collect::<std::result::Result<Vec<_>, entities::Error>>()?;
        fact.validate_allocations(&allocations)?;
        Ok((fact, allocations))
    }

    /// 计算订单成本余额（明细含税成本快照合计，§4.2 铁律 2）。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回订单含税成本余额。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn order_total_cost(&self, id: &str) -> Result<Amount> {
        let items = self
            .db
            .supplier_fulfillment_items()
            .find_items_by_order_ids(&[SupplierFulfillmentOrderId::new(id)], &mut NoTransaction)
            .await?;
        Ok(items.iter().fold(zero_amount(), |acc, item| {
            acc.checked_add(item.cost_snapshot_total_gross)
        }))
    }

    /// 事务外派发供应商动作并承接结果（P3 §7）。
    ///
    /// 网关调用不在任何事务闭包内；结果经 `inbox_message` +
    /// `integration_error_task` 承接后在同一事务写回订单/动作/消息。
    ///
    /// # 参数
    /// * `order` - 供应商子订单（就地更新）
    /// * `action` - 供应商动作（就地更新）
    /// * `message` - `inbox_message` 信封（就地更新）
    /// * `connection` - 供应商连接
    /// * `actor` - 审计操作人
    ///
    /// # 错误
    /// 结果写回失败时返回 `ConflictError`/`RepositoryError`/`OutcomeUnknown`。
    async fn settle_dispatch(
        &self,
        order: &mut SupplierFulfillmentOrder,
        action: &mut SupplierOrderAction,
        message: &mut InboxMessage,
        connection: &SupplierApiConnection,
        actor: &AuditActor,
    ) -> Result<()> {
        let outcome = self.gateway.dispatch(action, order, connection).await;
        tracing::info!(
            account = %actor.id(),
            order_id = %order.base.id,
            action_type = %action.action_type.as_str(),
            outcome = %outcome_label(&outcome),
            "供应商动作派发完成（事务外）"
        );
        let task = self.apply_dispatch_outcome(order, action, message, outcome)?;
        self.write_dispatch_result(order, action, message, task.as_ref())
            .await
    }

    /// 应用派发结果到订单/动作/消息（不落库），失败路径构造错误任务。
    ///
    /// # 参数
    /// * `order` - 供应商子订单（就地更新）
    /// * `action` - 供应商动作（就地更新）
    /// * `message` - `inbox_message` 信封（就地更新）
    /// * `outcome` - 网关分类结果
    ///
    /// # 返回
    /// 失败路径返回待落库的错误任务，成功路径返回 `None`。
    ///
    /// # 错误
    /// 实体更新校验失败时返回 `LogicError`。
    fn apply_dispatch_outcome(
        &self,
        order: &mut SupplierFulfillmentOrder,
        action: &mut SupplierOrderAction,
        message: &mut InboxMessage,
        outcome: DispatchOutcome,
    ) -> Result<Option<IntegrationErrorTask>> {
        match outcome {
            DispatchOutcome::Succeeded {
                external_request_id,
                external_order_no,
            } => {
                if action.action_type == SupplierOrderActionType::Place {
                    if let Some(order_no) = &external_order_no {
                        order.update(SupplierFulfillmentOrderUpdate {
                            external_order_no: Some(order_no.clone()),
                        })?;
                    }
                    order.advance_fulfillment(FulfillmentStatus::Accepted)?;
                }
                action.update(SupplierOrderActionUpdate {
                    status: Some(SupplierOrderActionStatus::Succeeded),
                    external_request_id: Some(external_request_id),
                    response_summary: Some("供应商已接单（模拟网关）".to_string()),
                    ..Default::default()
                })?;
                message.update(InboxMessageUpdate {
                    status: Some(InboxMessageStatus::Processed),
                    processed_at: Some(Instant::now()),
                })?;
                Ok(None)
            }
            DispatchOutcome::Rejected { summary } => {
                action.update(SupplierOrderActionUpdate {
                    status: Some(SupplierOrderActionStatus::Failed),
                    response_summary: Some(summary),
                    ..Default::default()
                })?;
                if action.action_type == SupplierOrderActionType::Place {
                    order.advance_fulfillment(FulfillmentStatus::Rejected)?;
                }
                message.update(InboxMessageUpdate {
                    status: Some(InboxMessageStatus::Processed),
                    processed_at: Some(Instant::now()),
                })?;
                Ok(None)
            }
            DispatchOutcome::ResultUnknown { summary } => {
                action.update(SupplierOrderActionUpdate {
                    status: Some(SupplierOrderActionStatus::ResultUnknown),
                    response_summary: Some(summary),
                    ..Default::default()
                })?;
                if action.action_type == SupplierOrderActionType::Place {
                    order.advance_fulfillment(FulfillmentStatus::ResultUnknown)?;
                }
                self.build_error_task(message, order, ErrorClass::ResultUnknown)
            }
            DispatchOutcome::Failed { error_class, summary } => {
                if error_class.can_auto_retry() {
                    action.record_attempt(Some(Instant::now()));
                } else {
                    action.update(SupplierOrderActionUpdate {
                        status: Some(SupplierOrderActionStatus::Failed),
                        response_summary: Some(summary),
                        ..Default::default()
                    })?;
                    if action.action_type == SupplierOrderActionType::Place {
                        order.advance_fulfillment(FulfillmentStatus::Exception)?;
                    }
                }
                self.build_error_task(message, order, error_class)
            }
        }
    }

    /// 构建失败路径错误任务并把消息置为失败（§6.21 错误分类）。
    ///
    /// # 参数
    /// * `message` - `inbox_message` 信封（置为失败）
    /// * `order` - 供应商子订单（业务对象引用）
    /// * `error_class` - 错误分类
    ///
    /// # 返回
    /// 返回待落库的错误任务。
    ///
    /// # 错误
    /// 实体构造校验失败时返回 `LogicError`。
    fn build_error_task(
        &self,
        message: &mut InboxMessage,
        order: &SupplierFulfillmentOrder,
        error_class: ErrorClass,
    ) -> Result<Option<IntegrationErrorTask>> {
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Failed),
            ..Default::default()
        })?;
        let task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new(next_id()),
            IntegrationErrorTaskData {
                message_id: Some(InboxMessageId::new(message.base.id.as_str())),
                business_object_id: Some(order.base.id.clone()),
                error_class,
                owner_role: None,
                owner_user_id: None,
            },
        )?;
        Ok(Some(task))
    }

    /// 在同一事务写回派发结果（订单 + 动作 + 消息/错误任务）。
    ///
    /// # 参数
    /// * `order` - 供应商子订单（就地更新并回读版本）
    /// * `action` - 供应商动作（就地更新并回读版本）
    /// * `message` - `inbox_message` 信封（就地更新并回读版本）
    /// * `task` - 失败路径错误任务；`None` 时消息按已处理写回
    ///
    /// # 错误
    /// 乐观锁冲突/唯一键冲突透出 `ConflictError`，提交结果未知透出 `OutcomeUnknown`。
    async fn write_dispatch_result(
        &self,
        order: &mut SupplierFulfillmentOrder,
        action: &mut SupplierOrderAction,
        message: &mut InboxMessage,
        task: Option<&IntegrationErrorTask>,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let mut order_for_tx = order.clone();
        let mut action_for_tx = action.clone();
        let mut message_for_tx = message.clone();
        let task_for_tx = task.cloned();
        let (order_out, action_out, message_out) = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_fulfillment_orders()
                        .update(&mut order_for_tx, session)
                        .await?;
                    db.supplier_order_actions()
                        .update(&mut action_for_tx, session)
                        .await?;
                    if let Some(task) = task_for_tx.as_ref() {
                        db.integration_ops()
                            .create_error_task_with_message_failure(task, &mut message_for_tx, session)
                            .await?;
                    } else {
                        db.inbox_messages().update(&mut message_for_tx, session).await?;
                    }
                    Ok::<(SupplierFulfillmentOrder, SupplierOrderAction, InboxMessage), crate::errors::Error>(
                        (order_for_tx, action_for_tx, message_for_tx),
                    )
                })
            })
            .await?;
        *order = order_out;
        *action = action_out;
        *message = message_out;
        Ok(())
    }

    /// 按子订单加载全部退款事实视图（含分配行）。
    ///
    /// # 参数
    /// * `order_id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回退款事实视图集合。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    async fn refund_views_for_order(
        &self,
        order_id: &SupplierFulfillmentOrderId,
    ) -> Result<Vec<SupplierRefundFactView>> {
        let facts = self
            .db
            .supplier_refund_facts()
            .find_refund_facts_by_order_ids(std::slice::from_ref(order_id), &mut NoTransaction)
            .await?;
        let fact_ids: Vec<SupplierRefundFactId> = facts
            .iter()
            .map(|fact| SupplierRefundFactId::new(fact.base.id.as_str()))
            .collect();
        let allocations = self
            .db
            .supplier_refund_allocations()
            .find_allocations_by_fact_ids(&fact_ids, &mut NoTransaction)
            .await?;
        Ok(facts
            .into_iter()
            .map(|fact| {
                let fact_allocations: Vec<SupplierRefundAllocation> = allocations
                    .iter()
                    .filter(|allocation| allocation.supplier_refund_fact_id.as_ref() == fact.base.id)
                    .cloned()
                    .collect();
                refund_fact_view(&fact, &fact_allocations)
            })
            .collect())
    }

    /// 按 ID 加载未删除供应商子订单。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回订单实体。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    async fn load_order(&self, id: &str) -> Result<SupplierFulfillmentOrder> {
        self.db
            .supplier_fulfillment_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))
    }

    /// 加载该订单最近一次 `PLACE` 动作。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回 `PLACE` 动作实体。
    ///
    /// # 错误
    /// * `NotFound` - 不存在 `PLACE` 动作
    async fn latest_place_action(&self, id: &str) -> Result<SupplierOrderAction> {
        let mut actions = self
            .db
            .supplier_order_actions()
            .find_many_sorted(
                doc! {
                    "supplier_fulfillment_order_id": id,
                    "action_type": SupplierOrderActionType::Place.as_str(),
                },
                doc! { "created_at": -1 },
                &mut NoTransaction,
            )
            .await?;
        actions
            .pop()
            .ok_or_else(|| Error::NotFound("该订单不存在下单动作".to_string()))
    }
}

/// 校验连接能力声明包含指定能力且为启用态（D25 跨域读取判定）。
///
/// # 参数
/// * `capabilities` - 连接能力集合
/// * `needed` - 所需能力代码
///
/// # 错误
/// 能力缺失或未启用时返回 `BusinessLogicError`。
fn ensure_capability(
    capabilities: &[SupplierApiCapability],
    needed: SupplierApiCapabilityCode,
) -> Result<()> {
    let supported = capabilities
        .iter()
        .any(|capability| capability.capability_code == needed && capability.is_active());
    if !supported {
        return Err(Error::BusinessLogicError(format!(
            "供应商连接缺少能力: {}",
            needed.as_str()
        )));
    }
    Ok(())
}

/// 构建动作 `inbox_message` 信封（P3 §7：事务内落消息，事务外派发）。
///
/// 来源身份取「supplier-api:{连接 ID}」，消息/事实键取动作幂等键，
/// 保证同一动作只产生一条正式记录。
///
/// # 参数
/// * `action` - 供应商动作
/// * `connection` - 供应商连接
/// * `status` - 初始消息状态（已接收）
///
/// # 返回
/// 返回消息实体。
///
/// # 错误
/// 实体构造校验失败时返回 `LogicError`。
fn build_action_message(
    action: &SupplierOrderAction,
    connection: &SupplierApiConnection,
    status: InboxMessageStatus,
) -> Result<InboxMessage> {
    Ok(InboxMessage::new(
        InboxMessageId::new(next_id()),
        InboxMessageData {
            source_system_id: supplier_source_system_id(connection),
            source_event_id: action.idempotency_key.clone(),
            message_type: MessageType::SupplierCallback,
            business_fact_key: action.idempotency_key.clone(),
            payload_schema_version: "1.0".to_string(),
            payload_reference: Some(format!("supplier-order-action:{}", action.base.id)),
            status,
            source_sent_at: None,
            received_at: Instant::now(),
            processed_at: None,
        },
    )?)
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
/// 实体构造校验失败时返回 `LogicError`。
fn build_refund_message(
    order: &SupplierFulfillmentOrder,
    req: &RecordRefundResultRequest,
    connection_id: &entities::ids::SupplierApiConnectionId,
    status: InboxMessageStatus,
) -> Result<InboxMessage> {
    let event_key = format!(
        "refund:{}:{}",
        req.external_refund_no, req.external_refund_version
    );
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

/// 构造供应商来源系统 ID（连接派生，`supplier-api:{连接 ID}`）。
///
/// # 参数
/// * `connection` - 供应商连接
///
/// # 返回
/// 返回来源系统 ID。
fn supplier_source_system_id(connection: &SupplierApiConnection) -> entities::ids::SourceSystemId {
    entities::ids::SourceSystemId::new(format!("supplier-api:{}", connection.base.id))
}

/// 返回派发结果的简短标签（结构化日志用）。
///
/// # 参数
/// * `outcome` - 派发结果
///
/// # 返回
/// 返回标签字符串。
fn outcome_label(outcome: &DispatchOutcome) -> &'static str {
    match outcome {
        DispatchOutcome::Succeeded { .. } => "succeeded",
        DispatchOutcome::Rejected { .. } => "rejected",
        DispatchOutcome::ResultUnknown { .. } => "result_unknown",
        DispatchOutcome::Failed { .. } => "failed",
    }
}

/// 从履约明细实体构造响应视图。
///
/// # 参数
/// * `item` - 履约明细实体
///
/// # 返回
/// 返回响应视图。
fn item_view(item: SupplierFulfillmentItem) -> crate::supplier_fulfillment::dto::SupplierFulfillmentItemView {
    crate::supplier_fulfillment::dto::SupplierFulfillmentItemView {
        id: item.base.id,
        supplier_fulfillment_order_id: item.supplier_fulfillment_order_id.to_string(),
        mall_order_item_id: item.mall_order_item_id.to_string(),
        supplier_offering_revision_id: item.supplier_offering_revision_id.to_string(),
        supplier_sku_code_snapshot: item.supplier_sku_code_snapshot,
        supplier_product_code_snapshot: item.supplier_product_code_snapshot,
        quantity: item.quantity,
        unit_cost_snapshot_gross: item.unit_cost_snapshot_gross,
        cost_snapshot_total_gross: item.cost_snapshot_total_gross,
        input_tax_rate: item.input_tax_rate,
    }
}

/// 从动作行实体构造响应视图。
///
/// # 参数
/// * `line` - 动作行实体
///
/// # 返回
/// 返回响应视图。
fn action_line_view(
    line: SupplierOrderActionLine,
) -> crate::supplier_fulfillment::dto::SupplierOrderActionLineView {
    crate::supplier_fulfillment::dto::SupplierOrderActionLineView {
        id: line.base.id,
        line_no: line.line_no,
        after_sales_request_line_id: line.after_sales_request_line_id.to_string(),
        supplier_fulfillment_item_id: line.supplier_fulfillment_item_id.to_string(),
        quantity: line.quantity,
        amount: line.amount,
    }
}

/// 从退款事实头与分配行构造响应视图。
///
/// # 参数
/// * `fact` - 退款事实头
/// * `allocations` - 分配行集合
///
/// # 返回
/// 返回响应视图。
fn refund_fact_view(
    fact: &SupplierRefundFact,
    allocations: &[SupplierRefundAllocation],
) -> crate::supplier_fulfillment::dto::SupplierRefundFactView {
    crate::supplier_fulfillment::dto::SupplierRefundFactView {
        id: fact.base.id.clone(),
        supplier_fulfillment_order_id: fact.supplier_fulfillment_order_id.to_string(),
        external_refund_no: fact.external_refund_no.clone(),
        external_refund_version: fact.external_refund_version.clone(),
        refund_amount: fact.refund_amount,
        refunded_at: fact.refunded_at.unix_secs(),
        source_event_id: fact.source_event_id.clone(),
        allocations: allocations.iter().map(refund_allocation_view).collect(),
        created_at: fact.base.created_at,
    }
}

/// 从退款分配行实体构造响应视图。
///
/// # 参数
/// * `allocation` - 退款分配行实体
///
/// # 返回
/// 返回响应视图。
fn refund_allocation_view(
    allocation: &SupplierRefundAllocation,
) -> crate::supplier_fulfillment::dto::SupplierRefundAllocationView {
    crate::supplier_fulfillment::dto::SupplierRefundAllocationView {
        id: allocation.base.id.clone(),
        allocation_no: allocation.allocation_no,
        supplier_fulfillment_item_id: allocation.supplier_fulfillment_item_id.to_string(),
        refund_quantity: allocation.refund_quantity,
        gross_amount: allocation.gross_amount,
        net_amount: allocation.net_amount,
        tax_amount: allocation.tax_amount,
        payable_reduction_amount: allocation.payable_reduction_amount,
        cash_refund_amount: allocation.cash_refund_amount,
        allocation_action: allocation.allocation_action,
    }
}
