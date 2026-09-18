use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_supply::dto::supplier_fulfillment::{
    SupplierOrderActionBlockerView, SupplierOrderInvestigationAction, SupplierOrderInvestigationEvidenceView,
    SupplierOrderInvestigationOutcome, SupplierOrderInvestigationResultStatus,
    SupplierOrderObjectInvestigationCommand, SupplierOrderTaskInvestigationCommand,
};
use erp_supply::entity::supplier_api::SupplierApiCapabilityCode;
use erp_supply::entity::supplier_fulfillment::{SupplierFulfillmentOrder, SupplierOrderAction};
use erp_supply::repository::prelude::*;
use erp_supply::repository::{SupplierApiExt, SupplierFulfillmentExt};
use erp_supply::service::supplier_fulfillment::place::ensure_capability;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use erp_workflow::repository::prelude::*;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::receipt::{
    InvestigationReceipt, investigation_receipt_message, parse_investigation_receipt, parse_positive_version,
    serialized_fingerprint, stable_digest, stable_evidence_id, stable_internal_idempotency_key,
};
use super::{SupplierFulfillmentProcess, W26_BUSINESS_OBJECT_TYPE};
use crate::adapters::workflow::work_item_service;
use crate::{Error, Result};

const INVESTIGATION_AUDIT_PREFIX: &str = "w26-investigation-";

#[derive(Debug, Clone)]
struct InvestigationCommandContext {
    order_id: String,
    expected_order_version: u64,
    action: SupplierOrderInvestigationAction,
    operation_id: String,
    target_action_id: String,
    task: Option<InvestigationTaskContext>,
}

#[derive(Debug, Clone)]
struct InvestigationTaskContext {
    work_item_id: String,
    expected_task_version: u64,
    expected_subject_version: String,
}

impl SupplierFulfillmentProcess {
    /// 从普通订单入口查询原结果或执行已证明安全的重放。
    ///
    /// 命令严格校验订单版本和原供应商动作；`REPLAY` 还必须存在最新的
    /// `VERIFIED_NO_RESULT` 查询证据，并始终沿原供应商动作幂等键派发。调查证据
    /// 与审计收据在同一事务提交，重复命令返回原结果。
    ///
    /// # 错误
    /// 订单/动作不存在、版本变化、查询能力不足、重放证据不足或幂等键复用不同
    /// 命令时失败关闭。
    pub async fn investigate_order(
        &self,
        command: SupplierOrderObjectInvestigationCommand,
        actor: &AuditActor,
    ) -> Result<SupplierOrderInvestigationResultView> {
        command.validate()?;
        self.require_scoped_order(command.order_id.as_ref(), actor, "investigate", &mut NoTransaction)
            .await?;
        let fingerprint = serialized_fingerprint(&command)?;
        let audit_id = investigation_audit_id(
            actor.id(),
            "supplier_fulfillment.investigate",
            command.order_id.as_ref(),
            &command.idempotency_key,
        );
        if let Some(result) =
            self.replay_investigation(&audit_id, &fingerprint, &command.operation_id, None).await?
        {
            return Ok(result);
        }
        let context = InvestigationCommandContext {
            order_id: command.order_id.to_string(),
            expected_order_version: command.expected_lock_version,
            action: command.action,
            operation_id: command.operation_id,
            target_action_id: command.target_supplier_action_id.to_string(),
            task: None,
        };
        self.execute_investigation(context, audit_id, fingerprint, actor).await
    }

