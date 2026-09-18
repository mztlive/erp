//! 集成错误任务的登记根；列表在领域服务，只读详情在读模型。
//!
//! 人工业务动作只通过 `task_decision` 的 W29 强命令；责任退回、转交和关闭只通过
//! W02 责任 API。本模块不保留旧动作入口。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_integration::dto::*;
use erp_integration::entity::integration_ops::IntegrationErrorTask;
use erp_integration::service::error_task::prepare_error_task;
use validator::Validate;

use super::IntegrationResolutionProcess;
use super::creation_writes::{CreatedFact, persist_created};
use super::producer::error_work_item;
use crate::Result;

impl IntegrationResolutionProcess {
    /// 登记集成错误任务。
    ///
    /// # 错误
    /// 请求非法、关联消息不存在或唯一性冲突时返回错误。
    pub async fn create_error_task(
        &self,
        req: CreateErrorTaskRequest,
        actor: &AuditActor,
    ) -> Result<ErrorTaskView> {
        req.validate()?;
        if let Some(message_id) = &req.message_id {
            self.domain().ensure_message_exists(message_id.as_ref()).await?;
        }
        let org = self
            .domain()
            .access()
            .require_handler_org(
                &req.owner_user_id,
                erp_core::common::time::Instant::now(),
                &mut persistence_core::NoTransaction,
            )
            .await?;
        let task = prepare_error_task(&req, org)?;
        let work_item = error_work_item(&task)?;
        self.store_error_task(task.clone(), work_item, actor).await?;
        Ok(task.into())
    }

    async fn store_error_task(
        &self,
        task: IntegrationErrorTask,
        work_item: erp_workflow::entity::work_item::WorkItem,
        actor: &AuditActor,
    ) -> Result<()> {
        let audit = actor.clone().resource_log(
            "integration_error_task.create",
            "integration_error_task",
            task.base.id.clone(),
        )?;
        self.run_audited(move |db, executor| {
            Box::pin(async move {
                persist_created(db, CreatedFact::ErrorTask(&task), &work_item, &audit, executor).await?;
                Ok(())
            })
        })
        .await
    }
}
