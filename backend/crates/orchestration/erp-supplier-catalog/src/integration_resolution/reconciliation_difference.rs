//! 对账差异正式事实的登记根；列表在领域服务，只读详情在读模型。
//!
//! 差异事实创建后不可修改；决定只由 `task_decision` 追加，不在本模块暴露旧处理或
//! 解决命令。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_identity::AccessTarget;
use erp_integration::dto::*;
use erp_integration::entity::integration_ops::ReconciliationDifference;
use erp_integration::service::reconciliation_difference::prepare_difference;
use erp_permissions::integration_ops;
use erp_workflow::entity::work_item::WorkItem;
use persistence_core::Executor;
use validator::Validate;

use super::IntegrationResolutionProcess;
use super::creation_writes::{CreatedFact, persist_created};
use super::producer::difference_work_item;
use crate::Result;

impl IntegrationResolutionProcess {
    /// 登记不可变对账差异事实。
    ///
    /// `req`：差异登记请求。
    ///
    /// 新建差异事实的视图。
    ///
    /// 请求非法、责任组织无法解析、两侧证据均缺失、差异或正式责任构造失败、唯一性冲突或事务写入失败时返回对应错误。
    pub async fn create_difference(
        &self,
        req: CreateDifferenceRequest,
        actor: &AuditActor,
    ) -> Result<DifferenceView> {
        let process = self.clone();
        let actor = actor.clone();
        self.run_audited(move |_, executor| {
            Box::pin(async move { process.create_difference_stage(req, &actor, executor).await })
        })
        .await
    }

    // 原事务内先授权，再读取责任及来源并原子登记事实和工作项。
    async fn create_difference_stage(
        &self,
        req: CreateDifferenceRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<DifferenceView> {
        self.authorization
            .require(actor, &integration_ops::DIFFERENCE_CREATE, AccessTarget::Operation, executor)
            .await?;

        req.validate()?;
        let org =
            self.domain().access().require_handler_org(&req.owner_user_id, Instant::now(), executor).await?;

        let difference = prepare_difference(&req, org)?;
        let work_item = difference_work_item(&difference)?;
        self.store_difference(difference.clone(), work_item, actor, executor).await?;
        Ok(difference.into())
    }

    async fn store_difference(
        &self,
        difference: ReconciliationDifference,
        work_item: WorkItem,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let audit = actor.clone().resource_log(
            "reconciliation_difference.create",
            "reconciliation_difference",
            difference.base.id.clone(),
        )?;
        persist_created(&self.db, CreatedFact::Difference(&difference), &work_item, &audit, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod authorization_tests {
    use persistence_core::testing::SnapshotExecutor;
    use serde_json::json;

    use super::super::testing::{actor, denied, process};

    #[tokio::test]
    async fn creation_authorizes_before_reading_handler_organization_or_evidence() {
        let process = process().await;
        let request = serde_json::from_value(json!({"business_object_type":"order", "business_object_id":"unread-order", "difference_type":"amount", "left_fact_reference":"left", "owner_user_id":"unread-owner"})).unwrap();
        denied(process.create_difference_stage(request, &actor(), &mut SnapshotExecutor::default()).await);
    }
}
