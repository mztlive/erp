//! W22 商品发布稳定投递、原结果查询、受控重试与 W29 升级。

use database::repository::{PublicationDeliveryEscalation, PublicationDeliveryFailure};
use database::{
    AccessControlExt, IntegrationOpsExt, NoTransaction, PublicationExt, Transactional, WorkItemExt,
};
use entities::common::time::Instant;
use entities::ids::{
    InboxMessageId, IntegrationErrorTaskId, ProductPublicationId, ProductPublicationRevisionId,
};
use entities::integration_ops::{
    ErrorClass, InboxMessage, InboxMessageData, InboxMessageStatus, InboxMessageUpdate, IntegrationErrorTask,
    IntegrationErrorTaskData, MessageType,
};
use entities::publication::{
    ProductPublication, ProductPublicationDelivery, ProductPublicationRevision, ProductPublicationStatus,
    ProductPublicationUpdate, PublicationDeliveryStatus, SaleStatus,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use validator::Validate;

use super::{ClassifiedError, PublicationService, PublishAck, QueryPublicationResult};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::integration_ops::{error_owner_role, error_work_item};
use crate::publication::dto::{
    DeliverPublicationRevisionRequest, ProcessPublicationDeliveriesRequest,
    ProcessPublicationDeliveriesResult, PublicationDeliveryAction, PublicationDeliveryActionResult,
    PublicationDeliveryActionResultView, PublicationDeliveryCommand, PublicationDeliveryResultView,
};

const RETRY_DELAY_SECONDS: i64 = 60;

struct PublicationSuccessSettlement<'a> {
    publication: ProductPublication,
    revision: ProductPublicationRevision,
    delivery: ProductPublicationDelivery,
    message: InboxMessage,
    ack: PublishAck,
    operation_id: String,
    command_action: &'static str,
    actor: &'a AuditActor,
}

impl PublicationService {
    /// 处理指定发布修订已预建的固定待发送投递。
    ///
    /// 本入口绝不创建第二条投递；并发调用仅一个能以 CAS 取得原记录。
    pub async fn deliver_revision(
        &self,
        publication_id: &str,
        revision_no: u32,
        req: DeliverPublicationRevisionRequest,
        actor: &AuditActor,
    ) -> Result<PublicationDeliveryResultView> {
        req.validate()?;
        let (publication, revision, delivery) = self
            .load_publication_revision_delivery(publication_id, revision_no)
            .await?;
        self.ensure_safety_pause_delivery_allowed(&publication, &revision)
            .await?;
        if !delivery.is_send_ready(Instant::now()) {
            return self.legacy_delivery_result(publication, delivery).await;
        }
        let operation_id =
            publication_operation_id(actor.id(), "SEND", &delivery.base.id, &req.idempotency_key);
        let result = self
            .process_publication_delivery(publication, revision, delivery, operation_id, "SEND", actor)
            .await?;
        self.legacy_result_from_action(result).await
    }

    /// 执行 `QUERY_RESULT / RETRY / ESCALATE` 强类型投递对象命令。
    pub async fn apply_publication_delivery_command(
        &self,
        path_delivery_id: &str,
        command: PublicationDeliveryCommand,
        actor: &AuditActor,
    ) -> Result<PublicationDeliveryActionResultView> {
        command.validate()?;
        if command.delivery_id != path_delivery_id {
            return Err(Error::ValidationError("路径投递ID与命令不一致".to_string()));
        }
        let operation_id = publication_operation_id(
            actor.id(),
            command.action.as_str(),
            path_delivery_id,
            &command.request_id,
        );
        if let Some(result) = self
            .replay_publication_operation(&operation_id, path_delivery_id, command.action.as_str(), actor)
            .await?
        {
            return Ok(result);
        }

        let attempted = match command.action {
            PublicationDeliveryAction::QueryResult => {
                self.query_publication_delivery(&command, &operation_id, actor)
                    .await
            }
            PublicationDeliveryAction::Retry => {
                self.retry_publication_delivery(&command, &operation_id, actor)
                    .await
            }
            PublicationDeliveryAction::Escalate => {
                self.escalate_publication_delivery(&command, &operation_id, actor)
                    .await
            }
        };
        match attempted {
            Ok(result) => Ok(result),
            Err(error) => match self
                .replay_publication_operation(&operation_id, path_delivery_id, command.action.as_str(), actor)
                .await?
            {
                Some(result) => Ok(result),
                None => Err(error),
            },
        }
    }

