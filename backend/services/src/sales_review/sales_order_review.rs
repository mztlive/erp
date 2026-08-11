// ---------------------------------------------------------------------
// 卡券销售审批轨（W05；§8.1.2 本批部分：不含执行投影修订）
// ---------------------------------------------------------------------

use database::{
    AccessControlExt, NoTransaction, ReceivableExt, SalesOrderExt, SalesReviewExt, Transactional, WorkItemExt,
};
use entities::common::time::Instant;
use entities::ids::{SalesOrderId, SalesOrderReviewId, SalesOrderSubmissionId, WorkItemId};
use entities::sales_order::RevisionSource;
use entities::sales_review::{SalesOrderReview, SalesOrderReviewData, SalesReviewStage};
use entities::work_item::{WorkItem, WorkItemData, WorkItemPriority, WorkItemType};
use id_generator::next_id;
use validator::Validate;

use super::dto;
use super::formalization::{build_receivable_account, build_receivable_entry, build_revision};
use super::{
    PageView, ReviewDecisionRequest, SalesOrderReviewFilter, SalesOrderReviewListParams,
    SalesOrderReviewView, SalesReviewService,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl SalesReviewService {
    /// 分页查询销售审批记录。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sales_order_review_list(
        &self,
        params: &SalesOrderReviewListParams,
    ) -> Result<PageView<SalesOrderReviewView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesOrderReviewFilter {
            submission_id: query.submission_id.map(SalesOrderSubmissionId::new),
            sales_order_id: query.sales_order_id.map(SalesOrderId::new),
            review_stage: query.review_stage,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .sales_order_reviews()
            .search_sales_order_reviews(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SalesOrderReviewView {
                id: row.id,
                sales_order_id: row.sales_order_id,
                submission_id: row.submission_id,
                review_stage: row.review_stage,
                status: row.status,
                reviewer_id: row.reviewer_id,
                reviewed_at: row.reviewed_at,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 通过卡券销售审批（销售领导 → 待运营；运营 → §8.1.2 本批部分生效事务）。
    ///
    /// # 参数
    /// * `id` - 审批记录 ID
    /// * `req` - 决策请求（意见可空）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回审批记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 审批记录/销售单/提交不存在
    /// * `ConflictError` - 审批已处理或状态机拒绝
    pub async fn approve_sales_order_review(
        &self,
        id: &str,
        req: ReviewDecisionRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderReviewView> {
        self.decide_sales_order_review(id, req, true, actor).await
    }

    /// 驳回卡券销售审批（销售单回草稿，提交标记驳回）。
    ///
    /// # 参数
    /// * `id` - 审批记录 ID
    /// * `req` - 决策请求（驳回原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回审批记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 审批记录/销售单/提交不存在
    /// * `ConflictError` - 审批已处理或状态机拒绝
    pub async fn reject_sales_order_review(
        &self,
        id: &str,
        req: ReviewDecisionRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderReviewView> {
        self.decide_sales_order_review(id, req, false, actor).await
    }

    /// 执行卡券审批决策（通过/驳回共用编排）。
    ///
    /// # 参数
    /// * `id` - 审批记录 ID
    /// * `req` - 决策请求
    /// * `approved` - `true` 通过，`false` 驳回
    /// * `actor` - 操作人
    ///
    /// # 返回
    /// 返回审批记录视图。
    ///
    /// # 错误
    /// 查询失败或状态机拒绝时返回错误。
    async fn decide_sales_order_review(
        &self,
        id: &str,
        req: ReviewDecisionRequest,
        approved: bool,
        actor: &AuditActor,
    ) -> Result<SalesOrderReviewView> {
        req.validate()?;
        let mut review = self
            .db
            .sales_order_reviews()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("审批记录不存在".to_string()))?;
        if review.stable.status != entities::sales_review::SalesReviewStatus::Pending {
            return Err(Error::ConflictError("审批已处理，不允许重复决策".to_string()));
        }
        let now = Instant::now();
        let order = self
            .db
            .sales_orders()
            .find_by_id(&review.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let submission = self
            .db
            .sales_order_submissions()
            .find_by_id(&review.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售提交不存在".to_string()))?;

        if approved {
            review.approve(actor.id(), now, req.decision_reason.clone())?;
        } else {
            let reason = req.decision_reason.clone().unwrap_or_default();
            review.reject(actor.id(), now, reason)?;
        }

        // 编排后续动作（实体状态迁移在事务外完成，仓库写入在事务内）。
        let mut order_for_tx = order.clone();
        let mut submission_for_tx = submission.clone();
        let mut next_review = None;
        let mut formalize_extra = None;
        if approved {
            match review.review_stage {
                SalesReviewStage::SalesLeader => {
                    order_for_tx.transition_review(
                        entities::sales_order::ReviewStatus::PendingOperations,
                        actor.id(),
                    )?;
                    next_review = Some(SalesOrderReview::new(
                        SalesOrderReviewId::new(next_id()),
                        SalesOrderReviewData {
                            sales_order_id: review.sales_order_id.clone(),
                            submission_id: review.submission_id.clone(),
                            review_stage: SalesReviewStage::Operations,
                        },
                        actor.id(),
                    )?);
                }
                SalesReviewStage::Operations => {
                    let submission_lines = self
                        .db
                        .sales_order_submission_lines()
                        .list_lines_by_submissions(&[review.submission_id.clone()], &mut NoTransaction)
                        .await?;
                    let revision = build_revision(
                        &order,
                        &submission,
                        &submission_lines,
                        RevisionSource::ErpApproval,
                        now,
                        actor,
                    )?;
                    order_for_tx.approve(now, actor.id())?;
                    order_for_tx.attach_revision(&revision.revision.base.id, actor.id());
                    let account = build_receivable_account(&order_for_tx, &revision);
                    let entry = build_receivable_entry(&account, &revision, now)?;
                    formalize_extra = Some((revision, account, entry));
                }
                SalesReviewStage::LowMarginSuperior => {}
            }
        } else {
            order_for_tx.return_to_draft(actor.id())?;
            submission_for_tx.reject(actor.id())?;
        }

        let next_work_item = match (&next_review, approved) {
            (Some(next), true) if review.review_stage == SalesReviewStage::SalesLeader => {
                Some(WorkItem::new(
                    WorkItemId::new(next_id()),
                    WorkItemData {
                        work_item_type: WorkItemType::CardSalesOperationApproval,
                        business_object_type: "sales_order_review".to_string(),
                        business_object_id: next.base.id.clone(),
                        subject_version: Some(review.submission_id.to_string()),
                        owner_role: Some("operations".to_string()),
                        owner_user_id: None,
                        priority: WorkItemPriority::High,
                        due_at: None,
                        reason_code: Some("review_dispatched".to_string()),
                        impact_summary: Some("卡券销售提交待运营审批".to_string()),
                        completion_action: "DECIDE_REVIEW".to_string(),
                    },
                )?)
            }
            _ => None,
        };
        let mut completed_work_item = self
            .complete_work_item("sales_order_review", &review.base.id, actor, now)
            .await?;
        let invalidated = if approved {
            Vec::new()
        } else {
            self.invalidate_pending_reviews(&review.submission_id, &review.base.id, now)
                .await?
        };
        let audit = actor.clone().resource_log(
            if approved {
                "sales_order_review.approve"
            } else {
                "sales_order_review.reject"
            },
            "sales_order",
            review.sales_order_id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let review = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_order_reviews().update(&mut review, session).await?;
                    if let Some(next) = &next_review {
                        db.sales_order_reviews().create(next, session).await?;
                    }
                    if let Some(item) = &next_work_item {
                        db.work_items().create(item, session).await?;
                    }
                    match formalize_extra {
                        Some((revision, account, entry)) => {
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
                            db.receivable()
                                .create_receivable_with_entry(&account, &entry, session)
                                .await?;
                        }
                        None => {
                            db.sales_orders().update(&mut order_for_tx, session).await?;
                        }
                    }
                    if !approved {
                        db.sales_order_submissions()
                            .update(&mut submission_for_tx, session)
                            .await?;
                        for mut invalid in invalidated {
                            db.sales_order_reviews().update(&mut invalid, session).await?;
                        }
                    }
                    db.work_items().update(&mut completed_work_item, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SalesOrderReview, crate::errors::Error>(review)
                })
            })
            .await?;

        Ok(SalesOrderReviewView {
            id: review.base.id,
            sales_order_id: review.sales_order_id.to_string(),
            submission_id: review.submission_id.to_string(),
            review_stage: review.review_stage,
            status: review.stable.status,
            reviewer_id: review.reviewer_id,
            reviewed_at: review.reviewed_at.map(|instant| instant.unix_secs() as u64),
            created_at: review.base.created_at,
        })
    }

    /// 待办完成查询（按业务对象取有效待办并迁移到 `Completed`）。
    ///
    /// # 参数
    /// * `business_object_type` - 业务对象类型
    /// * `business_object_id` - 业务对象 ID
    /// * `actor` - 操作人
    /// * `at` - 完成时间
    ///
    /// # 返回
    /// 返回已迁移到完成的待办实体。
    ///
    /// # 错误
    /// 有效待办不存在或状态迁移失败时返回错误。
    pub(super) async fn complete_work_item(
        &self,
        business_object_type: &str,
        business_object_id: &str,
        actor: &AuditActor,
        at: Instant,
    ) -> Result<WorkItem> {
        let mut item = self
            .owned_active_work_item(business_object_type, business_object_id, actor)
            .await?;
        item.complete(actor.id(), at)?;
        Ok(item)
    }

    /// 加载业务对象的有效待办并校验当前责任人。
    ///
    /// # 参数
    /// * `business_object_type` - 待办关联的业务对象类型
    /// * `business_object_id` - 待办关联的业务对象 ID
    /// * `actor` - 当前已认证操作人
    ///
    /// # 返回
    /// 返回由当前操作人领取的有效待办。
    ///
    /// # 错误
    /// 有效待办不存在时返回 `NotFound`；当前操作人不是责任人时返回 `Forbidden`。
    pub(super) async fn owned_active_work_item(
        &self,
        business_object_type: &str,
        business_object_id: &str,
        actor: &AuditActor,
    ) -> Result<WorkItem> {
        let mut items = self
            .db
            .work_items()
            .list_active_by_object(business_object_type, business_object_id, &mut NoTransaction)
            .await?;
        let item = items
            .pop()
            .ok_or_else(|| Error::NotFound("有效待办不存在".to_string()))?;
        if item.is_owned_by(actor.id()) {
            return Ok(item);
        }
        Err(Error::Forbidden(
            "该任务未由当前账号领取，或处理权已发生变化，请刷新待办后重试".to_string(),
        ))
    }

    /// 使同一提交的其余待处理审批失效（§6.5：新提交从第一步开始）。
    ///
    /// # 参数
    /// * `submission_id` - 被驳回的提交
    /// * `except_review_id` - 本次已决策的审批记录（跳过）
    /// * `at` - 失效时间
    ///
    /// # 返回
    /// 返回失效的审批记录列表。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn invalidate_pending_reviews(
        &self,
        submission_id: &SalesOrderSubmissionId,
        except_review_id: &str,
        at: Instant,
    ) -> Result<Vec<SalesOrderReview>> {
        let mut pending = self
            .db
            .sales_order_reviews()
            .find_many(
                mongodb::bson::doc! {
                    "submission_id": submission_id.to_string(),
                    "status": entities::sales_review::SalesReviewStatus::Pending.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        let mut invalidated = Vec::new();
        for mut review in pending.drain(..) {
            if review.base.id == except_review_id {
                continue;
            }
            review.invalidate(at)?;
            invalidated.push(review);
        }
        Ok(invalidated)
    }
}
