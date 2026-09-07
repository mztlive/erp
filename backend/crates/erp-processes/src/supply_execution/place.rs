use super::SupplierFulfillmentProcess;
use crate::Result;
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_core::ids::{InboxMessageId, WorkItemId};
use erp_integration::entity::integration_ops::{
    ErrorClass, InboxMessage, InboxMessageData, InboxMessageStatus, InboxMessageUpdate, IntegrationErrorTask,
    IntegrationErrorTaskData, IntegrationErrorTaskId, MessageType,
};
use erp_integration::repository::IntegrationOpsExt;
use erp_supply::dto::supplier_fulfillment::{PlaceFulfillmentOrderRequest, SupplierFulfillmentOrderView};
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_fulfillment::{SupplierFulfillmentOrder, SupplierOrderAction};
use erp_supply::ports::supplier_gateway::DispatchOutcome;
use erp_supply::repository::SupplierFulfillmentExt;
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

impl SupplierFulfillmentProcess {
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
        let (connection, offerings) = self.domain().ensure_placeable(&req).await?;
        let (mut order, items, mut action) = self.domain().build_place_facts(&req, &offerings)?;
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
        super::execution::after_intent(client
            .with_transaction(move |session| {
                Box::pin(async move {
                    persist_place_facts(&db, &order_for_tx, &items_for_tx, &action_for_tx, session).await?;
                    db.inbox_messages().create(&message_for_tx, session).await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::Error>(())
                })
            }), || async {
        tracing::info!(account = %actor.id(), order_id = %order.base.id, "下单事务已提交，开始事务外供应商派发");
        self.settle_dispatch(&mut order, &mut action, &mut message, &connection, actor)
            .await
        }).await?;
        Ok(order.into())
    }

    /// 事务外派发供应商动作并承接结果（P3 §7）。
    ///
    /// 网关调用不在任何事务闭包内；结果经 `inbox_message` +
    /// `integration_error_task` 承接后，在同一事务写回订单/动作/消息，并为人工
    /// 异常创建或复用 W26 正式任务。
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
    pub(super) async fn settle_dispatch(
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
        let can_auto_retry = match &outcome {
            DispatchOutcome::Failed { error_class, .. } => {
                crate::adapters::supplier_failure::integration_class(*error_class).can_auto_retry()
            }
            _ => false,
        };
        let applied =
            erp_supply::service::supplier_fulfillment::SupplierFulfillmentService::apply_dispatch_outcome(
                order,
                action,
                outcome,
                can_auto_retry,
            )?;
        let task = match applied {
            DispatchMessageResult::Processed => {
                message.update(InboxMessageUpdate {
                    status: Some(InboxMessageStatus::Processed),
                    processed_at: Some(Instant::now()),
                })?;
                None
            }
            DispatchMessageResult::Failed(class) => self.build_error_task(
                message,
                order,
                crate::adapters::supplier_failure::integration_class(class),
            )?,
        };
        self.write_dispatch_result(order, action, message, task.as_ref(), actor)
            .await
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

    /// 在同一事务写回派发结果、错误事实、W26 正式任务与审计。
    ///
    /// # 参数
    /// * `order` - 供应商子订单（就地更新并回读版本）
    /// * `action` - 供应商动作（就地更新并回读版本）
    /// * `message` - `inbox_message` 信封（就地更新并回读版本）
    /// * `task` - 失败路径错误任务；`None` 时消息按已处理写回
    /// * `actor` - 派发命令的审计操作人
    ///
    /// # 错误
    /// 乐观锁冲突/唯一键冲突透出 `ConflictError`，提交结果未知透出 `OutcomeUnknown`。
    async fn write_dispatch_result(
        &self,
        order: &mut SupplierFulfillmentOrder,
        action: &mut SupplierOrderAction,
        message: &mut InboxMessage,
        task: Option<&IntegrationErrorTask>,
        actor: &AuditActor,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let mut order_for_tx = order.clone();
        let mut action_for_tx = action.clone();
        let mut message_for_tx = message.clone();
        let task_for_tx = task.cloned();
        let work_item_id = WorkItemId::new(next_id());
        let task_audit_actor = actor.clone();
        let (order_out, action_out, message_out) = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    super::dispatch_writes::MongoWrites {
                        db: &db,
                        order: &mut order_for_tx,
                        action: &mut action_for_tx,
                        message: &mut message_for_tx,
                        task: task_for_tx.as_ref(),
                        work_item_id,
                        actor: &task_audit_actor,
                    }
                    .persist(session)
                    .await?;
                    Ok::<(SupplierFulfillmentOrder, SupplierOrderAction, InboxMessage), crate::Error>((
                        order_for_tx,
                        action_for_tx,
                        message_for_tx,
                    ))
                })
            })
            .await?;
        *order = order_out;
        *action = action_out;
        *message = message_out;
        Ok(())
    }
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
pub(super) fn build_action_message(
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

/// 构造供应商来源系统 ID（连接派生，`supplier-api:{连接 ID}`）。
///
/// # 参数
/// * `connection` - 供应商连接
///
/// # 返回
/// 返回来源系统 ID。
fn supplier_source_system_id(connection: &SupplierApiConnection) -> erp_core::ids::SourceSystemId {
    erp_core::ids::SourceSystemId::new(format!("supplier-api:{}", connection.base.id))
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

use erp_supply::service::supplier_fulfillment::place::{persist_place_facts, DispatchMessageResult};
