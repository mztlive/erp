//! 先登记调查意图，再在事务外取得并冻结供应商结果。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::AuthorizationService;
use erp_permissions::supplier_fulfillment;
use erp_supply::FulfillmentOrderAccess;
use erp_supply::dto::supplier_fulfillment::SupplierOrderInvestigationAction;
use erp_supply::entity::supplier_api::{SupplierApiCapabilityCode, SupplierApiConnection};
use erp_supply::entity::supplier_fulfillment::{
    SupplierFulfillmentOrder, SupplierOrderAction, SupplierOrderActionType,
};
use erp_supply::repository::{SupplierApiExt, SupplierFulfillmentExt};
use erp_supply::service::supplier_fulfillment::investigate::{
    PreparedInvestigation, capability_for_action, create_action, ensure_replay_safe,
    parse_prepared_investigation, persist_action, prepare_durable_evidence, prepare_intent,
    validate_investigation_intent,
};
use erp_supply::service::supplier_fulfillment::place::ensure_capability;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::super::{legacy, task_authorization};
use super::{
    InvestigationCommandContext, InvestigationIdentity, InvestigationTaskContext, SupplierFulfillmentProcess,
    ensure_no_active_w26_task, ensure_task_subject_matches_order, validate_w26_task,
};
use crate::workflow::FulfillmentWorkflow;
use crate::{Error, Result};

struct InvestigationIntent {
    context: InvestigationCommandContext,
    identity: InvestigationIdentity,
    actor: AuditActor,
    workflow: Arc<dyn FulfillmentWorkflow>,
    authorization: Arc<AuthorizationService>,
    access: FulfillmentOrderAccess,
}

/// 外呼输入在授权快照中冻结；供应商通信只在事务成功后执行。
enum InvestigationInput {
    Prepared(PreparedInvestigation),
    Gateway(Box<InvestigationGatewayInput>),
}

struct InvestigationGatewayInput {
    action: SupplierOrderInvestigationAction,
    order: SupplierFulfillmentOrder,
    target_action: SupplierOrderAction,
    connection: SupplierApiConnection,
}

