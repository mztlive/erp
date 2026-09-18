//! 责任队列写入结果、幂等回放与版本冲突投影。

use std::future::Future;

use application_core::{AuditActor, CommandReceipt};
use persistence_core::NoTransaction;

use super::access::detail_scope;
use super::{WorkItemConflict, WorkItemConflictKind, WorkItemMutationOutcome, WorkItemService};
use crate::entity::work_item::WorkItem;
use crate::error::{Error, Result};
use crate::repository::WorkItemExt;
use crate::repository::prelude::*;

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

impl<A: crate::ports::WorkflowAuthorizationPort + Send + Sync + 'static> WorkItemService<A> {
    /// Load a work item by id.
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
    pub(super) async fn idempotent_replay(
        &self,
        receipt: &CommandReceipt,
        item_id: &str,
    ) -> Result<Option<WorkItem>> {
        let Some(resource_id) = crate::ports::committed_resource_id(self.audit.as_ref(), receipt).await?
        else {
            return Ok(None);
        };
        if resource_id != item_id {
            return Err(Error::Internal("幂等审计资源与命令不一致".to_string()));
        }
        self.load(item_id.to_string()).await.map(Some)
    }

    /// 将成功写入的实体映射为命令结果。
    pub(super) async fn applied_outcome(
        &self,
        item: WorkItem,
        actor: &AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        let _ = actor;
        Ok(WorkItemMutationOutcome::Applied { work_item_id: item.base.id.clone() })
    }

    /// 冲突后返回任务 ID，供 HTTP 向 read-models 投影。
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

    /// Authorize a work item for remaining domain callers without returning the HTTP view.
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

/// Command-side authorization snapshot used by remaining domain services.
#[derive(Debug, Clone)]
pub struct AuthorizedWorkItem {
    /// Loaded work-item entity.
    pub item: WorkItem,
    /// Actions the current actor may take.
    pub allowed_actions: Vec<super::WorkItemAllowedAction>,
    /// Processing state after authorization.
    pub processing_state: super::ProcessingState,
    /// Optional processing blocker.
    pub processing_blocker: Option<super::ProcessingBlockerView>,
    /// Permission-safe action blockers.
    pub action_blockers: Vec<String>,
}

pub(super) fn required_text(value: &str, message: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(message.to_string()));
    }
    Ok(value.to_string())
}

/// 将 HTTP 任务版本解析为正整数乐观锁版本。
pub fn expected_task_version(value: &str) -> Result<u64> {
    let value = value.trim();
    let version =
        value.parse::<u64>().map_err(|_| Error::ValidationError("任务版本必须为正整数字符串".to_string()))?;
    if version == 0 {
        return Err(Error::ValidationError("任务版本必须为正整数字符串".to_string()));
    }
    Ok(version)
}
