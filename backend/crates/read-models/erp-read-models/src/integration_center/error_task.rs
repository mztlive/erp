use std::slice;

use application_core::AuditActor;
use erp_integration::dto::*;
use erp_integration::entity::integration_ops::{
    ErrorActionProjection, IntegrationErrorTask, error_terminal_policy, project_error_actions,
};
use erp_integration::ports::evidence::EvidenceSubject;
use erp_integration::service::evidence::{blocker_view, domain_kinds, error_evidence_policy};
use erp_permissions::integration_ops;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::repository::prelude::*;
use persistence_core::{Executor, Transactional};

use super::repository::error_task as authorized_error_task;
use super::{IntegrationCenterReadService, IntegrationErrorDetailView};
use crate::{Error, Result};

impl IntegrationCenterReadService {
    /// 查询集成错误任务详情，独立按 detail 动作解析。
    ///
    /// # 参数
    /// * `id` - 任务稳定 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回任务详情、开放动作与证据视图。
    ///
    /// # 错误
    /// 任务不存在或不在范围内时返回 NotFound。
    pub async fn error_task_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<IntegrationErrorDetailView> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.error_detail(&id, &actor, executor).await })
            })
            .await
    }

    /// 授权、根记录和全部关联事实在同一读取阶段内完成后统一交付。
    async fn error_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<IntegrationErrorDetailView> {
        let access = self.access.resolve(actor, &integration_ops::ERROR_TASK_DETAIL, executor).await?;
        let task = authorized_error_task(&self.db, id, &access, executor).await?;

        let view = self.error_view(task, executor).await?;
        self.access.validate(&access, executor).await?;
        Ok(view)
    }

    /// 只装配当前授权根的证据、责任、名称和审计，不切换执行器。
    async fn error_view(
        &self,
        task: IntegrationErrorTask,
        executor: &mut dyn Executor,
    ) -> Result<IntegrationErrorDetailView> {
        let resolution = task.resolution.clone();
        let work_item = self.find_task_work_item(&task.base.id, executor).await?;

        let subject = EvidenceSubject::error(&task);
        let linked_evidence = self.evidence.discover_evidence(&subject, executor).await?;
        let policy = error_evidence_policy(&task);
        let (allowed_actions, action_blockers) =
            error_action_projection(&task, work_item.is_some(), &linked_evidence);
        let resolution_evidence_policy = (!task.is_terminal() && work_item.is_some()).then_some(policy);

        let mut task = task.into();
        self.error_task_names(slice::from_mut(&mut task), executor).await?;
        let work_item_id = work_item.map(|item| item.base.id);

        let connection_failure = self.connection_failure(&task, executor).await?;
        let audit_history = self.error_audit_history(&task.id, work_item_id.as_deref(), executor).await?;

        let detail = ErrorTaskDetailView {
            task,
            resolution,
            allowed_actions,
            action_blockers,
            linked_evidence,
            resolution_evidence_policy,
        };
        Ok(IntegrationErrorDetailView { detail, work_item_id, connection_failure, audit_history })
    }

    /// 多个正式责任关联时返回 `ConflictError`，避免任选一条任务。
    async fn find_task_work_item(
        &self,
        task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        let mut items =
            self.db.work_items().find_unique_for_integration_error_task(task_id, executor).await?;
        if items.len() > 1 {
            return Err(Error::ConflictError("错误任务存在多个正式责任关联".to_string()));
        }
        Ok(items.pop())
    }
}

/// 推导错误任务开放动作与阻断视图（动作规则归领域，此处只做 view 映射）。
///
/// # 参数
/// * `task` - 集成错误任务（提供终态与重放条件）
/// * `has_work_item` - 是否已建立正式任务
/// * `linked_evidence` - 服务端发现的受控证据
///
/// # 返回
/// 返回开放动作代码与阻断视图。
fn error_action_projection(
    task: &IntegrationErrorTask,
    has_work_item: bool,
    linked_evidence: &[ControlledEvidenceRef],
) -> (Vec<String>, Vec<ActionBlockerView>) {
    let (actions, blockers) = project_error_actions(ErrorActionProjection {
        terminal: task.is_terminal(),
        has_work_item,
        can_replay: task.can_replay_original(),
        present: domain_kinds(linked_evidence),
        policy: error_terminal_policy(task),
    });
    (
        actions.iter().map(|action| action.as_str().to_string()).collect(),
        blockers.iter().map(blocker_view).collect(),
    )
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::integration_center::testing::service;

    #[tokio::test]
    async fn detail_stops_before_root_or_evidence_reads_when_authorization_is_unavailable() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let result = service().await.error_detail("task", &actor, &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::ValidationError(reason)) if reason == "缺少查询 Mapping 绑定"));
    }
}
