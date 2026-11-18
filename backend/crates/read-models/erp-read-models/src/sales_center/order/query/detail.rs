//! 销售详情编排：授权读取、业务投影、范围重验和返回映射。
mod facts;
mod reference_prices;

use application_core::AuditActor;
use erp_core::ids::{SalesOrderId, SalesOrderRevisionId, SalesOrderSubmissionId};
use erp_identity::AccessTarget;
use erp_permissions::{Permission, sales_order as sales_order_permissions, sales_order};
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::repository::prelude::*;
use erp_sales::repository::sales_order::inquiry_source::inquiry_sources;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use erp_support::FileAssetExt;
use erp_support::repository::prelude::*;
use persistence_core::{Executor, Transactional};

use self::facts::DetailFacts;
use super::super::approval_query::load_document_approval;
use super::super::dto::{
    ActiveCardSalesApprovalView, DocumentApprovalView, FreightFollowUpView, PurchaseCreationAccessView,
    SalesOrderDetailView, SalesOrderEvidenceFileView, SalesOrderStageSummary, SalesProcurementCoverageView,
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
    freight_follow_ups: Vec<FreightFollowUpView>,
    can_start_sales_change_order: bool,
    change_order_blocker: Option<String>,
    active_card_sales_approval: Option<ActiveCardSalesApprovalView>,
    approval: DocumentApprovalView,
    evidence_files: Vec<SalesOrderEvidenceFileView>,
}

