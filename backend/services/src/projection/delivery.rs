use database::{AccessControlExt, IntegrationOpsExt, NoTransaction, ProjectionExt, Transactional};
use entities::ids::{
    InboxMessageId, IntegrationErrorTaskId, SalesOrderProjectionDeliveryId, SalesOrderProjectionId,
    SalesOrderProjectionRevisionId,
};
use entities::integration_ops::{
    InboxMessage, InboxMessageData, InboxMessageStatus, InboxMessageUpdate, IntegrationErrorTask,
    IntegrationErrorTaskData, MessageType,
};
use entities::projection::{
    ProjectionDeliveryStatus, SalesOrderProjection, SalesOrderProjectionDelivery,
    SalesOrderProjectionDeliveryData, SalesOrderProjectionRevision, SalesOrderProjectionUpdate,
};
use id_generator::next_id;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::projection::connector::{ClassifiedError, DeliverAck};
use crate::projection::dto::{DeliverProjectionRevisionRequest, ProjectionDeliveryResultView};
use crate::projection::service::ProjectionService;

impl ProjectionService {
    /// 下发投影版本到目标商城（外部 HTTP 调用在事务之外完成）。
    ///
    /// 流程（二期专属，P3 §3/§7）：
    /// 1. 事务 1：落 `inbox_message`（`Received`）+ 审计；
    /// 2. 事务外：经 [`crate::projection::MallConnector`] 尝试下发；
    /// 3. 事务 2：成功 → 下发记录 `Confirmed` + 投影 `current_acked_revision_id`
    ///    推进 + `inbox_message` 置 `Processed`；失败 → 下发记录 `Failed`
    ///    （错误码/摘要）+ `inbox_message` 置 `Failed` + `integration_error_task`。
    ///
    /// 幂等：`(projection_revision_id, target_mall_id)` 唯一索引承接——已确认的
    /// 下发重复提交直接返回既有结果；未确认的重复下发返回 409。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    /// * `revision_no` - 修订序号
    /// * `req` - 下发请求（含幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回下发结果视图。
    ///
    /// # 错误
    /// * `NotFound` - 投影或投影版本不存在
    /// * `ConflictError` - 该版本已在下发中
    pub async fn deliver_revision(
        &self,
        projection_id: &str,
        revision_no: u32,
        req: DeliverProjectionRevisionRequest,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        req.validate()?;
        let mut projection = self
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
        let target_mall_id = projection.target_mall_id.clone();
        let existing = self
            .db
            .sales_order_projection_deliveries()
            .find_delivery_by_revision_and_mall(
                &SalesOrderProjectionRevisionId::new(revision.base.id.clone()),
                &target_mall_id,
                &mut NoTransaction,
            )
            .await?;
        if let Some(existing) = existing {
            return self.idempotent_delivery_result(existing, projection);
        }

        let message = InboxMessage::new(
            InboxMessageId::new(next_id()),
            InboxMessageData {
                source_system_id: target_mall_id.clone(),
                source_event_id: format!(
                    "projection_delivery:{}:{}:{}",
                    revision.base.id, target_mall_id, req.idempotency_key
                ),
                message_type: MessageType::MallActionRequest,
                business_fact_key: format!("projection_delivery:{}:{}", revision.base.id, target_mall_id),
                payload_schema_version: "v1".to_string(),
                payload_reference: Some(revision.content_hash.clone()),
                status: InboxMessageStatus::Received,
                source_sent_at: None,
                received_at: entities::common::time::Instant::now(),
                processed_at: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "sales_order_projection_delivery.deliver",
            "sales_order_projection_delivery",
            revision.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let message_tx = message.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.inbox_messages().create(&message_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        let now = entities::common::time::Instant::now();
        match self
            .connector
            .deliver_projection(&revision, &target_mall_id)
            .await
        {
            Ok(ack) => {
                self.settle_delivery_success(&mut projection, revision, message, now, ack, actor)
                    .await
            }
            Err(error) => {
                self.settle_delivery_failure(&mut projection, revision, message, now, error, actor)
                    .await
            }
        }
    }

    /// 已存在下发记录的幂等返回（已确认直接返回既有结果，未确认返回冲突）。
    ///
    /// # 参数
    /// * `existing` - 既有下发记录
    /// * `projection` - 所属投影
    ///
    /// # 返回
    /// 返回下发结果视图。
    ///
    /// # 错误
    /// 下发尚未确认时返回 `ConflictError`。
    fn idempotent_delivery_result(
        &self,
        existing: SalesOrderProjectionDelivery,
        projection: SalesOrderProjection,
    ) -> Result<ProjectionDeliveryResultView> {
        if existing.status != ProjectionDeliveryStatus::Confirmed {
            return Err(Error::ConflictError(
                "该版本已在下发中，请查询下发状态".to_string(),
            ));
        }
        Ok(ProjectionDeliveryResultView {
            delivery_id: existing.base.id,
            delivery_status: existing.status,
            inbox_message_id: String::new(),
            error_task_id: None,
            mall_execution_baseline: existing.mall_execution_baseline,
            projection_version: projection.base.version,
        })
    }

    /// 把成功下发落库（事务 2：下发记录 + 投影确认版本推进 + 消息已处理）。
    ///
    /// # 参数
    /// * `projection` - 待推进的投影实体
    /// * `revision` - 已下发的投影版本
    /// * `message` - 待置 `Processed` 的消息
    /// * `at` - 下发时间
    /// * `ack` - 商城确认
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回下发结果视图。
    ///
    /// # 错误
    /// 乐观锁冲突或 MongoDB 写入失败时返回错误。
    async fn settle_delivery_success(
        &self,
        projection: &mut SalesOrderProjection,
        revision: SalesOrderProjectionRevision,
        mut message: InboxMessage,
        at: entities::common::time::Instant,
        ack: DeliverAck,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        let delivery = SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new(next_id()),
            SalesOrderProjectionDeliveryData {
                projection_revision_id: revision.base.id.clone().into(),
                target_mall_id: projection.target_mall_id.clone(),
                status: ProjectionDeliveryStatus::Confirmed,
                attempt_count: 1,
                next_attempt_at: None,
                mall_ack_at: Some(at),
                mall_execution_baseline: Some(ack.mall_execution_baseline.clone()),
                error_code: None,
                error_summary: None,
            },
        )?;
        projection.update(SalesOrderProjectionUpdate {
            current_acked_revision_id: Some(revision.base.id.clone().into()),
        })?;
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Processed),
            processed_at: Some(at),
        })?;
        let audit = actor.clone().resource_log(
            "sales_order_projection_delivery.acked",
            "sales_order_projection_delivery",
            delivery.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        let projection_version = projection.base.version;
        let inbox_id = message.base.id.clone();
        let mut projection_tx = projection.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_order_projection_deliveries()
                        .create(&delivery, session)
                        .await?;
                    db.sales_order_projections()
                        .update(&mut projection_tx, session)
                        .await?;
                    db.inbox_messages().update(&mut message, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ProjectionDeliveryResultView {
            delivery_id,
            delivery_status: ProjectionDeliveryStatus::Confirmed,
            inbox_message_id: inbox_id,
            error_task_id: None,
            mall_execution_baseline: Some(ack.mall_execution_baseline),
            projection_version,
        })
    }

