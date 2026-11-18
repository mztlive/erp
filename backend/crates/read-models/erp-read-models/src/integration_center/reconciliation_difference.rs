use std::slice;

use application_core::AuditActor;
use erp_core::ids::ReconciliationDifferenceId;
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_integration::dto::*;
use erp_integration::entity::integration_ops::{
    DifferenceActionProjection, ReconciliationDifference, difference_terminal_policy,
    project_difference_actions,
};
use erp_integration::ports::evidence::EvidenceSubject;
use erp_integration::repository::IntegrationOpsExt;
use erp_integration::repository::integration_ops::ResolutionHistoryRow;
use erp_integration::repository::prelude::*;
use erp_integration::service::evidence::{
    blocker_view, difference_evidence_policy, domain_kinds, reconciliation_reason_registry,
};
use erp_permissions::integration_ops;
use erp_workflow::WorkItemExt;
use erp_workflow::repository::prelude::*;
use persistence_core::{Executor, Transactional};

use super::IntegrationCenterReadService;
use super::display::evidence_label;
use super::repository::difference as authorized_difference;
use crate::{Error, Result};

impl IntegrationCenterReadService {
    /// 查询对账差异详情与不可变决定时间线，独立按 detail 动作解析。
    ///
    /// # 参数
    /// * `id` - 差异稳定 ID
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回差异详情、处理时间线与证据视图。
    ///
    /// # 错误
    /// 差异不存在或不在范围内时返回 NotFound。
    pub async fn difference_detail(&self, id: &str, actor: &AuditActor) -> Result<DifferenceDetailView> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.difference_snapshot(&id, &actor, executor).await })
            })
            .await
    }

    /// 原查询凭证覆盖根记录、处理历史及证据装配，全部读取后再交付。
    async fn difference_snapshot(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<DifferenceDetailView> {
        let access = self.access.resolve(actor, &integration_ops::DIFFERENCE_DETAIL, executor).await?;
        let difference = authorized_difference(&self.db, id, &access, executor).await?;

        let history = self
            .db
            .reconciliation_difference_resolutions()
            .search_resolutions(&ReconciliationDifferenceId::new(difference.base.id.clone()), executor)
            .await?;
        let view = self.assemble_difference_detail(difference, history, executor).await?;

        self.access.validate(&access, executor).await?;
        Ok(view)
    }

    /// 最近一条决议覆盖状态与版本；终态不再附证据策略。
    async fn assemble_difference_detail(
        &self,
        difference: ReconciliationDifference,
        history: Vec<ResolutionHistoryRow>,
        executor: &mut dyn Executor,
    ) -> Result<DifferenceDetailView> {
        let subject = EvidenceSubject::difference(&difference);
        let mut view: DifferenceView = difference.clone().into();
        if let Some(latest) = history.last() {
            view.status = Some(latest.resulting_status);
            view.version = u64::from(latest.resolution_no);
        }
        let mut resolutions = resolution_views(history);

        let terminal = view.status.is_some_and(|status| status.is_terminal());
        let has_work_item = self.has_difference_work_item(&view.id, executor).await?;
        let linked_evidence = self.evidence.discover_evidence(&subject, executor).await?;

        self.difference_names(slice::from_mut(&mut view), executor).await?;
        view.left_fact_label = evidence_label(view.left_fact_reference.as_deref(), &linked_evidence);
        view.right_fact_label = evidence_label(view.right_fact_reference.as_deref(), &linked_evidence);
        self.resolution_names(&mut resolutions, &linked_evidence, executor).await?;

        let policy = difference_evidence_policy(&difference);
        let (allowed_actions, action_blockers) =
            difference_action_projection(&difference, terminal, has_work_item, &linked_evidence);

        Ok(DifferenceDetailView {
            difference: view,
            resolutions,
            allowed_actions,
            action_blockers,
            linked_evidence,
            resolution_evidence_policy: (!terminal && has_work_item).then_some(policy),
            reconciliation_reason_registry: (!terminal && !has_work_item)
                .then(reconciliation_reason_registry),
        })
    }

    async fn resolution_names(
        &self,
        resolutions: &mut [ResolutionView],
        linked_evidence: &[ControlledEvidenceRef],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = resolutions.iter().map(|item| item.handled_by.clone()).collect::<Vec<_>>();
        let names = self.db.accounts().names_by_ids(&ids, executor).await?;
        for resolution in resolutions {
            resolution.handled_by_name = names.get(&resolution.handled_by).cloned();
            resolution.evidence_label =
                evidence_label(resolution.evidence_reference.as_deref(), linked_evidence);
        }
        Ok(())
    }

    /// 多个正式责任关联时返回 `ConflictError`。
    async fn has_difference_work_item(
        &self,
        difference_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let items =
            self.db.work_items().find_unique_for_reconciliation_difference(difference_id, executor).await?;
        if items.len() > 1 {
            return Err(Error::ConflictError(
                "这条对账差异关联了多个处理待办，请联系管理员核对负责的待办".to_string(),
            ));
        }
        Ok(!items.is_empty())
    }
}

fn resolution_views(history: Vec<ResolutionHistoryRow>) -> Vec<ResolutionView> {
    history
        .into_iter()
        .map(|row| ResolutionView {
            id: row.id,
            resolution_no: row.resolution_no,
            resolution_action: row.resolution_action,
            resulting_status: row.resulting_status,
            evidence_reference: row.evidence_reference,
            evidence_label: None,
            handled_by: row.handled_by,
            handled_by_name: None,
            handled_at: row.handled_at.unix_secs(),
        })
        .collect()
}

/// 推导对账差异开放动作与阻断视图（动作规则归领域，此处只做 view 映射）。
///
/// # 参数
/// * `difference` - 对账差异（提供资金影响策略）
/// * `terminal` - 差异是否已形成正式结论
/// * `has_work_item` - 是否已建立正式任务
/// * `linked_evidence` - 服务端发现的受控证据
///
/// # 返回
/// 返回开放动作代码与阻断视图。
fn difference_action_projection(
    difference: &ReconciliationDifference,
    terminal: bool,
    has_work_item: bool,
    linked_evidence: &[ControlledEvidenceRef],
) -> (Vec<String>, Vec<ActionBlockerView>) {
    let (actions, blockers) = project_difference_actions(DifferenceActionProjection {
        terminal,
        has_work_item,
        present: domain_kinds(linked_evidence),
        policy: difference_terminal_policy(difference),
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
    async fn detail_stops_before_root_history_or_evidence_reads_when_authorization_is_unavailable() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let result =
            service().await.difference_snapshot("difference", &actor, &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::ValidationError(reason)) if reason == "缺少查询 Mapping 绑定"));
    }
}