impl SupplierFulfillmentProcess {
    /// 在任何供应商查询或重放之前原子登记稳定调查意图。
    ///
    /// # 参数
    /// * `context` - 原命令及正式任务身份。
    /// * `identity` - 稳定命令和证据身份。
    /// * `actor` - 当前操作人。
    /// # 返回
    /// 已冻结的结果；首次登记无结果时返回 `None`。
    /// # 错误
    /// 当前事实、任务授权、版本、重放证据或持久化失败时返回原错误。
    pub(super) async fn ensure_investigation_intent(
        &self,
        context: &InvestigationCommandContext,
        identity: &InvestigationIdentity,
        actor: &AuditActor,
    ) -> Result<Option<PreparedInvestigation>> {
        let intent = InvestigationIntent {
            context: context.clone(),
            identity: identity.clone(),
            actor: actor.clone(),
            workflow: self.identity.workflow.clone(),
            authorization: self.identity.rbac.clone(),
            access: self.data_scope.clone(),
        };
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |executor| Box::pin(async move { intent.persist(&db, executor).await }))
            .await
    }

    /// 冻结事务外网关结果，使后续任务或对象 CAS 失败时可复用原结果。
    ///
    /// # 参数
    /// * `context` - 原调查命令。
    /// * `identity` - 原调查意图的稳定身份。
    /// * `prepared` - 事务外取得的结果。
    /// # 返回
    /// 返回已冻结结果；并发命令已冻结结果时直接复用它。
    /// # 错误
    /// 意图身份、结果解析或持久化失败时返回原错误。
    pub(super) async fn persist_prepared_investigation(
        &self,
        context: &InvestigationCommandContext,
        identity: &InvestigationIdentity,
        prepared: PreparedInvestigation,
    ) -> Result<PreparedInvestigation> {
        let context = context.clone();
        let identity = identity.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut evidence = db
                        .supplier_order_actions()
                        .find_by_id(&identity.evidence_id, executor)
                        .await?
                        .ok_or_else(|| Error::Internal("供应商调查意图记录不存在".to_string()))?;
                    validate_investigation_intent(
                        &evidence,
                        &context.subject(),
                        &identity.evidence_idempotency_key,
                    )?;
                    if evidence.response_summary.is_some() {
                        return Ok(parse_prepared_investigation(&evidence, &context.subject())?);
                    }
                    prepare_durable_evidence(&mut evidence, &context.subject(), &prepared)?;
                    persist_action(&db, &mut evidence, executor).await?;
                    Ok::<PreparedInvestigation, Error>(prepared)
                })
            })
            .await
    }

    /// 在意图提交后重验调用事实，并在事务外查询或重放供应商原动作。
    ///
    /// # 参数
    /// * `context` - 原调查命令及可选正式任务。
    /// * `actor` - 当前操作人。
    /// # 返回
    /// 返回已有终态，或本次网关取得的结果。
    /// # 错误
    /// 事实、任务、版本、重放证据或连接能力不符时停止外呼。
    pub(super) async fn prepare_investigation(
        &self,
        context: &InvestigationCommandContext,
        actor: &AuditActor,
    ) -> Result<PreparedInvestigation> {
        let (process, context, actor) = (self.clone(), context.clone(), actor.clone());
        let input = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.investigation_input(&context, &actor, executor).await })
            })
            .await?;
        match input {
            InvestigationInput::Prepared(result) => Ok(result),
            InvestigationInput::Gateway(input) => self.call_investigation_gateway(*input).await,
        }
    }

    /// 授权、业务版本、任务来源及外呼连接沿同一快照取证。
    async fn investigation_input(
        &self,
        context: &InvestigationCommandContext,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<InvestigationInput> {
        authorize_investigation(
            &self.db,
            &self.identity.rbac,
            &self.data_scope,
            self.identity.workflow.as_ref(),
            context,
            actor,
            executor,
        )
        .await?;
        let (order, target_action) = load_investigation_facts(&self.db, context, executor).await?;
        if context.action == SupplierOrderInvestigationAction::QueryResult {
            if let Some(resolution) = order.verified_resolution(&target_action).map(Into::into) {
                return Ok(InvestigationInput::Prepared(PreparedInvestigation::PersistedTerminal(
                    resolution,
                )));
            }
        } else {
            ensure_replay_safe(&self.db, &order, &target_action, executor).await?;
        }
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&order.connection_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商连接不存在".into()))?;
        Ok(InvestigationInput::Gateway(Box::new(InvestigationGatewayInput {
            action: context.action,
            order,
            target_action,
            connection,
        })))
    }

    async fn call_investigation_gateway(
        &self,
        input: InvestigationGatewayInput,
    ) -> Result<PreparedInvestigation> {
        let InvestigationGatewayInput { action, order, target_action, connection } = input;
        let capabilities = self.domain().capabilities(&connection)?;
        match action {
            SupplierOrderInvestigationAction::QueryResult => {
                ensure_capability(&capabilities, SupplierApiCapabilityCode::Query)?;
                Ok(PreparedInvestigation::Queried(
                    self.gateway.investigate(&target_action, &order, &connection).await,
                ))
            },
            SupplierOrderInvestigationAction::Replay => {
                if target_action.action_type == SupplierOrderActionType::Place {
                    connection.ensure_active()?;
                }
                ensure_capability(&capabilities, capability_for_action(target_action.action_type)?)?;
                Ok(PreparedInvestigation::Replayed(
                    self.gateway.dispatch(&target_action, &order, &connection).await,
                ))
            },
        }
    }
}

impl InvestigationIntent {
    async fn persist(
        self,
        db: &Database,
        executor: &mut dyn Executor,
    ) -> Result<Option<PreparedInvestigation>> {
        self.authorize(db, executor).await?;
        let (order, target_action) = load_investigation_facts(db, &self.context, executor).await?;
        if self.context.action == SupplierOrderInvestigationAction::Replay {
            ensure_replay_safe(db, &order, &target_action, executor).await?;
        }
        if let Some(existing) =
            db.supplier_order_actions().find_by_id(&self.identity.evidence_id, executor).await?
        {
            validate_investigation_intent(
                &existing,
                &self.context.subject(),
                &self.identity.evidence_idempotency_key,
            )?;
            return Ok(existing
                .response_summary
                .as_deref()
                .map(|_| parse_prepared_investigation(&existing, &self.context.subject()))
                .transpose()?);
        }
        let intent = prepare_intent(
            &self.context.subject(),
            self.identity.evidence_id,
            self.identity.evidence_idempotency_key,
            order.base.id.as_str(),
        )?;
        create_action(db, &intent, executor).await?;
        Ok(None)
    }
    /// 在订单事实读取前核对原任务或对象资格，并执行旧入口排除检查。
    async fn authorize(&self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        authorize_investigation(
            db,
            &self.authorization,
            &self.access,
            self.workflow.as_ref(),
            &self.context,
            &self.actor,
            executor,
        )
        .await
        .map(|_| ())
    }
}

