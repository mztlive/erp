//! 责任队列写入结果、幂等回放与版本冲突投影。

use std::result::Result as StdResult;
use std::sync::Arc;

use application_core::{AuditActor, CommandReceipt};
use erp_identity::{CompiledAccess, ResourceTargets};
use erp_permissions::work_item;
use persistence_core::{Executor, Transactional};

use super::access::ensure_generic_work_item_mutation;
use super::{
    WorkItemConflict, WorkItemConflictKind, WorkItemFilter, WorkItemMutationOutcome, WorkItemScope,
    WorkItemService,
};
use crate::dto::work_item::ViewAccess;
use crate::entity::work_item::WorkItem;
use crate::error::{Error, Result};
use crate::ports::{
    PreparedWorkflowAudit, WorkflowAuditAttemptResult, WorkflowAuditPort, WorkflowAuthorizationPort,
};
use crate::repository::{
    WorkItemCommandExt, WorkItemCommandRepositoryExt, WorkItemExt, WorkItemRepositoryExt,
};

pub(super) const IDEMPOTENCY_AUDIT_PREFIX: &str = "work-item-command-";

pub(super) enum WorkItemWriteOutcome {
    Updated(Box<WorkItem>),
    VersionConflict,
}

#[derive(Debug)]
pub(super) enum WorkItemWriteError {
    VersionConflict,
    Service(Error),
}

impl From<persistence_core::Error> for WorkItemWriteError {
    fn from(error: persistence_core::Error) -> Self {
        Self::Service(Error::from(error))
    }
}

impl From<Error> for WorkItemWriteError {
    fn from(error: Error) -> Self {
        Self::Service(error)
    }
}

impl From<erp_core::Error> for WorkItemWriteError {
    fn from(error: erp_core::Error) -> Self {
        Self::Service(Error::from(error))
    }
}

/// 只把任务实体自身的 CAS 未命中归类为任务版本冲突。
///
/// # 参数
/// * `error` - 工作项更新返回的持久化错误
///
/// # 返回
/// `OptimisticLockingError` 返回版本冲突；其它错误包进服务错误。
///
/// # 错误
/// 不返回错误。
pub(super) fn work_item_update_error(error: persistence_core::Error) -> WorkItemWriteError {
    match error {
        persistence_core::Error::OptimisticLockingError => WorkItemWriteError::VersionConflict,
        error => WorkItemWriteError::Service(Error::from(error)),
    }
}

/// 原事务已结束后记录安全尝试；持久化失败不覆盖业务错误。
///
/// # 参数
/// * `audit` - 审计端口
/// * `prepared` - 已准备的审计
/// * `error` - 原命令错误
///
/// # 返回
/// 无返回值。未知结果记为未知，内部错误记为失败，其余记为拒绝。审计写入失败只记录警告。
///
/// # 错误
/// 不返回错误。
pub(super) async fn record_command_attempt(
    audit: &dyn WorkflowAuditPort,
    prepared: &PreparedWorkflowAudit,
    error: &Error,
) {
    let result = match error {
        Error::OutcomeUnknown(_) => WorkflowAuditAttemptResult::Unknown,
        _ if error.class() == application_core::ErrorClass::Internal => WorkflowAuditAttemptResult::Failed,
        _ => WorkflowAuditAttemptResult::Rejected,
    };
    if audit.persist_attempt(prepared, result).await.is_err() {
        tracing::warn!(event_kind = "command_attempt", action_code = %prepared.action, "工作流命令尝试审计写入失败");
    }
}

/// 查证失败不得替换首次未知提交的来源；其他恢复冲突沿原幂等合同。
///
/// # 参数
/// * `original` - 首次写入返回的错误
/// * `recovery` - 回读结果；`None` 表示尚未找到胜者
///
/// # 返回
/// 回读到值时返回该值。
///
/// # 错误
/// 没有胜者时返回 `original`。首次错误是 `OutcomeUnknown` 时，查证失败仍返回 `original`；其它查证失败返回查证错误。
pub(super) fn recover_command_error<T>(original: Error, recovery: Result<Option<T>>) -> Result<T> {
    match recovery {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(original),
        Err(_) if matches!(original, Error::OutcomeUnknown(_)) => Err(original),
        Err(error) => Err(error),
    }
}