impl SalesOrderReadService {
    /// 在同一授权快照中读取销售单及完整关联详情。
    /// # 参数
    /// id 为销售身份；actor 必须是当前已认证账号。
    /// # 返回
    /// 完整详情，所有数据库事实均来自同一快照。
    /// # 错误
    /// 缺少身份、无详情资格、事实读取或交付检查失败时拒绝。
    pub async fn sales_order_detail(
        &self,
        id: &str,
        actor: Option<&AuditActor>,
    ) -> Result<SalesOrderDetailView> {
        let actor = actor.ok_or_else(|| Error::Unauthenticated("销售详情必须提供当前账号".into()))?.clone();
        let id = id.to_string();
        let this = Self::new(self.db.clone(), self.authorization.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.authorization
                        .require(&actor, &sales_order::DETAIL, AccessTarget::record(&id)?, executor)
                        .await?;

                    let order = this
                        .db
                        .sales_orders()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售单不存在或无权查看".into()))?;
                    let view = this.detail_view(order, &actor, executor).await?;

                    let (context, scope) = SalesAccess::new(this.db.clone(), this.authorization.clone())
                        .resolve(&actor, &sales_order_permissions::DETAIL, executor)
                        .await?;
                    this.db
                        .sales_orders()
                        .find_authorized(&id, &scope, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售单不存在或无权查看".into()))?;
                    context.validate(executor).await?;
                    Ok(view)
                })
            })
            .await
    }

    /// 在调用方快照中按实际命令资格重新授权并读取结果。
    /// # 参数
    /// id、actor 为真实销售单和当前账号；permission 为该命令固定的记录权限，
    /// 创建回包使用 SALES_CREATED_RESULT_READ；executor 为活动快照执行器。
    /// # 返回
    /// 同一快照的完整结果；不会追加详情菜单资格。
    /// # 错误
    /// 非快照执行器、非销售权限、记录无权、读取或最终授权失败时拒绝。
    pub async fn command_detail(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<SalesOrderDetailView> {
        if permission.resource() != "sales_order"
            || !executor.boundary().is_some_and(|boundary| boundary.is_snapshot())
        {
            return Err(Error::ValidationError(
                "暂时无法从此处查看销售处理结果，请打开原销售单核对当前状态，勿重复操作".into(),
            ));
        }
        self.authorization.require(actor, permission, AccessTarget::record(id)?, executor).await?;

        let order = self
            .db
            .sales_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在或无权查看".into()))?;
        let view = self.detail_view(order, actor, executor).await?;

        self.authorization.require(actor, permission, AccessTarget::record(id)?, executor).await?;
        Ok(view)
    }

    /// 全部投影共用调用方快照；子查询在各自阶段完成交付。
    async fn detail_view(
        &self,
        order: SalesOrder,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesOrderDetailView> {
        let facts = self.load_detail_facts(&order, executor).await?;
        let presentation = self.detail_presentation(&order, &facts, Some(actor), executor).await?;
        let inquiry_source = inquiry_sources(&self.db).find_by_id(&order.base.id, executor).await?;
        let current_work = self.current_work(&order, executor).await?;

        let mut view = SalesOrderDetailView::from_facts(order, facts, presentation);
        view.inquiry_source = inquiry_source;
        view.current_work = current_work;
        Ok(view)
    }

    /// 仅读取已授权销售单自身凭证的安全元数据。
    async fn evidence_files(
        &self,
        order: &SalesOrder,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderEvidenceFileView>> {
        let files = self.db.file_assets().find_by_ids(&order.evidence_file_asset_ids, executor).await?;
        Ok(files
            .into_iter()
            .map(|file| SalesOrderEvidenceFileView {
                file_asset_id: file.base.id,
                file_name: file.file_name,
                content_type: file.content_type,
                byte_size: file.byte_size,
            })
            .collect())
    }

    /// 按原顺序读取采购资格、审批工作面、责任人、变更资格和审批结构。
    async fn detail_presentation(
        &self,
        order: &SalesOrder,
        facts: &DetailFacts,
        actor: Option<&AuditActor>,
        executor: &mut dyn Executor,
    ) -> Result<DetailPresentation> {
        let purchase_coverage = self.sales_procurement_coverage(order, actor, executor).await?;
        let purchase_creation_access =
            self.purchase_creation_access(order, &purchase_coverage, actor, executor).await?;
        let freight_follow_ups = self.freight_follow_ups(order, actor, executor).await?;

        let active_card_sales_approval = match (actor, facts.submissions.first()) {
            (Some(actor), Some(submission)) => {
                self.resolve_active_card_sales_approval(
                    order,
                    &SalesOrderSubmissionId::new(submission.id.clone()),
                    Some(submission),
                    actor,
                    executor,
                )
                .await?
            },
            _ => None,
        };

        let (owner_user_name, stage) = self.detail_stage(order, executor).await?;
        let active_change = self.has_active_sales_change(order, executor).await?;
        let (can_start_sales_change_order, change_order_blocker) =
            compute_can_start_sales_change(order.origin_system, stage.code, stage.label, active_change);

        let approval = load_document_approval(
            &self.db,
            order.business_type,
            &order.base.id,
            facts.binding.as_ref(),
            order.commercial_status,
            order.review_status,
            executor,
        )
        .await?;

        Ok(DetailPresentation {
            evidence_files: self.evidence_files(order, executor).await?,
            owner_user_name,
            stage,
            purchase_coverage,
            purchase_creation_access,
            freight_follow_ups,
            can_start_sales_change_order,
            change_order_blocker,
            active_card_sales_approval,
            approval,
        })
    }

    /// 批量补齐负责销售与阶段责任人姓名，保留原阶段与时限计算。
    async fn detail_stage(
        &self,
        order: &SalesOrder,
        executor: &mut dyn Executor,
    ) -> Result<(Option<String>, SalesOrderStageSummary)> {
        let (owner_role, owner_user_id, due_at) = self
            .resolve_stage_owner(
                &SalesOrderId::new(order.base.id.clone()),
                order.business_type,
                order.review_status,
                executor,
            )
            .await?;
        let mut name_ids = vec![order.sales_owner_user_id.clone()];
        if let Some(id) = owner_user_id.as_ref() {
            name_ids.push(id.clone());
        }
        let names = self.account_names(&name_ids, executor).await?;
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
    async fn has_active_sales_change(&self, order: &SalesOrder, executor: &mut dyn Executor) -> Result<bool> {
        let Some(revision_id) = order.current_revision_id() else {
            return Ok(false);
        };
        Ok(self
            .db
            .sales_change_orders()
            .has_in_progress_by_order_and_base(
                &SalesOrderId::new(order.base.id.clone()),
                &SalesOrderRevisionId::new(revision_id),
                executor,
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
            inquiry_source: None,
            selection_source: order.selection_source,
            id: order.base.id,
            order_no: order.order_no,
            business_type: order.business_type,
            origin_system: order.origin_system,
            customer_id: order.customer_id.to_string(),
            contract_id: order.contract_id.as_ref().map(ToString::to_string),
            evidence_file_asset_ids: order.evidence_file_asset_ids.iter().map(ToString::to_string).collect(),
            evidence_files: presentation.evidence_files,
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
            freight_follow_ups: presentation.freight_follow_ups,
            settled_total: facts.receivable_summary.settled_total,
            invoiced_total: facts.receivable_summary.invoiced_total,
            lines: facts.stable_lines,
            working_copy: facts.working_copy_view,
            submissions: facts.submissions,
            revisions: facts.revisions,
            stage: presentation.stage,
            current_work: Vec::new(),
            close_eligibility,
            can_start_sales_change_order: presentation.can_start_sales_change_order,
            change_order_blocker: presentation.change_order_blocker,
            active_card_sales_approval: presentation.active_card_sales_approval,
            approval: Some(presentation.approval),
        }
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use erp_permissions::customer;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 不可用数据库仅用于证明拒绝路径不会触发业务读取。
    async fn service() -> SalesOrderReadService {
        let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .expect("合法测试地址");
        SalesOrderReadService::new(client.database("sales_detail_authorization"), unavailable_authorization())
    }

    #[tokio::test]
    async fn detail_requires_an_actor_before_reading_business_data() {
        let error = service().await.sales_order_detail("order", None).await.expect_err("缺少身份必须拒绝");
        assert!(matches!(error, Error::Unauthenticated(_)));
    }

    #[tokio::test]
    async fn command_result_rejects_non_snapshot_and_foreign_resource_before_reading() {
        let service = service().await;
        let actor = AuditActor::new("sales".into(), "sales".into(), AccountKind::Admin);
        let error = service
            .command_detail("order", &actor, &sales_order::DETAIL, &mut ReadExecutor::default())
            .await
            .expect_err("非快照不能读取结果");
        assert!(matches!(error, Error::ValidationError(_)));

        let error = service
            .command_detail("order", &actor, &customer::DETAIL, &mut SnapshotExecutor::default())
            .await
            .expect_err("其他资源权限不能读取销售结果");
        assert!(matches!(error, Error::ValidationError(_)));
    }

    #[tokio::test]
    async fn command_result_propagates_authorization_failure_before_reading() {
        let service = service().await;
        let actor = AuditActor::new("sales".into(), "sales".into(), AccountKind::Admin);
        let error = service
            .command_detail("order", &actor, &sales_order::DETAIL, &mut SnapshotExecutor::default())
            .await
            .expect_err("身份取证失败必须停止");
        assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
    }
}