    /// 把失败下发落库（事务 2：下发记录失败 + 消息失败 + 错误任务）。
    ///
    /// # 参数
    /// * `projection` - 所属投影实体（确认版本不推进）
    /// * `revision` - 下发失败的投影版本
    /// * `message` - 待置 `Failed` 的消息
    /// * `at` - 下发时间
    /// * `error` - 分类错误
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回下发结果视图（含错误任务 ID）。
    ///
    /// # 错误
    /// 乐观锁冲突或 MongoDB 写入失败时返回错误。
    async fn settle_delivery_failure(
        &self,
        projection: &mut SalesOrderProjection,
        revision: SalesOrderProjectionRevision,
        mut message: InboxMessage,
        at: entities::common::time::Instant,
        error: ClassifiedError,
        actor: &AuditActor,
    ) -> Result<ProjectionDeliveryResultView> {
        let delivery = SalesOrderProjectionDelivery::new(
            SalesOrderProjectionDeliveryId::new(next_id()),
            SalesOrderProjectionDeliveryData {
                projection_revision_id: revision.base.id.clone().into(),
                target_mall_id: projection.target_mall_id.clone(),
                status: ProjectionDeliveryStatus::Failed,
                attempt_count: 1,
                next_attempt_at: None,
                mall_ack_at: None,
                mall_execution_baseline: None,
                error_code: Some(error.code.clone()),
                error_summary: Some(error.summary.clone()),
            },
        )?;
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Failed),
            processed_at: Some(at),
        })?;
        let task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new(next_id()),
            IntegrationErrorTaskData {
                message_id: Some(message.base.id.clone().into()),
                business_object_id: None,
                error_class: error.class,
                owner_role: Some("integration_ops".to_string()),
                owner_user_id: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "sales_order_projection_delivery.failed",
            "sales_order_projection_delivery",
            delivery.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delivery_id = delivery.base.id.clone();
        let task_id = task.base.id.clone();
        let inbox_id = message.base.id.clone();
        let projection_version = projection.base.version;
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_order_projection_deliveries()
                        .create(&delivery, session)
                        .await?;
                    db.integration_ops()
                        .create_error_task_with_message_failure(&task, &mut message, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ProjectionDeliveryResultView {
            delivery_id,
            delivery_status: ProjectionDeliveryStatus::Failed,
            inbox_message_id: inbox_id,
            error_task_id: Some(task_id),
            mall_execution_baseline: None,
            projection_version,
        })
    }
}
