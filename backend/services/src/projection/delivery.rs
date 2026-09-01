//! W23 执行投影投递、原结果查询、受控重试与 W29 升级。

use std::collections::HashSet;

use database::repository::{ProjectionDeliveryEscalation, ProjectionDeliveryFailure};
use database::{
    AccessControlExt, IntegrationOpsExt, NoTransaction, ProjectionExt, Transactional, WorkItemExt,
};
use entities::common::time::Instant;
use entities::ids::{
    InboxMessageId, IntegrationErrorTaskId, SalesOrderProjectionDeliveryId, SalesOrderProjectionId,
    SalesOrderProjectionRevisionId,
};
use entities::integration_ops::{
    ErrorClass, InboxMessage, InboxMessageData, InboxMessageStatus, InboxMessageUpdate, IntegrationErrorTask,
    IntegrationErrorTaskData, MessageType,
};
use entities::projection::{
    delivery_guard, ProjectionDeliveryStatus, SalesOrderProjection, SalesOrderProjectionDelivery,
    SalesOrderProjectionRevision, SalesOrderProjectionUpdate,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::integration_ops::{error_owner_role, error_work_item};
use crate::projection::connector::{ClassifiedError, DeliverAck, QueryProjectionResult};
use crate::projection::dto::{
    DeliverProjectionRevisionRequest, ProcessProjectionDeliveriesRequest, ProcessProjectionDeliveriesResult,
    ProjectionBulkCommandRequest, ProjectionBulkCommandResultView, ProjectionBulkItemResultView,
    ProjectionDeliveryAction, ProjectionDeliveryActionResult, ProjectionDeliveryCommand,
    ProjectionDeliveryResultView,
};
use crate::projection::service::ProjectionService;

const RETRY_DELAY_SECONDS: i64 = 60;

struct ProjectionSuccessSettlement<'a> {
    projection: SalesOrderProjection,
    revision: SalesOrderProjectionRevision,
    delivery: SalesOrderProjectionDelivery,
    message: InboxMessage,
    ack: DeliverAck,
    operation_id: String,
    command_action: &'static str,
    actor: &'a AuditActor,
}

impl ProjectionService {
    /// 处理指定投影修订的既有待发送记录。
    ///
    /// 本入口不创建投递；创建投影或修订时已经原子预建固定 `PENDING_SEND`
    /// 记录。请求只负责以 CAS 取得该记录并推进一次真实外部调用。
    pub async fn deliver_revision(
        &self,
        projection_id: &str,
        revision_no: u32,
        req: DeliverProjectionRevisionRequest,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        req.validate()?;
        let (projection, revision, delivery) =
            self.load_revision_delivery(projection_id, revision_no).await?;
        let operation_id = operation_id(actor.id(), "SEND", &delivery.base.id, &req.idempotency_key);
        if let Some(result) = self
            .replay_operation(&operation_id, &delivery.base.id, "SEND", actor)
            .await?
        {
            return Ok(result);
        }
        if let Some(result) = terminal_or_inflight_result(&delivery, operation_id.clone()) {
            return self
                .record_existing_result(
                    result,
                    "sales_order_projection_delivery.send_observed",
                    "SEND",
                    actor,
                )
                .await;
        }
        if !delivery.is_send_ready(Instant::now()) {
            return Err(Error::ConflictError(
                "投递尚未到受控处理时间，请查询当前状态".to_string(),
            ));
        }
        self.process_delivery(projection, revision, delivery, operation_id, "SEND", actor)
            .await
    }

    /// 执行 `QUERY_RESULT / RETRY / ESCALATE` 投递对象强命令。
    ///
    /// 同一请求 ID 的重复提交返回原结果；请求 ID 复用到不同命令时返回冲突。
    pub async fn apply_delivery_command(
        &self,
        path_delivery_id: &str,
        command: ProjectionDeliveryCommand,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        command.validate()?;
        SalesOrderProjectionDelivery::ensure_command_identity(
            &SalesOrderProjectionDeliveryId::new(path_delivery_id.to_string()),
            &SalesOrderProjectionDeliveryId::new(command.delivery_id.clone()),
        )
        .map_err(|err| Error::ValidationError(err.to_string()))?;
        let operation_id = operation_id(
            actor.id(),
            command.action.as_str(),
            path_delivery_id,
            &command.request_id,
        );
        if let Some(result) = self
            .replay_operation(&operation_id, path_delivery_id, command.action.as_str(), actor)
            .await?
        {
            return Ok(result);
        }

        let result = match command.action {
            ProjectionDeliveryAction::QueryResult => {
                self.query_delivery_result(&command, &operation_id, actor).await
            }
            ProjectionDeliveryAction::Retry => self.retry_delivery(&command, &operation_id, actor).await,
            ProjectionDeliveryAction::Escalate => {
                self.escalate_delivery(&command, &operation_id, actor).await
            }
        };
        match result {
            Ok(result) => Ok(result),
            Err(error) => match self
                .replay_operation(&operation_id, path_delivery_id, command.action.as_str(), actor)
                .await?
            {
                Some(result) => Ok(result),
                None => Err(error),
            },
        }
    }

