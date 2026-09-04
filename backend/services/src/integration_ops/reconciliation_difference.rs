//! 对账差异正式事实的登记、列表与只读详情。
//!
//! 差异事实创建后不可修改；决定只由 `task_decision` 追加，不在本模块暴露旧处理或
//! 解决命令。

use database::{AccessControlExt, IntegrationOpsExt, NoTransaction, WorkItemExt};
use entities::integration_ops::{
    difference_terminal_policy, project_difference_actions, DifferenceActionProjection,
    ReconciliationDifference, ReconciliationDifferenceData, ReconciliationDifferenceId,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::SortDir;
use super::evidence::{
    blocker_view, difference_evidence_policy, domain_kinds, reconciliation_reason_registry, EvidenceSubject,
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
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
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
        let difference_ids = page.items.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let latest_by_difference = self
            .db
            .reconciliation_difference_resolutions()
            .find_latest_by_differences(&difference_ids, &mut NoTransaction)
            .await?;
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            let (status, version) = latest_by_difference.get(&row.id).map_or((None, 0), |record| {
                (Some(record.resulting_status), u64::from(record.resolution_no))
            });
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
    linked_evidence: &[super::ControlledEvidenceRef],
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
    /// 生产代码（测试模块之前部分），供分层守卫断言，避免字面量自匹配。
    ///
    /// # 返回
    /// 返回去掉测试模块后的生产代码全文。
    fn production_source() -> &'static str {
        include_str!("reconciliation_difference.rs")
            .split("mod tests {")
            .next()
            .expect("必须存在生产代码")
    }

    /// 分层守卫（INT-R26）：差异列表经单次批量装载最新决定，无逐行查询。
    ///
    /// 锁定 `find_latest_by_differences` 单次批量入口与缺失映射为无状态版本零；
    /// 逐行 `difference_state` 与单条 `find_latest_by_difference` 不得回潮。
    #[test]
    fn difference_list_resolves_latest_via_single_batch() {
        let source = production_source();
        assert!(source.contains("find_latest_by_differences(&difference_ids"));
        assert!(source.contains("map_or((None, 0)"));
        assert!(!source.contains("fn difference_state"));
        assert!(!source.contains("find_latest_by_difference("));
    }

    /// 分层守卫（INT-E17）：至少一侧证据引用不变量由实体独占，服务只映射错误类别。
    ///
    /// 锁定服务不再保留重复业务判断，实体错误统一映射为 `ValidationError`；
    /// 四格矩阵（均无/仅左/仅右/两者）由实体单测覆盖，此处只锁定归属。
    #[test]
    fn create_difference_defers_reference_invariant_to_entity() {
        let source = production_source();
        assert!(!source.contains("left_fact_reference.is_none() && req.right_fact_reference.is_none()"));
        assert!(source.contains("ReconciliationDifference::new("));
        assert!(source.contains("Error::ValidationError(error.to_string())"));
    }
}