    /// 从 W26 正式任务入口查询原结果或执行已证明安全的重放。
    ///
    /// 外部调用前和证据提交事务内均重验任务版本、主体版本、订单版本、当前个人
    /// 责任与角色/组织资格。证据、任务处理记录和审计收据同事务提交，任务始终
    /// 保持 `OPEN`。
    ///
    /// # 错误
    /// 正式任务未注册到当前订单、处理权变化、任一版本变化、调查证据不足或幂等
    /// 键复用不同命令时失败关闭；不得降级到普通对象动作。
    pub async fn investigate_order_task(
        &self,
        command: SupplierOrderTaskInvestigationCommand,
        actor: &AuditActor,
    ) -> Result<SupplierOrderInvestigationResultView> {
        command.validate()?;
        self.require_scoped_order(command.action.order_id.as_ref(), actor, "investigate", &mut NoTransaction)
            .await?;
        let expected_task_version = parse_positive_version(&command.expected_task_version, "任务版本")?;
        let fingerprint = serialized_fingerprint(&command)?;
        let audit_id = investigation_audit_id(
            actor.id(),
            "supplier_fulfillment.task_investigate",
            command.work_item_id.as_ref(),
            &command.idempotency_key,
        );
        let task_context = InvestigationTaskContext {
            work_item_id: command.work_item_id.to_string(),
            expected_task_version,
            expected_subject_version: command.expected_subject_version,
        };
        if let Some(result) = self
            .replay_investigation(&audit_id, &fingerprint, &command.action.operation_id, Some(&task_context))
            .await?
        {
            return Ok(result);
        }
        let context = InvestigationCommandContext {
            order_id: command.action.order_id.to_string(),
            expected_order_version: command.action.expected_order_lock_version,
            action: command.action.action_type,
            operation_id: command.action.operation_id,
            target_action_id: command.action.target_supplier_action_id.to_string(),
            task: Some(task_context),
        };
        self.execute_investigation(context, audit_id, fingerprint, actor).await
    }

    async fn execute_investigation(
        &self,
        context: InvestigationCommandContext,
        audit_id: String,
        fingerprint: String,
        actor: &AuditActor,
    ) -> Result<SupplierOrderInvestigationResultView> {
        let evidence_id = stable_evidence_id("w26e", &audit_id);
        let evidence_idempotency_key = stable_internal_idempotency_key("w26e", &audit_id);
        let existing_prepared = self
            .ensure_investigation_intent(&context, &evidence_id, &evidence_idempotency_key, actor)
            .await?;
        let prepared = super::execution::reuse_or_prepare(existing_prepared, || async {
            let prepared = bounded_prepared_investigation(self.prepare_investigation(&context, actor).await?);
            self.persist_prepared_investigation(&context, &evidence_id, &evidence_idempotency_key, prepared)
                .await
        })
        .await?;
        let context_for_tx = context.clone();
        let prepared_for_tx = prepared.clone();
        let actor_id = actor.id().to_string();
        let actor_for_tx = actor.clone();
        let rbac_for_tx = crate::adapters::identity::shared_rbac_service(self.db.clone());
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let evidence_id_for_tx = evidence_id.clone();
        let evidence_idempotency_key_for_tx = evidence_idempotency_key.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut order = db
                        .supplier_fulfillment_orders()
                        .find_by_id(&context_for_tx.order_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))?;
                    order.ensure_version(context_for_tx.expected_order_version).map_err(|_| {
                        Error::ConflictError("供应商履约订单版本已变化，请刷新后重试".to_string())
                    })?;
                    let mut target_action = db
                        .supplier_order_actions()
                        .find_by_id(&context_for_tx.target_action_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("被调查的供应商原动作不存在".to_string()))?;
                    target_action
                        .ensure_original_for_order(&order.base.id)
                        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
                    let investigated_order_version = order.base.version;
                    if context_for_tx.task.is_none() {
                        ensure_no_active_w26_task(&db, &order.base.id, session).await?;
                    }
                    if context_for_tx.action == SupplierOrderInvestigationAction::Replay {
                        ensure_replay_safe(&db, &order, &target_action, session).await?;
                    }

                    let original_order = order.clone();
                    let original_target = target_action.clone();
                    let finding = apply_prepared_investigation(
                        &context_for_tx.subject(),
                        &prepared_for_tx,
                        &mut order,
                        &mut target_action,
                    )?;
                    if order != original_order {
                        persist_order(&db, &mut order, session).await?;
                    }
                    if target_action != original_target {
                        persist_action(&db, &mut target_action, session).await?;
                    }