    /// 对显式选中的投影执行一次服务端批量命令。
    ///
    /// 客户端不再逐项读取详情和提交动作。服务端为每个投影解析最新
    /// 修订、固定投递与当前版本，然后调用单项原子状态迁移。批量允许部分成功，
    /// 因此不将已取得的外部商城结果因其他项失败而回滚。
    ///
    /// # 参数
    /// * `req` - 批量动作、显式投影 ID 和幂等请求身份
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回批次汇总和逐项正式结果。
    ///
    /// # 错误
    /// 请求为空、超过 20 条、含空 ID 或重复 ID 时整批拒绝。
    pub async fn apply_bulk_delivery_command(
        &self,
        req: ProjectionBulkCommandRequest,
        actor: &AuditActor,
    ) -> Result<ProjectionBulkCommandResultView> {
        req.validate()?;
        let mut seen = HashSet::with_capacity(req.projection_ids.len());
        for projection_id in &req.projection_ids {
            let normalized = projection_id.trim();
            if normalized.is_empty() {
                return Err(Error::ValidationError("投影ID不能为空".to_string()));
            }
            if !seen.insert(normalized.to_string()) {
                return Err(Error::ValidationError(format!("投影ID重复: {normalized}")));
            }
        }

        let started_at = Instant::now().unix_secs();
        let total = req.projection_ids.len() as u32;
        let mut succeeded = 0;
        let skipped = 0;
        let mut failed = 0;
        let mut still_unknown = 0;
        let mut items = Vec::with_capacity(req.projection_ids.len());

        for projection_id in &req.projection_ids {
            match self
                .bulk_delivery_command(
                    projection_id.trim(),
                    req.action.delivery_action(),
                    &req.request_id,
                )
                .await
            {
                Ok((sales_order_id, command)) => {
                    let delivery_id = command.delivery_id.clone();
                    match self.apply_delivery_command(&delivery_id, command, actor).await {
                        Ok(result) => {
                            let (outcome, reason) = match result.result {
                                ProjectionDeliveryActionResult::StillUnknown => {
                                    still_unknown += 1;
                                    ("STILL_UNKNOWN", "商城仍未返回可验证的最终结果")
                                }
                                ProjectionDeliveryActionResult::Failed => {
                                    failed += 1;
                                    ("FAILED", "商城返回明确失败")
                                }
                                ProjectionDeliveryActionResult::Acked => {
                                    succeeded += 1;
                                    ("SUCCEEDED", "已取得商城权威确认")
                                }
                                ProjectionDeliveryActionResult::RetryScheduled => {
                                    succeeded += 1;
                                    ("SUCCEEDED", "已沿原稳定消息键安排受控重试")
                                }
                                ProjectionDeliveryActionResult::Escalated => {
                                    succeeded += 1;
                                    ("SUCCEEDED", "已升级为人工处理")
                                }
                            };
                            items.push(ProjectionBulkItemResultView {
                                projection_id: projection_id.trim().to_string(),
                                sales_order_no: sales_order_id,
                                delivery_id,
                                outcome: outcome.to_string(),
                                reason: result.next_action.unwrap_or_else(|| reason.to_string()),
                            });
                        }
                        Err(error) => {
                            failed += 1;
                            items.push(ProjectionBulkItemResultView {
                                projection_id: projection_id.trim().to_string(),
                                sales_order_no: sales_order_id,
                                delivery_id,
                                outcome: "FAILED".to_string(),
                                reason: error.to_string(),
                            });
                        }
                    }
                }
                Err(error) => {
                    failed += 1;
                    items.push(ProjectionBulkItemResultView {
                        projection_id: projection_id.trim().to_string(),
                        sales_order_no: projection_id.trim().to_string(),
                        delivery_id: String::new(),
                        outcome: "FAILED".to_string(),
                        reason: error.to_string(),
                    });
                }
            }
        }

        let status = if failed == 0 && still_unknown == 0 {
            "SUCCEEDED"
        } else if succeeded > 0 {
            "PARTIAL"
        } else {
            "FAILED"
        };
        let next_action = if still_unknown > 0 {
            "存在结果未知项：不得标记成功，请再次查询原结果"
        } else if failed > 0 {
            "部分项未执行，请按逐项结果处理"
        } else {
            "批量命令已完成"
        };
        let batch_identity = format!("{}|{}|{}", actor.id(), req.action.as_str(), req.request_id);
        Ok(ProjectionBulkCommandResultView {
            job_id: stable_entity_id("pbulk", &batch_identity),
            action: req.action,
            status: status.to_string(),
            total,
            completed: items.len() as u32,
            succeeded,
            skipped,
            failed,
            still_unknown,
            selection_snapshot_id: stable_entity_id("psnap", &batch_identity),
            items,
            started_at,
            finished_at: Instant::now().unix_secs(),
            next_action: next_action.to_string(),
        })
    }

