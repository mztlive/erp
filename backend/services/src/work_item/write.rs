//! 责任队列写入结果、幂等回放与版本冲突投影。

use application_core::CommandReceipt;
use database::WorkItemExt;
use entities::work_item::WorkItem;
use persistence_core::NoTransaction;

use crate::errors::{Error, Result};
use application_core::AuditActor;
use erp_audit::CommandReceiptServiceExt as _;

use super::access::detail_scope;
use super::query::single_item_context_id;
use super::{WorkItemConflict, WorkItemConflictKind, WorkItemMutationOutcome, WorkItemService, WorkItemView};

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
pub(super) fn work_item_update_error(error: persistence_core::Error) -> WorkItemWriteError {
    match error {
        persistence_core::Error::OptimisticLockingError => WorkItemWriteError::VersionConflict,
        error => WorkItemWriteError::Service(Error::from(error)),
    }
}

impl WorkItemService {
    /// 读取已完成的同一幂等命令，并拒绝相同键混用不同请求。
    pub(super) async fn idempotent_replay(
        &self,
        receipt: &CommandReceipt,
        item_id: &str,
    ) -> Result<Option<WorkItem>> {
        let Some(resource_id) = receipt.committed_resource_id(&self.db).await? else {
            return Ok(None);
        };
        if resource_id != item_id {
            return Err(Error::Internal("幂等审计资源与命令不一致".to_string()));
        }
        self.load(item_id).await.map(Some)
    }

    /// 将成功写入的实体重新按当前 actor 授权投影。
    pub(super) async fn applied_outcome(
        &self,
        item: WorkItem,
        actor: &AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        self.mutation_view(item, actor)
            .await
            .map(WorkItemMutationOutcome::Applied)
    }

    /// 冲突后重新读取任务并形成权限安全的最新投影。
    ///
    /// 最新任务不再可见或已经删除时固定返回空摘要；授权与对象读取的其他
    /// 基础设施错误继续失败，禁止退化为未经裁剪的实体。
    pub(super) async fn conflict_outcome(
        &self,
        item_id: &str,
        kind: WorkItemConflictKind,
        actor: &AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        let Some(item) = self
            .db
            .work_items()
            .find_work_item(item_id, &mut NoTransaction)
            .await?
        else {
            return Ok(WorkItemMutationOutcome::Conflict(WorkItemConflict::new(
                kind, None,
            )));
        };
        let current_work_item = match self.mutation_view(item, actor).await {
            Ok(view) => Some(view),
            Err(Error::Forbidden(_) | Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        Ok(WorkItemMutationOutcome::Conflict(WorkItemConflict::new(
            kind,
            current_work_item,
        )))
    }

    pub(super) async fn mutation_view(&self, item: WorkItem, actor: &AuditActor) -> Result<WorkItemView> {
        let access = self.actor_access(actor).await?;
        let scope = detail_scope(&item, actor.id(), &access)?;
        let item_id = item.base.id.clone();
        let fields = self
            .authorized_fields_for_items(vec![item], &access)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Forbidden("当前账号无权查看该业务对象".to_string()))?;
        let view_access = self.view_access(&fields, scope, actor, &access).await?;
        let mut view = WorkItemView::from_fields(fields, single_item_context_id(actor.id(), &item_id))?
            .with_access(
                view_access.processing_state,
                view_access.processing_blocker,
                view_access.allowed_actions,
                view_access.action_blockers,
            );
        self.apply_party_names(std::slice::from_mut(&mut view)).await?;
        self.apply_approval_contexts(std::slice::from_mut(&mut view))
            .await?;
        Ok(view)
    }
}

pub(super) fn required_text(value: &str, message: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(message.to_string()));
    }
    Ok(value.to_string())
}

/// 将 HTTP 任务版本解析为正整数乐观锁版本。
pub(crate) fn expected_task_version(value: &str) -> Result<u64> {
    let value = value.trim();
    let version = value
        .parse::<u64>()
        .map_err(|_| Error::ValidationError("任务版本必须为正整数字符串".to_string()))?;
    if version == 0 {
        return Err(Error::ValidationError("任务版本必须为正整数字符串".to_string()));
    }
    Ok(version)
}
