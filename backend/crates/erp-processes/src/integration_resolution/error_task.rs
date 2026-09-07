//! 集成错误任务的登记根；列表在领域服务，只读详情在读模型。
//!
//! 人工业务动作只通过 `task_decision` 的 W29 强命令；责任退回、转交和关闭只通过
//! W02 责任 API。本模块不保留旧动作入口。
use super::creation_writes::{persist_created, CreatedFact};
use super::producer::error_work_item;
use super::IntegrationResolutionProcess;
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_integration::dto::*;
use erp_integration::entity::integration_ops::IntegrationErrorTask;
use erp_integration::service::error_task::prepare_error_task;
use services::Result;
use validator::Validate;

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
        let task = prepare_error_task(&req)?;
        let work_item = error_work_item(&task, &req.owner_user_id)?;
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
        self.run_audited(move |db, session| {
            Box::pin(async move {
                persist_created(db, CreatedFact::ErrorTask(&task), &work_item, &audit, session).await?;
                Ok(())
            })
        })
        .await
    }
}
