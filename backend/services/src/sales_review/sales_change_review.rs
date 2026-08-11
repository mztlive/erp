use database::{
    AccessControlExt, NoTransaction, ReceivableExt, SalesOrderExt, SalesReviewExt, Transactional, WorkItemExt,
};
use entities::common::time::Instant;
use entities::ids::{SalesChangeReviewId, SalesChangeSubmissionId, WorkItemId};
use entities::sales_review::{SalesChangeReview, SalesChangeReviewData, SalesChangeReviewStage};
use entities::work_item::{WorkItem, WorkItemData, WorkItemPriority, WorkItemType};
use id_generator::next_id;
use validator::Validate;

use super::formalization::{build_change_revision, build_receivable_delta};
use super::{ChangeReviewDecisionRequest, SalesChangeOrderDetailView, SalesReviewService};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl SalesReviewService {
    /// 通过变更履约影响确认（进入财务复核；卡券变更完成运营确认后同样走财务复核）。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 决策请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单或待处理复核不存在
    /// * `ConflictError` - 状态机拒绝
    pub async fn confirm_impact(
        &self,
        id: &str,
        req: ChangeReviewDecisionRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        let submission_id = change_order
            .current_submission_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("变更单尚未发起影响确认".to_string()))?;
        let review = self.find_pending_change_review(&submission_id).await?;
        let mut change_for_tx = change_order.clone();
        let mut review_for_tx = review.clone();
        let now = Instant::now();
        review_for_tx.approve(actor.id(), now, req.decision_reason.clone())?;
        change_for_tx.to_finance_review(actor.id())?;

        let finance_review = SalesChangeReview::new(
            SalesChangeReviewId::new(next_id()),
            SalesChangeReviewData {
                sales_change_submission_id: submission_id.clone(),
                review_stage: SalesChangeReviewStage::FinanceReview,
            },
            actor.id(),
        )?;
        let work_item = WorkItem::new(
            WorkItemId::new(next_id()),
            WorkItemData {
                work_item_type: WorkItemType::PurchaseOrderReview,
                business_object_type: "sales_change_review".to_string(),
                business_object_id: finance_review.base.id.clone(),
                subject_version: Some(submission_id.to_string()),
                owner_role: Some("finance".to_string()),
                owner_user_id: None,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: Some("change_finance_dispatched".to_string()),
                impact_summary: Some("销售变更财务金额影响复核".to_string()),
                completion_action: "DECIDE_CHANGE_REVIEW".to_string(),
            },
        )?;
        let mut completed = self
            .complete_work_item("sales_change_review", &review.base.id, actor, now)
            .await?;
        let audit = actor.clone().resource_log(
            "sales_change_order.impact_confirmed",
            "sales_change_order",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_change_reviews()
                        .update(&mut review_for_tx, session)
                        .await?;
                    db.sales_change_orders()
                        .update(&mut change_for_tx, session)
                        .await?;
                    db.sales_change_reviews().create(&finance_review, session).await?;
                    db.work_items().create(&work_item, session).await?;
                    db.work_items().update(&mut completed, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.sales_change_order_detail(id).await
    }

    /// 驳回变更履约影响确认（变更单回驳回态，修改内容后从影响确认重新开始）。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 决策请求（驳回原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单或待处理复核不存在
    pub async fn reject_impact(
        &self,
        id: &str,
        req: ChangeReviewDecisionRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        self.decide_change_review(id, req, false, actor).await
    }

    /// 通过财务复核（§8.1.3 变更生效：新版本 + 应收差额 + 当前版本切换）。
    ///
    /// 校验基准版本仍为当前版本（防并发覆盖）后，在单事务内追加不可变销售版本、
    /// 更新销售单当前版本指针、追加应收差额分录（新金额减旧金额，零差额不写）、
    /// 完成财务复核待办并写审计；不修改已发生事实。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 决策请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单/待处理复核/销售单不存在
    /// * `ConflictError` - 基准版本已不是当前版本
    pub async fn confirm_finance(
        &self,
        id: &str,
        req: ChangeReviewDecisionRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        let submission_id = change_order
            .current_submission_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("变更单尚未发起影响确认".to_string()))?;
        let review = self.find_pending_change_review(&submission_id).await?;
        let submission = self
            .db
            .sales_change_submissions()
            .find_by_id(&submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("变更提交不存在".to_string()))?;
        let submission_lines = self
            .db
            .sales_change_submission_lines()
            .list_lines_by_submission(&submission.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(&change_order.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let current_revision_id = order
            .stable
            .current_revision_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本".to_string()))?;
        if current_revision_id != change_order.base_revision_id.to_string() {
            return Err(Error::ConflictError(
                "基准版本已不是销售单当前版本，请刷新后重新发起变更".to_string(),
            ));
        }

        let now = Instant::now();
        let existing_revisions = self
            .db
            .sales_order_revisions()
            .list_by_order(&change_order.sales_order_id, &mut NoTransaction)
            .await?;
        let revision_no = existing_revisions
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0)
            + 1;
        let revision = build_change_revision(&order, &submission, &submission_lines, revision_no, now)?;
        let current_revision = self
            .db
            .sales_order_revisions()
            .find_by_id(&current_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单当前版本不存在".to_string()))?;
        let mut order_for_tx = order.clone();
        order_for_tx.attach_revision(&revision.revision.base.id, actor.id());
        let mut change_for_tx = change_order.clone();
        change_for_tx.approve(revision.revision.base.id.clone().into(), actor.id())?;
        let mut review_for_tx = review.clone();
        review_for_tx.approve(actor.id(), now, req.decision_reason.clone())?;
        let existing_account = self
            .db
            .receivable_accounts()
            .find_one_by_field(
                "sales_order_id",
                change_order.sales_order_id.to_string(),
                &mut NoTransaction,
            )
            .await?;
        let delta = build_receivable_delta(
            &order,
            &revision,
            current_revision.gross_amount,
            existing_account,
            now,
        )?;
        let mut completed = self
            .complete_work_item("sales_change_review", &review.base.id, actor, now)
            .await?;
        let audit = actor.clone().resource_log(
            "sales_change_order.effective",
            "sales_change_order",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let delta_for_tx = delta.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_order()
                        .formalize_submission(
                            &mut order_for_tx,
                            &revision.revision,
                            &revision.lines,
                            &revision.goods_lines,
                            &revision.voucher_lines,
                            session,
                        )
                        .await?;
                    if let Some((account, entry)) = delta_for_tx {
                        db.receivable()
                            .create_receivable_with_entry(&account, &entry, session)
                            .await?;
                    }
                    db.sales_change_orders()
                        .update(&mut change_for_tx, session)
                        .await?;
                    db.sales_change_reviews()
                        .update(&mut review_for_tx, session)
                        .await?;
                    db.work_items().update(&mut completed, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.sales_change_order_detail(id).await
    }

    /// 驳回财务复核（变更单回驳回态）。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 决策请求（驳回原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单或待处理复核不存在
    pub async fn reject_finance(
        &self,
        id: &str,
        req: ChangeReviewDecisionRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        self.decide_change_review(id, req, false, actor).await
    }

    /// 变更复核驳回共用编排（影响确认/财务复核阶段）。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 决策请求
    /// * `approved` - 恒为 `false`（保留签名复用决策入口）
    /// * `actor` - 操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// 查询失败或状态机拒绝时返回错误。
    async fn decide_change_review(
        &self,
        id: &str,
        req: ChangeReviewDecisionRequest,
        approved: bool,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        debug_assert!(!approved, "变更复核驳回共用入口只用于驳回");
        req.validate()?;
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        let submission_id = change_order
            .current_submission_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("变更单尚未发起影响确认".to_string()))?;
        let mut review = self.find_pending_change_review(&submission_id).await?;
        let now = Instant::now();
        let reason = req.decision_reason.clone().unwrap_or_default();
        review.reject(actor.id(), now, reason)?;
        let mut change_for_tx = change_order.clone();
        change_for_tx.reject(actor.id())?;
        let mut completed = self
            .complete_work_item("sales_change_review", &review.base.id, actor, now)
            .await?;
        let audit = actor.clone().resource_log(
            "sales_change_order.review_rejected",
            "sales_change_order",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_change_reviews().update(&mut review, session).await?;
                    db.sales_change_orders()
                        .update(&mut change_for_tx, session)
                        .await?;
                    db.work_items().update(&mut completed, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.sales_change_order_detail(id).await
    }

    /// 按变更提交查找待处理复核记录。
    ///
    /// # 参数
    /// * `submission_id` - 变更提交 ID
    ///
    /// # 返回
    /// 返回待处理复核记录。
    ///
    /// # 错误
    /// 无待处理复核时返回 `NotFound`。
    async fn find_pending_change_review(
        &self,
        submission_id: &SalesChangeSubmissionId,
    ) -> Result<SalesChangeReview> {
        self.db
            .sales_change_reviews()
            .find_one(
                mongodb::bson::doc! {
                    "sales_change_submission_id": submission_id.to_string(),
                    "status": entities::sales_review::SalesReviewStatus::Pending.as_str(),
                },
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("待处理变更复核不存在".to_string()))
    }
}
