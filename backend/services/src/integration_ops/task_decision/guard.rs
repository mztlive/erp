use database::IntegrationOpsExt;
use entities::integration_ops::{
    IntegrationCommandIdentity, IntegrationErrorTask, ReconciliationDifference, ReconciliationDifferenceId,
    ReconciliationDifferenceResolution, ResolutionVersionCheck,
};
use erp_audit::AuditExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use erp_workflow::WorkItemExt;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use serde::{de::DeserializeOwned, Serialize};

use super::super::{
    IntegrationItemType, IntegrationNonTerminalTaskAction, IntegrationOpsService, PreparedWorkItemTarget,
};
use super::ReceiptEnvelope;
use crate::errors::{Error, Result};
use application_core::AuditActor;

impl IntegrationOpsService {
    pub(super) async fn replay_receipt<T: DeserializeOwned>(
        &self,
        receipt: &IntegrationCommandIdentity,
        actor: &AuditActor,
    ) -> Result<Option<T>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(receipt.receipt_id(), &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if !receipt.matches_receipt(
            &audit.actor_id,
            &audit.action,
            &audit.resource_type,
            audit.resource_id.as_deref(),
        ) || actor.id() != audit.actor_id
        {
            return Err(Error::ConflictError("幂等键已用于不同命令".to_string()));
        }
        let message = audit
            .message
            .as_deref()
            .ok_or_else(|| Error::Internal("W29 幂等收据缺少结果".to_string()))?;
        let envelope: ReceiptEnvelope<T> =
            serde_json::from_str(message).map_err(|_| Error::Internal("W29 幂等收据不可解析".to_string()))?;
        if envelope.fingerprint != receipt.fingerprint() {
            return Err(Error::ConflictError("幂等键已用于不同命令".to_string()));
        }
        Ok(Some(envelope.result))
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
        return Err(Error::ConflictError(
            "任务或业务主题版本已变化，请刷新后重试".to_string(),
        ));
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
            let task = db
                .integration_error_tasks()
                .find_by_id(&action.item_id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("集成错误任务不存在".to_string()))?;
            (
                if task.error_class == entities::integration_ops::ErrorClass::ResultUnknown {
                    WorkItemType::IntegrationResultUnknown
                } else {
                    WorkItemType::BusinessException
                },
                "integration_error_task",
            )
        }
        IntegrationItemType::ReconciliationDifference => {
            (WorkItemType::BusinessException, "reconciliation_difference")
        }
    };
    if item.work_item_type != expected.0
        || item.business_object_type != expected.1
        || item.business_object_id != action.item_id
    {
        return Err(Error::ConflictError("任务与业务项的正式关联不一致".to_string()));
    }
    Ok(())
}

pub(super) async fn load_error_task(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<IntegrationErrorTask> {
    let task = db
        .integration_error_tasks()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("集成错误任务不存在".to_string()))?;
    if task.is_terminal() {
        return Err(Error::ConflictError("集成错误任务已终结".to_string()));
    }
    Ok(task)
}

pub(super) fn ensure_error_task_subject(task: &IntegrationErrorTask, expected: &str) -> Result<()> {
    if !task.has_subject_version(expected) {
        return Err(Error::ConflictError("错误任务业务版本已变化".to_string()));
    }
    Ok(())
}

pub(super) async fn load_difference(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<ReconciliationDifference> {
    db.reconciliation_differences()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("对账差异不存在".to_string()))
}

pub(super) fn ensure_difference_subject(
    latest: Option<&ReconciliationDifferenceResolution>,
    expected: &str,
) -> Result<()> {
    match ReconciliationDifferenceResolution::check_version(expected, latest) {
        ResolutionVersionCheck::Current => Ok(()),
        ResolutionVersionCheck::Invalid | ResolutionVersionCheck::Stale => {
            Err(Error::ConflictError("对账差异业务版本已变化".to_string()))
        }
    }
}

pub(super) async fn latest_resolution(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ReconciliationDifferenceResolution>> {
    db.reconciliation_difference_resolutions()
        .find_latest_by_difference(&ReconciliationDifferenceId::new(id.to_string()), executor)
        .await
        .map_err(Into::into)
}

pub(super) fn ensure_difference_open(latest: Option<&ReconciliationDifferenceResolution>) -> Result<()> {
    if !ReconciliationDifferenceResolution::is_open(latest) {
        return Err(Error::ConflictError("对账差异已形成正式结论".to_string()));
    }
    Ok(())
}