impl<A: WorkflowAuthorizationPort> WorkItemService<A> {
    /// 读取已完成的同一幂等命令，并拒绝相同键混用不同请求。
    ///
    /// # 参数
    /// * `receipt` - 本次命令收据
    /// * `item` - 同一事务中已证明管理资格的真实任务
    /// * `executor` - 原事务执行器
    ///
    /// # 返回
    /// 没有已提交收据时返回 `None`；同载荷且任务仍可管理时返回该任务。
    ///
    /// # 错误
    /// 收据资源或版本与任务不一致时返回内部错误；任务不可做通用责任变更、不在管理范围或对象不可访问时返回对应错误。
    pub(super) async fn idempotent_replay(
        &self,
        receipt: &CommandReceipt,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        let Some(committed) =
            self.db.work_item_command_receipts().find_command(receipt.id(), executor).await?
        else {
            return Ok(None);
        };
        let (resource_id, version) = committed.committed_task(receipt)?;
        if resource_id != item.base.id {
            return Err(Error::Internal("幂等回执资源与命令不一致".to_string()));
        }
        if item.base.version < version {
            return Err(Error::Internal("幂等回执任务版本与事实不一致".to_string()));
        }
        ensure_generic_work_item_mutation(item)?;
        Ok(Some(item.clone()))
    }

