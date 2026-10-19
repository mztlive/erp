//! 采购付款跟进安排：精确任务、负责人、版本及回执在同一事务核验。
use application_core::{AuditActor, CommandReceipt};
use erp_audit::AuditActorLogs;
use erp_core::common::time::{BusinessDate, Instant};
use erp_read_models::purchase_center::payment_arrangement::arrangement_facts;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item_command::{WorkItemCommandReceipt, WorkItemCommandResult};
use erp_workflow::repository::{WorkItemCommandExt, WorkItemCommandRepositoryExt};
use mongodb::Database;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

use super::PurchaseOrderProcess;
use crate::audit::persist_log;
use crate::{Error, Result};

/// 跟进日为上海业务日期，不代表供应商付款到期日或对账完成。
#[derive(Clone, Serialize, Deserialize)]
pub struct SchedulePaymentFollowUp {
    pub task_id: String,
    pub expected_version: u64,
    pub follow_up_date: BusinessDate,
    pub idempotency_key: String,
}
impl PurchaseOrderProcess {
    /// 安排当前采购付款任务的下次跟进日。
    /// # 参数
    /// 采购身份、精确任务版本及日期、已认证采购经办人。
    /// # 返回
    /// 同键同载荷返回原成功结果，不重复写入或推迟跟进。
    /// # 错误
    /// 非当前负责人、采购范围越界、版本或余额变化、日期非法时拒绝。
    pub async fn schedule_payment_follow_up(
        &self,
        id: &str,
        req: SchedulePaymentFollowUp,
        actor: &AuditActor,
    ) -> Result<()> {
        let authorization = self.authorize_actor_permission(actor, "purchase_payment_request:submit").await?;
        let access = self.command_access(actor, "detail")?;
        let command = CommandReceipt::from_resource_parts(
            "payment-follow-up-",
            actor.id(),
            "work_item.payment_follow_up",
            "work_item",
            &req.task_id,
            &req.idempotency_key,
            [id.to_string(), req.expected_version.to_string(), req.follow_up_date.to_string()],
        )?;
        let db = self.db.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        authorization
            .rbac
            .run_authorized_policy_transaction(authorization.policy_revision, move |executor| {
                Box::pin(async move {
                    access.current(&id, executor).await?;
                    schedule(&db, &id, &req, &actor, &command, executor).await
                })
            })
            .await
    }
}
/// 写入任务、审计与独立命令结果；不启动新事务。
async fn schedule(
    db: &Database,
    id: &str,
    req: &SchedulePaymentFollowUp,
    actor: &AuditActor,
    command: &CommandReceipt,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut task = db
        .work_items()
        .find_by_id(&req.task_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("付款安排任务不存在".into()))?;
    if task.business_object_id != id || task.owner_user_id.as_deref() != Some(actor.id()) {
        return Err(Error::Forbidden("只能安排本人负责的采购付款跟进".into()));
    }
    if let Some(receipt) = db.work_item_command_receipts().find_command(command.id(), executor).await? {
        receipt.committed_task(command)?;
        return Ok(());
    }
    let facts = arrangement_facts(db, id, executor).await?;
    if !facts.state.needs_task()
        || facts.task.as_ref().map(|item| item.base.id.as_str()) != Some(req.task_id.as_str())
    {
        return Err(Error::ConflictError("付款安排已变化，请刷新查看申请进度".into()));
    }
    if task.base.version != req.expected_version {
        return Err(Error::ConflictError("付款安排已更新，请刷新后重新安排".into()));
    }
    task.schedule_payment_follow_up(actor.id(), req.follow_up_date, BusinessDate::today(), Instant::now())?;
    db.work_items().update(&mut task, executor).await?;
    let audit =
        actor.clone().resource_log("work_item.payment_follow_up", "work_item", task.base.id.clone())?;
    persist_log(db, &audit, executor).await?;
    let receipt = WorkItemCommandReceipt::new(
        command,
        WorkItemCommandResult::PaymentFollowUp {
            work_item_id: task.base.id,
            task_version: task.base.version,
            follow_up_date: req.follow_up_date.to_string(),
        },
        audit.base.id,
    )?;
    db.work_item_command_receipts().create(&receipt, executor).await?;
    Ok(())
}