    /// 从投影稳定身份解析最新修订、固定投递与当前版本命令。
    async fn bulk_delivery_command(
        &self,
        projection_id: &str,
        action: ProjectionDeliveryAction,
        request_id: &str,
    ) -> Result<(String, ProjectionDeliveryCommand)> {
        let projection = self
            .db
            .sales_order_projections()
            .find_by_id(projection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("执行投影不存在".to_string()))?;
        let revision = self
            .db
            .sales_order_projection_revisions()
            .list_revisions_by_projection(
                &SalesOrderProjectionId::new(projection.base.id.clone()),
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::NotFound("执行投影尚无修订".to_string()))?;
        let delivery = self
            .db
            .sales_order_projection_deliveries()
            .find_delivery_by_revision_and_mall(
                &SalesOrderProjectionRevisionId::new(revision.id.clone()),
                &projection.target_mall_id,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("投影最新修订缺少固定投递记录".to_string()))?;
        let item_request_id = stable_entity_id(
            "preq",
            &format!("{request_id}|{}|{}", action.as_str(), projection.base.id),
        );
        Ok((
            projection.sales_order_id.to_string(),
            ProjectionDeliveryCommand {
                projection_id: projection.base.id,
                projection_revision_id: revision.id,
                delivery_id: delivery.base.id,
                action,
                expected_object_version: delivery.base.version,
                request_id: item_request_id,
            },
        ))
    }

    /// 以有界批次处理 `PENDING_SEND` 与到期 `RETRYING` 记录。
    ///
    /// 每条记录仍通过相同 CAS 取得路径；并发 worker 只会有一个成功调用外部连接器。
    pub async fn process_pending_deliveries(
        &self,
        req: ProcessProjectionDeliveriesRequest,
        actor: &AuditActor,
    ) -> Result<ProcessProjectionDeliveriesResult> {
        req.validate()?;
        let now = Instant::now();
        let rows = self
            .db
            .sales_order_projection_deliveries()
            .list_processable_deliveries(now, req.limit.unwrap_or(50), &mut NoTransaction)
            .await?;
        let scanned = rows.len() as u32;
        let mut result = ProcessProjectionDeliveriesResult {
            scanned,
            acked: 0,
            failed: 0,
            still_unknown: 0,
            skipped: 0,
            items: Vec::with_capacity(rows.len()),
        };
        for delivery in rows {
            let operation_id = operation_id("system", "PROCESS", &delivery.base.id, &delivery.message_key);
            match self
                .process_delivery_by_identity(delivery, operation_id, actor)
                .await
            {
                Ok(item) => {
                    match item.result {
                        ProjectionDeliveryActionResult::Acked => result.acked += 1,
                        ProjectionDeliveryActionResult::Failed => result.failed += 1,
                        ProjectionDeliveryActionResult::StillUnknown => result.still_unknown += 1,
                        ProjectionDeliveryActionResult::RetryScheduled
                        | ProjectionDeliveryActionResult::Escalated => result.skipped += 1,
                    }
                    result.items.push(item);
                }
                Err(Error::ConflictError(_)) => result.skipped += 1,
                Err(error) => return Err(error),
            }
        }
        Ok(result)
    }

    async fn process_delivery_by_identity(
        &self,
        delivery: SalesOrderProjectionDelivery,
        operation_id: String,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        let revision = self
            .db
            .sales_order_projection_revisions()
            .find_by_id(delivery.projection_revision_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("投递关联的投影修订不存在".to_string()))?;
        let projection = self
            .db
            .sales_order_projections()
            .find_by_id(revision.projection_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("投递关联的稳定投影不存在".to_string()))?;
        self.process_delivery(projection, revision, delivery, operation_id, "PROCESS", actor)
            .await
    }

    async fn process_delivery(
        &self,
        projection: SalesOrderProjection,
        revision: SalesOrderProjectionRevision,
        delivery: SalesOrderProjectionDelivery,
        operation_id: String,
        command_action: &'static str,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        delivery_guard::ensure_delivery_relation(&projection, &revision, &delivery)
            .map_err(|err| Error::ConflictError(err.to_string()))?;
        let message = self.ensure_message(&delivery).await?;
        let now = Instant::now();
        let claimed = self
            .db
            .sales_order_projection_deliveries()
            .claim_for_send(
                &delivery.base.id,
                delivery.base.version,
                &InboxMessageId::new(message.base.id.clone()),
                now,
                &mut NoTransaction,
            )
            .await?;
        let Some(claimed) = claimed else {
            let current = self.current_delivery(&delivery.base.id).await?;
            return terminal_or_inflight_result(&current, operation_id)
                .ok_or_else(|| Error::ConflictError("投递已由其他处理器取得，请刷新".to_string()));
        };

        match self
            .connector
            .deliver_projection(&revision, &projection.target_mall_id)
            .await
        {
            Ok(ack) => {
                self.settle_success(ProjectionSuccessSettlement {
                    projection,
                    revision,
                    delivery: claimed,
                    message,
                    ack,
                    operation_id,
                    command_action,
                    actor,
                })
                .await
            }
            Err(error) if is_unknown_error(&error) => {
                self.settle_failure(
                    claimed,
                    message,
                    unknown_error(error),
                    operation_id,
                    command_action,
                    actor,
                )
                .await
            }
            Err(error) => {
                self.settle_failure(claimed, message, error, operation_id, command_action, actor)
                    .await
            }
        }
    }

