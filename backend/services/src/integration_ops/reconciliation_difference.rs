//! 对账差异正式事实的登记、列表与只读详情。
//!
//! 差异事实创建后不可修改；决定只由 `task_decision` 追加，不在本模块暴露旧处理或
//! 解决命令。

use database::{AccessControlExt, IntegrationOpsExt, NoTransaction, WorkItemExt};
use entities::integration_ops::{
    ReconciliationDifference, ReconciliationDifferenceData, ReconciliationDifferenceId,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::SortDir;
use super::evidence::{
    difference_evidence_policy, evidence_satisfies_policy, reconciliation_reason_registry, EvidenceSubject,
    IntegrationEvidenceAuthority,
};
use super::producer::difference_work_item;
use super::{
    ActionBlockerView, CreateDifferenceRequest, DifferenceDetailView, DifferenceFilter, DifferenceListParams,
    DifferenceView, IntegrationOpsService, PageView, ResolutionView,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl IntegrationOpsService {
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
        if req.left_fact_reference.is_none() && req.right_fact_reference.is_none() {
            return Err(Error::ValidationError(
                "差异必须至少提供一侧不可变证据引用".to_string(),
            ));
        }
        let owner_user_id = req.owner_user_id.clone();
        let difference = ReconciliationDifference::new(
            ReconciliationDifferenceId::new(next_id()),
            ReconciliationDifferenceData {
                business_object_type: req.business_object_type,
                business_object_id: req.business_object_id,
                difference_type: req.difference_type,
                left_fact_reference: req.left_fact_reference,
                right_fact_reference: req.right_fact_reference,
            },
        )?;
        let work_item = difference_work_item(&difference, &owner_user_id)?;
        self.store_difference(difference.clone(), work_item, actor)
            .await?;
        Ok(difference.into())
    }

    /// 分页查询对账差异，并按最新决定派生状态与版本。
    ///
    /// # 错误
    /// 查询参数非法或仓储查询失败时返回错误。
    pub async fn difference_list(&self, params: &DifferenceListParams) -> Result<PageView<DifferenceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = DifferenceFilter {
            business_object_type: query.business_object_type,
            business_object_id: query.business_object_id,
            difference_type: query.difference_type,
            created_at_from: query.created_at_from,
            created_at_to: query.created_at_to,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .reconciliation_differences()
            .search_differences(&filter, &mut NoTransaction)
            .await?;
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            let (status, version) = self.difference_state(&row.id).await?;
            items.push(DifferenceView {
                id: row.id,
                business_object_type: row.business_object_type,
                business_object_id: row.business_object_id,
                difference_type: row.difference_type,
                left_fact_reference: row.left_fact_reference,
                right_fact_reference: row.right_fact_reference,
                status,
                version,
                created_at: row.created_at,
            });
        }
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

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
        let linked_evidence = self.db.discover_evidence(&subject, &mut NoTransaction).await?;
        let policy = difference_evidence_policy(&difference);
        let (allowed_actions, action_blockers) =
            difference_action_projection(terminal, has_work_item, &linked_evidence, &policy);
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
            .find_many(
                mongodb::bson::doc! {
                    "business_object_type": "reconciliation_difference",
                    "business_object_id": difference_id,
                },
                &mut NoTransaction,
            )
            .await?;
        if items.len() > 1 {
            return Err(Error::ConflictError("对账差异存在多个正式责任关联".to_string()));
        }
        Ok(!items.is_empty())
    }

    async fn store_difference(
        &self,
        difference: ReconciliationDifference,
        work_item: entities::work_item::WorkItem,
        actor: &AuditActor,
    ) -> Result<()> {
        let audit = actor.clone().resource_log(
            "reconciliation_difference.create",
            "reconciliation_difference",
            difference.base.id.clone(),
        )?;
        self.run_audited(move |db, session| {
            Box::pin(async move {
                db.reconciliation_differences()
                    .create(&difference, session)
                    .await?;
                db.work_items().create(&work_item, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok(())
            })
        })
        .await
    }

    async fn difference_state(
        &self,
        id: &str,
    ) -> Result<(Option<entities::integration_ops::ResultingStatus>, u64)> {
        let latest = self
            .db
            .reconciliation_difference_resolutions()
            .find_latest_by_difference(
                &ReconciliationDifferenceId::new(id.to_string()),
                &mut NoTransaction,
            )
            .await?;
        Ok(latest.map_or((None, 0), |record| {
            (Some(record.resulting_status), u64::from(record.resolution_no))
        }))
    }
}

fn difference_action_projection(
    terminal: bool,
    has_work_item: bool,
    linked_evidence: &[super::ControlledEvidenceRef],
    policy: &super::ResolutionEvidencePolicyView,
) -> (Vec<String>, Vec<ActionBlockerView>) {
    if terminal {
        return (Vec::new(), Vec::new());
    }
    let mut actions = vec!["QUERY_ORIGINAL_RESULT".to_string(), "ADD_EVIDENCE".to_string()];
    if linked_evidence
        .iter()
        .any(|evidence| evidence.kind == super::ControlledEvidenceKind::BusinessObjectVerification)
    {
        actions.push("REATTRIBUTE".to_string());
    }
    if linked_evidence
        .iter()
        .any(|evidence| evidence.kind == super::ControlledEvidenceKind::CompensationResult)
    {
        actions.push("LINK_COMPENSATION".to_string());
    }
    if has_work_item {
        if evidence_satisfies_policy(linked_evidence, policy) {
            actions.push("RESOLVE".to_string());
            return (actions, Vec::new());
        }
        return (
            actions,
            vec![ActionBlockerView {
                action: "RESOLVE".to_string(),
                code: "VERIFIED_EVIDENCE_REQUIRED".to_string(),
                message: "终态证据尚未满足当前固定策略。".to_string(),
            }],
        );
    }
    actions.push("CONFIRM_NO_ERROR".to_string());
    actions.push("CONFIRM_VALID_DIFFERENCE".to_string());
    (actions, Vec::new())
}
