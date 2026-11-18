//! 任务始终按当前具体负责人和当前任务版本验证，冻结负责人只作历史。

use application_core::AuditActor;
use erp_workflow::entity::work_item::SupplierPortalReviewIdentity;
use erp_workflow::repository::WorkItemExt;
use erp_workflow::repository::prelude::*;
use persistence_core::Executor;

use super::{PortalReview, SupplierPortalProcess};
use crate::{Error, Result};

impl SupplierPortalProcess {
    // 当前完整办理资格与真实申请关联先于命令回放；不要求旧任务仍开放。
    //
    // request_id、task_id 为原命令申请及任务；actor 为当前人，executor 为原事务。
    //
    // 当前仍有完整资格并属于原申请时成功。
    //
    // 资格撤销、指派改变、任务或关联缺失时拒绝。
    pub(super) async fn require_review_result(
        &self,
        request_id: &str,
        task_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.services.work_items.require_result_access(actor, task_id, executor).await?;

        let task = self
            .db
            .work_items()
            .find_work_item(task_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商申请任务不存在".into()))?;
        if !task.matches_supplier_portal_review(SupplierPortalReviewIdentity {
            task_id,
            request_id,
            subject_version: &task.subject_version,
            task_version: task.base.version,
        }) {
            return Err(Error::ConflictError("任务与供应商申请不一致".into()));
        }
        Ok(())
    }

    // 按当前任务版本和申请主题校验门户审核任务仍可由该内部处理人处理。
    //
    // - `request_id`：申请 ID。
    // - `subject`：当前任务主题。
    // - `input`：含任务 ID 与期望任务版本的审核输入。
    // - `actor`：当前内部处理人。
    // - `executor`：调用方执行器。
    //
    // 任务身份、版本和处理人匹配时无返回值。
    //
    // 工作项校验失败时返回对应错误。
    pub(super) async fn validate_task(
        &self,
        request_id: &str,
        subject: &str,
        input: &PortalReview,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.services
            .work_items
            .validate_supplier_portal_review(
                SupplierPortalReviewIdentity {
                    task_id: &input.work_item_id,
                    request_id,
                    subject_version: subject,
                    task_version: input.work_item_version,
                },
                actor,
                executor,
            )
            .await?;
        Ok(())
    }

    // 在调用方事务内完成门户审核任务。
    //
    // - `request_id`：申请 ID。
    // - `subject`：当前任务主题。
    // - `input`：含任务 ID 与期望任务版本的审核输入。
    // - `actor`：当前内部处理人。
    // - `executor`：调用方事务执行器。
    //
    // 任务已完成时无返回值。
    //
    // 工作项完成失败时返回对应错误。
    pub(super) async fn complete_task(
        &self,
        request_id: &str,
        subject: &str,
        input: &PortalReview,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.services
            .work_items
            .complete_supplier_portal_review(
                SupplierPortalReviewIdentity {
                    task_id: &input.work_item_id,
                    request_id,
                    subject_version: subject,
                    task_version: input.work_item_version,
                },
                actor,
                executor,
            )
            .await?;
        Ok(())
    }

    // 按任务当前版本撤回供应商门户审核任务。
    //
    // - `request_id`：申请 ID。
    // - `subject`：当前任务主题。
    // - `task_id`：待撤回的工作项 ID。
    // - `actor_id`：撤回人账号 ID。
    // - `executor`：调用方事务执行器。
    //
    // 任务已按当前版本撤回时无返回值。
    //
    // 任务不存在时返回 `ConflictError`。读取或撤回失败时返回对应错误。
    pub(super) async fn withdraw_task(
        &self,
        request_id: &str,
        subject: &str,
        task_id: &str,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = self
            .db
            .work_items()
            .find_work_item(task_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("确认任务已变化".into()))?;
        self.services
            .work_items
            .withdraw_supplier_portal_review(
                SupplierPortalReviewIdentity {
                    task_id,
                    request_id,
                    subject_version: subject,
                    task_version: current.base.version,
                },
                actor_id,
                "供应商撤回申请",
                executor,
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod authorization_tests {
    use erp_core::AccountKind;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{Executor, ReadExecutor};

    use super::*;

    #[tokio::test]
    async fn review_replay_requires_current_task_execution_before_reading_request_or_receipt() {
        let process = super::super::testing::unavailable_process().await;
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        for executor in [&mut ReadExecutor::default() as &mut dyn Executor, &mut SnapshotExecutor::default()]
        {
            let result = process.require_review_result("request", "task", &actor, executor).await;
            // require_result_access 先执行普通 work_item:detail；两类执行器都应进入真实身份取证。
            assert!(matches!(result, Err(Error::Internal(message)) if message == "测试授权设施不可用"));
        }
    }
}
