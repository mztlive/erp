//! 付款申请后续审批及恢复节点沿同实例旧任务保留冻结希望付款日。

use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, ProcessKind};
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::work_item::{WorkItem, WorkItemType};
use crate::error::{Error, Result};
use crate::repository::prelude::*;
use crate::repository::{BpmExt, WorkItemExt};
use crate::service::approval::execution::apply_plan::PlannedWrites;

/// 同一事务从刚结束任务或同实例历史任务继承日期；无付款申请新任务时不读取。
///
/// # 参数
/// * `db` / `executor` - 已授权审批命令持有的数据库与事务。
/// * `writes` - 已核对主体、提交版本和当前执行的审批计划。
/// # 返回
/// 付款申请返回旧任务的冻结日期，其他类型保持无日期。
/// # 错误
/// 原任务缺失、重复、串实例、串版本或缺少日期时拒绝新任务写入。
pub(super) async fn inherited_due_at(
    db: &Database,
    writes: &PlannedWrites,
    executor: &mut dyn Executor,
) -> Result<Option<Instant>> {
    if writes.instance.process_kind != ProcessKind::PurchasePaymentRequest || writes.create_tasks.is_empty() {
        return Ok(None);
    }
    if let Some(due) = from_executions(db, &writes.instance, &writes.updated_executions, executor).await? {
        return Ok(Some(due));
    }
    let history = db
        .bpm_workflow()
        .list_execution_history(&ApprovalProcessInstanceId::new(&writes.instance.base.id), None, 50, executor)
        .await?;
    from_executions(db, &writes.instance, &history, executor)
        .await?
        .map(Some)
        .ok_or_else(|| Error::ConflictError("付款申请审批缺少可追溯的希望付款日".into()))
}

async fn from_executions(
    db: &Database,
    instance: &ApprovalProcessInstance,
    executions: &[ApprovalNodeExecution],
    executor: &mut dyn Executor,
) -> Result<Option<Instant>> {
    for execution in executions {
        let tasks = db.work_items().approval_tasks_for_execution(&execution.typed_id(), executor).await?;
        let task = match tasks.as_slice() {
            [] => continue,
            [task] => task,
            _ => return Err(Error::ConflictError("原审批执行关联多个日期来源任务".into())),
        };
        return task_due_at(instance, execution, task).map(Some);
    }
    Ok(None)
}

/// 日期只取同实例、同提交、同审批执行及责任人的原审批任务。
pub(in crate::service::approval::execution) fn task_due_at(
    instance: &ApprovalProcessInstance,
    execution: &ApprovalNodeExecution,
    task: &WorkItem,
) -> Result<Instant> {
    if instance.process_kind != ProcessKind::PurchasePaymentRequest
        || execution.process_instance_id.as_ref() != instance.base.id
        || task.work_item_type != WorkItemType::DocumentApproval
        || task.approval_node_execution_id.as_ref().map(AsRef::as_ref) != Some(execution.base.id.as_str())
        || !task.matches_business_object(instance.subject.subject_kind(), instance.subject.subject_id())
        || !task.matches_subject_version(&instance.subject_version.to_string())
        || task.owner_user_id.as_deref() != Some(execution.assignee_participant_id.as_str())
    {
        return Err(Error::ConflictError("付款申请审批日期来源与冻结运行身份不一致".into()));
    }
    task.due_at.ok_or_else(|| Error::ConflictError("原付款申请审批任务缺少希望付款日".into()))
}