    /// 以有界批次处理 `PENDING_SEND` 与到期 `RETRYING` 投递。
    pub async fn process_pending_publication_deliveries(
        &self,
        req: ProcessPublicationDeliveriesRequest,
        actor: &AuditActor,
    ) -> Result<ProcessPublicationDeliveriesResult> {
        req.validate()?;
        let rows = self
            .db
            .product_publication_deliveries()
            .list_processable_publication_deliveries(
                Instant::now(),
                req.limit.unwrap_or(50),
                &mut NoTransaction,
            )
            .await?;
        let mut result = ProcessPublicationDeliveriesResult {
            scanned: rows.len() as u32,
            acked: 0,
            failed: 0,
            still_unknown: 0,
            skipped: 0,
            items: Vec::with_capacity(rows.len()),
        };
        for delivery in rows {
            let revision = self
                .db
                .product_publication_revisions()
                .find_by_id(delivery.publication_revision_id.as_ref(), &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("投递关联的发布修订不存在".to_string()))?;
            let publication = self
                .db
                .product_publications()
                .find_by_id(revision.product_publication_id.as_ref(), &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("投递关联的稳定发布不存在".to_string()))?;
            let operation_id = publication_operation_id(
                actor.id(),
                "PROCESS",
                &delivery.base.id,
                &format!("{}:{}", delivery.message_key, delivery.base.version),
            );
            match self
                .process_publication_delivery(publication, revision, delivery, operation_id, "PROCESS", actor)
                .await
            {
                Ok(item) => {
                    match item.result {
                        PublicationDeliveryActionResult::Acked => result.acked += 1,
                        PublicationDeliveryActionResult::Failed => result.failed += 1,
                        PublicationDeliveryActionResult::StillUnknown => result.still_unknown += 1,
                        PublicationDeliveryActionResult::RetryScheduled
                        | PublicationDeliveryActionResult::Escalated => result.skipped += 1,
                    }
                    result.items.push(item);
                }
                Err(Error::ConflictError(_)) | Err(Error::BusinessLogicError(_)) => {
                    result.skipped += 1;
                }
                Err(error) => return Err(error),
            }
        }
        Ok(result)
    }