    async fn ensure_message(&self, delivery: &SalesOrderProjectionDelivery) -> Result<InboxMessage> {
        let id = delivery
            .inbox_message_id
            .clone()
            .unwrap_or_else(|| InboxMessageId::new(stable_entity_id("pmsg", &delivery.message_key)));
        let message = InboxMessage::new(
            id,
            InboxMessageData {
                source_system_id: delivery.target_mall_id.clone(),
                source_event_id: delivery.message_key.clone(),
                message_type: MessageType::MallActionRequest,
                business_fact_key: delivery.message_key.clone(),
                payload_schema_version: "projection-delivery-v1".to_string(),
                payload_reference: Some(delivery.projection_revision_id.to_string()),
                status: InboxMessageStatus::Received,
                source_sent_at: None,
                received_at: Instant::now(),
                processed_at: None,
            },
        )?;
        match self
            .db
            .inbox_messages()
            .create(&message, &mut NoTransaction)
            .await
        {
            Ok(()) => Ok(message),
            Err(database::Error::DuplicateKey(_)) => self
                .db
                .inbox_messages()
                .find_by_business_fact_key(
                    MessageType::MallActionRequest,
                    &delivery.message_key,
                    &mut NoTransaction,
                )
                .await?
                .ok_or_else(|| Error::ConflictError("投递消息唯一键冲突但无法读取原消息".to_string())),
            Err(error) => Err(error.into()),
        }
    }