    /// 在原事务中先授权，再读取决定写入或回放的任务。
    /// # 参数
    /// id、actor 为目标和管理人，executor 为原活动快照事务。
    /// # 返回
    /// 当前可管理且允许通用责任变更的任务。
    /// # 错误
    /// 无资格、目标缺失、任务类型禁止或数据库读取失败时拒绝。
    pub(super) async fn managed_item(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<WorkItem> {
        self.authorization.require(actor, &work_item::MANAGE, id, executor).await?;
        let item = self
            .db
            .work_items()
            .find_work_item(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("任务不存在".into()))?;
        ensure_generic_work_item_mutation(&item)?;
        Ok(item)
    }

    /// 原写事务退出后记录尝试，并按原幂等合同恢复结果。
    /// # 参数
    /// receipt、actor、audit 为原命令上下文，result 为已经结束的写事务结果。
    /// # 返回
    /// 已提交的真实结果或安全的版本冲突响应。
    /// # 错误
    /// 保留原业务错误；对并发冲突和结果未明的存储失败执行只读查证。
    pub(super) async fn finish_command(
        &self,
        receipt: &CommandReceipt,
        actor: &AuditActor,
        audit: &PreparedWorkflowAudit,
        result: StdResult<WorkItemWriteOutcome, WorkItemWriteError>,
    ) -> Result<WorkItemMutationOutcome> {
        let result = match result {
            Err(WorkItemWriteError::VersionConflict) => {
                record_command_attempt(
                    self.audit.as_ref(),
                    audit,
                    &Error::ConflictError("任务版本已变化".into()),
                )
                .await;
                WorkItemWriteOutcome::VersionConflict
            },
            Err(WorkItemWriteError::Service(error)) => {
                record_command_attempt(self.audit.as_ref(), audit, &error).await;
                if !matches!(
                    error,
                    Error::OutcomeUnknown(_)
                        | Error::ReceiptDuplicate(_)
                        | Error::RepositoryError(_)
                        | Error::ConflictError(_)
                ) {
                    return Err(error);
                }
                let replay = self.recover_command(receipt, actor).await;
                WorkItemWriteOutcome::Updated(Box::new(recover_command_error(error, replay)?))
            },
            Ok(result) => result,
        };
        match result {
            WorkItemWriteOutcome::Updated(item) => self.applied_outcome(*item, actor).await,
            WorkItemWriteOutcome::VersionConflict => {
                self.conflict_outcome(&audit.resource_id, WorkItemConflictKind::Version, actor).await
            },
        }
    }

    /// 失败事务结束后用新的只读快照取证原命令，禁止重放写入。
    async fn recover_command(
        &self,
        receipt: &CommandReceipt,
        actor: &AuditActor,
    ) -> Result<Option<WorkItem>> {
        let service = self.clone();
        let receipt = receipt.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.replay_command(&receipt, &actor, executor).await })
            })
            .await
    }

    /// 原回放读取先证明当前管理资格，再核对精确任务与命令回执。
    async fn replay_command(
        &self,
        receipt: &CommandReceipt,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        let id = receipt.scope_id().ok_or_else(|| Error::Internal("任务命令缺少目标身份".into()))?;
        let item = self.managed_item(id, actor, executor).await?;
        self.idempotent_replay(receipt, &item, executor).await
    }

    /// 将成功写入的实体映射为命令结果。
    ///
    /// # 参数
    /// * `item` - 已写入的工作项
    /// * `actor` - 当前操作人；本函数不读取
    ///
    /// # 返回
    /// 返回已应用结果，只带工作项 ID。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) async fn applied_outcome(
        &self,
        item: WorkItem,
        actor: &AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        let _ = actor;
        Ok(WorkItemMutationOutcome::Applied { work_item_id: item.base.id, task_version: item.base.version })
    }

    /// 冲突后返回任务 ID，供 HTTP 向 read-models 投影。
    ///
    /// # 参数
    /// * `item_id` - 工作项 ID
    /// * `kind` - 冲突类别
    /// * `actor` - 当前操作人；本函数不读取
    ///
    /// # 返回
    /// 返回冲突结果。任务仍存在时带上 ID，否则 ID 为空。
    ///
    /// # 错误
    /// 存在性查询失败时返回对应错误。
    pub(super) async fn conflict_outcome(
        &self,
        item_id: &str,
        kind: WorkItemConflictKind,
        actor: &AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        let service = self.clone();
        let actor = actor.clone();
        let item_id = item_id.to_owned();
        let visible = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let access = service.authorization.query(&actor, &work_item::MANAGE, executor).await?;
                    let filter = WorkItemFilter { authorization: Some(access.clone()), ..Default::default() };
                    let item =
                        service.db.work_items().find_visible_by_id(&item_id, &filter, executor).await?;
                    service.authorization.service.validate(&access, executor).await?;
                    Ok::<_, Error>(item.map(|item| item.base.id))
                })
            })
            .await?;
        Ok(WorkItemMutationOutcome::Conflict(WorkItemConflict::new(kind, visible)))
    }

    /// 为其余领域调用方授权工作项，不返回 HTTP 视图。
    ///
    /// # 参数
    /// * `id` - 工作项 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回任务、允许动作、处理状态和阻塞说明。
    ///
    /// # 错误
    /// 任务不存在、无权查看、对象不可访问或授权读取失败时返回错误。
    pub async fn authorize_work_item(&self, id: &str, actor: &AuditActor) -> Result<AuthorizedWorkItem> {
        let service = self.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { service.authorize_work_item_with_executor(&id, &actor, executor).await },
                )
            })
            .await
    }

    /// 任务、业务事实和完整授权沿调用方的同一事务快照读取。
    /// # 参数
    /// id 为真实任务主键，actor 为当前账号，executor 为调用方的原快照事务。
    /// # 返回
    /// 返回任务、允许动作与处理状态。
    /// # 错误
    /// 非事务执行器、任务缺失、无权或事实读取失败时拒绝。
    pub async fn authorize_work_item_with_executor(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedWorkItem> {
        let (item, access) = self.work_item_read(id, actor, executor).await?;
        self.authorization.service.validate(&access, executor).await?;
        Ok(item)
    }

    /// 读取任务及办理展示事实，保留原凭证供调用方完成业务补齐后交付检查。
    /// # 参数
    /// id 为真实任务主键，actor 为当前账号，executor 为整个读取用例的原快照事务。
    /// # 返回
    /// 任务投影及尚未交付的完整读取凭证；调用方必须在所有展示读取完成后 validate。
    /// # 错误
    /// 非事务执行器、目标非法、无权、任务缺失或业务事实读取失败时拒绝。
    pub async fn work_item_read(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(AuthorizedWorkItem, CompiledAccess)> {
        if !executor
            .boundary()
            .is_some_and(|boundary| boundary.is_snapshot() && boundary.is_current(executor))
        {
            return Err(Error::ValidationError("任务授权读取必须使用快照事务".into()));
        }
        ResourceTargets::new(&[id])?;
        let read = self.authorization.service.query(actor, &work_item::DETAIL, executor).await?;
        let filter = WorkItemFilter { authorization: Some(Arc::new(read.clone())), ..Default::default() };
        let Some(item) = self.db.work_items().find_visible_by_id(id, &filter, executor).await? else {
            self.authorization.require(actor, &work_item::DETAIL, id, executor).await?;
            return Err(Error::NotFound("任务不存在".into()));
        };
        let access = self.task_actions(actor, &item, &read, executor).await?;
        let scope = WorkItemScope::for_task(&item, actor.id());
        self.ensure_task_current(&item, executor).await?;
        let view_access = ViewAccess::from_fields(&item.clone().into(), scope, &access);
        Ok((
            AuthorizedWorkItem {
                item,
                allowed_actions: view_access.allowed_actions,
                processing_state: view_access.processing_state,
                processing_blocker: view_access.processing_blocker,
                action_blockers: view_access
                    .action_blockers
                    .into_iter()
                    .map(|blocker| blocker.message)
                    .collect(),
            },
            read,
        ))
    }
}

/// 同一读取阶段取得的任务与动作投影；交付由读取用例完成凭证复验。
#[derive(Debug, Clone)]
pub struct AuthorizedWorkItem {
    /// 已加载的工作项实体。
    pub item: WorkItem,
    /// 当前操作人可执行的动作。
    pub allowed_actions: Vec<super::WorkItemAllowedAction>,
    /// 授权后的处理状态。
    pub processing_state: super::ProcessingState,
    /// 可选的处理阻塞原因。
    pub processing_blocker: Option<super::ProcessingBlockerView>,
    /// 动作阻塞说明。
    pub action_blockers: Vec<String>,
}

/// 去掉空白后要求文本非空。
///
/// # 参数
/// * `value` - 原始文本
/// * `message` - 为空时的校验说明
///
/// # 返回
/// 返回去掉首尾空白的文本。
///
/// # 错误
/// 空值或仅空白时返回校验错误，说明使用 `message`。
pub(super) fn required_text(value: &str, message: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(message.to_string()));
    }
    Ok(value.to_string())
}