    async fn process_publication_delivery(
        &self,
        publication: ProductPublication,
        revision: ProductPublicationRevision,
        delivery: ProductPublicationDelivery,
        operation_id: String,
        command_action: &'static str,
        actor: &AuditActor,
    ) -> Result<PublicationDeliveryActionResultView> {
        ensure_publication_delivery_relation(&publication, &revision, &delivery)?;
        self.ensure_safety_pause_delivery_allowed(&publication, &revision)
            .await?;
        let mut message = self.ensure_publication_message(&delivery).await?;
        let now = Instant::now();
        let claimed = self
            .db
            .product_publication_deliveries()
            .claim_publication_delivery(
                &delivery.base.id,
                delivery.base.version,
                &InboxMessageId::new(message.base.id.clone()),
                now,
                &mut NoTransaction,
            )
            .await?;
        let Some(claimed) = claimed else {
            let current = self.current_publication_delivery(&delivery.base.id).await?;
            return publication_action_from_fact(operation_id, current)
                .ok_or_else(|| Error::ConflictError("投递已由其他处理器取得，请刷新".to_string()));
        };
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Processing),
            processed_at: None,
        })?;
        self.db
            .inbox_messages()
            .update(&mut message, &mut NoTransaction)
            .await?;

        match self
            .connector
            .publish_revision(&revision, &publication.target_mall_id)
            .await
        {
            Ok(ack) => {
                self.settle_publication_success(PublicationSuccessSettlement {
                    publication,
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
            Err(error) if publication_error_is_unknown(&error) => {
                self.settle_publication_failure(
                    claimed,
                    message,
                    publication_unknown_error(error),
                    operation_id,
                    command_action,
                    actor,
                )
                .await
            }
            Err(error) => {
                self.settle_publication_failure(claimed, message, error, operation_id, command_action, actor)
                    .await
            }
        }
    }

    async fn ensure_publication_message(
        &self,
        delivery: &ProductPublicationDelivery,
    ) -> Result<InboxMessage> {
        let id = delivery
            .inbox_message_id
            .clone()
            .unwrap_or_else(|| InboxMessageId::new(publication_stable_id("pubmsg", &delivery.message_key)));
        let message = InboxMessage::new(
            id,
            InboxMessageData {
                source_system_id: delivery.target_mall_id.clone(),
                source_event_id: delivery.message_key.clone(),
                message_type: MessageType::MallActionRequest,
                business_fact_key: delivery.message_key.clone(),
                payload_schema_version: "publication-delivery-v1".to_string(),
                payload_reference: Some(delivery.publication_revision_id.to_string()),
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

    async fn settle_publication_success(
        &self,
        settlement: PublicationSuccessSettlement<'_>,
    ) -> Result<PublicationDeliveryActionResultView> {
        let PublicationSuccessSettlement {
            mut publication,
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
        let advance = self.should_advance_publication(&publication, &revision).await?;
        if advance {
            publication.update(
                ProductPublicationUpdate {
                    status: Some(if revision.sale_status == SaleStatus::OnSale {
                        ProductPublicationStatus::MallEffective
                    } else {
                        ProductPublicationStatus::Paused
                    }),
                    current_revision_id: Some(revision.base.id.clone()),
                },
                actor.id(),
            )?;
        }
        let result = publication_action_result(
            operation_id.clone(),
            delivery.base.id.clone(),
            PublicationDeliveryActionResult::Acked,
            None,
            None,
            at,
        );
        let audit = publication_operation_audit(
            actor,
            operation_id,
            "product_publication_delivery.acked",
            &delivery.base.id,
            command_action,
            &result,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        let mall_version = ack.mall_version;
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let updated = db
                        .product_publication_deliveries()
                        .confirm_publication_delivery(
                            &delivery_id,
                            delivery.base.version,
                            &mall_version,
                            at,
                            session,
                        )
                        .await?;
                    if updated.is_none() {
                        return Err(Error::ConflictError("投递结果已由其他请求写入".to_string()));
                    }
                    if advance {
                        db.product_publications()
                            .update(&mut publication, session)
                            .await?;
                    }
                    db.inbox_messages().update(&mut message, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), Error>(())
                })
            })
            .await?;
        Ok(result)
    }

    async fn settle_publication_failure(
        &self,
        delivery: ProductPublicationDelivery,
        mut message: InboxMessage,
        error: ClassifiedError,
        operation_id: String,
        command_action: &'static str,
        actor: &AuditActor,
    ) -> Result<PublicationDeliveryActionResultView> {
        let at = Instant::now();
        let unknown = error.class == ErrorClass::ResultUnknown;
        let status = if unknown {
            PublicationDeliveryStatus::ResultUnknown
        } else {
            PublicationDeliveryStatus::Failed
        };
        message.update(InboxMessageUpdate {
            status: Some(if unknown {
                InboxMessageStatus::Processing
            } else {
                InboxMessageStatus::Failed
            }),
            processed_at: None,
        })?;
        let result = publication_action_result(
            operation_id.clone(),
            delivery.base.id.clone(),
            if unknown {
                PublicationDeliveryActionResult::StillUnknown
            } else {
                PublicationDeliveryActionResult::Failed
            },
            None,
            None,
            at,
        );
        let audit = publication_operation_audit(
            actor,
            operation_id,
            if unknown {
                "product_publication_delivery.result_unknown"
            } else {
                "product_publication_delivery.failed"
            },
            &delivery.base.id,
            command_action,
            &result,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        let code = error.code;
        let summary = error.summary;
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let updated = db
                        .product_publication_deliveries()
                        .fail_publication_delivery(
                            &delivery_id,
                            delivery.base.version,
                            PublicationDeliveryFailure {
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
                        return Err(Error::ConflictError("投递结果已由其他请求写入".to_string()));
                    }
                    db.inbox_messages().update(&mut message, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), Error>(())
                })
            })
            .await?;
        Ok(result)
    }

    async fn query_publication_delivery(
        &self,
        command: &PublicationDeliveryCommand,
        operation_id: &str,
        actor: &AuditActor,
    ) -> Result<PublicationDeliveryActionResultView> {
        let delivery = self.load_publication_command_delivery(command).await?;
        if delivery.delivery_status == PublicationDeliveryStatus::Confirmed {
            let result = publication_action_result(
                operation_id.to_string(),
                delivery.base.id,
                PublicationDeliveryActionResult::Acked,
                delivery.work_item_id.map(|id| id.to_string()),
                delivery.error_task_id.map(|id| id.to_string()),
                Instant::now(),
            );
            return self
                .record_existing_publication_result(
                    result,
                    "product_publication_delivery.query_observed",
                    PublicationDeliveryAction::QueryResult.as_str(),
                    actor,
                )
                .await;
        }
        if !delivery.can_query_result() {
            return Err(Error::BusinessLogicError(
                "当前投递状态或错误分类不允许查询原结果".to_string(),
            ));
        }
        let revision = self.load_publication_command_revision(command).await?;
        let publication = self.load_publication_command_publication(command).await?;
        match self
            .connector
            .query_publication(&revision, &delivery.target_mall_id, &delivery.message_key)
            .await
        {
            QueryPublicationResult::Confirmed(ack) => {
                let message = self.load_publication_delivery_message(&delivery).await?;
                self.settle_publication_success(PublicationSuccessSettlement {
                    publication,
                    revision,
                    delivery,
                    message,
                    ack,
                    operation_id: operation_id.to_string(),
                    command_action: PublicationDeliveryAction::QueryResult.as_str(),
                    actor,
                })
                .await
            }
            QueryPublicationResult::Failed(error) => {
                let message = self.load_publication_delivery_message(&delivery).await?;
                self.settle_publication_failure(
                    delivery,
                    message,
                    error,
                    operation_id.to_string(),
                    PublicationDeliveryAction::QueryResult.as_str(),
                    actor,
                )
                .await
            }
            QueryPublicationResult::StillUnknown => {
                self.record_publication_unknown(delivery, operation_id, actor)
                    .await
            }
        }
    }

    async fn record_publication_unknown(
        &self,
        delivery: ProductPublicationDelivery,
        operation_id: &str,
        actor: &AuditActor,
    ) -> Result<PublicationDeliveryActionResultView> {
        let at = Instant::now();
        let mut message = self.load_publication_delivery_message(&delivery).await?;
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Processing),
            processed_at: None,
        })?;
        let result = publication_action_result(
            operation_id.to_string(),
            delivery.base.id.clone(),
            PublicationDeliveryActionResult::StillUnknown,
            None,
            None,
            at,
        );
        let audit = publication_operation_audit(
            actor,
            operation_id.to_string(),
            "product_publication_delivery.query_unknown",
            &delivery.base.id,
            PublicationDeliveryAction::QueryResult.as_str(),
            &result,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let updated = db
                        .product_publication_deliveries()
                        .fail_publication_delivery(
                            &delivery_id,
                            delivery.base.version,
                            PublicationDeliveryFailure {
                                status: PublicationDeliveryStatus::ResultUnknown,
                                error_class: ErrorClass::ResultUnknown,
                                error_code: "MALL_RESULT_STILL_UNKNOWN",
                                error_summary: "商城未返回可验证的原投递最终结果",
                                at,
                            },
                            session,
                        )
                        .await?;
                    if updated.is_none() {
                        return Err(Error::ConflictError("查询期间投递结果已变化".to_string()));
                    }
                    db.inbox_messages().update(&mut message, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), Error>(())
                })
            })
            .await?;
        Ok(result)
    }

    async fn retry_publication_delivery(
        &self,
        command: &PublicationDeliveryCommand,
        operation_id: &str,
        actor: &AuditActor,
    ) -> Result<PublicationDeliveryActionResultView> {
        let delivery = self.load_publication_command_delivery(command).await?;
        if !delivery.can_retry() {
            return Err(Error::BusinessLogicError(
                "当前错误不可重试；结果未知必须先查询，映射差异必须升级 W29".to_string(),
            ));
        }
        let at = Instant::now();
        let next_attempt_at = Instant::from_unix_secs(at.unix_secs() + RETRY_DELAY_SECONDS);
        let result = publication_action_result(
            operation_id.to_string(),
            delivery.base.id.clone(),
            PublicationDeliveryActionResult::RetryScheduled,
            None,
            None,
            at,
        );
        let audit = publication_operation_audit(
            actor,
            operation_id.to_string(),
            "product_publication_delivery.retry_schedule",
            &delivery.base.id,
            PublicationDeliveryAction::Retry.as_str(),
            &result,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let updated = db
                        .product_publication_deliveries()
                        .schedule_publication_retry(
                            &delivery_id,
                            delivery.base.version,
                            at,
                            next_attempt_at,
                            session,
                        )
                        .await?;
                    if updated.is_none() {
                        return Err(Error::ConflictError("投递状态或版本已变化".to_string()));
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), Error>(())
                })
            })
            .await?;
        Ok(result)
    }

    async fn escalate_publication_delivery(
        &self,
        command: &PublicationDeliveryCommand,
        operation_id: &str,
        actor: &AuditActor,
    ) -> Result<PublicationDeliveryActionResultView> {
        let delivery = self.load_publication_command_delivery(command).await?;
        if delivery.delivery_status == PublicationDeliveryStatus::Manual {
            let result = publication_action_result(
                operation_id.to_string(),
                delivery.base.id,
                PublicationDeliveryActionResult::Escalated,
                delivery.work_item_id.map(|id| id.to_string()),
                delivery.error_task_id.map(|id| id.to_string()),
                Instant::now(),
            );
            return self
                .record_existing_publication_result(
                    result,
                    "product_publication_delivery.escalate_observed",
                    PublicationDeliveryAction::Escalate.as_str(),
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
            IntegrationErrorTaskId::new(publication_stable_id("w29pub", &delivery.message_key)),
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
        let result = publication_action_result(
            operation_id.to_string(),
            delivery.base.id.clone(),
            PublicationDeliveryActionResult::Escalated,
            Some(work_item.base.id.clone()),
            Some(task.base.id.clone()),
            at,
        );
        let audit = publication_operation_audit(
            actor,
            operation_id.to_string(),
            "product_publication_delivery.escalate",
            &delivery.base.id,
            PublicationDeliveryAction::Escalate.as_str(),
            &result,
        )?;
        let work_item_audit = actor.clone().resource_log(
            "integration_error_task.work_item.create",
            "work_item",
            work_item.base.id.clone(),
        )?;
        let mut message = self.load_publication_delivery_message(&delivery).await?;
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::ToManual),
            processed_at: None,
        })?;
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
                        .product_publication_deliveries()
                        .escalate_publication_delivery(
                            &delivery_id,
                            delivery.base.version,
                            PublicationDeliveryEscalation {
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
                        return Err(Error::ConflictError("升级期间投递状态已变化".to_string()));
                    }
                    db.inbox_messages().update(&mut message, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    db.audit_logs().create(&work_item_audit, session).await?;
                    Ok::<(), Error>(())
                })
            })
            .await?;
        Ok(result)
    }

    async fn load_publication_revision_delivery(
        &self,
        publication_id: &str,
        revision_no: u32,
    ) -> Result<(
        ProductPublication,
        ProductPublicationRevision,
        ProductPublicationDelivery,
    )> {
        let publication = self
            .db
            .product_publications()
            .find_by_id(publication_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品发布不存在".to_string()))?;
        let revision = self
            .db
            .product_publication_revisions()
            .find_revision_by_no(
                &ProductPublicationId::new(publication_id.to_string()),
                revision_no,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("发布修订不存在".to_string()))?;
        let delivery = self
            .db
            .product_publication_deliveries()
            .find_delivery_by_revision_and_mall(
                &ProductPublicationRevisionId::new(revision.base.id.clone()),
                &publication.target_mall_id,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("发布修订缺少固定投递记录".to_string()))?;
        Ok((publication, revision, delivery))
    }

    async fn load_publication_command_delivery(
        &self,
        command: &PublicationDeliveryCommand,
    ) -> Result<ProductPublicationDelivery> {
        let delivery = self.current_publication_delivery(&command.delivery_id).await?;
        if delivery.base.version != command.expected_object_version
            || delivery.publication_revision_id.to_string() != command.publication_revision_id
        {
            return Err(Error::ConflictError("投递对象版本或修订身份已变化".to_string()));
        }
        Ok(delivery)
    }

    async fn load_publication_command_revision(
        &self,
        command: &PublicationDeliveryCommand,
    ) -> Result<ProductPublicationRevision> {
        let revision = self
            .db
            .product_publication_revisions()
            .find_by_id(&command.publication_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("发布修订不存在".to_string()))?;
        if revision.product_publication_id.to_string() != command.publication_id {
            return Err(Error::ConflictError("发布修订不属于命令发布".to_string()));
        }
        Ok(revision)
    }

    async fn load_publication_command_publication(
        &self,
        command: &PublicationDeliveryCommand,
    ) -> Result<ProductPublication> {
        self.db
            .product_publications()
            .find_by_id(&command.publication_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品发布不存在".to_string()))
    }

    async fn current_publication_delivery(&self, id: &str) -> Result<ProductPublicationDelivery> {
        self.db
            .product_publication_deliveries()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("发布投递不存在".to_string()))
    }

    async fn load_publication_delivery_message(
        &self,
        delivery: &ProductPublicationDelivery,
    ) -> Result<InboxMessage> {
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

    async fn ensure_safety_pause_delivery_allowed(
        &self,
        publication: &ProductPublication,
        revision: &ProductPublicationRevision,
    ) -> Result<()> {
        if revision.sale_status == SaleStatus::OnSale
            && self
                .db
                .system_safety_pause_operations()
                .has_safety_pause_for_publication(&publication.base.id, &mut NoTransaction)
                .await?
        {
            return Err(Error::BusinessLogicError(
                "RECOVERY_RESPONSIBILITY_UNCONFIRMED：系统安全暂停发布禁止投递 ON_SALE 修订".to_string(),
            ));
        }
        Ok(())
    }

    async fn should_advance_publication(
        &self,
        publication: &ProductPublication,
        revision: &ProductPublicationRevision,
    ) -> Result<bool> {
        if revision.sale_status == SaleStatus::OnSale
            && self
                .db
                .system_safety_pause_operations()
                .has_safety_pause_for_publication(&publication.base.id, &mut NoTransaction)
                .await?
        {
            return Ok(false);
        }
        let Some(current_id) = publication.stable.current_revision_id.as_ref() else {
            return Ok(true);
        };
        if current_id == &revision.base.id {
            return Ok(false);
        }
        let current = self
            .db
            .product_publication_revisions()
            .find_by_id(current_id, &mut NoTransaction)
            .await?;
        Ok(current.is_none_or(|current| current.revision.revision_no <= revision.revision.revision_no))
    }

    async fn legacy_result_from_action(
        &self,
        result: PublicationDeliveryActionResultView,
    ) -> Result<PublicationDeliveryResultView> {
        let delivery = self.current_publication_delivery(&result.delivery_id).await?;
        let revision = self
            .db
            .product_publication_revisions()
            .find_by_id(delivery.publication_revision_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("投递关联的发布修订不存在".to_string()))?;
        let publication = self
            .db
            .product_publications()
            .find_by_id(revision.product_publication_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("投递关联的稳定发布不存在".to_string()))?;
        self.legacy_delivery_result(publication, delivery).await
    }

    async fn legacy_delivery_result(
        &self,
        publication: ProductPublication,
        delivery: ProductPublicationDelivery,
    ) -> Result<PublicationDeliveryResultView> {
        Ok(PublicationDeliveryResultView {
            delivery_id: delivery.base.id,
            delivery_status: delivery.delivery_status,
            inbox_message_id: delivery
                .inbox_message_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
            error_task_id: delivery.error_task_id.map(|id| id.to_string()),
            mall_version: delivery.mall_version,
            publication_version: publication.base.version,
        })
    }

    async fn record_existing_publication_result(
        &self,
        result: PublicationDeliveryActionResultView,
        audit_action: &str,
        command_action: &str,
        actor: &AuditActor,
    ) -> Result<PublicationDeliveryActionResultView> {
        let audit = publication_operation_audit(
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
                .replay_publication_operation(
                    &result.operation_id,
                    &result.delivery_id,
                    command_action,
                    actor,
                )
                .await?
                .ok_or_else(|| Error::ConflictError("幂等回执冲突且无法读取".to_string())),
            Err(error) => Err(error.into()),
        }
    }

    async fn replay_publication_operation(
        &self,
        operation_id: &str,
        delivery_id: &str,
        command_action: &str,
        actor: &AuditActor,
    ) -> Result<Option<PublicationDeliveryActionResultView>> {
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
            .and_then(|message| serde_json::from_str::<PublicationDeliveryReceipt>(message).ok())
            .ok_or_else(|| Error::ConflictError("请求ID对应的幂等回执无效".to_string()))?;
        if audit.actor_id != actor.id()
            || audit.resource_type != "product_publication_delivery"
            || audit.resource_id.as_deref() != Some(delivery_id)
            || receipt.command_action != command_action
        {
            return Err(Error::ConflictError("请求ID已用于不同投递命令".to_string()));
        }
        Ok(Some(
            receipt.into_result(operation_id.to_string(), delivery_id.to_string()),
        ))
    }
}

pub(super) fn publication_delivery_actions(
    status: PublicationDeliveryStatus,
    error_class: Option<ErrorClass>,
) -> Vec<String> {
    match status {
        PublicationDeliveryStatus::Sending | PublicationDeliveryStatus::ResultUnknown => {
            vec!["QUERY_RESULT".to_string(), "ESCALATE".to_string()]
        }
        PublicationDeliveryStatus::Failed if error_class == Some(ErrorClass::MappingError) => {
            vec!["ESCALATE".to_string()]
        }
        PublicationDeliveryStatus::Failed if error_class.is_some_and(|class| class.can_auto_retry()) => {
            vec![
                "QUERY_RESULT".to_string(),
                "RETRY".to_string(),
                "ESCALATE".to_string(),
            ]
        }
        PublicationDeliveryStatus::Failed => {
            vec!["QUERY_RESULT".to_string(), "ESCALATE".to_string()]
        }
        PublicationDeliveryStatus::PendingSend
        | PublicationDeliveryStatus::Retrying
        | PublicationDeliveryStatus::Confirmed
        | PublicationDeliveryStatus::Manual => Vec::new(),
    }
}

fn ensure_publication_delivery_relation(
    publication: &ProductPublication,
    revision: &ProductPublicationRevision,
    delivery: &ProductPublicationDelivery,
) -> Result<()> {
    if revision.product_publication_id.to_string() != publication.base.id
        || delivery.publication_revision_id.to_string() != revision.base.id
        || delivery.target_mall_id != publication.target_mall_id
    {
        return Err(Error::ConflictError("发布、修订与固定投递身份不一致".to_string()));
    }
    Ok(())
}

fn publication_error_is_unknown(error: &ClassifiedError) -> bool {
    error.class == ErrorClass::ResultUnknown
        || error.code.contains("TIMEOUT")
        || error.code.contains("OUTCOME_UNKNOWN")
}

fn publication_unknown_error(error: ClassifiedError) -> ClassifiedError {
    ClassifiedError {
        class: ErrorClass::ResultUnknown,
        code: error.code,
        summary: error.summary,
    }
}

fn publication_action_from_fact(
    operation_id: String,
    delivery: ProductPublicationDelivery,
) -> Option<PublicationDeliveryActionResultView> {
    let result = match delivery.delivery_status {
        PublicationDeliveryStatus::Confirmed => PublicationDeliveryActionResult::Acked,
        PublicationDeliveryStatus::Failed => PublicationDeliveryActionResult::Failed,
        PublicationDeliveryStatus::Manual => PublicationDeliveryActionResult::Escalated,
        PublicationDeliveryStatus::Sending | PublicationDeliveryStatus::ResultUnknown => {
            PublicationDeliveryActionResult::StillUnknown
        }
        PublicationDeliveryStatus::PendingSend | PublicationDeliveryStatus::Retrying => return None,
    };
    Some(publication_action_result(
        operation_id,
        delivery.base.id,
        result,
        delivery.work_item_id.map(|id| id.to_string()),
        delivery.error_task_id.map(|id| id.to_string()),
        Instant::now(),
    ))
}

fn publication_action_result(
    operation_id: String,
    delivery_id: String,
    result: PublicationDeliveryActionResult,
    work_item_id: Option<String>,
    error_task_id: Option<String>,
    occurred_at: Instant,
) -> PublicationDeliveryActionResultView {
    PublicationDeliveryActionResultView {
        operation_id,
        delivery_id,
        result,
        work_item_id,
        error_task_id,
        occurred_at: occurred_at.unix_secs(),
        next_action: publication_next_action(result),
    }
}

fn publication_next_action(result: PublicationDeliveryActionResult) -> Option<String> {
    match result {
        PublicationDeliveryActionResult::Acked => None,
        PublicationDeliveryActionResult::Failed => Some("按服务端 allowedActions 继续处理".to_string()),
        PublicationDeliveryActionResult::StillUnknown => Some("继续查询原结果或升级到 W29".to_string()),
        PublicationDeliveryActionResult::RetryScheduled => {
            Some(format!("将在 {RETRY_DELAY_SECONDS} 秒后沿原消息键重试"))
        }
        PublicationDeliveryActionResult::Escalated => Some("OPEN_W29".to_string()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PublicationDeliveryReceipt {
    #[serde(rename = "c")]
    command_action: String,
    #[serde(rename = "r")]
    result: PublicationDeliveryActionResult,
    #[serde(rename = "w", skip_serializing_if = "Option::is_none")]
    work_item_id: Option<String>,
    #[serde(rename = "e", skip_serializing_if = "Option::is_none")]
    error_task_id: Option<String>,
    #[serde(rename = "t")]
    occurred_at: i64,
}

impl PublicationDeliveryReceipt {
    fn from_result(command_action: &str, result: &PublicationDeliveryActionResultView) -> Self {
        Self {
            command_action: command_action.to_string(),
            result: result.result,
            work_item_id: result.work_item_id.clone(),
            error_task_id: result.error_task_id.clone(),
            occurred_at: result.occurred_at,
        }
    }

    fn into_result(self, operation_id: String, delivery_id: String) -> PublicationDeliveryActionResultView {
        PublicationDeliveryActionResultView {
            operation_id,
            delivery_id,
            result: self.result,
            work_item_id: self.work_item_id,
            error_task_id: self.error_task_id,
            occurred_at: self.occurred_at,
            next_action: publication_next_action(self.result),
        }
    }
}

fn publication_operation_audit(
    actor: &AuditActor,
    id: String,
    action: &str,
    delivery_id: &str,
    command_action: &str,
    result: &PublicationDeliveryActionResultView,
) -> Result<entities::AuditLog> {
    let receipt = serde_json::to_string(&PublicationDeliveryReceipt::from_result(command_action, result))
        .map_err(|error| Error::Internal(format!("序列化发布投递幂等回执失败: {error}")))?;
    actor.clone().resource_log_with_id(
        id,
        action,
        "product_publication_delivery",
        delivery_id.to_string(),
        Some(receipt),
    )
}

fn publication_operation_id(actor_id: &str, action: &str, delivery_id: &str, request_id: &str) -> String {
    publication_stable_id(
        "w22op",
        &format!("{actor_id}|{action}|{delivery_id}|{request_id}"),
    )
}

fn publication_stable_id(prefix: &str, identity: &str) -> String {
    format!("{prefix}_{:x}", Sha256::digest(identity.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::{publication_delivery_actions, publication_operation_id, publication_unknown_error};
    use crate::publication::ClassifiedError;
    use entities::integration_ops::ErrorClass;
    use entities::publication::PublicationDeliveryStatus;

    #[test]
    fn publication_operation_identity_is_stable_and_hides_request_id() {
        let first = publication_operation_id("actor", "RETRY", "delivery", "secret-key");
        let second = publication_operation_id("actor", "RETRY", "delivery", "secret-key");
        assert_eq!(first, second);
        assert!(!first.contains("secret-key"));
    }

    #[test]
    fn timeout_is_classified_as_result_unknown_without_losing_code() {
        let error = publication_unknown_error(ClassifiedError {
            class: ErrorClass::TransientFailure,
            code: "MALL_TIMEOUT".to_string(),
            summary: "请求超时".to_string(),
        });
        assert_eq!(error.class, ErrorClass::ResultUnknown);
        assert_eq!(error.code, "MALL_TIMEOUT");
    }

    #[test]
    fn transient_failure_exposes_query_retry_and_escalate() {
        assert_eq!(
            publication_delivery_actions(
                PublicationDeliveryStatus::Failed,
                Some(ErrorClass::TransientFailure),
            ),
            vec!["QUERY_RESULT", "RETRY", "ESCALATE"]
        );
    }
}