    async fn settle_success(
        &self,
        settlement: ProjectionSuccessSettlement<'_>,
    ) -> Result<ProjectionDeliveryResultView> {
        let ProjectionSuccessSettlement {
            mut projection,
            revision,
            delivery,
            mut message,
            ack,
            operation_id,
            command_action,
            actor,
        } = settlement;
        let at = Instant::now();
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Processed),
            processed_at: Some(at),
        })?;
        let advance = self.should_advance_projection(&projection, &revision).await?;
        if advance {
            projection.update(SalesOrderProjectionUpdate {
                current_acked_revision_id: Some(revision.base.id.clone().into()),
            })?;
        }
        let result = action_result(
            operation_id.clone(),
            delivery.base.id.clone(),
            ProjectionDeliveryActionResult::Acked,
            None,
            None,
            at,
        );
        let audit = operation_audit(
            actor,
            operation_id.clone(),
            "sales_order_projection_delivery.acked",
            &delivery.base.id,
            command_action,
            &result,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let baseline = ack.mall_execution_baseline.clone();
        let delivery_id = delivery.base.id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let updated = db
                        .sales_order_projection_deliveries()
                        .confirm_delivery(&delivery_id, delivery.base.version, &baseline, at, session)
                        .await?;
                    if updated.is_none() {
                        return Err(crate::errors::Error::ConflictError(
                            "投递结果已被其他请求写入".to_string(),
                        ));
                    }
                    if advance {
                        db.sales_order_projections()
                            .update(&mut projection, session)
                            .await?;
                    }
                    db.inbox_messages().update(&mut message, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(result)
    }

    async fn settle_failure(
        &self,
        delivery: SalesOrderProjectionDelivery,
        mut message: InboxMessage,
        error: ClassifiedError,
        operation_id: String,
        command_action: &'static str,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        let at = Instant::now();
        let unknown = error.class == ErrorClass::ResultUnknown;
        let status = if unknown {
            ProjectionDeliveryStatus::ResultUnknown
        } else {
            ProjectionDeliveryStatus::Failed
        };
        message.update(InboxMessageUpdate {
            status: Some(if unknown {
                InboxMessageStatus::Processing
            } else {
                InboxMessageStatus::Failed
            }),
            processed_at: None,
        })?;
        let result = action_result(
            operation_id.clone(),
            delivery.base.id.clone(),
            if unknown {
                ProjectionDeliveryActionResult::StillUnknown
            } else {
                ProjectionDeliveryActionResult::Failed
            },
            None,
            None,
            at,
        );
        let audit = operation_audit(
            actor,
            operation_id.clone(),
            if unknown {
                "sales_order_projection_delivery.result_unknown"
            } else {
                "sales_order_projection_delivery.failed"
            },
            &delivery.base.id,
            command_action,
            &result,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        let code = error.code.clone();
        let summary = error.summary.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let updated = db
                        .sales_order_projection_deliveries()
                        .fail_delivery(
                            &delivery_id,
                            delivery.base.version,
                            ProjectionDeliveryFailure {
                                status,
                                error_class: error.class,
                                error_code: &code,
                                error_summary: &summary,
                                at,
                            },
                            session,
                        )
                        .await?;
                    if updated.is_none() {
                        return Err(crate::errors::Error::ConflictError(
                            "投递结果已被其他请求写入".to_string(),
                        ));
                    }
                    db.inbox_messages().update(&mut message, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(result)
    }

    async fn query_delivery_result(
        &self,
        command: &ProjectionDeliveryCommand,
        operation_id: &str,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        let delivery = self.load_command_delivery(command).await?;
        if delivery.status == ProjectionDeliveryStatus::Confirmed {
            let result = action_result(
                operation_id.to_string(),
                delivery.base.id,
                ProjectionDeliveryActionResult::Acked,
                delivery.work_item_id.map(|id| id.to_string()),
                delivery.error_task_id.map(|id| id.to_string()),
                Instant::now(),
            );
            return self
                .record_existing_result(
                    result,
                    "sales_order_projection_delivery.query_observed",
                    ProjectionDeliveryAction::QueryResult.as_str(),
                    actor,
                )
                .await;
        }
        if !delivery.can_query_result() {
            return Err(Error::BusinessLogicError(
                "当前投递状态或错误分类不允许查询原结果".to_string(),
            ));
        }
        let revision = self.load_command_revision(command).await?;
        let projection = self.load_command_projection(command).await?;
        let queried = self
            .connector
            .query_projection(&revision, &delivery.target_mall_id, &delivery.message_key)
            .await;
        match queried {
            QueryProjectionResult::Confirmed(ack) => {
                let message = self.load_delivery_message(&delivery).await?;
                self.settle_success(ProjectionSuccessSettlement {
                    projection,
                    revision,
                    delivery,
                    message,
                    ack,
                    operation_id: operation_id.to_string(),
                    command_action: ProjectionDeliveryAction::QueryResult.as_str(),
                    actor,
                })
                .await
            }
            QueryProjectionResult::Failed(error) => {
                let message = self.load_delivery_message(&delivery).await?;
                self.settle_failure(
                    delivery,
                    message,
                    error,
                    operation_id.to_string(),
                    ProjectionDeliveryAction::QueryResult.as_str(),
                    actor,
                )
                .await
            }
            QueryProjectionResult::StillUnknown => {
                self.record_still_unknown(delivery, operation_id, actor).await
            }
        }
    }

    async fn should_advance_projection(
        &self,
        projection: &SalesOrderProjection,
        revision: &SalesOrderProjectionRevision,
    ) -> Result<bool> {
        if projection.is_same_acked_revision(&SalesOrderProjectionRevisionId::new(revision.base.id.clone())) {
            return Ok(false);
        }
        let Some(current_id) = projection.current_acked_revision_id.as_ref() else {
            return Ok(true);
        };
        let current = self
            .db
            .sales_order_projection_revisions()
            .find_by_id(current_id.as_ref(), &mut NoTransaction)
            .await?;
        Ok(current.is_none_or(|current| {
            SalesOrderProjection::should_advance_acked_revision(
                current.revision.revision_no,
                revision.revision.revision_no,
            )
        }))
    }

    async fn record_still_unknown(
        &self,
        delivery: SalesOrderProjectionDelivery,
        operation_id: &str,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        let at = Instant::now();
        let mut message = self.load_delivery_message(&delivery).await?;
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Processing),
            processed_at: None,
        })?;
        let result = action_result(
            operation_id.to_string(),
            delivery.base.id.clone(),
            ProjectionDeliveryActionResult::StillUnknown,
            None,
            None,
            at,
        );
        let audit = operation_audit(
            actor,
            operation_id.to_string(),
            "sales_order_projection_delivery.query_unknown",
            &delivery.base.id,
            ProjectionDeliveryAction::QueryResult.as_str(),
            &result,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let updated = db
                        .sales_order_projection_deliveries()
                        .fail_delivery(
                            &delivery_id,
                            delivery.base.version,
                            ProjectionDeliveryFailure {
                                status: ProjectionDeliveryStatus::ResultUnknown,
                                error_class: ErrorClass::ResultUnknown,
                                error_code: "MALL_RESULT_STILL_UNKNOWN",
                                error_summary: "商城未返回可验证的原投递最终结果",
                                at,
                            },
                            session,
                        )
                        .await?;
                    if updated.is_none() {
                        return Err(crate::errors::Error::ConflictError(
                            "查询期间投递结果已变化".to_string(),
                        ));
                    }
                    db.inbox_messages().update(&mut message, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(result)
    }

    async fn retry_delivery(
        &self,
        command: &ProjectionDeliveryCommand,
        operation_id: &str,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        let delivery = self.load_command_delivery(command).await?;
        if !delivery.can_retry() {
            return Err(Error::BusinessLogicError(
                "当前错误不可重试；结果未知必须先查询，映射差异必须升级 W29".to_string(),
            ));
        }
        let at = Instant::now();
        let next_attempt_at = Instant::from_unix_secs(at.unix_secs() + RETRY_DELAY_SECONDS);
        let result = action_result(
            operation_id.to_string(),
            delivery.base.id.clone(),
            ProjectionDeliveryActionResult::RetryScheduled,
            None,
            None,
            at,
        );
        let audit = operation_audit(
            actor,
            operation_id.to_string(),
            "sales_order_projection_delivery.retry_schedule",
            &delivery.base.id,
            ProjectionDeliveryAction::Retry.as_str(),
            &result,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let updated = db
                        .sales_order_projection_deliveries()
                        .schedule_retry(&delivery_id, delivery.base.version, at, next_attempt_at, session)
                        .await?;
                    if updated.is_none() {
                        return Err(crate::errors::Error::ConflictError(
                            "投递状态或版本已变化".to_string(),
                        ));
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(result)
    }

    async fn escalate_delivery(
        &self,
        command: &ProjectionDeliveryCommand,
        operation_id: &str,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        let delivery = self.load_command_delivery(command).await?;
        if delivery.status == ProjectionDeliveryStatus::Manual {
            let result = action_result(
                operation_id.to_string(),
                delivery.base.id,
                ProjectionDeliveryActionResult::Escalated,
                delivery.work_item_id.map(|id| id.to_string()),
                delivery.error_task_id.map(|id| id.to_string()),
                Instant::now(),
            );
            return self
                .record_existing_result(
                    result,
                    "sales_order_projection_delivery.escalate_observed",
                    ProjectionDeliveryAction::Escalate.as_str(),
                    actor,
                )
                .await;
        }
        if !delivery.can_escalate() {
            return Err(Error::BusinessLogicError(
                "当前投递状态不允许升级 W29".to_string(),
            ));
        }
        let error_class = delivery.error_class.unwrap_or(ErrorClass::ResultUnknown);
        let error_code = delivery
            .error_code
            .clone()
            .unwrap_or_else(|| "DELIVERY_RESULT_UNKNOWN".to_string());
        let error_summary = delivery
            .error_summary
            .clone()
            .unwrap_or_else(|| "投递最终结果尚未取得权威确认".to_string());
        let task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new(stable_entity_id("w29err", &delivery.message_key)),
            IntegrationErrorTaskData {
                message_id: delivery.inbox_message_id.clone(),
                business_object_id: Some(delivery.base.id.clone()),
                error_class,
                owner_role: Some(error_owner_role(error_class).to_string()),
                owner_user_id: Some(actor.id().to_string()),
            },
        )?;
        let work_item = error_work_item(&task, actor.id())?;
        let at = Instant::now();
        let result = action_result(
            operation_id.to_string(),
            delivery.base.id.clone(),
            ProjectionDeliveryActionResult::Escalated,
            Some(work_item.base.id.clone()),
            Some(task.base.id.clone()),
            at,
        );
        let audit = operation_audit(
            actor,
            operation_id.to_string(),
            "sales_order_projection_delivery.escalate",
            &delivery.base.id,
            ProjectionDeliveryAction::Escalate.as_str(),
            &result,
        )?;
        let work_item_audit = actor.clone().resource_log(
            "integration_error_task.work_item.create",
            "work_item",
            work_item.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        let task_id = task.base.id.clone();
        let work_item_id = work_item.base.id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.integration_error_tasks().create(&task, session).await?;
                    db.work_items().create(&work_item, session).await?;
                    let updated = db
                        .sales_order_projection_deliveries()
                        .escalate_delivery(
                            &delivery_id,
                            delivery.base.version,
                            ProjectionDeliveryEscalation {
                                error_class,
                                error_code: &error_code,
                                error_summary: &error_summary,
                                error_task_id: &task_id.clone().into(),
                                work_item_id: &work_item_id.clone().into(),
                                at,
                            },
                            session,
                        )
                        .await?;
                    if updated.is_none() {
                        return Err(crate::errors::Error::ConflictError(
                            "升级期间投递状态已变化".to_string(),
                        ));
                    }
                    db.audit_logs().create(&audit, session).await?;
                    db.audit_logs().create(&work_item_audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(result)
    }

    async fn record_existing_result(
        &self,
        result: ProjectionDeliveryResultView,
        audit_action: &str,
        command_action: &str,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        let audit = operation_audit(
            actor,
            result.operation_id.clone(),
            audit_action,
            &result.delivery_id,
            command_action,
            &result,
        )?;
        match self.db.audit_logs().create(&audit, &mut NoTransaction).await {
            Ok(()) => Ok(result),
            Err(database::Error::DuplicateKey(_)) => self
                .replay_operation(&result.operation_id, &result.delivery_id, command_action, actor)
                .await?
                .ok_or_else(|| Error::ConflictError("幂等回执冲突且无法读取".to_string())),
            Err(error) => Err(error.into()),
        }
    }

    async fn replay_operation(
        &self,
        operation_id: &str,
        delivery_id: &str,
        command_action: &str,
        actor: &AuditActor,
    ) -> Result<Option<ProjectionDeliveryResultView>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(operation_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        let receipt = audit
            .message
            .as_deref()
            .and_then(|message| serde_json::from_str::<DeliveryReceipt>(message).ok())
            .ok_or_else(|| Error::ConflictError("请求ID对应的幂等回执无效".to_string()))?;
        if audit.actor_id != actor.id()
            || audit.resource_type != "sales_order_projection_delivery"
            || audit.resource_id.as_deref() != Some(delivery_id)
            || receipt.command_action != command_action
        {
            return Err(Error::ConflictError("请求ID已用于不同投递命令".to_string()));
        }
        Ok(Some(
            receipt.into_result(operation_id.to_string(), delivery_id.to_string()),
        ))
    }

    async fn load_revision_delivery(
        &self,
        projection_id: &str,
        revision_no: u32,
    ) -> Result<(
        SalesOrderProjection,
        SalesOrderProjectionRevision,
        SalesOrderProjectionDelivery,
    )> {
        let projection = self
            .db
            .sales_order_projections()
            .find_by_id(projection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("执行投影不存在".to_string()))?;
        let revision = self
            .db
            .sales_order_projection_revisions()
            .find_revision_by_no(
                &SalesOrderProjectionId::new(projection_id.to_string()),
                revision_no,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("投影版本不存在".to_string()))?;
        let delivery = self
            .db
            .sales_order_projection_deliveries()
            .find_delivery_by_revision_and_mall(
                &SalesOrderProjectionRevisionId::new(revision.base.id.clone()),
                &projection.target_mall_id,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("投影版本缺少固定投递记录".to_string()))?;
        Ok((projection, revision, delivery))
    }

    async fn load_command_delivery(
        &self,
        command: &ProjectionDeliveryCommand,
    ) -> Result<SalesOrderProjectionDelivery> {
        let delivery = self.current_delivery(&command.delivery_id).await?;
        delivery
            .ensure_matches_command(
                command.expected_object_version,
                &SalesOrderProjectionRevisionId::new(command.projection_revision_id.clone()),
            )
            .map_err(|err| Error::ConflictError(err.to_string()))?;
        Ok(delivery)
    }

    async fn load_command_revision(
        &self,
        command: &ProjectionDeliveryCommand,
    ) -> Result<SalesOrderProjectionRevision> {
        let revision = self
            .db
            .sales_order_projection_revisions()
            .find_by_id(&command.projection_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("投影修订不存在".to_string()))?;
        revision
            .ensure_belongs_to_projection(&SalesOrderProjectionId::new(command.projection_id.clone()))
            .map_err(|err| Error::ConflictError(err.to_string()))?;
        Ok(revision)
    }

    async fn load_command_projection(
        &self,
        command: &ProjectionDeliveryCommand,
    ) -> Result<SalesOrderProjection> {
        self.db
            .sales_order_projections()
            .find_by_id(&command.projection_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("执行投影不存在".to_string()))
    }

    async fn current_delivery(&self, id: &str) -> Result<SalesOrderProjectionDelivery> {
        self.db
            .sales_order_projection_deliveries()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("投递不存在".to_string()))
    }

    async fn load_delivery_message(&self, delivery: &SalesOrderProjectionDelivery) -> Result<InboxMessage> {
        let message_id = delivery
            .inbox_message_id
            .as_ref()
            .ok_or_else(|| Error::ConflictError("投递尚未形成原消息身份".to_string()))?;
        self.db
            .inbox_messages()
            .find_by_id(message_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原投递消息不存在".to_string()))
    }
}

fn is_unknown_error(error: &ClassifiedError) -> bool {
    error.class == ErrorClass::ResultUnknown
        || error.code.contains("TIMEOUT")
        || error.code.contains("OUTCOME_UNKNOWN")
}

fn unknown_error(error: ClassifiedError) -> ClassifiedError {
    ClassifiedError {
        class: ErrorClass::ResultUnknown,
        code: error.code,
        summary: error.summary,
    }
}

fn terminal_or_inflight_result(
    delivery: &SalesOrderProjectionDelivery,
    operation_id: String,
) -> Option<ProjectionDeliveryResultView> {
    match delivery.status {
        ProjectionDeliveryStatus::Confirmed
        | ProjectionDeliveryStatus::Failed
        | ProjectionDeliveryStatus::ResultUnknown
        | ProjectionDeliveryStatus::Manual
        | ProjectionDeliveryStatus::Sending => {
            Some(delivery_result_from_fact(operation_id, delivery.clone()))
        }
        ProjectionDeliveryStatus::PendingSend | ProjectionDeliveryStatus::Retrying => None,
    }
}

fn delivery_result_from_fact(
    operation_id: String,
    delivery: SalesOrderProjectionDelivery,
) -> ProjectionDeliveryResultView {
    let result = match delivery.status {
        ProjectionDeliveryStatus::Confirmed => ProjectionDeliveryActionResult::Acked,
        ProjectionDeliveryStatus::Failed => ProjectionDeliveryActionResult::Failed,
        ProjectionDeliveryStatus::Manual => ProjectionDeliveryActionResult::Escalated,
        ProjectionDeliveryStatus::Sending | ProjectionDeliveryStatus::ResultUnknown => {
            ProjectionDeliveryActionResult::StillUnknown
        }
        ProjectionDeliveryStatus::PendingSend | ProjectionDeliveryStatus::Retrying => {
            ProjectionDeliveryActionResult::RetryScheduled
        }
    };
    action_result(
        operation_id,
        delivery.base.id,
        result,
        delivery.work_item_id.map(|id| id.to_string()),
        delivery.error_task_id.map(|id| id.to_string()),
        Instant::now(),
    )
}

fn action_result(
    operation_id: String,
    delivery_id: String,
    result: ProjectionDeliveryActionResult,
    work_item_id: Option<String>,
    error_task_id: Option<String>,
    occurred_at: Instant,
) -> ProjectionDeliveryResultView {
    let next_action = next_action_for(result);
    ProjectionDeliveryResultView {
        operation_id,
        delivery_id,
        result,
        work_item_id,
        error_task_id,
        occurred_at: occurred_at.unix_secs(),
        next_action,
    }
}

fn next_action_for(result: ProjectionDeliveryActionResult) -> Option<String> {
    match result {
        ProjectionDeliveryActionResult::Acked => None,
        ProjectionDeliveryActionResult::Failed => Some("按服务端 allowedActions 继续处理".to_string()),
        ProjectionDeliveryActionResult::StillUnknown => Some("继续查询原结果或升级到 W29".to_string()),
        ProjectionDeliveryActionResult::RetryScheduled => {
            Some(format!("将在 {} 秒后沿原消息键重试", RETRY_DELAY_SECONDS))
        }
        ProjectionDeliveryActionResult::Escalated => Some("OPEN_W29".to_string()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DeliveryReceipt {
    #[serde(rename = "c")]
    command_action: String,
    #[serde(rename = "r")]
    result: ProjectionDeliveryActionResult,
    #[serde(rename = "w", skip_serializing_if = "Option::is_none")]
    work_item_id: Option<String>,
    #[serde(rename = "e", skip_serializing_if = "Option::is_none")]
    error_task_id: Option<String>,
    #[serde(rename = "t")]
    occurred_at: i64,
}

impl DeliveryReceipt {
    fn from_result(command_action: &str, result: &ProjectionDeliveryResultView) -> Self {
        Self {
            command_action: command_action.to_string(),
            result: result.result,
            work_item_id: result.work_item_id.clone(),
            error_task_id: result.error_task_id.clone(),
            occurred_at: result.occurred_at,
        }
    }

    fn into_result(self, operation_id: String, delivery_id: String) -> ProjectionDeliveryResultView {
        ProjectionDeliveryResultView {
            operation_id,
            delivery_id,
            result: self.result,
            work_item_id: self.work_item_id,
            error_task_id: self.error_task_id,
            occurred_at: self.occurred_at,
            next_action: next_action_for(self.result),
        }
    }
}

fn operation_audit(
    actor: &AuditActor,
    id: String,
    action: &str,
    delivery_id: &str,
    command_action: &str,
    result: &ProjectionDeliveryResultView,
) -> Result<entities::AuditLog> {
    let receipt = serde_json::to_string(&DeliveryReceipt::from_result(command_action, result))
        .map_err(|error| Error::Internal(format!("序列化投递幂等回执失败: {error}")))?;
    actor.clone().resource_log_with_id(
        id,
        action,
        "sales_order_projection_delivery",
        delivery_id.to_string(),
        Some(receipt),
    )
}

fn operation_id(actor_id: &str, action: &str, delivery_id: &str, request_id: &str) -> String {
    stable_entity_id(
        "w23op",
        &format!("{actor_id}|{action}|{delivery_id}|{request_id}"),
    )
}

fn stable_entity_id(prefix: &str, identity: &str) -> String {
    format!("{prefix}_{}", hex::encode(Sha256::digest(identity.as_bytes())))
}

#[cfg(test)]
mod tests {
    use super::{operation_id, stable_entity_id, unknown_error};
    use crate::projection::ClassifiedError;
    use entities::integration_ops::ErrorClass;

    #[test]
    fn operation_identity_is_stable_without_exposing_raw_request_id() {
        let first = operation_id("actor-1", "RETRY", "delivery-1", "secret-request-key");
        let second = operation_id("actor-1", "RETRY", "delivery-1", "secret-request-key");
        assert_eq!(first, second);
        assert!(!first.contains("secret-request-key"));
        assert!(first.starts_with("w23op_"));
    }

    #[test]
    fn timeout_error_is_preserved_but_classified_as_result_unknown() {
        let error = unknown_error(ClassifiedError {
            class: ErrorClass::TransientFailure,
            code: "MALL_TIMEOUT".to_string(),
            summary: "请求超时".to_string(),
        });
        assert_eq!(error.class, ErrorClass::ResultUnknown);
        assert_eq!(error.code, "MALL_TIMEOUT");
    }

    #[test]
    fn stable_w29_identity_is_deterministic() {
        assert_eq!(
            stable_entity_id("w29err", "projection_delivery:r1:m1"),
            stable_entity_id("w29err", "projection_delivery:r1:m1")
        );
    }
}
