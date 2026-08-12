//! 集成错误任务业务：登记、列表、详情、QUERY、REPLAY、DEFER/SKIP、TRANSFER、
//! RESOLVE、CLOSE。
//!
//! QUERY 结果写入脱敏摘要，只有 `no_result_confirmed` 才开放 REPLAY；REPLAY 永不
//! 接受客户端原幂等键，服务端锁定关联消息的业务事实键；DEFER/SKIP/TRANSFER 非终结，
//! RESOLVE/CLOSE 是终态；所有业务写入与审计日志在同一 MongoDB 事务原子提交
//! （模板见 `super::transaction`）。

use database::{AccessControlExt, IntegrationOpsExt, NoTransaction};
use entities::common::time::Instant;
use entities::integration_ops::{
    ErrorClass, ErrorTaskStatus, IntegrationErrorTask, IntegrationErrorTaskData, IntegrationErrorTaskId,
    IntegrationErrorTaskUpdate, ResolutionType,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::SortDir;
use super::validation::ensure_version;
use super::{
    CloseErrorTaskRequest, CloseReason, CreateErrorTaskRequest, ErrorTaskDetailView, ErrorTaskFilter,
    ErrorTaskListParams, ErrorTaskView, HoldErrorTaskRequest, HoldKind, IntegrationOpsService, PageView,
    QueryOriginalResultRequest, ReplayOriginalRequest, ReplayResultView, ResolveErrorTaskRequest,
    TransferErrorTaskRequest,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl IntegrationOpsService {
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
    // -----------------------------------------------------------------------
    // 私有辅助
    // -----------------------------------------------------------------------

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
