//! 结算详情的供应链快照与工作流任务组合。
use std::collections::BTreeMap;

use application_core::AuditActor;
use erp_supply::dto::supplier_settlement as dto;
use erp_supply::entity::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementDifferenceEvidence, SupplierSettlementStatement,
};
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::repository::supplier_settlement::SupplierSettlementStatementDetailSnapshot;
use erp_supply::service::supplier_settlement::difference::settlement_difference_view;
use erp_supply::service::supplier_settlement::evidence::evidence_view;
use erp_supply::service::supplier_settlement::query::{settlement_item_view, settlement_object_actions};
use erp_supply::service::supplier_settlement::review::{
    review_blocker, review_task_identity_matches, settlement_review_access,
};
use erp_supply::service::supplier_settlement::shared::zero_amount;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::repository::prelude::*;
use persistence_core::NoTransaction;
use view_dto::SupplierSettlementStatementDetailView;

use super::display_dto::SettlementStatementDisplayDetail;
use super::{SupplierSettlementReadService, dto as view_dto};
use crate::{Error, Result};
impl SupplierSettlementReadService {
    /// 查询供应商结算单详情（结算单 + 全部明细 + 全部差异）。
    ///
    /// # 参数
    /// * `id` - 结算单 ID。
    /// * `actor` - 已认证操作人，用于复核任务的岗位分离与动作投影。
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    /// * `RepositoryError` - 数据库查询失败
    /// * `Logic` - 成本差额计算失败
    /// * `Internal` - 已确认唯一的复核任务随后缺失
    pub async fn supplier_settlement_statement_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SettlementStatementDisplayDetail> {
        let snapshot = self
            .db
            .supplier_settlement()
            .statement_detail_snapshot(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算单不存在".to_string()))?;
        let stats = settlement_detail_stats(&snapshot);
        let statement = snapshot.statement;
        let items = snapshot.items;
        let differences = snapshot.differences;
        let (mut allowed_actions, mut action_blockers, processing_state) =
            settlement_object_actions(&statement, &differences, actor.id());
        let cost_delta = statement.accepted_cost_delta(&items, &differences)?;
        let cost_adjustment_ready = cost_delta.is_zero();
        let (review_work_item, review_action_blockers, review_domain_actions) =
            self.settlement_review_work_item_view(&statement, actor).await?;
        allowed_actions.extend(
            review_domain_actions.into_iter().filter(|action| action != "CONFIRM" || cost_adjustment_ready),
        );
        if !cost_adjustment_ready {
            action_blockers.push(review_blocker(
                "CONFIRM",
                "AUTHORITATIVE_COST_ALLOCATION_MISSING",
                "当前非零成本差额尚未锁定原成本与分配链，禁止确认并伪造成本事实",
            ));
        }
        if let Some(work_item) = &review_work_item {
            action_blockers.extend(work_item.action_blockers.clone());
        }
        let review_processing_state = if review_action_blockers.is_empty() {
            dto::SettlementReviewProcessingState::Ready
        } else {
            dto::SettlementReviewProcessingState::ApprovalBlocked
        };

        let detail = SupplierSettlementStatementDetailView {
            statement: statement.into(),
            items: items.into_iter().map(settlement_item_view).collect(),
            differences: settlement_difference_views(differences, snapshot.evidence_by_difference),
            stats,
            processing_state,
            review_work_item,
            review_processing_state,
            review_action_blockers,
            allowed_actions,
            action_blockers,
        };
        self.detail_display_view(detail).await
    }

    /// 为详情返回当前 actor 的 W27 领域动作，不把领域动作塞进通用责任注册表。
    async fn settlement_review_work_item_view(
        &self,
        statement: &SupplierSettlementStatement,
        actor: &AuditActor,
    ) -> Result<(
        Option<view_dto::SettlementReviewWorkItemView>,
        Vec<dto::SettlementReviewActionBlockerView>,
        Vec<String>,
    )> {
        if !statement.is_pending_review() {
            return Ok((None, Vec::new(), Vec::new()));
        }
        let mut items = self
            .db
            .work_items()
            .list_active_by_object("supplier_settlement_statement", &statement.base.id, &mut NoTransaction)
            .await?
            .into_iter()
            .filter(|item| item.work_item_type == WorkItemType::SupplierSettlementReview)
            .collect::<Vec<_>>();
        if items.len() != 1 {
            return Ok(blocked_review(
                "FORMAL_REVIEW_WORK_ITEM_MISSING_OR_AMBIGUOUS",
                "未找到与当前结算主题唯一匹配的正式复核任务，已禁止决定",
            ));
        }
        let item = items.pop().ok_or_else(|| Error::Internal("正式结算复核任务读取失败".to_string()))?;
        if item.business_object_type != "supplier_settlement_statement"
            || item.business_object_id != statement.base.id
            || item.subject_version != statement.subject_hash
            || !review_task_identity_matches(&item.owner_role, &item.owner_organization_id, statement)
        {
            return Ok(blocked_review(
                "FORMAL_REVIEW_WORK_ITEM_MISMATCH",
                "复核任务与当前结算主题不一致，已禁止决定",
            ));
        }
        let eligible = true;
        let separation_satisfied = !statement.is_prepared_by(actor.id());
        let (domain_actions, action_blockers) =
            settlement_review_access(item.is_owned_by(actor.id()), eligible, separation_satisfied);
        Ok((
            Some(view_dto::SettlementReviewWorkItemView {
                work_item_id: item.base.id,
                work_item_type: item.work_item_type,
                task_version: item.base.version,
                subject_version: item.subject_version,
                status: item.status,
                processing_state: dto::SettlementReviewProcessingState::Ready,
                owner_role: item.owner_role,
                owner_organization_id: item.owner_organization_id,
                owner_user_id: item.owner_user_id,
                owner_user_name: None,
                action_blockers,
            }),
            Vec::new(),
            domain_actions,
        ))
    }
}

impl SupplierSettlementReadService {
    /// 结算列表在分页前解析供应商名称，保留本域日期/状态与统计口径。
    ///
    /// # 参数
    /// * `params` - 结算列表查询参数。
    ///
    /// # 返回
    /// 返回结算列表视图。空关键词不按名称收窄。
    ///
    /// # 错误
    /// 参数校验失败时返回 `ValidationError`。供应商名称或结算列表读取失败时返回对应错误。
    pub async fn supplier_settlement_statement_list(
        &self,
        params: &dto::SupplierSettlementStatementListParams,
    ) -> Result<dto::SupplierSettlementStatementListView> {
        validator::Validate::validate(params)?;
        let suppliers = crate::supplier_center::keyword_supplier_ids(&self.db, params.q.as_deref()).await?;
        Ok(erp_supply::service::supplier_settlement::SupplierSettlementService::new(self.db.clone())
            .supplier_settlement_statement_list(params, suppliers)
            .await?)
    }
}

fn settlement_detail_stats(
    snapshot: &SupplierSettlementStatementDetailSnapshot,
) -> dto::SettlementStatementStatsView {
    let items = &snapshot.items;
    dto::SettlementStatementStatsView {
        item_count: items.len(),
        difference_count: snapshot.differences.len(),
        pending_difference_count: snapshot
            .differences
            .iter()
            .filter(|difference| difference.is_pending())
            .count(),
        evidenced_difference_count: snapshot.evidence_by_difference.len(),
        order_amount: items.iter().fold(zero_amount(), |total, item| total.checked_add(item.order_amount)),
        freight_amount: items
            .iter()
            .fold(zero_amount(), |total, item| total.checked_add(item.freight_amount)),
        service_fee_amount: items
            .iter()
            .fold(zero_amount(), |total, item| total.checked_add(item.service_fee_amount)),
        refund_amount: items.iter().fold(zero_amount(), |total, item| total.checked_add(item.refund_amount)),
        erp_amount: snapshot.statement.erp_amount,
        supplier_amount: snapshot.statement.supplier_amount,
        difference_amount: snapshot.statement.difference_amount,
    }
}

fn settlement_difference_views(
    differences: Vec<SupplierSettlementDifference>,
    evidence_by_difference: BTreeMap<String, Vec<SupplierSettlementDifferenceEvidence>>,
) -> Vec<dto::SupplierSettlementDifferenceView> {
    let mut evidence = evidence_by_difference
        .into_iter()
        .map(|(id, values)| (id, values.into_iter().map(evidence_view).collect()))
        .collect::<BTreeMap<String, Vec<dto::SettlementDifferenceEvidenceView>>>();
    differences
        .into_iter()
        .map(|difference| {
            let difference_id = difference.base.id.clone();
            let mut view = settlement_difference_view(difference);
            view.evidence = evidence.remove(&difference_id).unwrap_or_default();
            view
        })
        .collect()
}

fn blocked_review(
    code: &str,
    message: &str,
) -> (Option<view_dto::SettlementReviewWorkItemView>, Vec<dto::SettlementReviewActionBlockerView>, Vec<String>)
{
    (None, vec![review_blocker("REVIEW_DECISION", code, message)], Vec::new())
}
