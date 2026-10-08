//! 任务始终按当前具体负责人和当前任务版本验证，冻结负责人只作历史。

use application_core::AuditActor;
use erp_workflow::entity::work_item::SupplierPortalReviewIdentity;
use erp_workflow::repository::WorkItemExt;
use erp_workflow::repository::prelude::*;
use persistence_core::Executor;

use super::{PortalReview, SupplierPortalProcess};
use crate::adapters::workflow::work_item_service;
use crate::{Error, Result};

impl SupplierPortalProcess {
    /// 按当前任务版本和申请主题校验门户审核任务仍可由该内部处理人处理。
    ///
    /// # 参数
    /// * `request_id` - 申请 ID。
    /// * `subject` - 当前任务主题。
    /// * `input` - 含任务 ID 与期望任务版本的审核输入。
    /// * `actor` - 当前内部处理人。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 任务身份、版本和处理人匹配时无返回值。
    ///
    /// # 错误
    /// 工作项校验失败时返回对应错误。
    pub(super) async fn validate_task(
        &self,
        request_id: &str,
        subject: &str,
        input: &PortalReview,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        work_item_service(self.db.clone(), self.rbac.clone())
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

    /// 在调用方事务内完成门户审核任务。
    ///
    /// # 参数
    /// * `request_id` - 申请 ID。
    /// * `subject` - 当前任务主题。
    /// * `input` - 含任务 ID 与期望任务版本的审核输入。
    /// * `actor` - 当前内部处理人。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 任务已完成时无返回值。
    ///
    /// # 错误
    /// 工作项完成失败时返回对应错误。
    pub(super) async fn complete_task(
        &self,
        request_id: &str,
        subject: &str,
        input: &PortalReview,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        work_item_service(self.db.clone(), self.rbac.clone())
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

    /// 按任务当前版本撤回供应商门户审核任务。
    ///
    /// # 参数
    /// * `request_id` - 申请 ID。
    /// * `subject` - 当前任务主题。
    /// * `task_id` - 待撤回的工作项 ID。
    /// * `actor_id` - 撤回人账号 ID。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 任务已按当前版本撤回时无返回值。
    ///
    /// # 错误
    /// 任务不存在时返回 `ConflictError`。读取或撤回失败时返回对应错误。
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
        work_item_service(self.db.clone(), self.rbac.clone())
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
