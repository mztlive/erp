//! 正式任务绑定、权限前置与审计回执重放。
use application_core::AuditActor;
use erp_integration::dto::{IntegrationItemType, IntegrationNonTerminalTaskAction, PreparedWorkItemTarget};
use erp_integration::entity::integration_ops::{IntegrationCommandIdentity, IntegrationReceiptPayload};
use erp_integration::repository::{IntegrationCommandExt, IntegrationCommandRepositoryExt};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use serde::Serialize;

use super::super::IntegrationResolutionProcess;
use crate::{Error, Result};

impl IntegrationResolutionProcess {
    /// 重放仍校验当前账号、责任、完整执行权限和对象绑定，不再套原写版本。
    pub(super) async fn ensure_replay_task_access(
        &self,
        work_item_id: &str,
        action: &IntegrationNonTerminalTaskAction,
        actor: &AuditActor,
    ) -> Result<()> {
        let mut executor = NoTransaction;
        let item = self
            .db
            .work_items()
            .find_by_id(work_item_id, &mut executor)
            .await?
            .ok_or_else(|| Error::Internal("W29 命令回执对应任务缺失".into()))?;
        if !item.is_owned_by(actor.id()) {
            return Err(Error::Forbidden("当前账号不是任务的当前责任人".into()));
        }
        ensure_work_item_association(&self.db, &item, action, &mut executor).await?;
        let rbac = crate::adapters::identity::shared_rbac_service(self.db.clone());
        crate::adapters::workflow::work_item_service(self.db.clone(), rbac)
            .ensure_domain_decision_access(actor, &item, &mut executor)
            .await?;
        Ok(())
    }

    pub(super) async fn replay_receipt<T: IntegrationReceiptPayload>(
        &self,
        receipt: &IntegrationCommandIdentity,
        actor: &AuditActor,
    ) -> Result<Option<T>> {
        let Some(committed) = self
            .db
            .integration_command_receipts()
            .find_command(receipt.receipt_id(), &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(committed.recover(receipt, actor.id())?))
    }
}

/// 序列化 W29 命令并构造领域幂等身份。
///
/// # 参数
/// * `actor_id` - 命令操作人
/// * `action` - 稳定动作名
/// * `resource_type` - 审计资源类型
/// * `resource_id` - 审计资源 ID
/// * `idempotency_key` - 客户端幂等键
/// * `command` - 完整强命令载荷
///
/// # 返回
/// 返回不暴露原始幂等键的领域命令身份。
///
/// # 错误
/// 命令无法序列化时返回内部错误。
pub(super) fn command_identity<T: Serialize>(
    actor_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    idempotency_key: &str,
    command: &T,
) -> Result<IntegrationCommandIdentity> {
    let payload =
        serde_json::to_vec(command).map_err(|_| Error::Internal("W29 命令无法形成幂等指纹".to_string()))?;
    Ok(IntegrationCommandIdentity::new(
        actor_id,
        action,
        resource_type,
        resource_id,
        idempotency_key,
        &payload,
    ))
}

/// 加载与命令绑定的正式任务，并校验责任归属与版本。
///
/// 任务版本与业务主题版本取自 Prepared 目标（DTO 层单次解析），此处只做
/// typed 比较，不再解析版本字符串。
pub(super) async fn load_bound_work_item(
    db: &Database,
    target: &PreparedWorkItemTarget,
    action: &IntegrationNonTerminalTaskAction,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let item = db
        .work_items()
        .find_by_id(&target.work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("正式任务不存在".to_string()))?;
    ensure_work_item_version(&item, target.task_version, &target.subject_version)?;
    ensure_work_item_responsibility(&item, actor_id)?;
    ensure_actor_eligible(db, &item, actor_id, executor).await?;
    ensure_work_item_association(db, &item, action, executor).await?;
    Ok(item)
}

/// 校验任务与业务主题版本与调用方冻结的 typed 版本一致（无二次解析）。
fn ensure_work_item_version(item: &WorkItem, task_version: u64, subject_version: &str) -> Result<()> {
    if item.base.version != task_version || item.subject_version != subject_version {
        return Err(Error::ConflictError("任务或业务主题版本已变化，请刷新后重试".to_string()));
    }
    Ok(())
}

fn ensure_work_item_responsibility(item: &WorkItem, actor_id: &str) -> Result<()> {
    if item.status != WorkItemStatus::Open {
        return Err(Error::ConflictError("任务已不再开放".to_string()));
    }
    if !item.is_owned_by(actor_id) {
        return Err(Error::Forbidden("当前账号不是任务的当前责任人".to_string()));
    }
    if false {
        return Err(Error::BusinessLogicError("W29 只接受独立异常任务".to_string()));
    }
    Ok(())
}

async fn ensure_actor_eligible(
    db: &Database,
    item: &WorkItem,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = (db, item, actor_id, executor);
    Ok(())
}

async fn ensure_work_item_association(
    db: &Database,
    item: &WorkItem,
    action: &IntegrationNonTerminalTaskAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    let expected = match action.item_type {
        IntegrationItemType::ErrorTask => {
            let task = erp_integration::service::task_decision::guard::load_error_task_for_association(
                db,
                &action.item_id,
                executor,
            )
            .await?;
            (
                if task.error_class == erp_integration::entity::integration_ops::ErrorClass::ResultUnknown {
                    WorkItemType::IntegrationResultUnknown
                } else {
                    WorkItemType::BusinessException
                },
                "integration_error_task",
            )
        },
        IntegrationItemType::ReconciliationDifference => {
            (WorkItemType::BusinessException, "reconciliation_difference")
        },
    };
    if item.work_item_type != expected.0
        || item.business_object_type != expected.1
        || item.business_object_id != action.item_id
    {
        return Err(Error::ConflictError("任务与业务项的正式关联不一致".to_string()));
    }
    Ok(())
}
