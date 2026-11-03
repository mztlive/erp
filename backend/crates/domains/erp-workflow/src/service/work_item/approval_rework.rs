//! 在审批与业务事务内维护原提交人的驳回处理责任。
use bpm::ApprovalProcessInstanceId;
use bpm::model::{ApprovalProcessInstance, ApprovalProcessInstanceStatus};
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use crate::repository::WorkItemExt;
use crate::repository::prelude::*;
use crate::service::approval::process_kind::document_type_of;
use crate::{Error, Result};

/// 为终态驳回实例建立唯一原提交人待办。
/// # 参数
/// 数据库、已驳回实例、责任组织、单号、原因、时间及原事务执行器。
/// # 返回
/// 创建任务；同一实例已经存在任务时校验身份后保持原状态。
/// # 错误
/// 实例未驳回、既有任务身份不符或持久化失败时拒绝。
pub async fn create_rework(
    db: &Database,
    instance: &ApprovalProcessInstance,
    organization_id: &str,
    document_no: &str,
    reason: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    if instance.status != ApprovalProcessInstanceStatus::Rejected {
        return Err(Error::ConflictError("仅已驳回审批可以生成待处理任务".into()));
    }
    let id = task_id(&instance.typed_id());
    let kind = document_type_of(instance.process_kind).as_str();
    if let Some(existing) = db.work_items().find_work_item(&id, executor).await? {
        if existing.work_item_type != WorkItemType::ApprovalRework
            || !existing.matches_business_object(kind, instance.subject.subject_id())
            || existing.owner_user_id.as_deref() != Some(instance.started_by.as_str())
            || existing.subject_version != instance.subject_version.to_string()
            || existing.responsibility_key() != Some(format!("approval_rework:{}", instance.base.id).as_str())
        {
            return Err(Error::ConflictError("驳回待处理任务身份不一致".into()));
        }
        return Ok(());
    }
    let summary = format!("{}：{}", document_no, reason).chars().take(500).collect();
    let mut item = WorkItem::new_with_responsibility_key(
        WorkItemId::new(id),
        WorkItemData {
            work_item_type: WorkItemType::ApprovalRework,
            business_object_type: kind.to_owned(),
            business_object_id: instance.subject.subject_id().to_owned(),
            subject_version: instance.subject_version.to_string(),
            owner_role: "approval_submitter".into(),
            owner_organization_id: organization_id.to_owned(),
            owner_user_id: instance.started_by.as_str().to_owned(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some("APPROVAL_REJECTED".into()),
            impact_summary: Some(summary),
        },
        format!("approval_rework:{}", instance.base.id),
    )?;
    item.assigned_at = Some(now);
    item.current_assignment_at = Some(now);
    db.work_items().create(&item, executor).await?;
    Ok(())
}

/// 重提成功后完成指定旧审批实例的待办。
/// # 参数
/// 数据库、旧实例ID、已授权业务操作人、时间和原事务执行器。
/// # 返回
/// 开放任务完成；不存在或已放弃的任务保持原状。
/// # 错误
/// 固定任务身份损坏或更新失败时返回错误。
pub async fn complete_rework(
    db: &Database,
    instance_id: &ApprovalProcessInstanceId,
    actor_id: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(mut item) = db.work_items().find_work_item(&task_id(instance_id), executor).await? else {
        return Ok(());
    };
    if item.work_item_type != WorkItemType::ApprovalRework
        || item.responsibility_key() != Some(format!("approval_rework:{instance_id}").as_str())
    {
        return Err(Error::ConflictError("驳回待处理任务实例关联不一致".into()));
    }
    if item.status == WorkItemStatus::Open {
        item.complete_approval_rework(actor_id, now)?;
        db.work_items().update(&mut item, executor).await?;
    }
    Ok(())
}

/// 单据作废或合法放弃业务办理时完成当前驳回待办。
/// # 参数
/// 数据库、单据类型和ID、已授权操作人、时间和原业务事务执行器。
/// # 返回
/// 只完成该对象开放的驳回处理任务，保留审批和已关闭任务。
/// # 错误
/// 任务身份或仓储读写失败时返回错误。
pub async fn complete_document_rework(
    db: &Database,
    document_type: &str,
    document_id: &str,
    actor_id: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    for mut item in db.work_items().list_active_by_object(document_type, document_id, executor).await? {
        if item.work_item_type == WorkItemType::ApprovalRework {
            item.complete_approval_rework(actor_id, now)?;
            db.work_items().update(&mut item, executor).await?;
        }
    }
    Ok(())
}

fn task_id(instance_id: &ApprovalProcessInstanceId) -> String {
    format!("approval-rework:{instance_id}")
}