/// 将 HTTP 任务版本解析为正整数乐观锁版本。
///
/// # 参数
/// * `value` - HTTP 提交的任务版本
///
/// # 返回
/// 返回大于零的版本。
///
/// # 错误
/// 不是十进制 `u64`，或值为 0 时返回校验错误。
pub fn expected_task_version(value: &str) -> Result<u64> {
    let value = value.trim();
    let version =
        value.parse::<u64>().map_err(|_| Error::ValidationError("任务版本必须为正整数字符串".to_string()))?;
    if version == 0 {
        return Err(Error::ValidationError("任务版本必须为正整数字符串".to_string()));
    }
    Ok(version)
}

#[cfg(test)]
mod receipt_recovery_tests {
    use erp_core::AccountKind;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::super::testing::unavailable_service;
    use super::*;

    #[tokio::test]
    async fn task_read_requires_snapshot_and_stops_before_business_reads_on_authorization_failure() {
        let service = unavailable_service();
        let actor = AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin);
        assert!(matches!(
            service.work_item_read("task", &actor, &mut NoTransaction).await,
            Err(Error::ValidationError(reason)) if reason == "任务授权读取必须使用快照事务"
        ));
        assert!(matches!(
            service.work_item_read("task", &actor, &mut SnapshotExecutor::default()).await,
            Err(Error::Internal(reason)) if reason == "测试授权设施不可用"
        ));
        assert!(matches!(
            service.authorize_work_item_with_executor("task", &actor, &mut SnapshotExecutor::default()).await,
            Err(Error::Internal(reason)) if reason == "测试授权设施不可用"
        ));
    }

    #[tokio::test]
    async fn command_recovery_requires_current_snapshot_authorization_before_reading_receipts() {
        let service = unavailable_service();
        let actor = AuditActor::new("manager".into(), "manager".into(), AccountKind::Admin);
        let receipt = CommandReceipt::from_resource_parts(
            IDEMPOTENCY_AUDIT_PREFIX,
            actor.id(),
            "work_item.reassign",
            "work_item",
            "task",
            "key",
            ["1".into()],
        )
        .unwrap();
        let error =
            service.replay_command(&receipt, &actor, &mut SnapshotExecutor::default()).await.err().unwrap();
        assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn authorization_denial_keeps_the_original_error_without_starting_receipt_recovery() {
        let service = unavailable_service();
        let actor = AuditActor::new("manager".into(), "manager".into(), AccountKind::Admin);
        let receipt = CommandReceipt::from_resource_parts(
            IDEMPOTENCY_AUDIT_PREFIX,
            actor.id(),
            "work_item.reassign",
            "work_item",
            "task",
            "key",
            ["1".into()],
        )
        .unwrap();
        let audit = PreparedWorkflowAudit::resource_with_message(
            actor.clone(),
            receipt.action(),
            receipt.resource_type(),
            "task".into(),
            None,
        )
        .unwrap();
        let error = service
            .finish_command(
                &receipt,
                &actor,
                &audit,
                Err(WorkItemWriteError::Service(Error::Forbidden("scope denied".into()))),
            )
            .await
            .err()
            .unwrap();
        assert!(matches!(error, Error::Forbidden(reason) if reason == "scope denied"));
    }

    #[test]
    fn unverified_recovery_keeps_original_unknown_commit_source() {
        for recovery in [Ok(None), Err(Error::Internal("receipt read failed".into()))] {
            let original = Error::OutcomeUnknown(persistence_core::Error::CommitOutcomeUnknown(
                mongodb::error::Error::custom("original work item commit"),
            ));
            let result: Result<()> = recover_command_error(original, recovery);
            match result.unwrap_err() {
                Error::OutcomeUnknown(persistence_core::Error::CommitOutcomeUnknown(source)) => {
                    assert_eq!(source.get_custom::<&str>(), Some(&"original work item commit"));
                },
                error => panic!("未知提交来源被替换: {error:?}"),
            }
        }
    }
    #[test]
    fn exact_committed_receipt_recovers_without_a_second_write() {
        let result = recover_command_error(Error::Internal("first write".into()), Ok(Some(7)));
        assert_eq!(result.unwrap(), 7);
        let conflict: Result<()> = recover_command_error(
            Error::Internal("first write".into()),
            Err(Error::ConflictError("same key different payload".into())),
        );
        assert!(matches!(conflict, Err(Error::ConflictError(_))));
    }
}