                    let evidence_record = investigation_evidence_record(
                        &context_for_tx.subject(),
                        target_action.base.id.clone(),
                        &finding,
                    );
                    let response_summary = serde_json::to_string(&evidence_record)
                        .map_err(|error| Error::Internal(format!("调查证据序列化失败: {error}")))?;
                    let mut evidence = db
                        .supplier_order_actions()
                        .find_by_id(&evidence_id_for_tx, session)
                        .await?
                        .ok_or_else(|| Error::Internal("供应商调查意图记录不存在".to_string()))?;
                    validate_investigation_intent(
                        &evidence,
                        &context_for_tx.subject(),
                        &evidence_idempotency_key_for_tx,
                    )?;
                    let durable = parse_prepared_investigation(&evidence, &context_for_tx.subject())?;
                    if durable != prepared_for_tx {
                        return Err(Error::ConflictError(
                            "供应商调查结果已由同一命令的另一执行确定".to_string(),
                        ));
                    }
                    finish_evidence(&mut evidence, finding.outcome, response_summary)?;
                    persist_action(&db, &mut evidence, session).await?;

                    let task_version = if let Some(task_context) = &context_for_tx.task {
                        let mut work_item = db
                            .work_items()
                            .find_by_id(&task_context.work_item_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
                        validate_w26_task(
                            &work_item,
                            &order.base.id,
                            task_context.expected_task_version,
                            &task_context.expected_subject_version,
                            &actor_id,
                        )?;
                        ensure_task_subject_matches_order(
                            &work_item,
                            &task_context.expected_subject_version,
                            investigated_order_version,
                        )?;
                        ensure_task_actor_eligible(&db, &work_item, &actor_id, session).await?;
                        work_item_service(db.clone(), rbac_for_tx.clone())
                            .ensure_domain_decision_access(&actor_for_tx, &work_item, session)
                            .await?;
                        work_item.subject_version = order.base.version.to_string();
                        work_item.record_activity(&actor_id, Instant::now())?;
                        db.work_items().update(&mut work_item, session).await?;
                        Some(work_item.base.version)
                    } else {
                        None
                    };
                    let receipt = InvestigationReceipt {
                        evidence_id: evidence.base.id.clone(),
                        order_version: order.base.version,
                        task_version,
                    };
                    let audit = actor_for_tx.resource_log_with_id(
                        audit_id_for_tx,
                        match context_for_tx.task {
                            Some(_) => "supplier_fulfillment.task_investigate",
                            None => "supplier_fulfillment.investigate",
                        },
                        W26_BUSINESS_OBJECT_TYPE,
                        order.base.id.clone(),
                        Some(investigation_receipt_message(&fingerprint_for_tx, &receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<
                        (
                            SupplierFulfillmentOrder,
                            SupplierOrderAction,
                            InvestigationEvidenceRecord,
                            Option<u64>,
                        ),
                        crate::Error,
                    >((order, evidence, evidence_record, task_version))
                })
            })
            .await;

