use std::collections::HashMap;

use database::{AccessControlExt, IntegrationOpsExt, SupplierApiExt, SupplierFulfillmentExt, WorkItemExt};
use entities::integration_ops::{
    ErrorClass, InboxMessage, InboxMessageData, InboxMessageStatus, InboxMessageUpdate, IntegrationErrorTask,
    IntegrationErrorTaskData, IntegrationErrorTaskId, MessageType,
};
use entities::supplier_api::{SupplierApiCapability, SupplierApiCapabilityCode, SupplierApiConnection};
use entities::supplier_fulfillment::{
    FulfillmentStatus, SupplierFulfillmentItem, SupplierFulfillmentItemData, SupplierFulfillmentItemId,
    SupplierFulfillmentOrder, SupplierFulfillmentOrderData, SupplierFulfillmentOrderId,
    SupplierFulfillmentOrderUpdate, SupplierOrderAction, SupplierOrderActionData, SupplierOrderActionId,
    SupplierOrderActionStatus, SupplierOrderActionType, SupplierOrderActionUpdate,
};
use entities::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};
use erp_core::common::time::Instant;
use erp_core::ids::{InboxMessageId, WorkItemId};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::dto::{PlaceFulfillmentOrderRequest, SupplierFulfillmentOrderView};
use super::gateway::DispatchOutcome;
use super::{SupplierFulfillmentService, W26_BUSINESS_OBJECT_TYPE};
use crate::audit::AuditActorLogs;
use crate::errors::{Error, Result};
use application_core::AuditActor;

const W26_OWNER_ROLE: &str = "role-procurement";
const W26_OWNER_ORGANIZATION: &str = "company";

impl SupplierFulfillmentService {
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
        let mut revision_ids = req
            .items
            .iter()
            .map(|item| item.supplier_offering_revision_id.clone())
            .collect::<Vec<_>>();
        revision_ids.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
        revision_ids.dedup_by(|left, right| left.as_ref() == right.as_ref());
        let by_revision = self
            .db
            .supplier_fulfillment()
            .load_offerings_by_revision_ids(&revision_ids, &mut NoTransaction)
            .await?;
        if by_revision.len() != revision_ids.len() {
            return Err(Error::NotFound("供应商供给修订或供给不存在".to_string()));
        }
        for offering in by_revision.values() {
            if !offering.belongs_to_ordering_source(&req.supplier_id, &req.connection_id) {
                return Err(Error::BusinessLogicError(
                    "供给不属于下单供应商或供应商连接不匹配".to_string(),
                ));
            }
        }
        Ok((connection, by_revision))
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
            SupplierFulfillmentOrderData::submitting(
                req.fulfillment_order_no.clone(),
                req.supplier_id.clone(),
                req.connection_id.clone(),
                req.split_no,
                Instant::now(),
                req.address_snapshot_encrypted.clone(),
                req.address_snapshot_fingerprint.clone(),
            ),
        )?;
        let items = self.build_place_items(&order_id, req, offerings)?;
        let action = SupplierOrderAction::new(
            SupplierOrderActionId::new(next_id()),
            SupplierOrderActionData::place(order_id, req.fulfillment_order_no.clone(), items.len()),
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
                let data = SupplierFulfillmentItemData::from_unit_cost(
                    order_id.clone(),
                    item.supplier_offering_revision_id.clone(),
                    offering.supplier_sku_code.clone(),
                    offering.supplier_product_code.clone(),
                    item.quantity,
                    item.unit_cost_snapshot_gross,
                    item.input_tax_rate,
                )?;
                SupplierFulfillmentItem::new(SupplierFulfillmentItemId::new(next_id()), data)
                    .map_err(Error::from)
            })
            .collect()
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
        let task = self.apply_dispatch_outcome(order, action, message, outcome)?;
        self.write_dispatch_result(order, action, message, task.as_ref(), actor)
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
                        let work_item_type = if task.error_class == ErrorClass::ResultUnknown {
                            WorkItemType::IntegrationResultUnknown
                        } else {
                            WorkItemType::BusinessException
                        };
                        let existing = db
                            .work_items()
                            .list_active_by_object(W26_BUSINESS_OBJECT_TYPE, &order_for_tx.base.id, session)
                            .await?;
                        if let Some(mut work_item) = existing
                            .into_iter()
                            .find(|item| item.work_item_type == work_item_type)
                        {
                            let current_subject_version = order_for_tx.base.version.to_string();
                            if work_item.subject_version != current_subject_version {
                                work_item.subject_version = current_subject_version;
                                db.work_items().update(&mut work_item, session).await?;
                                let audit = task_audit_actor.clone().resource_log(
                                    "supplier_fulfillment.work_item.refresh_subject",
                                    "work_item",
                                    work_item.base.id.clone(),
                                )?;
                                db.audit_logs().create(&audit, session).await?;
                            }
                        } else {
                            let work_item = WorkItem::new(
                                work_item_id,
                                WorkItemData {
                                    work_item_type,
                                    business_object_type: W26_BUSINESS_OBJECT_TYPE.to_string(),
                                    business_object_id: order_for_tx.base.id.clone(),
                                    subject_version: order_for_tx.base.version.to_string(),
                                    owner_role: W26_OWNER_ROLE.to_string(),
                                    owner_organization_id: W26_OWNER_ORGANIZATION.to_string(),
                                    owner_user_id: task_audit_actor.id().to_string(),
                                    assignment_source: AssignmentSource::SystemRule,
                                    priority: WorkItemPriority::High,
                                    due_at: None,
                                    reason_code: Some(match work_item_type {
                                        WorkItemType::IntegrationResultUnknown => {
                                            "SUPPLIER_RESULT_UNKNOWN".to_string()
                                        }
                                        WorkItemType::BusinessException => {
                                            "SUPPLIER_BUSINESS_EXCEPTION".to_string()
                                        }
                                        _ => unreachable!(
                                            "W26 producer only creates registered exception tasks"
                                        ),
                                    }),
                                    impact_summary: Some(format!(
                                        "供应商订单 {} 需要核实原动作结果",
                                        order_for_tx.fulfillment_order_no
                                    )),
                                },
                            )?;
                            db.work_items().create(&work_item, session).await?;
                            let audit = task_audit_actor.clone().resource_log(
                                "supplier_fulfillment.work_item.create",
                                "work_item",
                                work_item.base.id.clone(),
                            )?;
                            db.audit_logs().create(&audit, session).await?;
                        }
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
}

/// 校验连接能力声明包含指定能力且为启用态（D25 跨域读取判定）。
///
/// # 参数
/// * `capabilities` - 连接能力集合
/// * `needed` - 所需能力代码
///
/// # 错误
/// 能力缺失或未启用时返回 `BusinessLogicError`。
pub(super) fn ensure_capability(
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
