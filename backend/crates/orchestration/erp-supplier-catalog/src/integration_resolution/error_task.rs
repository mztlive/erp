//! 集成错误任务的登记根；列表在领域服务，只读详情在读模型。
//!
//! 人工业务动作只通过 `task_decision` 的 W29 强命令；责任退回、转交和关闭只通过
//! W02 责任 API。本模块不保留旧动作入口。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_identity::AccessTarget;
use erp_integration::dto::*;
use erp_integration::entity::integration_ops::IntegrationErrorTask;
use erp_integration::service::error_task::prepare_error_task;
use erp_permissions::integration_ops;
use erp_workflow::entity::work_item::WorkItem;
use persistence_core::Executor;
use validator::Validate;

use super::IntegrationResolutionProcess;
use super::creation_writes::{CreatedFact, persist_created};
use super::producer::error_work_item;
use crate::Result;

impl IntegrationResolutionProcess {
    /// 登记集成错误任务。
    ///
    /// `req`：错误任务登记请求。
    ///
    /// 新建错误任务的视图。
    ///
    /// 请求非法、关联消息不存在、责任组织无法解析、任务或正式责任构造失败、唯一性冲突或事务写入失败时返回对应错误。
    pub async fn create_error_task(
        &self,
        req: CreateErrorTaskRequest,
        actor: &AuditActor,
    ) -> Result<ErrorTaskView> {
        let process = self.clone();
        let actor = actor.clone();
        self.run_audited(move |_, executor| {
            Box::pin(async move { process.create_error_task_stage(req, &actor, executor).await })
        })
        .await
    }

    // 原事务内先授权，再读取责任及来源并原子登记事实和工作项。
    async fn create_error_task_stage(
        &self,
        req: CreateErrorTaskRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ErrorTaskView> {
        self.authorization
            .require(actor, &integration_ops::ERROR_TASK_CREATE, AccessTarget::Operation, executor)
            .await?;

        req.validate()?;
        if let Some(message_id) = &req.message_id {
            self.domain().ensure_message_exists(message_id.as_ref(), executor).await?;
        }
        let org =
            self.domain().access().require_handler_org(&req.owner_user_id, Instant::now(), executor).await?;

        let task = prepare_error_task(&req, org)?;
        let work_item = error_work_item(&task)?;
        self.store_error_task(task.clone(), work_item, actor, executor).await?;
        Ok(task.into())
    }

    async fn store_error_task(
        &self,
        task: IntegrationErrorTask,
        work_item: WorkItem,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let audit = actor.clone().resource_log(
            "integration_error_task.create",
            "integration_error_task",
            task.base.id.clone(),
        )?;
        persist_created(&self.db, CreatedFact::ErrorTask(&task), &work_item, &audit, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod authorization_tests {
    use persistence_core::testing::SnapshotExecutor;
    use serde_json::json;

    use super::super::testing::{actor, denied, process};

    #[tokio::test]
    async fn creation_authorizes_before_linked_message_or_handler_reads() {
        let process = process().await;
        for message in [None, Some("missing-message")] {
            let request = serde_json::from_value(json!({"message_id":message, "error_class":"transient_failure", "owner_user_id":"unread-owner"})).unwrap();
            denied(
                process.create_error_task_stage(request, &actor(), &mut SnapshotExecutor::default()).await,
            );
        }
    }
}