        super::execution::recover_final_result(
            transaction_result.map(|(order, evidence, record, task_version)| {
                investigation_result(
                    order,
                    evidence,
                    record,
                    context.task.as_ref().map(|task| task.work_item_id.as_str()),
                    task_version,
                )
            }),
            || {
                self.replay_investigation(
                    &audit_id,
                    &fingerprint,
                    &context.operation_id,
                    context.task.as_ref(),
                )
            },
        )
        .await
    }

    /// 在任何供应商查询或重放之前原子登记稳定调查意图。
    async fn ensure_investigation_intent(
        &self,
        context: &InvestigationCommandContext,
        evidence_id: &str,
        evidence_idempotency_key: &str,
        actor: &AuditActor,
    ) -> Result<Option<PreparedInvestigation>> {
        let context = context.clone();
        let evidence_id = evidence_id.to_string();
        let evidence_idempotency_key = evidence_idempotency_key.to_string();
        let actor = actor.clone();
        let actor_id = actor.id().to_string();
        let rbac = crate::adapters::identity::shared_rbac_service(self.db.clone());
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let order = db
                        .supplier_fulfillment_orders()
                        .find_by_id(&context.order_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商履约订单不存在".to_string()))?;
                    order.ensure_version(context.expected_order_version).map_err(|_| {
                        Error::ConflictError("供应商履约订单版本已变化，请刷新后重试".to_string())
                    })?;
                    let target_action = db
                        .supplier_order_actions()
                        .find_by_id(&context.target_action_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("被调查的供应商原动作不存在".to_string()))?;
                    target_action
                        .ensure_original_for_order(&order.base.id)
                        .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
                    if let Some(task_context) = &context.task {
                        let work_item = db
                            .work_items()
                            .find_by_id(&task_context.work_item_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
                        validate_w26_task(
                            &work_item,
                            &order.base.id,
                            task_context.expected_task_version,
                            &task_context.expected_subject_version,
                            &actor_id,
                        )?;
                        ensure_task_subject_matches_order(
                            &work_item,
                            &task_context.expected_subject_version,
                            order.base.version,
                        )?;
                        ensure_task_actor_eligible(&db, &work_item, &actor_id, session).await?;
                        work_item_service(db.clone(), rbac.clone())
                            .ensure_domain_decision_access(&actor, &work_item, session)
                            .await?;
                    } else {
                        ensure_no_active_w26_task(&db, &order.base.id, session).await?;
                    }
                    if context.action == SupplierOrderInvestigationAction::Replay {
                        ensure_replay_safe(&db, &order, &target_action, session).await?;
                    }

                    if let Some(existing) =
                        db.supplier_order_actions().find_by_id(&evidence_id, session).await?
                    {
                        validate_investigation_intent(
                            &existing,
                            &context.subject(),
                            &evidence_idempotency_key,
                        )?;
                        return Ok(existing
                            .response_summary
                            .as_deref()
                            .map(|_| parse_prepared_investigation(&existing, &context.subject()))
                            .transpose()?);
                    }

                    let intent = prepare_intent(
                        &context.subject(),
                        evidence_id,
                        evidence_idempotency_key,
                        order.base.id.as_str(),
                    )?;
                    create_action(&db, &intent, session).await?;
                    Ok::<Option<PreparedInvestigation>, Error>(None)
                })
            })
            .await
    }

    /// 将事务外网关结果先持久化，再进入可能因任务或对象 CAS 失败的领域结算事务。
    async fn persist_prepared_investigation(
        &self,
        context: &InvestigationCommandContext,
        evidence_id: &str,
        evidence_idempotency_key: &str,
        prepared: PreparedInvestigation,
    ) -> Result<PreparedInvestigation> {
        let context = context.clone();
        let evidence_id = evidence_id.to_string();
        let evidence_idempotency_key = evidence_idempotency_key.to_string();
        let prepared_for_tx = prepared.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut evidence =
                        db.supplier_order_actions()
                            .find_by_id(&evidence_id, session)
                            .await?
                            .ok_or_else(|| Error::Internal("供应商调查意图记录不存在".to_string()))?;
                    validate_investigation_intent(&evidence, &context.subject(), &evidence_idempotency_key)?;
                    if evidence.response_summary.is_some() {
                        return Ok(parse_prepared_investigation(&evidence, &context.subject())?);
                    }
                    prepare_durable_evidence(&mut evidence, &context.subject(), &prepared_for_tx)?;
                    persist_action(&db, &mut evidence, session).await?;
                    Ok::<PreparedInvestigation, Error>(prepared_for_tx)
                })
            })
            .await
    }

    async fn prepare_investigation(
        &self,
        context: &InvestigationCommandContext,
        actor: &AuditActor,
    ) -> Result<PreparedInvestigation> {
        let order = self.load_order(&context.order_id).await?;
        order
            .ensure_version(context.expected_order_version)
            .map_err(|_| Error::ConflictError("供应商履约订单版本已变化，请刷新后重试".to_string()))?;
        let target_action = self
            .db
            .supplier_order_actions()
            .find_by_id(&context.target_action_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("被调查的供应商原动作不存在".to_string()))?;
        target_action
            .ensure_original_for_order(&order.base.id)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        if let Some(task_context) = &context.task {
            let work_item = self
                .db
                .work_items()
                .find_by_id(&task_context.work_item_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("供应商履约正式任务不存在".to_string()))?;
            validate_w26_task(
                &work_item,
                &order.base.id,
                task_context.expected_task_version,
                &task_context.expected_subject_version,
                actor.id(),
            )?;
            ensure_task_subject_matches_order(
                &work_item,
                &task_context.expected_subject_version,
                order.base.version,
            )?;
            ensure_task_actor_eligible(&self.db, &work_item, actor.id(), &mut NoTransaction).await?;
        } else {
            ensure_no_active_w26_task(&self.db, &order.base.id, &mut NoTransaction).await?;
        }

        if context.action == SupplierOrderInvestigationAction::QueryResult {
            if let Some(resolution) = order.verified_resolution(&target_action).map(Into::into) {
                return Ok(PreparedInvestigation::PersistedTerminal(resolution));
            }
        } else {
            ensure_replay_safe(&self.db, &order, &target_action, &mut NoTransaction).await?;
        }

        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(&order.connection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商连接不存在".to_string()))?;
        if !connection.is_active() {
            return Err(Error::BusinessLogicError("供应商连接未启用".to_string()));
        }
        let capabilities = self
            .db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(&order.connection_id, &mut NoTransaction)
            .await?;
        match context.action {
            SupplierOrderInvestigationAction::QueryResult => {
                ensure_capability(&capabilities, SupplierApiCapabilityCode::Query)?;
                Ok(PreparedInvestigation::Queried(
                    self.gateway.investigate(&target_action, &order, &connection).await,
                ))
            },
            SupplierOrderInvestigationAction::Replay => {
                ensure_capability(&capabilities, capability_for_action(target_action.action_type)?)?;
                Ok(PreparedInvestigation::Replayed(
                    self.gateway.dispatch(&target_action, &order, &connection).await,
                ))
            },
        }
    }

    async fn replay_investigation(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        expected_operation_id: &str,
        task: Option<&InvestigationTaskContext>,
    ) -> Result<Option<SupplierOrderInvestigationResultView>> {
        let Some(audit) = self.db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        if !audit.success
            || audit.resource_type != W26_BUSINESS_OBJECT_TYPE
            || !matches!(
                audit.action.as_str(),
                "supplier_fulfillment.investigate" | "supplier_fulfillment.task_investigate"
            )
        {
            return Err(Error::Internal("W26 调查幂等收据身份非法".to_string()));
        }
        let receipt = parse_investigation_receipt(
            audit.message.as_deref().ok_or_else(|| Error::Internal("W26 调查幂等收据为空".to_string()))?,
            expected_fingerprint,
        )?;
        if task.is_some() != receipt.task_version.is_some() {
            return Err(Error::ConflictError("请求标识已用于不同的调查入口".to_string()));
        }
        let evidence = self
            .db
            .supplier_order_actions()
            .find_by_id(&receipt.evidence_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("W26 调查幂等证据不存在".to_string()))?;
        let record = parse_investigation_evidence(&evidence)?;
        if record.operation_id() != expected_operation_id {
            return Err(Error::ConflictError("请求标识已用于不同的调查命令".to_string()));
        }
        let order = self.load_order(evidence.supplier_fulfillment_order_id.as_ref()).await?;
        if audit.resource_id.as_deref() != Some(order.base.id.as_str()) {
            return Err(Error::Internal("W26 调查幂等收据对象不一致".to_string()));
        }
        Ok(Some(investigation_result(
            order,
            evidence,
            record,
            task.map(|task| task.work_item_id.as_str()),
            receipt.task_version,
        )))
    }
}

