//! 销售详情编排：授权读取、业务投影、范围重验和返回映射。
mod facts;
mod reference_prices;

use application_core::AuditActor;
use erp_core::ids::{SalesOrderId, SalesOrderRevisionId, SalesOrderSubmissionId};
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::prelude::*;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use persistence_core::NoTransaction;

use self::facts::DetailFacts;
use super::super::approval_query::load_document_approval;
use super::super::dto::{
    ActiveCardSalesApprovalView, DocumentApprovalView, PurchaseCreationAccessView, SalesOrderDetailView,
    SalesOrderStageSummary, SalesProcurementCoverageView,
};
use super::super::status::{close_eligibility_view, compute_can_start_sales_change, stage_code_label_tone};
use super::SalesOrderReadService;
use crate::sales_center::access::SalesAccess;
use crate::{Error, Result};

/// 已按当前操作人和业务事实计算的展示、审批及可执行动作。
struct DetailPresentation {
    owner_user_name: Option<String>,
    stage: SalesOrderStageSummary,
    purchase_coverage: SalesProcurementCoverageView,
    purchase_creation_access: PurchaseCreationAccessView,
    can_start_sales_change_order: bool,
    change_order_blocker: Option<String>,
    active_card_sales_approval: Option<ActiveCardSalesApprovalView>,
    approval: DocumentApprovalView,
}

impl SalesOrderReadService {
    /// 查询销售单详情，并在事实装配完成后重验授权和销售单版本。
    ///
    /// # 参数
    /// * `id` - 销售单稳定身份
    /// * `actor` - 已认证操作人；内部查询可不提供
    ///
    /// # 返回
    /// 包含稳定明细、草稿、提交、版本、采购、财务与审批的详情视图。
    ///
    /// # 错误
    /// 单据缺失、无权读取、关联查询失败或读取期间范围发生变化时拒绝。
    #[tracing::instrument(
        name = "sales_order.detail",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "detail")
    )]
    pub async fn sales_order_detail(
        &self,
        id: &str,
        actor: Option<&AuditActor>,
    ) -> Result<SalesOrderDetailView> {
        let (order, access_version) = self.detail_order(id, actor).await?;
        let facts = self.load_detail_facts(&order).await?;
        let presentation = self.detail_presentation(&order, &facts, actor).await?;
        self.recheck_detail_access(id, actor, access_version).await?;
        Ok(SalesOrderDetailView::from_facts(order, facts, presentation))
    }

    /// 有操作人时取得授权单据与范围版本；内部调用保留原有直接读取入口。
    async fn detail_order(
        &self,
        id: &str,
        actor: Option<&AuditActor>,
    ) -> Result<(SalesOrder, Option<String>)> {
        if let Some(actor) = actor {
            let (order, version) =
                SalesAccess::new(self.db.clone(), self.require_rbac()?.clone()).detail(actor, id).await?;
            return Ok((order, Some(version)));
        }
        let order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".into()))?;
        Ok((order, None))
    }

    /// 装配后再次执行原授权查询，拒绝期间撤权或单据版本变化。
    async fn recheck_detail_access(
        &self,
        id: &str,
        actor: Option<&AuditActor>,
        expected: Option<String>,
    ) -> Result<()> {
        if let (Some(actor), Some(expected)) = (actor, expected) {
            let (_, current) =
                SalesAccess::new(self.db.clone(), self.require_rbac()?.clone()).detail(actor, id).await?;
            if current != expected {
                return Err(crate::support::data_scope_changed("数据范围或销售单已变化，请刷新"));
            }
        }
        Ok(())
    }

    /// 按原顺序读取采购资格、审批工作面、责任人、变更资格和审批结构。
    async fn detail_presentation(
        &self,
        order: &SalesOrder,
        facts: &DetailFacts,
        actor: Option<&AuditActor>,
    ) -> Result<DetailPresentation> {
        let purchase_coverage = self.sales_procurement_coverage(order).await?;
        let purchase_creation_access =
            self.purchase_creation_access(order, &purchase_coverage, actor).await?;
        let active_card_sales_approval = match (actor, facts.submissions.first()) {
            (Some(actor), Some(submission)) => {
                self.resolve_active_card_sales_approval(
                    order,
                    &SalesOrderSubmissionId::new(submission.id.clone()),
                    Some(submission),
                    actor,
                )
                .await?
            },
            _ => None,
        };
        let (owner_user_name, stage) = self.detail_stage(order).await?;
        let active_change = self.has_active_sales_change(order).await?;
        let (can_start_sales_change_order, change_order_blocker) =
            compute_can_start_sales_change(order.origin_system, stage.code, stage.label, active_change);
        let approval = load_document_approval(
            &self.db,
            order.business_type,
            &order.base.id,
            facts.binding.as_ref(),
            order.commercial_status,
            order.review_status,
        )
        .await?;
        Ok(DetailPresentation {
            owner_user_name,
            stage,
            purchase_coverage,
            purchase_creation_access,
            can_start_sales_change_order,
            change_order_blocker,
            active_card_sales_approval,
            approval,
        })
    }

    /// 批量补齐负责销售与阶段责任人姓名，保留原阶段与时限计算。
    async fn detail_stage(&self, order: &SalesOrder) -> Result<(Option<String>, SalesOrderStageSummary)> {
        let (owner_role, owner_user_id, due_at) = self
            .resolve_stage_owner(
                &SalesOrderId::new(order.base.id.clone()),
                order.business_type,
                order.review_status,
            )
            .await?;
        let mut name_ids = vec![order.sales_owner_user_id.clone()];
        if let Some(id) = owner_user_id.as_ref() {
            name_ids.push(id.clone());
        }
        let names = self.resolve_account_names_batch(&name_ids).await?;
        let owner_user_name = owner_user_id.as_ref().and_then(|id| names.get(id).cloned());
        let (code, label, tone) = stage_code_label_tone(
            order.commercial_status,
            order.review_status,
            order.close_status,
            order.fulfillment_progress,
        );
        Ok((
            names.get(&order.sales_owner_user_id).cloned(),
            SalesOrderStageSummary { code, label, tone, owner_role, owner_user_id, owner_user_name, due_at },
        ))
    }

    /// 只按当前生效版本查询进行中的销售变更；无当前版本时保持无变更。
    async fn has_active_sales_change(&self, order: &SalesOrder) -> Result<bool> {
        let Some(revision_id) = order.current_revision_id() else {
            return Ok(false);
        };
        Ok(self
            .db
            .sales_change_orders()
            .has_in_progress_by_order_and_base(
                &SalesOrderId::new(order.base.id.clone()),
                &SalesOrderRevisionId::new(revision_id),
                &mut NoTransaction,
            )
            .await?)
    }
}

