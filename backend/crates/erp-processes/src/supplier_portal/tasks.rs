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