fn ensure_version(actual: u64, expected: u64, object: &str) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError(format!("{object}版本已变化，请刷新后重试")))
}

pub(super) fn validate_w26_task(
    item: &WorkItem,
    order_id: &str,
    expected_task_version: u64,
    expected_subject_version: &str,
    actor_id: &str,
) -> Result<()> {
    ensure_version(item.base.version, expected_task_version, "供应商履约任务")?;
    if item.status != WorkItemStatus::Open {
        return Err(Error::ConflictError("供应商履约任务已不是开放状态".to_string()));
    }
    if !matches!(
        item.work_item_type,
        WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException
    ) || item.business_object_type != W26_BUSINESS_OBJECT_TYPE
        || item.business_object_id != order_id
        || false
    {
        return Err(Error::BusinessLogicError("正式任务未注册到当前供应商履约订单".to_string()));
    }
    if item.subject_version != expected_subject_version {
        return Err(Error::ConflictError("任务主体版本已变化，请刷新后重试".to_string()));
    }
    if !item.is_owned_by(actor_id) {
        return Err(Error::Forbidden("当前用户不是该供应商履约任务的当前责任人".to_string()));
    }
    Ok(())
}

pub(super) fn ensure_task_subject_matches_order(
    item: &WorkItem,
    expected_subject_version: &str,
    order_version: u64,
) -> Result<()> {
    let current_order_version = order_version.to_string();
    if item.subject_version == current_order_version && expected_subject_version == current_order_version {
        return Ok(());
    }
    Err(Error::ConflictError("任务关联的订单版本已变化，请刷新后重试".to_string()))
}