/// 统一调查预检、意图和提交阶段的原对象或精确任务授权。
///
/// # 参数
/// db、authorization、access、workflow 为同一应用注入的事实与授权能力；context、actor 和 executor 属于当前调查命令。
/// # 返回
/// 当前完整资格与原入口约束均成立时，返回原快照中已核验的正式任务；对象入口返回 None。
/// # 错误
/// 无活动快照、当前资格或任务来源失效，以及旧入口约束不符时拒绝。
pub(super) async fn authorize_investigation(
    db: &Database,
    authorization: &AuthorizationService,
    access: &FulfillmentOrderAccess,
    workflow: &dyn FulfillmentWorkflow,
    context: &InvestigationCommandContext,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<WorkItem>> {
    let task = if let Some(task) = &context.task {
        task_authorization::require_target(
            authorization,
            actor,
            &task.work_item_id,
            task.expected_task_version,
            executor,
        )
        .await?;
        Some(
            load_investigation_task(
                db,
                task,
                &context.order_id,
                context.expected_order_version,
                actor,
                workflow,
                executor,
            )
            .await?,
        )
    } else {
        access
            .require_order(actor, &supplier_fulfillment::ORDER_INVESTIGATION, &context.order_id, executor)
            .await?;
        ensure_no_active_w26_task(db, &context.order_id, executor).await?;
        None
    };
    legacy::ensure(db, &context.order_id, executor).await?;
    Ok(task)
}

/// 在调用方读取边界内按订单、版本、原动作的次序取得调查事实。
///
/// # 参数
/// * `db` - 当前数据库。
/// * `context` - 原调查命令。
/// * `executor` - 调用方执行器。
/// # 返回
/// 返回已校验版本和原动作关联的订单与动作。
/// # 错误
/// 缺失、版本冲突或原动作关联不符时返回首个错误。
pub(super) async fn load_investigation_facts(
    db: &Database,
    context: &InvestigationCommandContext,
    executor: &mut dyn Executor,
) -> Result<(SupplierFulfillmentOrder, SupplierOrderAction)> {
    let order = db
        .supplier_fulfillment_orders()
        .find_by_id(&context.order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))?;
    order
        .ensure_version(context.expected_order_version)
        .map_err(|_| Error::ConflictError("供应商履约订单版本已变化，请刷新后重试".to_string()))?;
    let target_action = db
        .supplier_order_actions()
        .find_by_id(&context.target_action_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("被调查的供应商原动作不存在".to_string()))?;
    target_action
        .ensure_original_for_order(&order.base.id)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
    Ok((order, target_action))
}

/// 按版本、主体、责任、资格、决策权限的顺序重验调查任务。
///
/// # 参数
/// * `db` - 当前数据库。
/// * `task` - 原命令的任务身份与期望版本。
/// * `order_id` - 当前订单。
/// * `order_version` - 调查前订单版本。
/// * `actor` - 当前操作人。
/// * `workflow` - 原快照中必须验证的任务决策权限。
/// * `executor` - 调用方执行器。
/// # 返回
/// 返回已校验任务。
/// # 错误
/// 任务缺失或任一版本、授权检查失败时立即返回首个错误。
pub(super) async fn load_investigation_task(
    db: &Database,
    task: &InvestigationTaskContext,
    order_id: &str,
    order_version: u64,
    actor: &AuditActor,
    workflow: &dyn FulfillmentWorkflow,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let work_item = db
        .work_items()
        .find_by_id(&task.work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
    validate_w26_task(&work_item, order_id, task.expected_task_version, &task.expected_subject_version)?;
    ensure_task_subject_matches_order(&work_item, &task.expected_subject_version, order_version)?;
    workflow.ensure_domain_decision_access(actor, &work_item, executor).await?;
    Ok(work_item)
}
