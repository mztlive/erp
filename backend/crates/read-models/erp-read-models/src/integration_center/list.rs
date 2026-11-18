//! 集成列表根记录、名称补齐和授权交付共用原读取阶段。
use application_core::AuditActor;
use erp_integration::IntegrationOpsService;
use erp_integration::dto::{
    DifferenceListParams, DifferenceListView, ErrorTaskListParams, ErrorTaskListView,
};
use persistence_core::{Executor, Transactional};

use super::IntegrationCenterReadService;
use crate::Result;

impl IntegrationCenterReadService {
    /// 查询完整错误任务分页，包括责任人与业务对象展示名。
    ///
    /// # 参数
    /// `params` 为列表筛选，`actor` 为当前已认证账号。
    ///
    /// # 返回
    /// 同次授权快照中完成根读取与名称补齐并验证后的分页。
    ///
    /// # 错误
    /// 无资格、筛选非法、范围变化或根记录及关联读取失败时拒绝整页。
    pub async fn error_task_list(
        &self,
        params: &ErrorTaskListParams,
        actor: &AuditActor,
    ) -> Result<ErrorTaskListView> {
        let service = self.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.error_list_snapshot(&params, &actor, executor).await })
            })
            .await
    }

    /// 查询完整对账差异分页，包括责任人与业务对象展示名。
    ///
    /// # 参数
    /// `params` 为列表筛选，`actor` 为当前已认证账号。
    ///
    /// # 返回
    /// 同次授权快照中完成根读取与名称补齐并验证后的分页。
    ///
    /// # 错误
    /// 无资格、筛选非法、范围变化或根记录及关联读取失败时拒绝整页。
    pub async fn difference_list(
        &self,
        params: &DifferenceListParams,
        actor: &AuditActor,
    ) -> Result<DifferenceListView> {
        let service = self.clone();
        let params = params.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.difference_list_snapshot(&params, &actor, executor).await })
            })
            .await
    }

    /// 根读取签发凭证后仅在原执行器补齐名称，最后交付一次。
    async fn error_list_snapshot(
        &self,
        params: &ErrorTaskListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ErrorTaskListView> {
        let domain = IntegrationOpsService::new(self.db.clone(), self.access.clone());
        let (mut page, access) = domain.error_task_list_read(params, actor, executor).await?;
        self.error_task_names(&mut page.data.items, executor).await?;
        self.access.validate(&access, executor).await?;
        Ok(page)
    }

    /// 根读取签发凭证后仅在原执行器补齐名称，最后交付一次。
    async fn difference_list_snapshot(
        &self,
        params: &DifferenceListParams,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<DifferenceListView> {
        let domain = IntegrationOpsService::new(self.db.clone(), self.access.clone());
        let (mut page, access) = domain.difference_list_read(params, actor, executor).await?;
        self.difference_names(&mut page.data.items, executor).await?;
        self.access.validate(&access, executor).await?;
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;
    use crate::integration_center::testing::service;

    #[tokio::test]
    async fn lists_stop_before_root_and_display_reads_when_authorization_is_unavailable() {
        let service = service().await;
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let errors = service
            .error_list_snapshot(&ErrorTaskListParams::default(), &actor, &mut SnapshotExecutor::default())
            .await;
        assert!(matches!(errors, Err(Error::ValidationError(reason)) if reason == "缺少查询 Mapping 绑定"));

        let differences = service
            .difference_list_snapshot(
                &DifferenceListParams::default(),
                &actor,
                &mut SnapshotExecutor::default(),
            )
            .await;
        assert!(
            matches!(differences, Err(Error::ValidationError(reason)) if reason == "缺少查询 Mapping 绑定")
        );
    }
}
