use application_core::AuditActor;
use erp_integration::dto::*;
use erp_integration::entity::integration_ops::{
    ErrorActionProjection, IntegrationErrorTask, error_terminal_policy, project_error_actions,
};
use erp_integration::ports::evidence::EvidenceSubject;
use erp_integration::repository::IntegrationOpsExt;
use erp_integration::service::evidence::{blocker_view, domain_kinds, error_evidence_policy};
use erp_workflow::WorkItemExt;
use persistence_core::NoTransaction;

use super::IntegrationCenterReadService;
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
    pub async fn error_task_detail(&self, id: &str, actor: &AuditActor) -> Result<ErrorTaskDetailView> {
        let task = self
            .db
            .integration_error_tasks()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("任务不存在".to_string()))?;
        self.require_visible(
            actor,
            "integration_error_task",
            task.owner_user_id.as_deref().unwrap_or(""),
            &task.owner_org_unit_id,
        )
        .await?;
        let resolution = task.resolution.clone();
        let work_item = self.find_task_work_item(&task.base.id).await?;
        let subject = EvidenceSubject::error(&task);
        let linked_evidence = self.evidence.discover_evidence(&subject, &mut NoTransaction).await?;
        let policy = error_evidence_policy(&task);
        let (allowed_actions, action_blockers) =
            error_action_projection(&task, work_item.is_some(), &linked_evidence);
        let resolution_evidence_policy = (!task.is_terminal() && work_item.is_some()).then_some(policy);
        Ok(ErrorTaskDetailView {
            task: task.into(),
            resolution,
            allowed_actions,
            action_blockers,
            linked_evidence,
            resolution_evidence_policy,
        })
    }

    async fn find_task_work_item(
        &self,
        task_id: &str,
    ) -> Result<Option<erp_workflow::entity::work_item::WorkItem>> {
        let mut items =
            self.db.work_items().find_unique_for_integration_error_task(task_id, &mut NoTransaction).await?;
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
