use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{InboxMessageId, SourceSystemId, WorkItemId};
use erp_identity::repository::OrganizationRepository;
use erp_integration::entity::integration_ops::{
    ErrorClass, InboxMessage, InboxMessageData, InboxMessageStatus, InboxMessageUpdate, IntegrationErrorTask,
    IntegrationErrorTaskData, IntegrationErrorTaskId, MessageType,
};
use erp_supply::dto::supplier_fulfillment::{PlaceFulfillmentOrderRequest, SupplierFulfillmentOrderView};
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_fulfillment::{SupplierFulfillmentOrder, SupplierOrderAction};
use erp_supply::ports::supplier_gateway::DispatchOutcome;
use erp_supply::service::supplier_fulfillment::SupplierFulfillmentService;
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::SupplierFulfillmentProcess;
use crate::{Error, Result};

mod intent;
use intent::PlaceIntent;

impl SupplierFulfillmentProcess {
    /// 供应商下单以 `fulfillment_order_no` 标识原命令（§6.19）。先在一个事务写入子订单、明细、首个 `PLACE` 动作和 `inbox_message`，
    /// 确认意图已提交后才在事务外派发供应商 API（P3 §7）；结果由 `inbox_message` 和 `integration_error_task` 承接。
    /// 同订单号重复提交返回原订单当前视图，不重新取号或再次下单。
    ///
    /// 请求非法返回 `ValidationError`；连接、商城订单、明细或供给修订缺失返回 `NotFound`；连接未启用或不能下单返回 `BusinessLogicError`；
    /// 并发唯一键冲突返回 `ConflictError`；提交无法确认返回 `OutcomeUnknown`。
    pub async fn submit_place(
        &self,
        req: PlaceFulfillmentOrderRequest,
        actor: &AuditActor,
    ) -> Result<SupplierFulfillmentOrderView> {
        req.validate()?;
        let process = self.clone();
        let actor_for_tx = actor.clone();
        let intent = self.db.client().with_transaction(move |executor| {
            Box::pin(async move { process.prepare_place_intent(&req, &actor_for_tx, executor).await })
        });

        super::execution::after_intent(intent, |intent| async move {
            match intent {
                PlaceIntent::Existing(order) => Ok((*order).into()),
                PlaceIntent::Prepared(mut dispatch) => {
                    tracing::info!(account = %actor.id(), order_id = %dispatch.order.base.id, "下单事务已提交，开始事务外供应商派发");
                    self.settle_dispatch(
                        &mut dispatch.order,
                        &mut dispatch.action,
                        &mut dispatch.message,
                        &dispatch.connection,
                        actor,
                    )
                    .await?;
                    Ok(dispatch.order.into())
                },
            }
        })
        .await
    }

    // 已持久化的供应商动作在事务外经网关派发（P3 §7），按响应就地更新子订单、动作及消息，
    // 再用同一事务保存结果和消息；需人工处理时，由 `integration_error_task` 承接并创建或复用 W26 正式任务。
    // 结果不适用、处理人缺少有效内部组织、错误任务无法构造时返回原错误。
    // 结果写回失败保留 `ConflictError`、`RepositoryError` 或 `OutcomeUnknown`。
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
                super::failure::integration_class(*error_class).can_auto_retry()
            },
            _ => false,
        };
        let applied =
            SupplierFulfillmentService::apply_dispatch_outcome(order, action, outcome, can_auto_retry)?;

        let task = match applied {
            DispatchMessageResult::Processed => {
                message.update(InboxMessageUpdate {
                    status: Some(InboxMessageStatus::Processed),
                    processed_at: Some(Instant::now()),
                })?;
                None
            },
            DispatchMessageResult::Failed(class) => {
                let org = OrganizationRepository::new(&self.db)
                    .state(&mut NoTransaction)
                    .await?
                    .own_org(actor.id(), Instant::now())?
                    .filter(|org| !org.eq_ignore_ascii_case("company"))
                    .map(str::to_string)
                    .ok_or_else(|| Error::ValidationError("处理人缺少有效内部组织".into()))?;
                self.build_error_task(message, order, super::failure::integration_class(class), actor, org)?
            },
        };

        self.write_dispatch_result(order, action, message, task.as_ref(), actor).await
    }

    // 派发失败时把 `inbox_message` 置为失败，并按 §6.21 为供应商子订单生成待入库的错误任务。
    // 任务使用当前处理人的账号和有效内部组织；消息更新或任务构造失败返回 `Logic`。
    fn build_error_task(
        &self,
        message: &mut InboxMessage,
        order: &SupplierFulfillmentOrder,
        error_class: ErrorClass,
        actor: &AuditActor,
        owner_org_unit_id: String,
    ) -> Result<Option<IntegrationErrorTask>> {
        message
            .update(InboxMessageUpdate { status: Some(InboxMessageStatus::Failed), ..Default::default() })?;
        let task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new(next_id()),
            IntegrationErrorTaskData {
                message_id: Some(InboxMessageId::new(message.base.id.as_str())),
                business_object_id: Some(order.base.id.clone()),
                error_class,
                owner_role: None,
                owner_user_id: Some(actor.id().to_string()),
                owner_org_unit_id,
            },
        )?;
        Ok(Some(task))
    }

    // 网关派发结束后，在一个事务内写回子订单、动作、入站消息、错误事实、W26 任务和审计。
    // 无错误任务时按已处理保存消息；确认提交后才用新版本覆盖调用方的订单、动作和消息。
    // 乐观锁或唯一键冲突返回 `ConflictError`，未知提交返回 `OutcomeUnknown`。
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
            .with_transaction(move |executor| {
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
                    .persist(executor)
                    .await?;
                    Ok::<(SupplierFulfillmentOrder, SupplierOrderAction, InboxMessage), Error>((
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

// 构建动作 `inbox_message` 信封（P3 §7：事务内落消息，事务外派发）。
//
// 来源身份取「supplier-api:{连接 ID}」，消息/事实键取动作幂等键，保证同一动作只产生一条正式记录。
//
// `action` 为供应商动作；`connection` 为供应商连接；`status` 为初始消息状态（已接收）。返回消息实体。
//
// 实体构造校验失败时返回 `Logic`。
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

// 构造供应商来源系统 ID（连接派生，`supplier-api:{连接 ID}`）。
//
// `connection` 为供应商连接。返回来源系统 ID。
fn supplier_source_system_id(connection: &SupplierApiConnection) -> SourceSystemId {
    SourceSystemId::new(format!("supplier-api:{}", connection.base.id))
}

// 返回派发结果的简短标签（结构化日志用）。
//
// `outcome` 为派发结果。返回标签字符串。
fn outcome_label(outcome: &DispatchOutcome) -> &'static str {
    match outcome {
        DispatchOutcome::Succeeded { .. } => "succeeded",
        DispatchOutcome::Rejected { .. } => "rejected",
        DispatchOutcome::ResultUnknown { .. } => "result_unknown",
        DispatchOutcome::Failed { .. } => "failed",
    }
}

use erp_supply::service::supplier_fulfillment::place::DispatchMessageResult;