impl SalesOrderDetailView {
    /// 将已授权事实映射为原详情合同，金额和结案资格继续由原领域事实计算。
    fn from_facts(order: SalesOrder, facts: DetailFacts, presentation: DetailPresentation) -> Self {
        let close_eligibility = close_eligibility_view(order.closure_facts().assess(
            facts.receivable_summary.has_accounts(),
            facts.receivable_summary.settled_total,
            facts.receivable_summary.gross_total,
        ));
        Self {
            id: order.base.id,
            order_no: order.order_no,
            business_type: order.business_type,
            origin_system: order.origin_system,
            customer_id: order.customer_id.to_string(),
            contract_id: order.contract_id.as_ref().map(ToString::to_string),
            settlement_party_id: order.settlement_party_id.to_string(),
            commercial_status: order.commercial_status,
            review_status: order.review_status,
            fulfillment_progress: order.fulfillment_progress,
            collection_progress: order.collection_progress,
            invoice_progress: order.invoice_progress,
            close_status: order.close_status,
            current_revision_id: order.stable.current_revision_id,
            effective_at: order.effective_at.map(|instant| instant.unix_secs() as u64),
            version: order.base.version,
            created_at: order.base.created_at,
            owner_user_id: order.sales_owner_user_id,
            owner_user_name: presentation.owner_user_name,
            purchase_order_count: facts.purchase_order_count,
            purchase_coverage: presentation.purchase_coverage,
            purchase_creation_access: presentation.purchase_creation_access,
            settled_total: facts.receivable_summary.settled_total,
            invoiced_total: facts.receivable_summary.invoiced_total,
            lines: facts.stable_lines,
            working_copy: facts.working_copy_view,
            submissions: facts.submissions,
            revisions: facts.revisions,
            stage: presentation.stage,
            close_eligibility,
            can_start_sales_change_order: presentation.can_start_sales_change_order,
            change_order_blocker: presentation.change_order_blocker,
            active_card_sales_approval: presentation.active_card_sales_approval,
            approval: Some(presentation.approval),
        }
    }
}
