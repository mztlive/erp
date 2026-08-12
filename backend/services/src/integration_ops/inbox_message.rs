//! 入站消息业务：登记、列表、详情、结果回写（processed / failed+错误任务）。
//!
//! 消息层/业务事实层幂等由唯一索引保证，服务层不做「先查后插」重复性判断；
//! 所有业务写入与审计日志在同一 MongoDB 事务原子提交（模板见 `super::transaction`）。

use database::{AccessControlExt, IntegrationOpsExt, NoTransaction, SourceRegistryExt};
use entities::common::time::Instant;
use entities::integration_ops::{
    InboxMessage, InboxMessageData, InboxMessageId, InboxMessageStatus, InboxMessageUpdate,
    IntegrationErrorTask, IntegrationErrorTaskData, IntegrationErrorTaskId,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::SortDir;
use super::validation::ensure_version;
use super::{
    InboxMessageFilter, InboxMessageListParams, InboxMessageListView, InboxMessageView,
    IntegrationOpsService, PageView, RegisterInboxMessageRequest, WriteBackInboxResultRequest,
    WriteBackOutcome,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl IntegrationOpsService {
    /// 登记入站消息（消息层与业务事实层幂等由唯一索引保证）。
    ///
    /// 消息状态由服务端置为 `received`；来源系统存在性经 D01 `SourceRegistryExt`
    /// 跨域只读校验。重复投递（同来源事件或同业务事实键）由唯一索引透出 409。
    ///
    /// # 参数
    /// * `req` - 登记请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建消息的详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源系统不存在
    /// * `ConflictError` - 消息身份或业务事实键重复（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    pub async fn register_inbox_message(
        &self,
        req: RegisterInboxMessageRequest,
        actor: &AuditActor,
    ) -> Result<InboxMessageView> {
        req.validate()?;
        self.db
            .source_systems()
            .find_by_id(req.source_system_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源系统不存在".to_string()))?;

        let message = self.build_inbox_message(req)?;
        let audit =
            actor
                .clone()
                .resource_log("inbox_message.register", "inbox_message", message.base.id.clone())?;
        let stored = message.clone();
        self.run_audited(move |db, session| {
            Box::pin(async move {
                db.inbox_messages().create(&stored, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok(())
            })
        })
        .await?;

        Ok(message.into())
    }

    /// 分页查询入站消息列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传；
    /// 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结，无法命名），
    /// 此处按字段映射为响应视图。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn inbox_message_list(
        &self,
        params: &InboxMessageListParams,
    ) -> Result<PageView<InboxMessageListView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = InboxMessageFilter {
            source_system_id: query.source_system_id,
            message_type: query.message_type,
            status: query.status,
            source_event_id: query.source_event_id,
            received_at_from: query.received_at_from,
            received_at_to: query.received_at_to,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .inbox_messages()
            .search_inbox_messages(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| InboxMessageListView {
                id: row.id,
                source_system_id: row.source_system_id.to_string(),
                source_event_id: row.source_event_id,
                message_type: row.message_type,
                business_fact_key: row.business_fact_key,
                payload_schema_version: row.payload_schema_version,
                status: row.status,
                source_sent_at: row.source_sent_at.map(|at| at.unix_secs()),
                received_at: row.received_at.unix_secs(),
                processed_at: row.processed_at.map(|at| at.unix_secs()),
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

    /// 查询入站消息详情（含规范化内容引用）。
    ///
    /// # 参数
    /// * `id` - 消息 ID
    ///
    /// # 返回
    /// 返回消息详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 消息不存在
    pub async fn inbox_message_detail(&self, id: &str) -> Result<InboxMessageView> {
        let message = self
            .db
            .inbox_messages()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("消息不存在".to_string()))?;
        Ok(message.into())
    }

    /// 回写入站消息处理结果。
    ///
    /// `processed`：状态置为已处理并记录处理完成时间；`failed`：状态置为失败，
    /// 并在同一事务登记错误任务（仓库 `create_error_task_with_message_failure`
    /// 必须收到事务执行器）。消息处理状态与任务登记原子可见。
    ///
    /// # 参数
    /// * `id` - 消息 ID
    /// * `req` - 回写请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回回写后的消息详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 消息不存在
    /// * `ConflictError` - 期望版本不一致，或消息已有进行中的同分类错误任务
    /// * `ValidationError` - 请求体校验失败或失败回写缺少错误分类
    pub async fn write_back_inbox_result(
        &self,
        id: &str,
        req: WriteBackInboxResultRequest,
        actor: &AuditActor,
    ) -> Result<InboxMessageView> {
        req.validate()?;
        let mut message = self
            .db
            .inbox_messages()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("消息不存在".to_string()))?;
        ensure_version(message.base.version, req.version)?;

        let processed_at = Instant::from_unix_secs(req.processed_at.unwrap_or_else(now_secs));
        match req.outcome {
            WriteBackOutcome::Processed => {
                message.update(InboxMessageUpdate {
                    status: Some(InboxMessageStatus::Processed),
                    processed_at: Some(processed_at),
                })?;
                let audit = actor.clone().resource_log(
                    "inbox_message.processed",
                    "inbox_message",
                    message.base.id.clone(),
                )?;
                let stored = self
                    .run_audited(move |db, session| {
                        let mut stored = message;
                        Box::pin(async move {
                            db.inbox_messages().update(&mut stored, session).await?;
                            db.audit_logs().create(&audit, session).await?;
                            Ok(stored)
                        })
                    })
                    .await?;
                Ok(stored.into())
            }
            WriteBackOutcome::Failed => {
                let error_class = req
                    .error_class
                    .ok_or_else(|| Error::ValidationError("标记失败必须提供错误分类".to_string()))?;
                message.update(InboxMessageUpdate {
                    status: Some(InboxMessageStatus::Failed),
                    processed_at: None,
                })?;
                let task = IntegrationErrorTask::new(
                    IntegrationErrorTaskId::new(next_id()),
                    IntegrationErrorTaskData {
                        message_id: Some(InboxMessageId::new(message.base.id.clone())),
                        business_object_id: None,
                        error_class,
                        owner_role: req.owner_role,
                        owner_user_id: req.owner_user_id,
                    },
                )?;
                let audit = actor.clone().resource_log(
                    "inbox_message.failed",
                    "inbox_message",
                    message.base.id.clone(),
                )?;
                let stored = self
                    .run_audited(move |db, session| {
                        let mut stored = message;
                        Box::pin(async move {
                            db.integration_ops()
                                .create_error_task_with_message_failure(&task, &mut stored, session)
                                .await?;
                            db.audit_logs().create(&audit, session).await?;
                            Ok(stored)
                        })
                    })
                    .await?;
                Ok(stored.into())
            }
        }
    }
    // -----------------------------------------------------------------------
    // 私有辅助
    // -----------------------------------------------------------------------

    /// 构造入站消息实体（登记态：`received`，接收时间缺省取当前时间）。
    ///
    /// # 参数
    /// * `req` - 已通过校验的登记请求
    ///
    /// # 返回
    /// 返回新建的入站消息实体。
    ///
    /// # 错误
    /// 实体不变式校验失败时返回错误。
    fn build_inbox_message(&self, req: RegisterInboxMessageRequest) -> Result<InboxMessage> {
        Ok(InboxMessage::new(
            InboxMessageId::new(next_id()),
            InboxMessageData {
                source_system_id: req.source_system_id,
                source_event_id: req.source_event_id,
                message_type: req.message_type,
                business_fact_key: req.business_fact_key,
                payload_schema_version: req.payload_schema_version,
                payload_reference: req.payload_reference,
                status: InboxMessageStatus::Received,
                source_sent_at: req.source_sent_at.map(Instant::from_unix_secs),
                received_at: Instant::from_unix_secs(req.received_at.unwrap_or_else(now_secs)),
                processed_at: None,
            },
        )?)
    }
}

/// 返回当前时间的秒级时间戳。
fn now_secs() -> i64 {
    Instant::now().unix_secs()
}
