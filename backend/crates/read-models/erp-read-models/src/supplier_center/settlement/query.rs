//! 结算详情的供应链快照与工作流任务组合。
use std::collections::BTreeMap;

use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::supplier_settlement;
use erp_supply::SettlementAccess;
use erp_supply::dto::supplier_settlement as dto;
use erp_supply::entity::supplier_settlement::{
    SupplierSettlementDifference, SupplierSettlementDifferenceEvidence, SupplierSettlementStatement,
};
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::repository::supplier_settlement::{
    SupplierSettlementStatementDetailSnapshot, SupplierSettlementStatementRepositoryExt,
};
use erp_supply::service::supplier_settlement::difference::settlement_difference_view;
use erp_supply::service::supplier_settlement::evidence::evidence_view;
use erp_supply::service::supplier_settlement::query::{settlement_item_view, settlement_object_actions};
use erp_supply::service::supplier_settlement::review::{
    review_blocker, review_task_identity_matches, settlement_review_access,
};
use erp_supply::service::supplier_settlement::shared::zero_amount;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemType};
use erp_workflow::repository::prelude::*;
use persistence_core::{Executor, Transactional};
use view_dto::SupplierSettlementStatementDetailView;

use super::display_dto::SettlementStatementDisplayDetail;
use super::{SupplierSettlementReadService, dto as view_dto, require_reviewer};
use crate::{Error, Result};
/// 动作检查产生的展示资格，不携带已读取的根、金额、明细或凭证。
struct DetailPresentation {
    processing_state: String,
    review_work_item: Option<view_dto::SettlementReviewWorkItemView>,
    review_processing_state: dto::SettlementReviewProcessingState,
    review_action_blockers: Vec<dto::SettlementReviewActionBlockerView>,
    allowed_actions: Vec<String>,
    action_blockers: Vec<dto::SettlementReviewActionBlockerView>,
}

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
        let (db, authorization, id, actor) =
            (self.db.clone(), self.identity.clone(), id.to_string(), actor.clone());
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { Self::new(db, authorization).detail(&id, &actor, executor).await })
            })
            .await
    }

    async fn authorized_snapshot(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementStatementDetailSnapshot> {
        self.identity
            .require(actor, &supplier_settlement::STATEMENT_DETAIL, AccessTarget::record(id)?, executor)
            .await?;
        self.db
            .supplier_settlement()
            .statement_detail_snapshot(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算单不存在".into()))
    }

    async fn detail(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SettlementStatementDisplayDetail> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("供应商结算详情读取必须使用快照事务".into()));
        }
        let prepared = self.authorized_snapshot(id, actor, executor).await?;
        let presentation = self.detail_actions(id, &prepared, actor, executor).await?;
        self.detail_read(id, actor, presentation, executor).await
    }

    /// 业务动作只形成展示结果；所有会推进阶段的动作检查在最终根查询之前完成。
    async fn detail_actions(
        &self,
        id: &str,
        snapshot: &SupplierSettlementStatementDetailSnapshot,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<DetailPresentation> {
        let access = SettlementAccess::new(self.db.clone(), self.identity.clone())
            .actions(actor, id, executor)
            .await?;
        let (mut allowed_actions, mut action_blockers, processing_state) =
            settlement_object_actions(&snapshot.statement, &snapshot.differences, &access);
        let cost_delta = snapshot.statement.accepted_cost_delta(&snapshot.items, &snapshot.differences)?;
        let cost_adjustment_ready = cost_delta.is_zero();
        let (review_work_item, review_action_blockers, review_domain_actions) =
            self.settlement_review_work_item_view(&snapshot.statement, actor, executor).await?;
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

        Ok(DetailPresentation {
            processing_state,
            review_work_item,
            review_processing_state,
            review_action_blockers,
            allowed_actions,
            action_blockers,
        })
    }

    /// 最终根查询之后重新读取交付数据，展示补齐后验证这一原查询。
    async fn detail_read(
        &self,
        id: &str,
        actor: &AuditActor,
        presentation: DetailPresentation,
        executor: &mut dyn Executor,
    ) -> Result<SettlementStatementDisplayDetail> {
        let scope = SettlementAccess::new(self.db.clone(), self.identity.clone())
            .resolve(actor, &supplier_settlement::STATEMENT_DETAIL, executor)
            .await?;
        self.db
            .supplier_settlement_statements()
            .find_authorized(id, &scope, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算单不存在或无权查看".into()))?;
        let snapshot = self
            .db
            .supplier_settlement()
            .statement_detail_snapshot(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商结算单不存在或无权查看".into()))?;
        let stats = settlement_detail_stats(&snapshot);
        let detail = SupplierSettlementStatementDetailView {
            statement: snapshot.statement.into(),
            items: snapshot.items.into_iter().map(settlement_item_view).collect(),
            differences: settlement_difference_views(snapshot.differences, snapshot.evidence_by_difference),
            stats,
            processing_state: presentation.processing_state,
            review_work_item: presentation.review_work_item,
            review_processing_state: presentation.review_processing_state,
            review_action_blockers: presentation.review_action_blockers,
            allowed_actions: presentation.allowed_actions,
            action_blockers: presentation.action_blockers,
        };
        let view = self.detail_display_view(detail, executor).await?;
        scope.validate(executor).await?;
        Ok(view)
    }

    /// 为详情返回当前 actor 的 W27 领域动作，不把领域动作塞进通用责任注册表。
    async fn settlement_review_work_item_view(
        &self,
        statement: &SupplierSettlementStatement,
        actor: &AuditActor,
        executor: &mut dyn Executor,
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
            .list_active_by_object("supplier_settlement_statement", &statement.base.id, executor)
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
        let (domain_actions, action_blockers) =
            self.review_actions(statement, actor, &item, executor).await?;
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

    /// 复核资格由 identity 检查；岗位分离沿用结算领域规则。
    async fn review_actions(
        &self,
        statement: &SupplierSettlementStatement,
        actor: &AuditActor,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<(Vec<String>, Vec<dto::SettlementReviewActionBlockerView>)> {
        let result = require_reviewer(&self.db, &self.identity, actor, item, executor).await;
        let authorized = match result {
            Ok(()) => true,
            Err(Error::Forbidden(_) | Error::NotFound(_)) => false,
            Err(error) => return Err(error),
        };
        let separation_satisfied = !statement.is_prepared_by(actor.id());
        let (domain_actions, action_blockers) = if authorized {
            settlement_review_access(true, true, separation_satisfied)
        } else {
            (
                Vec::new(),
                vec![review_blocker(
                    "REVIEW_DECISION",
                    "REVIEW_ACCESS_DENIED",
                    "当前账号不具备该复核任务的完整操作资格",
                )],
            )
        };
        Ok((domain_actions, action_blockers))
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

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 详情入口资格失败时，主表及关联名称都不得先行读取。
    #[tokio::test]
    async fn detail_stops_before_statement_or_display_reads_when_authorization_fails() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("settlement_detail_authorization");
        let service = SupplierSettlementReadService::new(db, unavailable_authorization());
        let actor = AuditActor::new("actor".into(), "操作人".into(), AccountKind::Admin);
        let error = service
            .detail("statement", &actor, &mut ReadExecutor::default())
            .await
            .expect_err("详情及动作补齐只允许活动快照");
        assert!(
            matches!(error, Error::ValidationError(reason) if reason == "供应商结算详情读取必须使用快照事务")
        );
        let error = service
            .detail("statement", &actor, &mut SnapshotExecutor::default())
            .await
            .expect_err("入口授权失败不能读取结算及名称");
        assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn prepared_display_actions_cannot_bypass_final_detail_query() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("settlement_final_query_authorization");
        let service = SupplierSettlementReadService::new(db, unavailable_authorization());
        let actor = AuditActor::new("actor".into(), "操作人".into(), AccountKind::Admin);
        let presentation = DetailPresentation {
            processing_state: "READY".into(),
            review_work_item: None,
            review_processing_state: dto::SettlementReviewProcessingState::Ready,
            review_action_blockers: vec![],
            allowed_actions: vec!["SUBMIT".into()],
            action_blockers: vec![],
        };
        let result =
            service.detail_read("statement", &actor, presentation, &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
    }
}