async fn ensure_no_active_w26_task(db: &Database, order_id: &str, executor: &mut dyn Executor) -> Result<()> {
    let has_active_task = db
        .work_items()
        .list_active_by_object(W26_BUSINESS_OBJECT_TYPE, order_id, executor)
        .await?
        .into_iter()
        .next()
        .is_some();
    if has_active_task {
        return Err(Error::ConflictError("当前订单存在正式异常任务，必须使用任务调查命令".to_string()));
    }
    Ok(())
}

fn investigation_result(
    order: SupplierFulfillmentOrder,
    evidence: SupplierOrderAction,
    record: InvestigationEvidenceRecord,
    work_item_id: Option<&str>,
    task_version: Option<u64>,
) -> SupplierOrderInvestigationResultView {
    let result_status = match record.outcome() {
        SupplierOrderInvestigationOutcome::VerifiedTerminal
        | SupplierOrderInvestigationOutcome::VerifiedNoResult => {
            SupplierOrderInvestigationResultStatus::Succeeded
        },
        SupplierOrderInvestigationOutcome::ResultUnknown => SupplierOrderInvestigationResultStatus::Unknown,
    };
    let allowed_actions = match record.outcome() {
        SupplierOrderInvestigationOutcome::VerifiedTerminal => {
            vec!["CONFIRM_VERIFIED_TERMINAL_RESULT".to_string()]
        },
        SupplierOrderInvestigationOutcome::VerifiedNoResult => vec!["REPLAY".to_string()],
        SupplierOrderInvestigationOutcome::ResultUnknown => vec!["QUERY_RESULT".to_string()],
    };
    let work_item =
        work_item_id.zip(task_version).map(|(id, task_version)| SupplierOrderInvestigationWorkItemView {
            id: id.to_string(),
            status: WorkItemStatus::Open,
            task_version,
        });
    SupplierOrderInvestigationResultView {
        result_status,
        message: record.summary().to_string(),
        operation_id: record.operation_id().to_string(),
        evidence: SupplierOrderInvestigationEvidenceView {
            evidence_id: evidence.base.id.clone(),
            target_supplier_action_id: record.target_supplier_action_id().to_string(),
            outcome: record.outcome(),
            recorded_at: i64::try_from(evidence.base.created_at).unwrap_or(i64::MAX),
            can_safe_retry: record.outcome() == SupplierOrderInvestigationOutcome::VerifiedNoResult,
            external_order_no: order.external_order_no.clone(),
            summary: record.summary().to_string(),
            verified_supplier_action_result_id: (record.outcome()
                == SupplierOrderInvestigationOutcome::VerifiedTerminal)
                .then(|| evidence.base.id.clone()),
            verified_resolution: record.verified_resolution(),
        },
        order: order.into(),
        work_item,
        allowed_actions,
        action_blockers: Vec::<SupplierOrderActionBlockerView>::new(),
    }
}

fn investigation_audit_id(actor_id: &str, action: &str, object_id: &str, key: &str) -> String {
    format!(
        "{INVESTIGATION_AUDIT_PREFIX}{}",
        stable_digest(&format!("{actor_id}|{action}|{object_id}|{key}"))
    )
}

use erp_read_models::supplier_center::fulfillment_access::ensure_task_actor_eligible;
use erp_supply::service::supplier_fulfillment::investigate::*;

use super::dto::{SupplierOrderInvestigationResultView, SupplierOrderInvestigationWorkItemView};
impl InvestigationCommandContext {
    fn subject(&self) -> InvestigationSubject {
        InvestigationSubject {
            order_id: self.order_id.clone(),
            expected_order_version: self.expected_order_version,
            action: self.action,
            operation_id: self.operation_id.clone(),
            target_action_id: self.target_action_id.clone(),
        }
    }
}
