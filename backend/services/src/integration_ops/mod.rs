//! 域 D34 `integration_ops` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 单集合、无跨步骤原子性要求的查询一律 `&mut NoTransaction`；
//! - 所有业务写入均与审计日志跨集合（`audit_logs` 属 D06），统一
//!   `database::Transactional::with_transaction` 原子提交（D01 样板写法）；
//!   `run_audited_transaction` 模板位于 `iam` 私有子树，本域按 P0 样板直接编排。
//! - 跨域协作只调对方域 Repository（D01 `SourceRegistryExt::source_systems`），
//!   禁止 Service 依赖 Service（conventions §6.2）。
//!
//! 集成治理核心动作（W29 §7/§8.2，数据模型 §6.21、§7.7）：
//! - `inbox_message`：登记（消息层/业务事实层幂等由唯一索引保证，服务层不做
//!   「先查后插」重复性判断）、结果回写（processed / failed+错误任务）；
//! - `integration_error_task`：查询原结果（QUERY）→ 明确无结果才开放 REPLAY；
//!   REPLAY 永不接受客户端原幂等键，服务端锁定关联消息的业务事实键；暂挂/跳过
//!   保留在队列；RESOLVE/CLOSE/TRANSFER 终结当前处理；已解决/已关闭是终态；
//! - `reconciliation_difference`：查询/人工处理（只追加处理记录）/解决（固定
//!   原因枚举 + 受控证据，派生终态）。

use std::{future::Future, pin::Pin};

