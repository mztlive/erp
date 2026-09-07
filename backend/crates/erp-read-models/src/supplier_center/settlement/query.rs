//! 结算详情的供应链快照与工作流任务组合。
use super::{dto as view_dto, SupplierSettlementReadService};
use application_core::AuditActor;
use erp_supply::dto::supplier_settlement as dto;
use erp_supply::entity::supplier_settlement::SupplierSettlementStatement;
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::service::supplier_settlement::{
    difference::settlement_difference_view,
    evidence::evidence_view,
    query::{settlement_item_view, settlement_object_actions},
    review::{
        review_blocker, settlement_review_access, SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID,
        SETTLEMENT_REVIEW_OWNER_ROLE,
    },
    shared::zero_amount,
};
use erp_workflow::{entity::work_item::WorkItemType, WorkItemExt};
use persistence_core::NoTransaction;
use services::{Error, Result};
use std::collections::HashMap;
use view_dto::SupplierSettlementStatementDetailView;
impl SupplierSettlementReadService {
    /// 查询供应商结算单详情（结算单 + 全部明细 + 全部差异）。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn supplier_settlement_statement_detail(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementStatementDetailView> {
        let snapshot = self
            .db
            .supplier_settlement()
            .statement_detail_snapshot(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算单不存在".to_string()))?;
        let statement = snapshot.statement;
        let items = snapshot.items;
        let differences = snapshot.differences;
        let mut evidence_by_difference = HashMap::<String, Vec<dto::SettlementDifferenceEvidenceView>>::new();
        for (difference_id, values) in snapshot.evidence_by_difference {
            evidence_by_difference.insert(difference_id, values.into_iter().map(evidence_view).collect());
        }
        let evidenced_difference_count = evidence_by_difference.len();
        let pending_difference_count = differences
            .iter()
            .filter(|difference| difference.is_pending())
            .count();
        let (mut allowed_actions, mut action_blockers, processing_state) =
            settlement_object_actions(&statement, &differences, actor.id());
        let difference_count = differences.len();
        let item_count = items.len();
        let order_amount = items
            .iter()
            .fold(zero_amount(), |total, item| total.checked_add(item.order_amount));
        let freight_amount = items.iter().fold(zero_amount(), |total, item| {
            total.checked_add(item.freight_amount)
        });
        let service_fee_amount = items.iter().fold(zero_amount(), |total, item| {
            total.checked_add(item.service_fee_amount)
        });
        let refund_amount = items
            .iter()
            .fold(zero_amount(), |total, item| total.checked_add(item.refund_amount));
        let erp_amount = statement.erp_amount;
        let supplier_amount = statement.supplier_amount;
        let difference_amount = statement.difference_amount;
        let cost_delta = statement.accepted_cost_delta(&items, &differences)?;
        let cost_adjustment_ready = cost_delta.is_zero();
        let difference_views = differences
            .into_iter()
            .map(|difference| {
                let difference_id = difference.base.id.clone();
                let mut view = settlement_difference_view(difference);
                view.evidence = evidence_by_difference.remove(&difference_id).unwrap_or_default();
                view
            })
            .collect();
        let (review_work_item, review_action_blockers, review_domain_actions) =
            self.settlement_review_work_item_view(&statement, actor).await?;
        allowed_actions.extend(
            review_domain_actions
                .into_iter()
                .filter(|action| action != "CONFIRM" || cost_adjustment_ready),
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

        Ok(SupplierSettlementStatementDetailView {
            statement: statement.into(),
            items: items.into_iter().map(settlement_item_view).collect(),
            differences: difference_views,
            stats: dto::SettlementStatementStatsView {
                item_count,
                difference_count,
                pending_difference_count,
                evidenced_difference_count,
                order_amount,
                freight_amount,
                service_fee_amount,
                refund_amount,
                erp_amount,
                supplier_amount,
                difference_amount,
            },
            processing_state,
            review_work_item,
            review_processing_state,
            review_action_blockers,
            allowed_actions,
            action_blockers,
        })
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
            .list_active_by_object(
                "supplier_settlement_statement",
                &statement.base.id,
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .filter(|item| item.work_item_type == WorkItemType::SupplierSettlementReview)
            .collect::<Vec<_>>();
        if items.len() != 1 {
            return Ok((
                None,
                vec![review_blocker(
                    "REVIEW_DECISION",
                    "FORMAL_REVIEW_WORK_ITEM_MISSING_OR_AMBIGUOUS",
                    "未找到与当前结算主题唯一匹配的正式复核任务，已禁止决定",
                )],
                Vec::new(),
            ));
        }
        let item = items
            .pop()
            .ok_or_else(|| Error::Internal("正式结算复核任务读取失败".to_string()))?;
        if item.business_object_type != "supplier_settlement_statement"
            || item.business_object_id != statement.base.id
            || item.subject_version != statement.subject_hash
            || item.owner_role != SETTLEMENT_REVIEW_OWNER_ROLE
            || item.owner_organization_id != SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID
        {
            return Ok((
                None,
                vec![review_blocker(
                    "REVIEW_DECISION",
                    "FORMAL_REVIEW_WORK_ITEM_MISMATCH",
                    "复核任务与当前结算主题不一致，已禁止决定",
                )],
                Vec::new(),
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
                action_blockers,
            }),
            Vec::new(),
            domain_actions,
        ))
    }
}
