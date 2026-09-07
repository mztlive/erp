use super::IntegrationCenterReadService;
use erp_core::ids::ReconciliationDifferenceId;
use erp_integration::dto::*;
use erp_integration::entity::integration_ops::{
    difference_terminal_policy, project_difference_actions, DifferenceActionProjection,
    ReconciliationDifference,
};
use erp_integration::ports::evidence::EvidenceSubject;
use erp_integration::repository::IntegrationOpsExt;
use erp_integration::service::evidence::{
    blocker_view, difference_evidence_policy, domain_kinds, reconciliation_reason_registry,
};
use erp_workflow::WorkItemExt;
use persistence_core::NoTransaction;
use services::{Error, Result};

impl IntegrationCenterReadService {
    /// 查询对账差异详情与不可变决定时间线。
    ///
    /// # 错误
    /// 差异不存在或仓储查询失败时返回错误。
    pub async fn difference_detail(&self, id: &str) -> Result<DifferenceDetailView> {
        let difference = self
            .db
            .reconciliation_differences()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("差异不存在".to_string()))?;
        let difference_id = ReconciliationDifferenceId::new(difference.base.id.clone());
        let history = self
            .db
            .reconciliation_difference_resolutions()
            .search_resolutions(&difference_id, &mut NoTransaction)
            .await?;
        let subject = EvidenceSubject::difference(&difference);
        let mut view: DifferenceView = difference.clone().into();
        if let Some(latest) = history.last() {
            view.status = Some(latest.resulting_status);
            view.version = u64::from(latest.resolution_no);
        }
        let resolutions = history
            .into_iter()
            .map(|row| ResolutionView {
                id: row.id,
                resolution_no: row.resolution_no,
                resolution_action: row.resolution_action,
                resulting_status: row.resulting_status,
                evidence_reference: row.evidence_reference,
                handled_by: row.handled_by,
                handled_at: row.handled_at.unix_secs(),
            })
            .collect();
        let terminal = view.status.is_some_and(|status| status.is_terminal());
        let has_work_item = self.has_difference_work_item(&view.id).await?;
        let linked_evidence = self
            .evidence
            .discover_evidence(&subject, &mut NoTransaction)
            .await?;
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

    async fn has_difference_work_item(&self, difference_id: &str) -> Result<bool> {
        let items = self
            .db
            .work_items()
            .find_unique_for_reconciliation_difference(difference_id, &mut NoTransaction)
            .await?;
        if items.len() > 1 {
            return Err(Error::ConflictError("对账差异存在多个正式责任关联".to_string()));
        }
        Ok(!items.is_empty())
    }
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
