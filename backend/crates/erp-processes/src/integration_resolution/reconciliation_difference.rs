//! 对账差异正式事实的登记根；列表在领域服务，只读详情在读模型。
//!
//! 差异事实创建后不可修改；决定只由 `task_decision` 追加，不在本模块暴露旧处理或
//! 解决命令。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_integration::dto::*;
use erp_integration::entity::integration_ops::ReconciliationDifference;
use erp_integration::service::reconciliation_difference::prepare_difference;
use validator::Validate;

use super::IntegrationResolutionProcess;
use super::creation_writes::{CreatedFact, persist_created};
use super::producer::difference_work_item;
use crate::Result;

impl IntegrationResolutionProcess {
    /// 登记不可变对账差异事实。
    ///
    /// # 错误
    /// 请求非法、两侧证据均缺失或唯一性冲突时返回错误。
    pub async fn create_difference(
        &self,
        req: CreateDifferenceRequest,
        actor: &AuditActor,
    ) -> Result<DifferenceView> {
        req.validate()?;
        let org = self
            .domain()
            .access()
            .require_handler_org(
                &req.owner_user_id,
                erp_core::common::time::Instant::now(),
                &mut persistence_core::NoTransaction,
            )
            .await?;
        let difference = prepare_difference(&req, org)?;
        let work_item = difference_work_item(&difference)?;
        self.store_difference(difference.clone(), work_item, actor).await?;
        Ok(difference.into())
    }

    async fn store_difference(
        &self,
        difference: ReconciliationDifference,
        work_item: erp_workflow::entity::work_item::WorkItem,
        actor: &AuditActor,
    ) -> Result<()> {
        let audit = actor.clone().resource_log(
            "reconciliation_difference.create",
            "reconciliation_difference",
            difference.base.id.clone(),
        )?;
        self.run_audited(move |db, executor| {
            Box::pin(async move {
                persist_created(db, CreatedFact::Difference(&difference), &work_item, &audit, executor)
                    .await?;
                Ok(())
            })
        })
        .await
    }
}