use database::{AccessControlExt, IntegrationOpsExt, NoTransaction, SourceRegistryExt, Transactional};
use entities::common::time::Instant;
use entities::integration_ops::{
    ErrorClass, ErrorTaskStatus, InboxMessage, InboxMessageData, InboxMessageId, InboxMessageStatus,
    InboxMessageUpdate, IntegrationErrorTask, IntegrationErrorTaskData, IntegrationErrorTaskId,
    IntegrationErrorTaskUpdate, ReconciliationDifference, ReconciliationDifferenceData,
    ReconciliationDifferenceId, ReconciliationDifferenceResolution, ReconciliationDifferenceResolutionData,
    ReconciliationDifferenceResolutionId, ResolutionAction, ResolutionType, ResultingStatus,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

use self::dto::SortDir;
pub use self::dto::{
    CloseErrorTaskRequest, CloseReason, CreateDifferenceRequest, CreateErrorTaskRequest,
    DifferenceActionView, DifferenceConclusion, DifferenceDetailView, DifferenceListParams,
    DifferenceProcessAction, DifferenceReasonCode, DifferenceView, ErrorTaskDetailView, ErrorTaskListParams,
    ErrorTaskView, HoldErrorTaskRequest, HoldKind, InboxMessageListParams, InboxMessageListView,
    InboxMessageView, PageView, ProcessDifferenceRequest, QueryOriginalResultRequest, QueryOutcome,
    RegisterInboxMessageRequest, ReplayOriginalRequest, ReplayResultView, ResolutionView,
    ResolveDifferenceRequest, ResolveErrorTaskRequest, TransferErrorTaskRequest, WriteBackInboxResultRequest,
    WriteBackOutcome,
};

/// 入站消息列表筛选条件类型（经 `IntegrationOpsExt` 关联类型跨 crate 可达）。
type InboxMessageFilter = <mongodb::Database as IntegrationOpsExt>::InboxMessageFilter;
/// 错误任务列表筛选条件类型。
type ErrorTaskFilter = <mongodb::Database as IntegrationOpsExt>::IntegrationErrorTaskFilter;
/// 对账差异列表筛选条件类型。
type DifferenceFilter = <mongodb::Database as IntegrationOpsExt>::ReconciliationDifferenceFilter;

/// 集成治理服务。
///
/// 提供 inbox_message 登记/回写、integration_error_task 人工处理与终态动作、
/// reconciliation_difference 查询/人工处理/解决的编排。
pub struct IntegrationOpsService {
    db: Database,
}

impl IntegrationOpsService {
    /// 创建集成治理服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

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

    /// 登记集成错误任务（消息类失败必填消息，业务对象类失败必填业务对象）。
    ///
    /// 同一消息与错误分类只允许一个进行中任务（部分唯一索引），重复登记透出 409。
    ///
    /// # 参数
    /// * `req` - 登记请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建任务的视图。
    ///
    /// # 错误
    /// * `NotFound` - 关联消息不存在
    /// * `ConflictError` - 同消息同分类的进行中任务已存在
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_error_task(
        &self,
        req: CreateErrorTaskRequest,
        actor: &AuditActor,
    ) -> Result<ErrorTaskView> {
        req.validate()?;
        if let Some(message_id) = &req.message_id {
            self.db
                .inbox_messages()
                .find_by_id(message_id.as_ref(), &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("关联消息不存在".to_string()))?;
        }
        let task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new(next_id()),
            IntegrationErrorTaskData {
                message_id: req.message_id,
                business_object_id: req.business_object_id,
                error_class: req.error_class,
                owner_role: req.owner_role,
                owner_user_id: req.owner_user_id,
            },
        )?;
        let audit = actor.clone().resource_log(
            "integration_error_task.create",
            "integration_error_task",
            task.base.id.clone(),
        )?;
        let stored = task.clone();
        self.run_audited(move |db, session| {
            Box::pin(async move {
                db.integration_error_tasks().create(&stored, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok(())
            })
        })
        .await?;

        Ok(task.into())
    }

    /// 分页查询集成错误任务列表。
    ///
    /// 排序字段白名单在 Service 层校验；解决证据文本不进入列表投影；
    /// 投影行类型按字段映射为响应视图（仓储私有子树不可命名）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn error_task_list(&self, params: &ErrorTaskListParams) -> Result<PageView<ErrorTaskView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ErrorTaskFilter {
            message_id: query.message_id,
            business_object_id: query.business_object_id,
            error_class: query.error_class,
            status: query.status,
            owner_role: query.owner_role,
            owner_user_id: query.owner_user_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .integration_error_tasks()
            .search_error_tasks(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ErrorTaskView {
                id: row.id,
                message_id: row.message_id.map(|id| id.to_string()),
                business_object_id: row.business_object_id,
                error_class: row.error_class,
                status: row.status,
                owner_role: row.owner_role,
                owner_user_id: row.owner_user_id,
                attempt_count: row.attempt_count,
                last_attempt_at: row.last_attempt_at.map(|at| at.unix_secs()),
                last_attempt_summary: row.last_attempt_summary,
                resolution_type: row.resolution_type,
                resolved_at: row.resolved_at.map(|at| at.unix_secs()),
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

    /// 查询集成错误任务详情（含解决/关闭证据文本）。
    ///
    /// # 参数
    /// * `id` - 任务 ID
    ///
    /// # 返回
    /// 返回任务详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 任务不存在
    pub async fn error_task_detail(&self, id: &str) -> Result<ErrorTaskDetailView> {
        let task = self
            .db
            .integration_error_tasks()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("任务不存在".to_string()))?;
        let resolution = task.resolution.clone();
        Ok(ErrorTaskDetailView {
            task: task.into(),
            resolution,
        })
    }

    /// 查询原结果（QUERY_ORIGINAL_RESULT，非终结动作）。
    ///
    /// 查询结果写入最近尝试摘要（脱敏）；只有 `no_result_confirmed` 才可能开放
    /// REPLAY（§7.7）。任务保持非终结状态。
    ///
    /// # 参数
    /// * `id` - 任务 ID
    /// * `req` - 查询请求（含期望版本与结果）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回查询后的任务视图。
    ///
    /// # 错误
    /// * `NotFound` - 任务不存在
    /// * `ConflictError` - 期望版本不一致或任务已终结
    pub async fn query_error_task_result(
        &self,
        id: &str,
        req: QueryOriginalResultRequest,
        actor: &AuditActor,
    ) -> Result<ErrorTaskView> {
        req.validate()?;
        let mut task = self.load_active_task(id, req.version).await?;
        task.record_attempt(Instant::now(), Some(req.outcome.summary_marker().to_string()))?;
        let audit = actor.clone().resource_log(
            "integration_error_task.query",
            "integration_error_task",
            task.base.id.clone(),
        )?;
        let updated = self
            .run_audited(move |db, session| {
                let mut task = task;
                Box::pin(async move {
                    db.integration_error_tasks().update(&mut task, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(task)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 重放原动作（REPLAY_ORIGINAL，非终结动作）。
    ///
    /// 幂等与安全契约（W29 §8.2、§12.1）：
    /// - 客户端**不得**传入原幂等键（DTO 无该字段且拒绝未知字段）；服务端锁定
    ///   关联消息的业务事实键并自行沿用，只返回脱敏摘要；
    /// - 结果未知任务必须先查询且明确无结果，否则拒绝；
    /// - 能力不足/映射错误/业务拒绝/鉴权签名分类不允许重放；
    /// - 已按原键重放后重复提交被拒（原键锁定，复用同一原事实）。
    ///
    /// # 参数
    /// * `id` - 任务 ID
    /// * `req` - 重放请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回重放结果视图（服务端锁定的原键摘要 + 锁定标识）。
    ///
    /// # 错误
    /// * `NotFound` - 任务或原消息不存在
    /// * `ConflictError` - 期望版本不一致、任务已终结或已重放
    /// * `ValidationError` - 重放前置条件不满足（未查询/查询未确认/分类不允许）
    pub async fn replay_error_task(
        &self,
        id: &str,
        req: ReplayOriginalRequest,
        actor: &AuditActor,
    ) -> Result<ReplayResultView> {
        req.validate()?;
        let mut task = self.load_active_task(id, req.version).await?;
        self.ensure_replay_allowed(&task)?;
        let message_id = task
            .message_id
            .clone()
            .ok_or_else(|| Error::ValidationError("该任务未关联原消息，无法锁定原幂等键".to_string()))?;
        let message = self
            .db
            .inbox_messages()
            .find_by_id(message_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原消息不存在".to_string()))?;
        let key_summary = mask_key(&message.business_fact_key);
        task.record_attempt(
            Instant::now(),
            Some(format!("replay_accepted(original_key_summary={key_summary})")),
        )?;
        let audit = actor.clone().resource_log(
            "integration_error_task.replay",
            "integration_error_task",
            task.base.id.clone(),
        )?;
        let updated = self
            .run_audited(move |db, session| {
                let mut task = task;
                Box::pin(async move {
                    db.integration_error_tasks().update(&mut task, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(task)
                })
            })
            .await?;

        Ok(ReplayResultView {
            task_id: updated.base.id.clone(),
            original_action_idempotency_key_summary: key_summary,
            original_action_idempotency_key_locked: true,
            replay_accepted: true,
            task_status: updated.status,
            attempt_count: updated.attempt_count,
            task_version: updated.base.version,
        })
    }

    /// 暂挂/跳过当前任务（DEFER/SKIP，非终结动作）。
    ///
    /// 只追加尝试摘要与审计，任务状态不变、保留在开放队列（W29 §8.2）。
    ///
    /// # 参数
    /// * `id` - 任务 ID
    /// * `req` - 暂挂/跳过请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回任务视图（状态未变，仍在队列）。
    ///
    /// # 错误
    /// * `NotFound` - 任务不存在
    /// * `ConflictError` - 期望版本不一致或任务已终结
    pub async fn hold_error_task(
        &self,
        id: &str,
        req: HoldErrorTaskRequest,
        actor: &AuditActor,
    ) -> Result<ErrorTaskView> {
        req.validate()?;
        let mut task = self.load_active_task(id, req.version).await?;
        task.record_attempt(Instant::now(), Some(req.kind.summary_marker().to_string()))?;
        let action = if req.kind == HoldKind::Defer {
            "defer"
        } else {
            "skip"
        };
        let audit = actor.clone().resource_log(
            &format!("integration_error_task.{action}"),
            "integration_error_task",
            task.base.id.clone(),
        )?;
        let updated = self
            .run_audited(move |db, session| {
                let mut task = task;
                Box::pin(async move {
                    db.integration_error_tasks().update(&mut task, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(task)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 转交任务（TRANSFER）。
    ///
    /// 只更新责任人与转交审计，任务状态不变（转交不是解决）；同一事务提交。
    ///
    /// # 参数
    /// * `id` - 任务 ID
    /// * `req` - 转交请求（含期望版本与新责任人）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回转交后的任务视图。
    ///
    /// # 错误
    /// * `NotFound` - 任务不存在
    /// * `ConflictError` - 期望版本不一致或任务已终结
    /// * `ValidationError` - 新责任人与新责任角色都未提供
    pub async fn transfer_error_task(
        &self,
        id: &str,
        req: TransferErrorTaskRequest,
        actor: &AuditActor,
    ) -> Result<ErrorTaskView> {
        req.validate()?;
        if req.owner_role.is_none() && req.owner_user_id.is_none() {
            return Err(Error::ValidationError("必须提供新的责任角色或责任人".to_string()));
        }
        let mut task = self.load_active_task(id, req.version).await?;
        task.update(IntegrationErrorTaskUpdate {
            owner_role: req.owner_role,
            owner_user_id: req.owner_user_id,
        })?;
        let audit = actor.clone().resource_log(
            "integration_error_task.transfer",
            "integration_error_task",
            task.base.id.clone(),
        )?;
        let updated = self
            .run_audited(move |db, session| {
                let mut task = task;
                Box::pin(async move {
                    db.integration_error_tasks().update(&mut task, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(task)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 解决任务（RESOLVE，终态：已解决）。
    ///
    /// 必须取得可验证终态或形成复核事实：解决方式非「关闭」且提供终态证据
    /// （实体校验）；任务与审计在同一事务提交。
    ///
    /// # 参数
    /// * `id` - 任务 ID
    /// * `req` - 解决请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回已解决的任务视图。
    ///
    /// # 错误
    /// * `NotFound` - 任务不存在
    /// * `ConflictError` - 期望版本不一致或任务已终结
    /// * `Unprocessable` - 解决方式/证据不满足终态要求
    pub async fn resolve_error_task(
        &self,
        id: &str,
        req: ResolveErrorTaskRequest,
        actor: &AuditActor,
    ) -> Result<ErrorTaskView> {
        req.validate()?;
        let mut task = self.load_active_task(id, req.version).await?;
        task.transition(
            ErrorTaskStatus::Resolved,
            Some(req.resolution_type),
            Some(req.resolution),
            Instant::now(),
        )?;
        let audit = actor.clone().resource_log(
            "integration_error_task.resolve",
            "integration_error_task",
            task.base.id.clone(),
        )?;
        let updated = self
            .run_audited(move |db, session| {
                let mut task = task;
                Box::pin(async move {
                    db.integration_error_tasks().update(&mut task, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(task)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 关闭任务（CLOSE，终态：已关闭）。
    ///
    /// 重复关闭必须关联替代任务（存在性校验）；误派关闭提供终态证据。结果未知
    /// 任务不得以通用关闭退出（实体校验）。任务与审计在同一事务提交。
    ///
    /// # 参数
    /// * `id` - 任务 ID
    /// * `req` - 关闭请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回已关闭的任务视图。
    ///
    /// # 错误
    /// * `NotFound` - 任务或替代任务不存在
    /// * `ConflictError` - 期望版本不一致或任务已终结
    /// * `Unprocessable` - 关闭条件不满足（结果未知/重复缺替代任务）
    pub async fn close_error_task(
        &self,
        id: &str,
        req: CloseErrorTaskRequest,
        actor: &AuditActor,
    ) -> Result<ErrorTaskView> {
        req.validate()?;
        let mut task = self.load_active_task(id, req.version).await?;
        if req.reason == CloseReason::Duplicate {
            let replacement = req
                .replacement_task_id
                .as_ref()
                .ok_or_else(|| Error::ValidationError("关闭重复任务必须提供替代任务".to_string()))?;
            self.db
                .integration_error_tasks()
                .find_by_id(replacement.as_ref(), &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("替代任务不存在".to_string()))?;
        }
        task.transition(
            ErrorTaskStatus::Closed,
            Some(ResolutionType::Close),
            Some(req.resolution),
            Instant::now(),
        )?;
        let audit = actor.clone().resource_log(
            "integration_error_task.close",
            "integration_error_task",
            task.base.id.clone(),
        )?;
        let updated = self
            .run_audited(move |db, session| {
                let mut task = task;
                Box::pin(async move {
                    db.integration_error_tasks().update(&mut task, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok(task)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 登记对账差异（正式差异事实，创建后不可修改）。
    ///
    /// 对象唯一键幂等由唯一索引保证，重复登记透出 409（对账任务不直接修改正式事实）。
    ///
    /// # 参数
    /// * `req` - 登记请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建差异的视图。
    ///
    /// # 错误
    /// * `ConflictError` - 同对象同分类差异已存在
    /// * `ValidationError` - 请求体校验失败或两侧证据都未提供
    pub async fn create_difference(
        &self,
        req: CreateDifferenceRequest,
        actor: &AuditActor,
    ) -> Result<DifferenceView> {
        req.validate()?;
        if req.left_fact_reference.is_none() && req.right_fact_reference.is_none() {
            return Err(Error::ValidationError(
                "差异必须至少提供一侧不可变证据引用".to_string(),
            ));
        }
        let difference = ReconciliationDifference::new(
            ReconciliationDifferenceId::new(next_id()),
            ReconciliationDifferenceData {
                business_object_type: req.business_object_type,
                business_object_id: req.business_object_id,
                difference_type: req.difference_type,
                left_fact_reference: req.left_fact_reference,
                right_fact_reference: req.right_fact_reference,
            },
        )?;
        let audit = actor.clone().resource_log(
            "reconciliation_difference.create",
            "reconciliation_difference",
            difference.base.id.clone(),
        )?;
        let stored = difference.clone();
        self.run_audited(move |db, session| {
            Box::pin(async move {
                db.reconciliation_differences().create(&stored, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok(())
            })
        })
        .await?;

        Ok(difference.into())
    }

    /// 分页查询对账差异列表（`status` 由最新处理记录派生）。
    ///
    /// 每行派生状态按差异 ID 取最新处理记录（当前仓储无批量方法，页内逐行查询，
    /// 页大小 ≤ 100，走 `(reconciliation_difference_id, resolution_no)` 唯一索引）；
    /// 投影行类型按字段映射为响应视图（仓储私有子树不可命名）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn difference_list(&self, params: &DifferenceListParams) -> Result<PageView<DifferenceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = DifferenceFilter {
            business_object_type: query.business_object_type,
            business_object_id: query.business_object_id,
            difference_type: query.difference_type,
            created_at_from: query.created_at_from,
            created_at_to: query.created_at_to,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .reconciliation_differences()
            .search_differences(&filter, &mut NoTransaction)
            .await?;
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            let status = self.derived_difference_status(&row.id).await?;
            items.push(DifferenceView {
                id: row.id,
                business_object_type: row.business_object_type,
                business_object_id: row.business_object_id,
                difference_type: row.difference_type,
                left_fact_reference: row.left_fact_reference,
                right_fact_reference: row.right_fact_reference,
                status,
                version: row.version,
                created_at: row.created_at,
            });
        }

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询对账差异详情（含处理记录时间线）。
    ///
    /// # 参数
    /// * `id` - 差异 ID
    ///
    /// # 返回
    /// 返回差异详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 差异不存在
    pub async fn difference_detail(&self, id: &str) -> Result<DifferenceDetailView> {
        let difference = self
            .db
            .reconciliation_differences()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("差异不存在".to_string()))?;
        let difference_id = ReconciliationDifferenceId::new(difference.base.id.clone());
        let status = self.derived_difference_status(&difference.base.id).await?;
        let history = self
            .db
            .reconciliation_difference_resolutions()
            .search_resolutions(&difference_id, &mut NoTransaction)
            .await?;
        let mut view: DifferenceView = difference.into();
        view.status = status;
        let resolutions = history
            .into_iter()
            .map(|row| ResolutionView {
                id: row.id,
                resolution_no: row.resolution_no,
                resolution_action: row.resolution_action,
                resulting_status: row.resulting_status,
                evidence_reference: row.evidence_reference,
                replacement_task_id: row.replacement_task_id.map(|id| id.to_string()),
                handled_by: row.handled_by,
                handled_at: row.handled_at.unix_secs(),
            })
            .collect();

        Ok(DifferenceDetailView {
            difference: view,
            resolutions,
        })
    }

    /// 人工处理对账差异（非终结动作，只追加处理记录）。
    ///
    /// 领取仅允许作为首条处理记录；处理中/补充证据追加处理记录并派生处理中状态。
    /// 处理记录不可更新或删除；差异已终结时拒绝继续处理。并发保护以处理记录
    /// 序号为乐观锁令牌：`version` 与最新序号不一致返回 409。
    ///
    /// # 参数
    /// * `id` - 差异 ID
    /// * `req` - 处理请求（含期望的最新处理序号）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回追加后的处理记录视图与最新处理序号。
    ///
    /// # 错误
    /// * `NotFound` - 差异不存在
    /// * `ConflictError` - 期望序号不一致或差异已终结
    /// * `ValidationError` - 领取动作在已有处理记录时被拒绝
    pub async fn process_difference(
        &self,
        id: &str,
        req: ProcessDifferenceRequest,
        actor: &AuditActor,
    ) -> Result<DifferenceActionView> {
        req.validate()?;
        let difference = self.load_active_difference(id, req.version).await?;
        let action = match req.action {
            DifferenceProcessAction::Claim => ResolutionAction::Claim,
            DifferenceProcessAction::Processing | DifferenceProcessAction::AddEvidence => {
                ResolutionAction::Processing
            }
        };
        let resolution = self
            .append_difference_resolution(&difference, action, req.evidence_reference, actor)
            .await?;
        let audit = actor.clone().resource_log(
            "reconciliation_difference.process",
            "reconciliation_difference",
            difference.base.id,
        )?;
        let stored = resolution.clone();
        self.run_audited(move |db, session| {
            Box::pin(async move {
                db.reconciliation_difference_resolutions()
                    .create(&stored, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok(())
            })
        })
        .await?;

        let resolution_no = resolution.resolution_no;
        Ok(DifferenceActionView {
            resolution: resolution.into(),
            version: u64::from(resolution_no),
        })
    }

    /// 解决对账差异（终态结论，只追加处理记录）。
    ///
    /// 结论必须是固定原因枚举（W29 §7，禁止自由文本）且提供受控证据；原因代码
    /// 与证据引用合并写入处理记录的证据引用。确认无误/确认有效差异均派生已解决，
    /// 差异已终结时拒绝再次解决。
    ///
    /// # 参数
    /// * `id` - 差异 ID
    /// * `req` - 解决请求（含期望的最新处理序号）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回追加的处理记录视图与最新处理序号。
    ///
    /// # 错误
    /// * `NotFound` - 差异不存在
    /// * `ConflictError` - 期望序号不一致或差异已终结
    /// * `ValidationError` - 受控证据为空
    pub async fn resolve_difference(
        &self,
        id: &str,
        req: ResolveDifferenceRequest,
        actor: &AuditActor,
    ) -> Result<DifferenceActionView> {
        req.validate()?;
        let difference = self.load_active_difference(id, req.version).await?;
        let action = match req.conclusion {
            DifferenceConclusion::ConfirmNoError => ResolutionAction::Confirmed,
            DifferenceConclusion::ConfirmValidDifference => ResolutionAction::Resolved,
        };
        let evidence = format!(
            "reason_code={};{}",
            req.reason_code.as_str(),
            req.evidence_reference
        );
        let resolution = self
            .append_difference_resolution(&difference, action, Some(evidence), actor)
            .await?;
        let audit = actor.clone().resource_log(
            "reconciliation_difference.resolve",
            "reconciliation_difference",
            difference.base.id,
        )?;
        let stored = resolution.clone();
        self.run_audited(move |db, session| {
            Box::pin(async move {
                db.reconciliation_difference_resolutions()
                    .create(&stored, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok(())
            })
        })
        .await?;

        let resolution_no = resolution.resolution_no;
        Ok(DifferenceActionView {
            resolution: resolution.into(),
            version: u64::from(resolution_no),
        })
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

    /// 按 ID 加载任务并校验期望版本与活跃状态。
    ///
    /// # 参数
    /// * `id` - 任务 ID
    /// * `expected_version` - 请求携带的期望版本
    ///
    /// # 返回
    /// 返回未终结的任务实体。
    ///
    /// # 错误
    /// * `NotFound` - 任务不存在
    /// * `ConflictError` - 期望版本不一致或任务已终结
    async fn load_active_task(&self, id: &str, expected_version: u64) -> Result<IntegrationErrorTask> {
        let task = self
            .db
            .integration_error_tasks()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("任务不存在".to_string()))?;
        ensure_version(task.base.version, expected_version)?;
        if task.is_terminal() {
            return Err(Error::ConflictError("任务已终结，不允许再操作".to_string()));
        }
        Ok(task)
    }

    /// 校验 REPLAY 前置条件（分类、查询确认、重复重放）。
    ///
    /// # 参数
    /// * `task` - 已加载的活跃任务
    ///
    /// # 返回
    /// 校验通过返回 `Ok(())`。
    ///
    /// # 错误
    /// * `ValidationError` - 分类不允许重放，或结果未知任务未查询/未确认无结果
    /// * `ConflictError` - 已按原任务号重放（原键锁定）
    fn ensure_replay_allowed(&self, task: &IntegrationErrorTask) -> Result<()> {
        if task
            .last_attempt_summary
            .as_deref()
            .is_some_and(|summary| summary.starts_with("replay_accepted"))
        {
            return Err(Error::ConflictError(
                "已按原任务号重新提交，等待处理结果，请勿重复提交".to_string(),
            ));
        }
        match task.error_class {
            ErrorClass::CapabilityGap
            | ErrorClass::MappingError
            | ErrorClass::BusinessRejected
            | ErrorClass::AuthSignature => {
                return Err(Error::ValidationError(
                    "该错误分类不允许重放，请走修复或补偿路径".to_string(),
                ));
            }
            ErrorClass::ResultUnknown => {
                let confirmed = task
                    .last_attempt_summary
                    .as_deref()
                    .is_some_and(|summary| summary.starts_with("query_outcome=no_result_confirmed"));
                if !confirmed {
                    return Err(Error::ValidationError(
                        "结果未知任务必须先查询原结果；仅确认无结果且服务端判定安全后才可重新提交"
                            .to_string(),
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// 按 ID 加载差异并校验期望处理序号与活跃状态。
    ///
    /// 差异本身不可变（锁版本永不变化），以最新处理记录序号为乐观锁令牌：
    /// `expected_version` 与最新序号不一致（含并发追加后序号前移）返回 409。
    ///
    /// # 参数
    /// * `id` - 差异 ID
    /// * `expected_version` - 期望的最新处理序号（0 表示无处理记录）
    ///
    /// # 返回
    /// 返回未终结的差异实体。
    ///
    /// # 错误
    /// * `NotFound` - 差异不存在
    /// * `ConflictError` - 期望序号不一致或差异已终结
    async fn load_active_difference(
        &self,
        id: &str,
        expected_version: Option<u64>,
    ) -> Result<ReconciliationDifference> {
        let difference = self
            .db
            .reconciliation_differences()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("差异不存在".to_string()))?;
        let latest = self
            .db
            .reconciliation_difference_resolutions()
            .find_latest_by_difference(
                &ReconciliationDifferenceId::new(difference.base.id.clone()),
                &mut NoTransaction,
            )
            .await?;
        let latest_no = u64::from(latest.as_ref().map(|record| record.resolution_no).unwrap_or(0));
        if expected_version.unwrap_or(0) != latest_no {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        if latest.is_some_and(|record| {
            matches!(
                record.resulting_status,
                ResultingStatus::Resolved | ResultingStatus::Closed
            )
        }) {
            return Err(Error::ConflictError("差异已终结，不允许再操作".to_string()));
        }
        Ok(difference)
    }

    /// 派生差异当前处理状态（由最后一条处理记录派生，§6.21）。
    ///
    /// # 参数
    /// * `id` - 差异 ID
    ///
    /// # 返回
    /// 返回派生状态；尚无处理记录时返回 `None`。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn derived_difference_status(&self, id: &str) -> Result<Option<ResultingStatus>> {
        let latest = self
            .db
            .reconciliation_difference_resolutions()
            .find_latest_by_difference(
                &ReconciliationDifferenceId::new(id.to_string()),
                &mut NoTransaction,
            )
            .await?;
        Ok(latest.map(|resolution| resolution.resulting_status))
    }

    /// 追加一条差异处理记录（领取/处理中/终结动作）。
    ///
    /// 领取仅在无既有处理记录时允许；处理序号取最新序号 + 1（首条从 1 起）。
    ///
    /// # 参数
    /// * `difference` - 目标差异
    /// * `action` - 解决动作
    /// * `evidence_reference` - 终态证据引用（追加式）
    /// * `actor` - 处理人
    ///
    /// # 返回
    /// 返回构造完成的处理记录实体。
    ///
    /// # 错误
    /// * `ValidationError` - 领取动作在已有处理记录时被拒绝
    async fn append_difference_resolution(
        &self,
        difference: &ReconciliationDifference,
        action: ResolutionAction,
        evidence_reference: Option<String>,
        actor: &AuditActor,
    ) -> Result<ReconciliationDifferenceResolution> {
        let difference_id = ReconciliationDifferenceId::new(difference.base.id.clone());
        let latest = self
            .db
            .reconciliation_difference_resolutions()
            .find_latest_by_difference(&difference_id, &mut NoTransaction)
            .await?;
        if action == ResolutionAction::Claim && latest.is_some() {
            return Err(Error::ValidationError("领取仅允许作为首条处理记录".to_string()));
        }
        let resolution_no = latest.map(|record| record.resolution_no + 1).unwrap_or(1);
        Ok(ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new(next_id()),
            ReconciliationDifferenceResolutionData {
                reconciliation_difference_id: difference_id,
                resolution_no,
                resolution_action: action,
                resulting_status: action.derived_status(),
                evidence_reference,
                replacement_task_id: None,
                handled_by: actor.id().to_string(),
                handled_at: Instant::now(),
            },
        )?)
    }

    /// 在事务中执行业务写入与审计日志写入（跨集合原子提交，D01 样板写法）。
    ///
    /// # 参数
    /// * `f` - 事务闭包（业务写入 + 审计写入；禁止外部 HTTP/文件 IO）
    ///
    /// # 返回
    /// 返回事务结果（闭包返回值）。
    ///
    /// # 错误
    /// 事务内错误透出；提交结果未知映射为 `OutcomeUnknown`。
    async fn run_audited<R, F>(&self, f: F) -> Result<R>
    where
        R: Send,
        F: for<'a> FnOnce(
                &'a mongodb::Database,
                &'a mut mongodb::ClientSession,
            ) -> Pin<Box<dyn Future<Output = Result<R>> + Send + 'a>>
            + Send
            + 'static,
    {
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| Box::pin(async move { f(&db, session).await }))
            .await
    }
}

/// 校验期望乐观锁版本与当前版本一致（不一致返回 409）。
///
/// # 参数
/// * `current_version` - 当前版本
/// * `expected_version` - 请求携带的期望版本
///
/// # 返回
/// 一致返回 `Ok(())`。
///
/// # 错误
/// 版本不一致时返回 `ConflictError`。
fn ensure_version(current_version: u64, expected_version: u64) -> Result<()> {
    if current_version != expected_version {
        return Err(Error::ConflictError(
            "数据已被其他请求修改，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

/// 返回当前时间的秒级时间戳。
fn now_secs() -> i64 {
    Instant::now().unix_secs()
}

/// 生成原幂等键的脱敏摘要（保留头尾，中间省略；不暴露完整键）。
///
/// # 参数
/// * `key` - 完整业务事实键
///
/// # 返回
/// 返回脱敏摘要。
fn mask_key(key: &str) -> String {
    if key.len() <= 10 {
        return key.to_string();
    }
    let head_end = 6.min(key.len() - 4);
    format!("{}…{}", &key[..head_end], &key[key.len() - 4..])
}

#[cfg(test)]
mod tests {
    use super::mask_key;

    #[test]
    fn mask_key_keeps_short_keys_and_masks_long_ones() {
        assert_eq!(mask_key("short"), "short");
        assert_eq!(mask_key("1234567890"), "1234567890");
        let masked = mask_key("mall-1|PAYMENT_SUCCEEDED|SO-2026-001|v3");
        assert!(masked.starts_with("mall-1"));
        assert!(masked.ends_with("|v3"));
        assert!(masked.contains('…'));
        assert!(!masked.contains("PAYMENT_SUCCEEDED"));
    }
}
