//! 责任队列写入结果、幂等回放与版本冲突投影。

use std::future::Future;

use application_core::{AuditActor, CommandReceipt};
use persistence_core::NoTransaction;

use super::access::{detail_scope, ensure_generic_work_item_mutation, ensure_item_in_managed_scope};
use super::{WorkItemConflict, WorkItemConflictKind, WorkItemMutationOutcome, WorkItemService};
use crate::entity::work_item::WorkItem;
use crate::error::{Error, Result};
use crate::ports::{PreparedWorkflowAudit, WorkflowAuditAttemptResult, WorkflowAuditPort};
use crate::repository::prelude::*;
use crate::repository::{WorkItemCommandExt, WorkItemCommandRepositoryExt, WorkItemExt};

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

impl<A: crate::ports::WorkflowAuthorizationPort + Send + Sync + 'static> WorkItemService<A> {
    /// 按 ID 读取工作项。
    ///
    /// # 参数
    /// * `id` - 工作项 ID
    ///
    /// # 返回
    /// 返回已存在的工作项。
    ///
    /// # 错误
    /// 任务不存在时返回未找到；仓储读取失败时返回对应错误。
    pub fn load(&self, id: String) -> impl Future<Output = Result<WorkItem>> + Send + 'static {
        let db = self.db.clone();
        async move {
            db.work_items()
                .find_work_item(&id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("任务不存在".to_string()))
        }
    }

    /// 读取已完成的同一幂等命令，并拒绝相同键混用不同请求。
    ///
    /// # 参数
    /// * `receipt` - 本次命令收据
    /// * `item_id` - 命令指向的任务 ID
    /// * `actor` - 当前操作人
    ///
    /// # 返回
    /// 没有已提交收据时返回 `None`；同载荷且任务仍可管理时返回该任务。
    ///
    /// # 错误
    /// 收据资源或版本与任务不一致时返回内部错误；任务不可做通用责任变更、不在管理范围或对象不可访问时返回对应错误。
    pub(super) async fn idempotent_replay(
        &self,
        receipt: &CommandReceipt,
        item_id: &str,
        actor: &AuditActor,
    ) -> Result<Option<WorkItem>> {
        let Some(committed) =
            self.db.work_item_command_receipts().find_command(receipt.id(), &mut NoTransaction).await?
        else {
            return Ok(None);
        };
        let (resource_id, version) = committed.committed_task(receipt)?;
        if resource_id != item_id {
            return Err(Error::Internal("幂等回执资源与命令不一致".to_string()));
        }
        let item = self.load(item_id.to_string()).await?;
        if item.base.version < version {
            return Err(Error::Internal("幂等回执任务版本与事实不一致".to_string()));
        }
        ensure_generic_work_item_mutation(&item)?;
        let access = self.managed_access(actor).await?;
        ensure_item_in_managed_scope(&item, &access)?;
        self.ensure_object_participation(actor, &item).await?;
        Ok(Some(item))
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
        Ok(WorkItemMutationOutcome::Applied { work_item_id: item.base.id })
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
        let _ = actor;
        let exists = self.db.work_items().find_work_item(item_id, &mut NoTransaction).await?.is_some();
        Ok(WorkItemMutationOutcome::Conflict(WorkItemConflict::new(
            kind,
            exists.then(|| item_id.to_string()),
        )))
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
        let item = self.load(id.to_string()).await?;
        let access = self.actor_access(actor).await?;
        let scope = detail_scope(&item, actor.id(), &access)?;
        let fields = self
            .authorized_fields_for_items(vec![item.clone()], &access)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Forbidden("当前账号无权查看该业务对象".to_string()))?;
        let view_access = self.view_access(&fields, scope, actor, &access).await?;
        let _ = fields;
        Ok(AuthorizedWorkItem {
            item,
            allowed_actions: view_access.allowed_actions,
            processing_state: view_access.processing_state,
            processing_blocker: view_access.processing_blocker,
            action_blockers: view_access.action_blockers.into_iter().map(|blocker| blocker.message).collect(),
        })
    }
}

/// 供其余领域服务使用的命令侧授权快照。
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
    use super::*;
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
