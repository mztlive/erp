//! 域 D14 `sales_review` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - §8.1.1 采购确认通过：单个业务事务内同时写「销售版本 + 销售状态 + 应收
//!   原始分录 + 待办收尾/派发 + 审计」（P3 §2 跨域协作模板）；
//! - §8.1.2（本批部分）卡券运营审批通过：同事务形成销售版本、应收并推进销售
//!   状态（执行投影修订属 D27，本批不写）；
//! - §8.1.3（本批部分）销售变更生效：校验基准版本仍为当前版本，追加新版本与
//!   应收差额，不修改已发生事实；
//! - 驳回 / 保存分行 / 变更生命周期其余入口：跨集合事务或单集合直写。
//!
//! 跨域协作（P3 §2）：
//! - D13 `sales_order*` Repository：读取销售单/提交/提交行，形成正式版本
//!   （`formalize_submission`）；
//! - D18 `receivable*` Repository：应收往来子账 + 原始应收分录/差额分录
//!   （`create_receivable_with_entry`，无对应 Service 依赖）；
//! - D03 `work_items`：待办完成与派发；
//! - D02 `audit_logs`：审计。
//!
//! 幂等：通过/提交类入口按「状态机终态 + 业务对象查询」去重（重复通过返回既有
//! 结果，不重复产生版本/分录/待办）。

use std::str::FromStr;

use database::{
    AccessControlExt, NoTransaction, ReceivableExt, SalesOrderExt, SalesReviewExt, SupplierCatalogExt,
    SupplierExt, Transactional, WorkItemExt,
};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    ReceivableAccountId, ReceivableEntryId, SalesChangeOrderId, SalesChangeReviewId, SalesChangeSubmissionId,
    SalesChangeSubmissionLineId, SalesOrderId, SalesOrderReviewId, SalesOrderRevisionId,
    SalesOrderSubmissionId, SalesOrderWorkingCopyId, WorkItemId,
};
use entities::money::{Amount, Quantity};
use entities::sales_order::{
    GoodsLineFields, LineType, RevisionSource, SalesOrder, SalesOrderGoodsServiceLineRevision,
    SalesOrderGoodsServiceLineRevisionData, SalesOrderGoodsServiceLineRevisionId, SalesOrderLineId,
    SalesOrderRevision, SalesOrderRevisionData, SalesOrderRevisionLine, SalesOrderRevisionLineData,
    SalesOrderRevisionLineId, SalesOrderSubmission, SalesOrderSubmissionLine, SalesOrderVoucherLineRevision,
    SalesOrderVoucherLineRevisionData, SalesOrderVoucherLineRevisionId, VoucherLineDraft, WorkingPurpose,
};
use entities::sales_review::{
    ProcurementConfirmation, ProcurementConfirmationLine, ProcurementConfirmationLineData,
    ProcurementConfirmationStatus, SalesChangeOrder, SalesChangeOrderData, SalesChangeReview,
    SalesChangeReviewData, SalesChangeReviewStage, SalesChangeSubmission, SalesChangeSubmissionData,
    SalesChangeSubmissionLine, SalesChangeSubmissionLineData, SalesOrderReview, SalesOrderReviewData,
    SalesReviewStage,
};
use entities::supplier::CapabilityStatus;
use entities::supplier_catalog::{OfferingStatus, SupplierOffering};
use entities::work_item::{WorkItem, WorkItemData, WorkItemPriority, WorkItemType};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

pub use self::dto::{
    ApproveProcurementConfirmationRequest, ChangeReviewDecisionRequest, CreateSalesChangeOrderRequest,
    PageView, ProcurementConfirmationDecisionView, ProcurementConfirmationDetailView,
    ProcurementConfirmationLineView, ProcurementConfirmationListParams, ProcurementConfirmationView,
    RejectProcurementConfirmationRequest, ReviewDecisionRequest, SalesChangeDraftRequest,
    SalesChangeLineRequest, SalesChangeOrderDetailView, SalesChangeOrderListParams, SalesChangeOrderView,
    SalesOrderReviewListParams, SalesOrderReviewView, SaveProcurementConfirmationLinesRequest,
    SubmitSalesChangeRequest, VoidSalesChangeOrderRequest,
};

/// 审批记录列表筛选条件类型（经 `SalesReviewExt` 关联类型跨 crate 可达）。
type SalesOrderReviewFilter = <mongodb::Database as SalesReviewExt>::SalesOrderReviewFilter;
/// 采购确认列表筛选条件类型。
type ProcurementConfirmationFilter = <mongodb::Database as SalesReviewExt>::ProcurementConfirmationFilter;
/// 销售变更单列表筛选条件类型。
type SalesChangeOrderFilter = <mongodb::Database as SalesReviewExt>::SalesChangeOrderFilter;

/// 销售复核服务。
///
/// 提供采购二次确认、卡券销售审批、销售变更单的决策与生命周期编排。
pub struct SalesReviewService {
    db: Database,
}

impl SalesReviewService {
    /// 创建销售复核服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    // ---------------------------------------------------------------------
    // 采购二次确认（W07）
    // ---------------------------------------------------------------------

    /// 分页查询采购确认队列。
    ///
    /// # 参数
    /// * `params` - 查询参数（`submission_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn procurement_confirmation_list(
        &self,
        params: &ProcurementConfirmationListParams,
    ) -> Result<PageView<ProcurementConfirmationView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ProcurementConfirmationFilter {
            submission_id: query.submission_id.map(SalesOrderSubmissionId::new),
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .procurement_confirmations()
            .search_procurement_confirmations(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ProcurementConfirmationView {
                id: row.id,
                sales_order_id: row.sales_order_id,
                submission_id: row.submission_id,
                status: row.status,
                handled_by: row.handled_by,
                handled_at: row.handled_at,
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

    /// 查询采购确认详情（批次 + 分行）。
    ///
    /// # 参数
    /// * `id` - 确认批次 ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 确认批次不存在
    pub async fn procurement_confirmation_detail(
        &self,
        id: &str,
    ) -> Result<ProcurementConfirmationDetailView> {
        let confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购确认不存在".to_string()))?;
        let lines = self
            .db
            .procurement_confirmation_lines()
            .list_lines_by_confirmation(&confirmation.base.id.clone().into(), &mut NoTransaction)
            .await?;
        Ok(confirmation_detail_view(&confirmation, lines))
    }

    /// 保存采购确认分行（W07 草稿编辑，乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 确认批次 ID
    /// * `req` - 保存请求（含期望版本与分行清单）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回保存后的详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 确认批次不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn save_procurement_confirmation_lines(
        &self,
        id: &str,
        req: SaveProcurementConfirmationLinesRequest,
        actor: &AuditActor,
    ) -> Result<ProcurementConfirmationDetailView> {
        req.validate()?;
        let mut confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购确认不存在".to_string()))?;
        if confirmation.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        self.owned_active_work_item("procurement_confirmation", id, actor)
            .await?;
        let lines = build_confirmation_lines(&confirmation, &req.lines)?;
        let submission_lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(
                std::slice::from_ref(&confirmation.submission_id),
                &mut NoTransaction,
            )
            .await?;
        self.ensure_confirmation_sources(&lines, &submission_lines)
            .await?;
        let old_lines = self
            .db
            .procurement_confirmation_lines()
            .list_lines_by_confirmation(&confirmation.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let audit = actor.clone().resource_log(
            "procurement_confirmation.save_lines",
            "procurement_confirmation",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let lines_for_tx = lines.clone();
        let confirmation = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    for mut old in old_lines {
                        db.procurement_confirmation_lines()
                            .soft_delete(&mut old, session)
                            .await?;
                    }
                    for line in &lines_for_tx {
                        db.procurement_confirmation_lines().create(line, session).await?;
                    }
                    db.procurement_confirmations()
                        .update(&mut confirmation, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ProcurementConfirmation, crate::errors::Error>(confirmation)
                })
            })
            .await?;

        let lines = self
            .db
            .procurement_confirmation_lines()
            .list_lines_by_confirmation(&confirmation.base.id.clone().into(), &mut NoTransaction)
            .await?;
        Ok(confirmation_detail_view(&confirmation, lines))
    }

    /// 采购确认通过（§8.1.1 单事务不变量）。
    ///
    /// 校验采购确认覆盖全部需外采明细（§6.5 跨行断言）后，在**单个事务**内：
    /// 锁定提交并形成不可变销售版本与版本行、更新销售单当前版本与状态
    /// （`EFFECTIVE` + 审核轨 `APPROVED`）、形成应收往来子账与原始应收分录、
    /// 完成采购确认待办、派发采购单创建待办、写审计。重复通过幂等返回既有结果。
    ///
    /// # 参数
    /// * `id` - 确认批次 ID
    /// * `req` - 通过请求（幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回决策结果视图（含生效版本与应收子账）。
    ///
    /// # 错误
    /// * `NotFound` - 确认批次/提交/销售单不存在
    /// * `ValidationError` - 覆盖不足或行字段组缺失
    /// * `ConflictError` - 状态机/乐观锁/唯一索引冲突
    pub async fn approve_procurement_confirmation(
        &self,
        id: &str,
        req: ApproveProcurementConfirmationRequest,
        actor: &AuditActor,
    ) -> Result<ProcurementConfirmationDecisionView> {
        req.validate()?;
        let confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购确认不存在".to_string()))?;
        if confirmation.stable.status == ProcurementConfirmationStatus::Approved {
            return self
                .procurement_decision_idempotent(&confirmation, ProcurementConfirmationStatus::Approved)
                .await;
        }
        if confirmation.stable.status != ProcurementConfirmationStatus::Pending {
            return Err(Error::ConflictError("采购确认已处理，不允许重复决策".to_string()));
        }

        let submission = self
            .db
            .sales_order_submissions()
            .find_by_id(&confirmation.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售提交不存在".to_string()))?;
        let submission_lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(
                std::slice::from_ref(&confirmation.submission_id),
                &mut NoTransaction,
            )
            .await?;
        let confirmation_lines = self
            .db
            .procurement_confirmation_lines()
            .list_lines_by_confirmation(&confirmation.base.id.clone().into(), &mut NoTransaction)
            .await?;
        self.ensure_confirmation_sources(&confirmation_lines, &submission_lines)
            .await?;
        ensure_confirmation_coverage(&submission_lines, &confirmation_lines)?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(&confirmation.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        ensure_order_awaiting_confirmation(&order)?;

        let now = Instant::now();
        let revision = build_revision(
            &order,
            &submission,
            &submission_lines,
            RevisionSource::ErpApproval,
            now,
            actor,
        )?;
        let mut order_for_tx = order.clone();
        order_for_tx.approve(now, actor.id())?;
        order_for_tx.attach_revision(&revision.revision.base.id, actor.id());

        let account = build_receivable_account(&order_for_tx, &revision);
        let entry = build_receivable_entry(&account, &revision, now)?;
        let mut confirmation_for_tx = confirmation.clone();
        confirmation_for_tx.approve(actor.id(), now)?;
        let mut work_item = self
            .complete_work_item("procurement_confirmation", id, actor, now)
            .await?;
        let creation_item = build_purchase_creation_work_item(&confirmation, &submission)?;

        let audit = actor.clone().resource_log(
            "procurement_confirmation.approve",
            "sales_order",
            confirmation.sales_order_id.to_string(),
        )?;
        let revision_id = revision.revision.base.id.clone();
        let account_id = account.base.id.clone();
        let db = self.db.clone();
        let client = db.client().clone();
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
                    db.receivable()
                        .create_receivable_with_entry(&account, &entry, session)
                        .await?;
                    db.procurement_confirmations()
                        .update(&mut confirmation_for_tx, session)
                        .await?;
                    db.work_items().update(&mut work_item, session).await?;
                    db.work_items().create(&creation_item, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ProcurementConfirmationDecisionView {
            confirmation_id: confirmation.base.id,
            sales_order_id: confirmation.sales_order_id.to_string(),
            status: ProcurementConfirmationStatus::Approved,
            revision_id: Some(revision_id),
            receivable_account_id: Some(account_id),
            handled_at: now.unix_secs() as u64,
            reference: format!("PC-OK-{}", order.order_no),
        })
    }

    /// 采购确认驳回（销售单回到销售可处理草稿，不把「驳回」混入已生效状态）。
    ///
    /// # 参数
    /// * `id` - 确认批次 ID
    /// * `req` - 驳回请求（原因代码必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回决策结果视图。
    ///
    /// # 错误
    /// * `NotFound` - 确认批次/销售单不存在
    /// * `ConflictError` - 确认批次已处理
    pub async fn reject_procurement_confirmation(
        &self,
        id: &str,
        req: RejectProcurementConfirmationRequest,
        actor: &AuditActor,
    ) -> Result<ProcurementConfirmationDecisionView> {
        req.validate()?;
        let confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购确认不存在".to_string()))?;
        if confirmation.stable.status == ProcurementConfirmationStatus::Rejected {
            return self
                .procurement_decision_idempotent(&confirmation, ProcurementConfirmationStatus::Rejected)
                .await;
        }
        if confirmation.stable.status != ProcurementConfirmationStatus::Pending {
            return Err(Error::ConflictError("采购确认已处理，不允许重复决策".to_string()));
        }
        let mut order = self
            .db
            .sales_orders()
            .find_by_id(&confirmation.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let order_no = order.order_no.clone();
        let mut submission = self
            .db
            .sales_order_submissions()
            .find_by_id(&confirmation.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售提交不存在".to_string()))?;

        let now = Instant::now();
        let mut confirmation_for_tx = confirmation.clone();
        confirmation_for_tx.reject(actor.id(), now, req.reject_reason_code)?;
        order.return_to_draft(actor.id())?;
        submission.reject(actor.id())?;
        let mut work_item = self
            .complete_work_item("procurement_confirmation", id, actor, now)
            .await?;
        let audit = actor.clone().resource_log(
            "procurement_confirmation.reject",
            "sales_order",
            confirmation.sales_order_id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.procurement_confirmations()
                        .update(&mut confirmation_for_tx, session)
                        .await?;
                    db.sales_orders().update(&mut order, session).await?;
                    db.sales_order_submissions()
                        .update(&mut submission, session)
                        .await?;
                    db.work_items().update(&mut work_item, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ProcurementConfirmationDecisionView {
            confirmation_id: confirmation.base.id,
            sales_order_id: confirmation.sales_order_id.to_string(),
            status: ProcurementConfirmationStatus::Rejected,
            revision_id: None,
            receivable_account_id: None,
            handled_at: now.unix_secs() as u64,
            reference: format!("PC-REJ-{order_no}"),
        })
    }

    /// 采购确认决策幂等回查（已处理批次重复决策返回既有结果）。
    ///
    /// # 参数
    /// * `confirmation` - 已处理的确认批次
    /// * `status` - 既有结论
    ///
    /// # 返回
    /// 返回决策结果视图。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn procurement_decision_idempotent(
        &self,
        confirmation: &ProcurementConfirmation,
        status: ProcurementConfirmationStatus,
    ) -> Result<ProcurementConfirmationDecisionView> {
        let order = self
            .db
            .sales_orders()
            .find_by_id(&confirmation.sales_order_id, &mut NoTransaction)
            .await?;
        let revision_id = order
            .as_ref()
            .and_then(|order| order.stable.current_revision_id.clone());
        let account = self
            .db
            .receivable_accounts()
            .find_one_by_field(
                "sales_order_id",
                confirmation.sales_order_id.to_string(),
                &mut NoTransaction,
            )
            .await?;
        Ok(ProcurementConfirmationDecisionView {
            confirmation_id: confirmation.base.id.clone(),
            sales_order_id: confirmation.sales_order_id.to_string(),
            status,
            revision_id,
            receivable_account_id: account.map(|account| account.base.id),
            handled_at: confirmation
                .handled_at
                .map(|instant| instant.unix_secs() as u64)
                .unwrap_or(0),
            reference: "PC-DONE".to_string(),
        })
    }

    // ---------------------------------------------------------------------
    // 卡券销售审批轨（W05；§8.1.2 本批部分：不含执行投影修订）
    // ---------------------------------------------------------------------

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
    async fn complete_work_item(
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
    async fn owned_active_work_item(
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

    /// 重新校验确认行引用的供给修订与能力修订仍是当前有效版本。
    ///
    /// # 参数
    /// * `lines` - 待保存或待审批的采购确认分行
    ///
    /// # 返回
    /// 全部分行来源与供应商匹配且仍有效时返回 `Ok(())`。
    ///
    /// # 错误
    /// 来源缺失、供应商不匹配或版本已更新时返回 `ValidationError`。
    async fn ensure_confirmation_sources(
        &self,
        lines: &[ProcurementConfirmationLine],
        submission_lines: &[SalesOrderSubmissionLine],
    ) -> Result<()> {
        for line in lines {
            let offering = self.current_confirmation_offering(line).await?;
            ensure_confirmation_line_sku(line, &offering, submission_lines)?;
            self.ensure_confirmation_capability(line).await?;
        }
        Ok(())
    }

    /// 加载并校验确认行引用的当前供给。
    async fn current_confirmation_offering(
        &self,
        line: &ProcurementConfirmationLine,
    ) -> Result<SupplierOffering> {
        let revision_id = line
            .supplier_offering_revision_id
            .as_ref()
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行缺少供给版本", line.line_no)))?;
        let revision = self
            .db
            .supplier_offering_revisions()
            .find_by_id(revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行供给版本不存在", line.line_no)))?;
        let offering = self
            .db
            .supplier_offerings()
            .find_by_id(&revision.supplier_offering_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行供给不存在", line.line_no)))?;
        let is_current = offering.stable.current_revision_id.as_deref() == Some(revision_id.as_ref());
        if offering.supplier_id != line.supplier_id
            || offering.stable.status != OfferingStatus::Active
            || !is_current
        {
            return Err(Error::ValidationError(format!(
                "采购确认第 {} 行供给已变化，请重新选择",
                line.line_no
            )));
        }
        Ok(offering)
    }

    /// 校验确认行引用的供应商能力仍为当前启用版本。
    async fn ensure_confirmation_capability(&self, line: &ProcurementConfirmationLine) -> Result<()> {
        let revision = self
            .db
            .supplier_capability_revisions()
            .find_by_id(&line.supplier_capability_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行能力版本不存在", line.line_no)))?;
        let capability = self
            .db
            .supplier_capabilities()
            .find_by_supplier_and_code(&line.supplier_id, revision.capability_code, &mut NoTransaction)
            .await?
            .ok_or_else(|| {
                Error::ValidationError(format!("采购确认第 {} 行供应商能力不存在", line.line_no))
            })?;
        let is_current = capability.stable.current_revision_id.as_deref()
            == Some(line.supplier_capability_revision_id.as_ref());
        if revision.supplier_id != line.supplier_id
            || revision.status != CapabilityStatus::Active
            || capability.stable.status != CapabilityStatus::Active
            || !is_current
        {
            return Err(Error::ValidationError(format!(
                "采购确认第 {} 行能力版本已变化，请重新选择",
                line.line_no
            )));
        }
        Ok(())
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

    // ---------------------------------------------------------------------
    // 销售变更单（W05 变更轨；§8.1.3 本批部分）
    // ---------------------------------------------------------------------

    /// 分页查询销售变更单。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sales_change_order_list(
        &self,
        params: &SalesChangeOrderListParams,
    ) -> Result<PageView<SalesChangeOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesChangeOrderFilter {
            sales_order_id: query.sales_order_id.map(SalesOrderId::new),
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .sales_change_orders()
            .search_sales_change_orders(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SalesChangeOrderView {
                id: row.id,
                sales_order_id: row.sales_order_id,
                base_revision_id: row.base_revision_id,
                change_type: row.change_type,
                status: row.status,
                current_submission_id: row.current_submission_id,
                version: row.version,
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

    /// 查询销售变更单详情。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    pub async fn sales_change_order_detail(&self, id: &str) -> Result<SalesChangeOrderDetailView> {
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        Ok(SalesChangeOrderDetailView {
            id: change_order.base.id,
            sales_order_id: change_order.sales_order_id.to_string(),
            base_revision_id: change_order.base_revision_id.to_string(),
            change_type: change_order.change_type,
            reason: change_order.reason,
            status: change_order.stable.status,
            current_submission_id: change_order
                .current_submission_id
                .as_ref()
                .map(ToString::to_string),
            target_content_hash: change_order.target_content_hash,
            effective_revision_id: change_order
                .effective_revision_id
                .as_ref()
                .map(ToString::to_string),
            version: change_order.base.version,
            created_at: change_order.base.created_at,
        })
    }

    /// 创建销售变更单（草稿 + 变更工作副本原子形成；基准版本为当前版本）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 原销售单不存在或未生效
    /// * `ConflictError` - 同一基准版本已有进行中变更
    pub async fn create_sales_change_order(
        &self,
        req: CreateSalesChangeOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(&req.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.stable.status != entities::sales_order::CommercialStatus::Effective {
            return Err(Error::BusinessLogicError(
                "只有已生效的销售单才能发起变更".to_string(),
            ));
        }
        let base_revision_id = order
            .stable
            .current_revision_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本，无法发起变更".to_string()))?;
        let existing = self
            .db
            .sales_change_orders()
            .find_in_progress_by_order_and_base(
                &req.sales_order_id,
                &base_revision_id.clone().into(),
                &mut NoTransaction,
            )
            .await?;
        if existing.is_some() {
            return Err(Error::ConflictError(
                "同一基准版本已有进行中的销售变更单".to_string(),
            ));
        }

        let change_order = SalesChangeOrder::new(
            SalesChangeOrderId::new(next_id()),
            SalesChangeOrderData {
                sales_order_id: req.sales_order_id.clone(),
                base_revision_id: base_revision_id.clone().into(),
                change_type: req.change_type,
                reason: req.reason,
            },
            actor.id(),
        )?;
        let stable_lines = self
            .db
            .sales_order_lines()
            .list_lines_by_order(&req.sales_order_id, &mut NoTransaction)
            .await?;
        let working_copy_id = SalesOrderWorkingCopyId::new(next_id());
        let lines = build_change_working_copy_lines(&working_copy_id, &stable_lines, &req.draft.lines)?;
        let (gross, net, tax) = change_line_totals(&lines);
        let working_copy = entities::sales_order::SalesOrderWorkingCopy::new(
            working_copy_id,
            entities::sales_order::SalesOrderWorkingCopyData {
                sales_order_id: req.sales_order_id.clone(),
                working_purpose: WorkingPurpose::SalesChange,
                sales_change_order_id: Some(change_order.base.id.clone().into()),
                base_revision_id: Some(base_revision_id.clone().into()),
                draft_version: 1,
                content_hash: format!("change:{}:1", change_order.base.id),
                editor_user_id: req.draft.editor_user_id.clone(),
                business_type: order.business_type,
                customer_id: order.customer_id.clone(),
                contract_id: order.contract_id.clone(),
                settlement_party_id: order.settlement_party_id.clone(),
                snapshot: change_header_snapshot(&req.draft)?,
                project_name: req.draft.project_name.clone(),
                business_remark: req.draft.business_remark.clone(),
                voucher_category_sku_id: req.draft.voucher_category_sku_id.clone(),
                voucher_expiry_at: req
                    .draft
                    .voucher_expiry_at
                    .map(|secs| Instant::from_unix_secs(secs as i64)),
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
                lines: Vec::new(),
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "sales_change_order.create",
            "sales_change_order",
            change_order.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let change_for_tx = change_order.clone();
        let copy_for_tx = working_copy.clone();
        let lines_for_tx = lines.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_change_orders().create(&change_for_tx, session).await?;
                    db.sales_order_working_copies()
                        .create(&copy_for_tx, session)
                        .await?;
                    for line in &lines_for_tx {
                        db.sales_order_working_copy_lines().create(line, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.sales_change_order_detail(&change_order.base.id).await
    }

    /// 发起销售变更影响确认（形成不可变变更提交并进入影响确认/财务复核链）。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 提交请求（含期望版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图；已发起过影响确认的变更单幂等返回既有结果。
    ///
    /// # 错误
    /// * `NotFound` - 变更单或变更工作副本不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn submit_sales_change(
        &self,
        id: &str,
        req: SubmitSalesChangeRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        if change_order.current_submission_id.is_some() {
            return self.sales_change_order_detail(id).await;
        }
        let mut change_for_tx = change_order.clone();
        if change_for_tx.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        let working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(
                &change_for_tx.sales_order_id,
                WorkingPurpose::SalesChange,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("变更工作副本不存在".to_string()))?;
        let copy_id = SalesOrderWorkingCopyId::new(working_copy.base.id.clone());
        let copy_lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&copy_id, &mut NoTransaction)
            .await?;
        let submission = build_change_submission(&change_for_tx, &working_copy, &copy_lines, actor)?;
        let submission_lines = build_change_submission_lines(&submission, &copy_lines)?;
        let mut working_copy_for_tx = working_copy.clone();
        working_copy_for_tx.submit()?;
        change_for_tx.submit_impact(
            submission.base.id.clone().into(),
            format!("sub:{}", submission.base.id),
            actor.id(),
        )?;

        let review = SalesChangeReview::new(
            SalesChangeReviewId::new(next_id()),
            SalesChangeReviewData {
                sales_change_submission_id: submission.base.id.clone().into(),
                review_stage: if working_copy.business_type
                    == entities::sales_order::BusinessType::GoodsService
                {
                    SalesChangeReviewStage::ProcurementImpact
                } else {
                    SalesChangeReviewStage::OperationsImpact
                },
            },
            actor.id(),
        )?;
        let work_item = WorkItem::new(
            WorkItemId::new(next_id()),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementConfirmation,
                business_object_type: "sales_change_review".to_string(),
                business_object_id: review.base.id.clone(),
                subject_version: Some(submission.base.id.clone()),
                owner_role: Some("procurement".to_string()),
                owner_user_id: None,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: Some("change_impact_dispatched".to_string()),
                impact_summary: Some("销售变更履约影响确认".to_string()),
                completion_action: "DECIDE_CHANGE_REVIEW".to_string(),
            },
        )?;
        let audit = actor.clone().resource_log(
            "sales_change_order.submit_impact",
            "sales_change_order",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let submission_for_tx = submission.clone();
        let lines_for_tx = submission_lines.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_review()
                        .submit_sales_change(&mut change_for_tx, &submission_for_tx, &lines_for_tx, session)
                        .await?;
                    db.sales_order_working_copies()
                        .update(&mut working_copy_for_tx, session)
                        .await?;
                    db.sales_change_reviews().create(&review, session).await?;
                    db.work_items().create(&work_item, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.sales_change_order_detail(id).await
    }

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

    /// 作废销售变更单（仅草稿态）。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    /// * `req` - 作废请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn void_sales_change(
        &self,
        id: &str,
        req: VoidSalesChangeOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        req.validate()?;
        let mut change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        if change_order.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        change_order.void(actor.id())?;
        let mut working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(
                &change_order.sales_order_id,
                WorkingPurpose::SalesChange,
                &mut NoTransaction,
            )
            .await?;
        if let Some(copy) = &mut working_copy {
            copy.abandon()?;
        }
        let audit =
            actor
                .clone()
                .resource_log("sales_change_order.void", "sales_change_order", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_change_orders()
                        .update(&mut change_order, session)
                        .await?;
                    if let Some(copy) = &mut working_copy {
                        db.sales_order_working_copies().update(copy, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.sales_change_order_detail(id).await
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

// ---------------------------------------------------------------------------
// 聚合构造与校验（纯内存，不依赖仓储）
// ---------------------------------------------------------------------------

/// 校验确认分行选择的供给属于对应销售提交商品。
fn ensure_confirmation_line_sku(
    line: &ProcurementConfirmationLine,
    offering: &SupplierOffering,
    submission_lines: &[SalesOrderSubmissionLine],
) -> Result<()> {
    let submission_line = submission_lines
        .iter()
        .find(|row| row.base.id == line.sales_order_submission_line_id.as_ref())
        .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行销售明细不存在", line.line_no)))?;
    if submission_line.sku_id.as_ref() == Some(&offering.sku_id) {
        return Ok(());
    }
    Err(Error::ValidationError(format!(
        "采购确认第 {} 行供给不属于该销售商品，请重新选择",
        line.line_no
    )))
}

/// 校验采购确认覆盖全部需外采明细（§6.5 跨行断言）。
///
/// # 参数
/// * `submission_lines` - 销售提交明细
/// * `confirmation_lines` - 采购确认分行
///
/// # 返回
/// 全部行确认数量覆盖承诺数量时返回 `Ok(())`。
///
/// # 错误
/// 任一提交明细确认数量不足时返回 `ValidationError`。
fn ensure_confirmation_coverage(
    submission_lines: &[SalesOrderSubmissionLine],
    confirmation_lines: &[ProcurementConfirmationLine],
) -> Result<()> {
    for sub_line in submission_lines {
        if sub_line.line_type == LineType::Voucher {
            continue;
        }
        let required = sub_line
            .quantity
            .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少承诺数量", sub_line.line_no)))?;
        let mut confirmed = Quantity::from_str("0").expect("静态零值必须合法");
        for line in confirmation_lines {
            if line.sales_order_submission_line_id.to_string() == sub_line.base.id {
                if line.supplier_offering_revision_id.is_none() {
                    return Err(Error::ValidationError(format!(
                        "第 {} 行缺少有效供给版本，请重新选择供应商供给",
                        sub_line.line_no
                    )));
                }
                confirmed = Quantity::try_from(confirmed.to_decimal() + line.confirmed_quantity.to_decimal())
                    .map_err(|error| Error::ValidationError(error.to_string()))?;
            }
        }
        if confirmed.to_decimal() < required.to_decimal() {
            return Err(Error::ValidationError(format!(
                "第 {} 行确认数量不足：需 {required}，已确认 {confirmed}",
                sub_line.line_no
            )));
        }
    }
    Ok(())
}

/// 校验销售单处于待采购确认状态。
///
/// # 参数
/// * `order` - 销售单
///
/// # 返回
/// 状态合法时返回 `Ok(())`。
///
/// # 错误
/// 状态非法时返回 `ValidationError`。
fn ensure_order_awaiting_confirmation(order: &SalesOrder) -> Result<()> {
    if order.stable.status != entities::sales_order::CommercialStatus::PendingReview
        || order.review_status != entities::sales_order::ReviewStatus::PendingProcurementConfirmation
    {
        return Err(Error::ValidationError(
            "销售单不在待采购确认状态，无法通过采购确认".to_string(),
        ));
    }
    Ok(())
}

/// 销售版本聚合载体（版本头 + 公共行 + 子类型行）。
struct RevisionAggregate {
    /// 版本头实体。
    pub revision: SalesOrderRevision,
    /// 公共行版本。
    pub lines: Vec<SalesOrderRevisionLine>,
    /// 公共行版本。
    pub goods_lines: Vec<SalesOrderGoodsServiceLineRevision>,
    /// 卡券行版本。
    pub voucher_lines: Vec<SalesOrderVoucherLineRevision>,
}

/// 从销售提交构建正式版本聚合（§8.1.1/§8.1.2 共用）。
///
/// 版本号 = 既有版本数 + 1；内容指纹以提交 ID 确定性派生（幂等去重键）。
///
/// # 参数
/// * `order` - 销售单
/// * `submission` - 已通过的提交快照
/// * `submission_lines` - 提交快照明细
/// * `source` - 版本来源
/// * `effective_at` - 生效时间
/// * `actor` - 操作人
///
/// # 返回
/// 返回版本聚合。
///
/// # 错误
/// 行字段组缺失或版本字段校验失败时返回错误。
fn build_revision(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    submission_lines: &[SalesOrderSubmissionLine],
    source: RevisionSource,
    effective_at: Instant,
    actor: &AuditActor,
) -> Result<RevisionAggregate> {
    let _ = actor;
    // 首次生效路径（§8.1.1/§8.1.2）版本号恒为 1：销售单只能被通过一次，
    // 后续版本一律经销售变更单（§8.1.3，版本号由变更入口另行计算）。
    let revision_no = 1;
    let revision_id = SalesOrderRevisionId::new(next_id());
    let revision = SalesOrderRevision::new(
        revision_id.clone(),
        SalesOrderRevisionData {
            sales_order_id: submission.sales_order_id.clone(),
            revision_no,
            revision_source: source,
            source_snapshot_id: None,
            previous_revision_id: order.stable.current_revision_id.clone().map(Into::into),
            content_hash: format!("sub:{}", submission.base.id),
            customer_revision_id: None,
            contract_revision_id: submission.contract_revision_id.clone(),
            snapshot: entities::sales_order::HeaderSnapshotData {
                customer_name: submission.customer_snapshot.customer_name.clone(),
                contract_no: submission
                    .contract_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.contract_no.clone()),
                settlement_party_name: submission
                    .settlement_party_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.settlement_party_name.clone()),
                payment_term_code: submission.payment_term_snapshot.payment_term_code.clone(),
                payment_term_name: submission.payment_term_snapshot.payment_term_name.clone(),
                invoice_type: submission.invoice_requirement_snapshot.invoice_type.clone(),
                tax_point: submission.invoice_requirement_snapshot.tax_point.clone(),
            },
            project_name: submission.project_name.clone(),
            business_remark: submission.business_remark.clone(),
            voucher_category_sku_id: submission.voucher_category_sku_id.clone(),
            voucher_expiry_at: submission.voucher_expiry_at,
            gross_amount: submission.gross_amount,
            net_amount: submission.net_amount,
            tax_amount: submission.tax_amount,
            effective_at,
            recorded_at: effective_at,
        },
    )?;
    let mut revision_lines = Vec::with_capacity(submission_lines.len());
    let mut goods_lines = Vec::new();
    let mut voucher_lines = Vec::new();
    for sub_line in submission_lines {
        let revision_line_id = SalesOrderRevisionLineId::new(next_id());
        let revision_line = SalesOrderRevisionLine::new(
            revision_line_id.clone(),
            SalesOrderRevisionLineData {
                sales_order_revision_id: revision_id.clone(),
                sales_order_line_id: sub_line.sales_order_line_id.clone(),
                line_no: sub_line.line_no,
                line_type: sub_line.line_type,
                gross_amount: sub_line.gross_amount,
                net_amount: sub_line.net_amount,
                tax_amount: sub_line.tax_amount,
                sales_tax_rate: sub_line.sales_tax_rate,
                item_name_snapshot: sub_line.item_name_snapshot.clone(),
                spec_snapshot: sub_line.spec_snapshot.clone(),
                unit_snapshot: sub_line.unit_snapshot.clone(),
            },
        )?;
        match sub_line.line_type {
            LineType::GoodsService => {
                let goods = submission_line_goods(sub_line)?;
                goods_lines.push(SalesOrderGoodsServiceLineRevision::new(
                    SalesOrderGoodsServiceLineRevisionId::new(next_id()),
                    SalesOrderGoodsServiceLineRevisionData {
                        revision_line_id: revision_line_id.clone(),
                        sku_id: goods.sku_id,
                        sku_revision_id: goods.sku_revision_id,
                        welfare_scenario: goods.welfare_scenario,
                        fulfillment_mode: goods.fulfillment_mode,
                        fulfillment_due_at: goods.fulfillment_due_at,
                        quantity: goods.quantity,
                        base_unit_code: goods.base_unit_code,
                        unit_price_gross: goods.unit_price_gross,
                    },
                )?);
            }
            LineType::Voucher => {
                let voucher = submission_line_voucher(sub_line)?;
                voucher_lines.push(SalesOrderVoucherLineRevision::new(
                    SalesOrderVoucherLineRevisionId::new(next_id()),
                    SalesOrderVoucherLineRevisionData {
                        revision_line_id: revision_line_id.clone(),
                        face_value: voucher.face_value,
                        card_count: voucher.card_count,
                        unit_price_gross: voucher.unit_price_gross,
                        card_form: voucher.card_form,
                    },
                )?);
            }
        }
        revision_lines.push(revision_line);
    }
    Ok(RevisionAggregate {
        revision,
        lines: revision_lines,
        goods_lines,
        voucher_lines,
    })
}

/// 从提交行还原实物及服务字段组。
///
/// # 参数
/// * `line` - 提交行
///
/// # 返回
/// 返回字段组。
///
/// # 错误
/// 实物及服务行缺商品字段组时返回错误。
fn submission_line_goods(line: &SalesOrderSubmissionLine) -> Result<GoodsLineFields> {
    let sku_id = line
        .sku_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少商品字段组", line.line_no)))?;
    let sku_revision_id = line
        .sku_revision_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU 修订", line.line_no)))?;
    let fulfillment_mode = line
        .fulfillment_mode
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约方式", line.line_no)))?;
    let fulfillment_due_at = line
        .fulfillment_due_at
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约期限", line.line_no)))?;
    let quantity = line
        .quantity
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少数量", line.line_no)))?;
    let base_unit_code = line
        .base_unit_code
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少单位", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少含税单价", line.line_no)))?;
    Ok(GoodsLineFields {
        sku_id,
        sku_revision_id,
        welfare_scenario: line.welfare_scenario,
        fulfillment_mode,
        fulfillment_due_at,
        quantity,
        base_unit_code,
        unit_price_gross,
    })
}

/// 从提交行还原卡券字段组。
///
/// # 参数
/// * `line` - 提交行
///
/// # 返回
/// 返回字段组。
///
/// # 错误
/// 卡券行缺卡券字段组时返回错误。
fn submission_line_voucher(line: &SalesOrderSubmissionLine) -> Result<VoucherLineDraft> {
    let face_value = line
        .face_value
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券字段组", line.line_no)))?;
    let card_count = line
        .card_count
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡张数", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券成交单价", line.line_no)))?;
    let face_value_total = line
        .face_value_total
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少面额小计", line.line_no)))?;
    let transaction_amount = line
        .transaction_amount
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少成交金额", line.line_no)))?;
    let gift_amount = line
        .gift_amount
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少配赠金额", line.line_no)))?;
    Ok(VoucherLineDraft {
        face_value,
        card_count,
        unit_price_gross,
        face_value_total,
        transaction_amount,
        gift_amount,
        gift_rate: line.gift_rate,
        card_form: line
            .card_form
            .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡形态", line.line_no)))?,
    })
}

/// 构建应收往来子账（§8.1.1 原始应收）。
///
/// # 参数
/// * `order` - 已生效的销售单
/// * `revision` - 生效版本
///
/// # 返回
/// 返回子账实体。
fn build_receivable_account(
    order: &SalesOrder,
    revision: &RevisionAggregate,
) -> entities::receivable::ReceivableAccount {
    let revision_id = revision.revision.base.id.clone().into();
    entities::receivable::ReceivableAccount::new(
        ReceivableAccountId::new(next_id()),
        entities::receivable::ReceivableAccountData {
            sales_order_id: order.base.id.clone().into(),
            account_seq: 1,
            customer_id: order.customer_id.clone(),
            counterparty_party_id: order.settlement_party_id.clone(),
            source_sales_order_revision_id: revision_id,
            review_status: entities::receivable::AccountReviewStatus::NotApplicable,
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: Amount::from_str("0.00").expect("静态零值必须合法"),
            settled_total: Amount::from_str("0.00").expect("静态零值必须合法"),
            invoiceable_total: Amount::from_str("0.00").expect("静态零值必须合法"),
            invoiced_total: Amount::from_str("0.00").expect("静态零值必须合法"),
        },
        "system",
    )
    .expect("新建应收子账必须通过实体校验")
}

/// 构建原始应收分录（§8.1.1）。
///
/// 到期日按当前业务日（付款条件→到期日映射属公共规则，待地基修订下沉）。
///
/// # 参数
/// * `account` - 应收往来子账
/// * `revision` - 生效版本
/// * `posted_at` - 入账时间
///
/// # 返回
/// 返回分录实体。
///
/// # 错误
/// 分录字段校验失败时返回错误。
fn build_receivable_entry(
    account: &entities::receivable::ReceivableAccount,
    revision: &RevisionAggregate,
    posted_at: Instant,
) -> Result<entities::receivable::ReceivableEntry> {
    let gross = revision_gross(revision)?;
    entities::receivable::ReceivableEntry::new(
        ReceivableEntryId::new(next_id()),
        entities::receivable::ReceivableEntryData {
            receivable_account_id: account.base.id.clone().into(),
            entry_type: entities::receivable::ReceivableEntryType::Original,
            direction: entities::receivable::EntryDirection::Increase,
            amount: gross,
            due_date: BusinessDate::today(),
            source_fact_type: "SALES_ORDER".to_string(),
            source_document_id: account.sales_order_id.to_string(),
            source_revision_id: revision.revision.base.id.clone(),
            source_sequence: 1,
            posted_at,
        },
    )
    .map_err(Error::Logic)
}

/// 汇总版本聚合的含税金额（取公共行合计；与版本头金额一致由实体保证）。
///
/// # 参数
/// * `revision` - 版本聚合
///
/// # 返回
/// 返回含税合计。
///
/// # 错误
/// 无行时返回 `ValidationError`。
fn revision_gross(revision: &RevisionAggregate) -> Result<Amount> {
    let zero = Amount::from_str("0.00").expect("静态零值必须合法");
    Ok(revision
        .lines
        .iter()
        .fold(zero, |acc, line| acc.checked_add(line.gross_amount)))
}

/// 构建采购单创建待办（§8.1.1「生成后续采购待办」）。
///
/// 固定 15 类待办中无「采购单创建」专属类型，沿用 `PurchaseOrderReview` 承载
/// 采购侧后继动作；业务对象指向确认批次，D15 批次消费。
///
/// # 参数
/// * `confirmation` - 已通过的确认批次
/// * `submission` - 被确认的销售提交
///
/// # 返回
/// 返回未领取的待办实体。
///
/// # 错误
/// 待办字段校验失败时返回错误。
fn build_purchase_creation_work_item(
    confirmation: &ProcurementConfirmation,
    submission: &SalesOrderSubmission,
) -> Result<WorkItem> {
    WorkItem::new(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: WorkItemType::PurchaseOrderReview,
            business_object_type: "purchase_order_creation".to_string(),
            business_object_id: confirmation.base.id.clone(),
            subject_version: Some(submission.base.id.clone()),
            owner_role: Some("procurement".to_string()),
            owner_user_id: None,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some("purchase_order_creation_basis".to_string()),
            impact_summary: Some("采购确认通过，待创建采购单草稿依据".to_string()),
            completion_action: "CREATE_PURCHASE_ORDER".to_string(),
        },
    )
    .map_err(Into::into)
}

/// 构建采购确认分行实体。
///
/// # 参数
/// * `confirmation` - 所属确认批次
/// * `lines` - 分行请求
///
/// # 返回
/// 返回分行实体清单。
///
/// # 错误
/// 行号重复时返回错误。
fn build_confirmation_lines(
    confirmation: &ProcurementConfirmation,
    lines: &[dto::ProcurementConfirmationLineRequest],
) -> Result<Vec<ProcurementConfirmationLine>> {
    let mut built = Vec::with_capacity(lines.len());
    for line in lines {
        if built
            .iter()
            .any(|existing: &ProcurementConfirmationLine| existing.line_no == line.line_no)
        {
            return Err(Error::ValidationError(format!("行号 {} 重复", line.line_no)));
        }
        built.push(ProcurementConfirmationLine::new(
            entities::ids::ProcurementConfirmationLineId::new(next_id()),
            ProcurementConfirmationLineData {
                procurement_confirmation_id: confirmation.base.id.clone().into(),
                line_no: line.line_no,
                sales_order_submission_line_id: line.sales_order_submission_line_id.clone(),
                supplier_id: line.supplier_id.clone(),
                supplier_offering_revision_id: line.supplier_offering_revision_id.clone(),
                confirmed_quantity: line.confirmed_quantity,
                latest_cost_gross: line.latest_cost_gross,
                input_tax_rate: line.input_tax_rate,
                expected_delivery_date: line.expected_delivery_date,
                fulfillment_mode: line.fulfillment_mode,
                supplier_capability_revision_id: line.supplier_capability_revision_id.clone(),
            },
        )?);
    }
    Ok(built)
}

/// 构造采购确认详情视图。
///
/// # 参数
/// * `confirmation` - 确认批次实体
/// * `lines` - 分行实体
///
/// # 返回
/// 返回详情视图。
fn confirmation_detail_view(
    confirmation: &ProcurementConfirmation,
    lines: Vec<ProcurementConfirmationLine>,
) -> ProcurementConfirmationDetailView {
    ProcurementConfirmationDetailView {
        id: confirmation.base.id.clone(),
        sales_order_id: confirmation.sales_order_id.to_string(),
        submission_id: confirmation.submission_id.to_string(),
        status: confirmation.stable.status,
        handled_by: confirmation.handled_by.clone(),
        handled_at: confirmation.handled_at.map(|instant| instant.unix_secs() as u64),
        version: confirmation.base.version,
        created_at: confirmation.base.created_at,
        lines: lines
            .into_iter()
            .map(|line| ProcurementConfirmationLineView {
                id: line.base.id,
                line_no: line.line_no,
                sales_order_submission_line_id: line.sales_order_submission_line_id.to_string(),
                supplier_id: line.supplier_id.to_string(),
                supplier_offering_revision_id: line.supplier_offering_revision_id.map(|id| id.to_string()),
                confirmed_quantity: line.confirmed_quantity,
                latest_cost_gross: line.latest_cost_gross,
                input_tax_rate: line.input_tax_rate,
                expected_delivery_date: line.expected_delivery_date,
                fulfillment_mode: line.fulfillment_mode,
                supplier_capability_revision_id: line.supplier_capability_revision_id.to_string(),
            })
            .collect(),
    }
}

/// 构建变更工作副本行实体。
///
/// # 参数
/// * `working_copy_id` - 所属工作副本 ID
/// * `stable_lines` - 稳定明细行
/// * `lines` - 变更目标行请求
///
/// # 返回
/// 返回工作副本行清单。
///
/// # 错误
/// 行字段组与行类型不一致、金额非法时返回错误。
fn build_change_working_copy_lines(
    working_copy_id: &SalesOrderWorkingCopyId,
    stable_lines: &[entities::sales_order::SalesOrderLine],
    lines: &[SalesChangeLineRequest],
) -> Result<Vec<entities::sales_order::SalesOrderWorkingCopyLine>> {
    let mut built = Vec::with_capacity(lines.len());
    for line in lines {
        let stable_id = stable_lines
            .iter()
            .find(|stable| stable.line_no == line.line_no)
            .map(|stable| stable.base.id.clone())
            .ok_or_else(|| Error::ValidationError(format!("行号 {} 无对应稳定明细", line.line_no)))?;
        built.push(entities::sales_order::SalesOrderWorkingCopyLine::new(
            entities::ids::SalesOrderWorkingCopyLineId::new(next_id()),
            working_copy_id.clone(),
            entities::sales_order::SalesOrderWorkingCopyLineData {
                sales_order_line_id: SalesOrderLineId::new(stable_id),
                line_no: line.line_no,
                line_type: line.line_type,
                sales_tax_rate: line.sales_tax_rate,
                item_name_snapshot: line.item_name_snapshot.clone(),
                spec_snapshot: line.spec_snapshot.clone(),
                unit_snapshot: line.unit_snapshot.clone(),
                goods: line.goods.clone(),
                voucher: line.voucher.clone(),
            },
        )?);
    }
    Ok(built)
}

/// 变更行金额访问器。
trait ChangeLineAmounts {
    /// 返回行含税金额。
    fn gross_amount(&self) -> Amount;
    /// 返回行不含税金额。
    fn net_amount(&self) -> Amount;
    /// 返回行税额。
    fn tax_amount(&self) -> Amount;
}

impl ChangeLineAmounts for entities::sales_order::SalesOrderWorkingCopyLine {
    fn gross_amount(&self) -> Amount {
        self.gross_amount
    }
    fn net_amount(&self) -> Amount {
        self.net_amount
    }
    fn tax_amount(&self) -> Amount {
        self.tax_amount
    }
}

/// 汇总已舍入的行金额三元组（§4.2 铁律 2）。
///
/// # 参数
/// * `lines` - 行实体
///
/// # 返回
/// 返回 `(含税合计, 不含税合计, 税额合计)`。
fn change_line_totals(
    lines: &[entities::sales_order::SalesOrderWorkingCopyLine],
) -> (Amount, Amount, Amount) {
    let zero = Amount::from_str("0.00").expect("静态零值必须合法");
    let gross = lines
        .iter()
        .fold(zero, |acc, line| acc.checked_add(line.gross_amount()));
    let net = lines
        .iter()
        .fold(zero, |acc, line| acc.checked_add(line.net_amount()));
    let tax = lines
        .iter()
        .fold(zero, |acc, line| acc.checked_add(line.tax_amount()));
    (gross, net, tax)
}

/// 构建变更表头快照入参。
///
/// # 参数
/// * `draft` - 变更目标草稿请求
///
/// # 返回
/// 返回表头快照入参。
fn change_header_snapshot(
    draft: &SalesChangeDraftRequest,
) -> Result<entities::sales_order::HeaderSnapshotData> {
    Ok(entities::sales_order::HeaderSnapshotData {
        customer_name: draft.customer_name.clone(),
        contract_no: draft.contract_no.clone(),
        settlement_party_name: draft.settlement_party_name.clone(),
        payment_term_code: draft.payment_term_code.clone(),
        payment_term_name: draft.payment_term_name.clone(),
        invoice_type: draft.invoice_type.clone(),
        tax_point: draft.tax_point.clone(),
    })
}

/// 从变更工作副本构建变更提交快照。
///
/// # 参数
/// * `change_order` - 变更单
/// * `working_copy` - 变更工作副本
/// * `lines` - 工作副本行
/// * `actor` - 提交人
///
/// # 返回
/// 返回变更提交实体。
///
/// # 错误
/// 提交字段校验失败时返回错误。
fn build_change_submission(
    change_order: &SalesChangeOrder,
    working_copy: &entities::sales_order::SalesOrderWorkingCopy,
    lines: &[entities::sales_order::SalesOrderWorkingCopyLine],
    actor: &AuditActor,
) -> Result<SalesChangeSubmission> {
    let (gross, net, tax) = change_line_totals(lines);
    SalesChangeSubmission::new(
        SalesChangeSubmissionId::new(next_id()),
        SalesChangeSubmissionData {
            sales_change_order_id: change_order.base.id.clone().into(),
            submission_no: 1,
            base_revision_id: change_order.base_revision_id.clone(),
            sales_order_id: change_order.sales_order_id.clone(),
            working_copy_id: working_copy.base.id.clone().into(),
            working_copy_version: working_copy.draft_version,
            business_type: convert_business_type(working_copy.business_type),
            customer_id: working_copy.customer_id.clone(),
            contract_revision_id: None,
            settlement_party_id: working_copy.settlement_party_id.clone(),
            snapshot: entities::sales_review::HeaderSnapshotData {
                customer_name: working_copy.customer_snapshot.customer_name.clone(),
                contract_no: working_copy
                    .contract_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.contract_no.clone()),
                settlement_party_name: working_copy
                    .settlement_party_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.settlement_party_name.clone()),
                payment_term_code: working_copy.payment_term_snapshot.payment_term_code.clone(),
                payment_term_name: working_copy.payment_term_snapshot.payment_term_name.clone(),
                invoice_type: working_copy.invoice_requirement_snapshot.invoice_type.clone(),
                tax_point: working_copy.invoice_requirement_snapshot.tax_point.clone(),
            },
            project_name: working_copy.project_name.clone(),
            business_remark: working_copy.business_remark.clone(),
            voucher_category_sku_id: working_copy.voucher_category_sku_id.clone(),
            voucher_expiry_at: working_copy.voucher_expiry_at,
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            submitted_at: Instant::now(),
            submitted_by: actor.id().to_string(),
            lines: Vec::new(),
        },
    )
    .map_err(Error::Logic)
}

/// D13 业务性质 → D14 同形类型转换。
fn convert_business_type(value: entities::sales_order::BusinessType) -> entities::sales_review::BusinessType {
    match value {
        entities::sales_order::BusinessType::GoodsService => {
            entities::sales_review::BusinessType::GoodsService
        }
        entities::sales_order::BusinessType::Voucher => entities::sales_review::BusinessType::Voucher,
    }
}

/// 从变更工作副本行构建变更提交行。
///
/// # 参数
/// * `submission` - 变更提交
/// * `lines` - 工作副本行
///
/// # 返回
/// 返回变更提交行清单。
///
/// # 错误
/// 行字段组缺失或非法时返回错误。
fn build_change_submission_lines(
    submission: &SalesChangeSubmission,
    lines: &[entities::sales_order::SalesOrderWorkingCopyLine],
) -> Result<Vec<SalesChangeSubmissionLine>> {
    let mut built = Vec::with_capacity(lines.len());
    for line in lines {
        let goods = change_copy_goods(line)?;
        let voucher = change_copy_voucher(line)?;
        built.push(SalesChangeSubmissionLine::new(
            SalesChangeSubmissionLineId::new(next_id()),
            submission.base.id.clone().into(),
            SalesChangeSubmissionLineData {
                sales_order_line_id: line.sales_order_line_id.clone(),
                line_no: line.line_no,
                line_type: convert_line_type(line.line_type),
                sales_tax_rate: line.sales_tax_rate,
                item_name_snapshot: line.item_name_snapshot.clone(),
                spec_snapshot: line.spec_snapshot.clone(),
                unit_snapshot: line.unit_snapshot.clone(),
                goods,
                voucher,
            },
        )?);
    }
    Ok(built)
}

/// 从变更工作副本行还原实物及服务字段组（D13 行字段组转换为 D14 同形类型）。
///
/// # 参数
/// * `line` - 工作副本行
///
/// # 返回
/// 返回字段组；卡券行返回 `None`。
///
/// # 错误
/// 实物及服务行缺商品字段组时返回错误。
fn change_copy_goods(
    line: &entities::sales_order::SalesOrderWorkingCopyLine,
) -> Result<Option<entities::sales_review::GoodsLineFields>> {
    if line.line_type != LineType::GoodsService {
        return Ok(None);
    }
    let sku_id = line
        .sku_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少商品字段组", line.line_no)))?;
    let sku_revision_id = line
        .sku_revision_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU 修订", line.line_no)))?;
    let fulfillment_mode = line
        .fulfillment_mode
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约方式", line.line_no)))?;
    let fulfillment_due_at = line
        .fulfillment_due_at
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约期限", line.line_no)))?;
    let quantity = line
        .quantity
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少数量", line.line_no)))?;
    let base_unit_code = line
        .base_unit_code
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少单位", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少含税单价", line.line_no)))?;
    Ok(Some(entities::sales_review::GoodsLineFields {
        sku_id,
        sku_revision_id,
        welfare_scenario: line.welfare_scenario.map(convert_welfare_scenario),
        fulfillment_mode: convert_fulfillment_mode(fulfillment_mode),
        fulfillment_due_at,
        quantity,
        base_unit_code,
        unit_price_gross,
    }))
}

/// D13 福利场景 → D14 同形类型转换（两域同形副本，待地基修订下沉）。
fn convert_welfare_scenario(
    value: entities::sales_order::WelfareScenario,
) -> entities::sales_review::WelfareScenario {
    match value {
        entities::sales_order::WelfareScenario::AnnualGiftBag => {
            entities::sales_review::WelfareScenario::AnnualGiftBag
        }
        entities::sales_order::WelfareScenario::MealSubsidy => {
            entities::sales_review::WelfareScenario::MealSubsidy
        }
        entities::sales_order::WelfareScenario::CondolenceGift => {
            entities::sales_review::WelfareScenario::CondolenceGift
        }
        entities::sales_order::WelfareScenario::ConsumptionFund => {
            entities::sales_review::WelfareScenario::ConsumptionFund
        }
        entities::sales_order::WelfareScenario::Other => entities::sales_review::WelfareScenario::Other,
    }
}

/// D13 履约方式 → D14 同形类型转换。
fn convert_fulfillment_mode(
    value: entities::sales_order::FulfillmentMode,
) -> entities::sales_review::FulfillmentMode {
    match value {
        entities::sales_order::FulfillmentMode::CompanyWarehouse => {
            entities::sales_review::FulfillmentMode::CompanyWarehouse
        }
        entities::sales_order::FulfillmentMode::SupplierDirect => {
            entities::sales_review::FulfillmentMode::SupplierDirect
        }
        entities::sales_order::FulfillmentMode::ElectronicDelivery => {
            entities::sales_review::FulfillmentMode::ElectronicDelivery
        }
        entities::sales_order::FulfillmentMode::OfflineService => {
            entities::sales_review::FulfillmentMode::OfflineService
        }
    }
}

/// D13 卡形态 → D14 同形类型转换。
fn convert_card_form(value: entities::sales_order::CardForm) -> entities::sales_review::CardForm {
    match value {
        entities::sales_order::CardForm::Electronic => entities::sales_review::CardForm::Electronic,
        entities::sales_order::CardForm::Physical => entities::sales_review::CardForm::Physical,
    }
}

/// D13 行类型 → D14 同形类型转换。
fn convert_line_type(value: entities::sales_order::LineType) -> entities::sales_review::LineType {
    match value {
        entities::sales_order::LineType::GoodsService => entities::sales_review::LineType::GoodsService,
        entities::sales_order::LineType::Voucher => entities::sales_review::LineType::Voucher,
    }
}

/// 从变更工作副本行还原卡券字段组（D13 行字段组转换为 D14 同形类型）。
///
/// # 参数
/// * `line` - 工作副本行
///
/// # 返回
/// 返回字段组；实物及服务行返回 `None`。
///
/// # 错误
/// 卡券行缺卡券字段组时返回错误。
fn change_copy_voucher(
    line: &entities::sales_order::SalesOrderWorkingCopyLine,
) -> Result<Option<entities::sales_review::VoucherLineDraft>> {
    use entities::sales_review::VoucherLineDraft as ChangeVoucherLineDraft;
    if line.line_type != LineType::Voucher {
        return Ok(None);
    }
    let face_value = line
        .face_value
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券字段组", line.line_no)))?;
    let card_count = line
        .card_count
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡张数", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券成交单价", line.line_no)))?;
    Ok(Some(ChangeVoucherLineDraft {
        face_value,
        card_count,
        unit_price_gross,
        face_value_total: line
            .face_value_total
            .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少面额小计", line.line_no)))?,
        transaction_amount: line
            .transaction_amount
            .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少成交金额", line.line_no)))?,
        gift_amount: line
            .gift_amount
            .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少配赠金额", line.line_no)))?,
        gift_rate: line.gift_rate,
        card_form: convert_card_form(
            line.card_form
                .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡形态", line.line_no)))?,
        ),
    }))
}

/// D14 卡形态 → D13 同形类型转换。
fn convert_card_form_to_sales(value: entities::sales_review::CardForm) -> entities::sales_order::CardForm {
    match value {
        entities::sales_review::CardForm::Electronic => entities::sales_order::CardForm::Electronic,
        entities::sales_review::CardForm::Physical => entities::sales_order::CardForm::Physical,
    }
}

/// 从变更提交构建正式版本聚合（§8.1.3 变更生效）。
///
/// # 参数
/// * `order` - 销售单
/// * `submission` - 变更提交
/// * `submission_lines` - 变更提交行
/// * `revision_no` - 目标版本号（既有最大版本号 + 1）
/// * `effective_at` - 生效时间
///
/// # 返回
/// 返回版本聚合。
///
/// # 错误
/// 行字段组缺失或版本字段校验失败时返回错误。
fn build_change_revision(
    order: &SalesOrder,
    submission: &SalesChangeSubmission,
    submission_lines: &[SalesChangeSubmissionLine],
    revision_no: u32,
    effective_at: Instant,
) -> Result<RevisionAggregate> {
    let revision_id = SalesOrderRevisionId::new(next_id());
    let revision = SalesOrderRevision::new(
        revision_id.clone(),
        SalesOrderRevisionData {
            sales_order_id: submission.sales_order_id.clone(),
            revision_no,
            revision_source: RevisionSource::SalesChange,
            source_snapshot_id: None,
            previous_revision_id: order.stable.current_revision_id.clone().map(Into::into),
            content_hash: format!("sub:{}", submission.base.id),
            customer_revision_id: None,
            contract_revision_id: submission.contract_revision_id.clone(),
            snapshot: entities::sales_order::HeaderSnapshotData {
                customer_name: submission.customer_snapshot.customer_name.clone(),
                contract_no: submission
                    .contract_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.contract_no.clone()),
                settlement_party_name: submission
                    .settlement_party_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.settlement_party_name.clone()),
                payment_term_code: submission.payment_term_snapshot.payment_term_code.clone(),
                payment_term_name: submission.payment_term_snapshot.payment_term_name.clone(),
                invoice_type: submission.invoice_requirement_snapshot.invoice_type.clone(),
                tax_point: submission.invoice_requirement_snapshot.tax_point.clone(),
            },
            project_name: submission.project_name.clone(),
            business_remark: submission.business_remark.clone(),
            voucher_category_sku_id: submission.voucher_category_sku_id.clone(),
            voucher_expiry_at: submission.voucher_expiry_at,
            gross_amount: submission.gross_amount,
            net_amount: submission.net_amount,
            tax_amount: submission.tax_amount,
            effective_at,
            recorded_at: effective_at,
        },
    )?;
    let mut revision_lines = Vec::with_capacity(submission_lines.len());
    let mut goods_lines = Vec::new();
    let mut voucher_lines = Vec::new();
    for sub_line in submission_lines {
        let revision_line_id = SalesOrderRevisionLineId::new(next_id());
        let revision_line = SalesOrderRevisionLine::new(
            revision_line_id.clone(),
            SalesOrderRevisionLineData {
                sales_order_revision_id: revision_id.clone(),
                sales_order_line_id: sub_line.sales_order_line_id.clone(),
                line_no: sub_line.line_no,
                line_type: convert_line_type_to_sales(sub_line.line_type),
                gross_amount: sub_line.gross_amount,
                net_amount: sub_line.net_amount,
                tax_amount: sub_line.tax_amount,
                sales_tax_rate: sub_line.sales_tax_rate,
                item_name_snapshot: sub_line.item_name_snapshot.clone(),
                spec_snapshot: sub_line.spec_snapshot.clone(),
                unit_snapshot: sub_line.unit_snapshot.clone(),
            },
        )?;
        match sub_line.line_type {
            entities::sales_review::LineType::GoodsService => {
                let goods = change_submission_goods(sub_line)?;
                goods_lines.push(SalesOrderGoodsServiceLineRevision::new(
                    SalesOrderGoodsServiceLineRevisionId::new(next_id()),
                    SalesOrderGoodsServiceLineRevisionData {
                        revision_line_id: revision_line_id.clone(),
                        sku_id: goods.sku_id,
                        sku_revision_id: goods.sku_revision_id,
                        welfare_scenario: goods.welfare_scenario,
                        fulfillment_mode: goods.fulfillment_mode,
                        fulfillment_due_at: goods.fulfillment_due_at,
                        quantity: goods.quantity,
                        base_unit_code: goods.base_unit_code,
                        unit_price_gross: goods.unit_price_gross,
                    },
                )?);
            }
            entities::sales_review::LineType::Voucher => {
                let voucher = change_submission_voucher(sub_line)?;
                voucher_lines.push(SalesOrderVoucherLineRevision::new(
                    SalesOrderVoucherLineRevisionId::new(next_id()),
                    SalesOrderVoucherLineRevisionData {
                        revision_line_id: revision_line_id.clone(),
                        face_value: voucher.face_value,
                        card_count: voucher.card_count,
                        unit_price_gross: voucher.unit_price_gross,
                        card_form: voucher.card_form,
                    },
                )?);
            }
        }
        revision_lines.push(revision_line);
    }
    Ok(RevisionAggregate {
        revision,
        lines: revision_lines,
        goods_lines,
        voucher_lines,
    })
}

/// D14 行类型 → D13 同形类型转换（版本行使用 D13 类型）。
fn convert_line_type_to_sales(value: entities::sales_review::LineType) -> entities::sales_order::LineType {
    match value {
        entities::sales_review::LineType::GoodsService => entities::sales_order::LineType::GoodsService,
        entities::sales_review::LineType::Voucher => entities::sales_order::LineType::Voucher,
    }
}

/// D14 履约方式 → D13 同形类型转换。
fn convert_fulfillment_mode_to_sales(
    value: entities::sales_review::FulfillmentMode,
) -> entities::sales_order::FulfillmentMode {
    match value {
        entities::sales_review::FulfillmentMode::CompanyWarehouse => {
            entities::sales_order::FulfillmentMode::CompanyWarehouse
        }
        entities::sales_review::FulfillmentMode::SupplierDirect => {
            entities::sales_order::FulfillmentMode::SupplierDirect
        }
        entities::sales_review::FulfillmentMode::ElectronicDelivery => {
            entities::sales_order::FulfillmentMode::ElectronicDelivery
        }
        entities::sales_review::FulfillmentMode::OfflineService => {
            entities::sales_order::FulfillmentMode::OfflineService
        }
    }
}

/// D14 福利场景 → D13 同形类型转换。
fn convert_welfare_scenario_to_sales(
    value: entities::sales_review::WelfareScenario,
) -> entities::sales_order::WelfareScenario {
    match value {
        entities::sales_review::WelfareScenario::AnnualGiftBag => {
            entities::sales_order::WelfareScenario::AnnualGiftBag
        }
        entities::sales_review::WelfareScenario::MealSubsidy => {
            entities::sales_order::WelfareScenario::MealSubsidy
        }
        entities::sales_review::WelfareScenario::CondolenceGift => {
            entities::sales_order::WelfareScenario::CondolenceGift
        }
        entities::sales_review::WelfareScenario::ConsumptionFund => {
            entities::sales_order::WelfareScenario::ConsumptionFund
        }
        entities::sales_review::WelfareScenario::Other => entities::sales_order::WelfareScenario::Other,
    }
}

/// 从变更提交行还原实物及服务字段组。
///
/// # 参数
/// * `line` - 变更提交行
///
/// # 返回
/// 返回字段组。
///
/// # 错误
/// 实物及服务行缺商品字段组时返回错误。
fn change_submission_goods(line: &SalesChangeSubmissionLine) -> Result<GoodsLineFields> {
    #[allow(clippy::redundant_closure_call)]
    let sku_id = line
        .sku_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少商品字段组", line.line_no)))?;
    let sku_revision_id = line
        .sku_revision_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU 修订", line.line_no)))?;
    let fulfillment_mode = line
        .fulfillment_mode
        .map(convert_fulfillment_mode_to_sales)
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约方式", line.line_no)))?;
    let fulfillment_due_at = line
        .fulfillment_due_at
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约期限", line.line_no)))?;
    let quantity = line
        .quantity
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少数量", line.line_no)))?;
    let base_unit_code = line
        .base_unit_code
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少单位", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少含税单价", line.line_no)))?;
    Ok(GoodsLineFields {
        sku_id,
        sku_revision_id,
        welfare_scenario: line.welfare_scenario.map(convert_welfare_scenario_to_sales),
        fulfillment_mode,
        fulfillment_due_at,
        quantity,
        base_unit_code,
        unit_price_gross,
    })
}

/// 从变更提交行还原卡券字段组。
///
/// # 参数
/// * `line` - 变更提交行
///
/// # 返回
/// 返回字段组。
///
/// # 错误
/// 卡券行缺卡券字段组时返回错误。
fn change_submission_voucher(line: &SalesChangeSubmissionLine) -> Result<VoucherLineDraft> {
    let face_value = line
        .face_value
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券字段组", line.line_no)))?;
    let card_count = line
        .card_count
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡张数", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券成交单价", line.line_no)))?;
    let face_value_total = line
        .face_value_total
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少面额小计", line.line_no)))?;
    let transaction_amount = line
        .transaction_amount
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少成交金额", line.line_no)))?;
    let gift_amount = line
        .gift_amount
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少配赠金额", line.line_no)))?;
    Ok(VoucherLineDraft {
        face_value,
        card_count,
        unit_price_gross,
        face_value_total,
        transaction_amount,
        gift_amount,
        gift_rate: line.gift_rate,
        card_form: convert_card_form_to_sales(
            line.card_form
                .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡形态", line.line_no)))?,
        ),
    })
}

/// 构建应收差额分录（§8.1.3：新版本金额减当前版本金额，零差额不写）。
///
/// 差额必须挂到销售单既有应收子账（`account_seq = 1`）；子账缺失时按新版本
/// 新建（初始审批未形成应收的历史数据兜底）。
///
/// # 参数
/// * `order` - 销售单（含当前生效版本）
/// * `revision` - 新版本聚合
/// * `current_gross` - 当前生效版本含税合计（差额基准）
/// * `existing_account` - 既有应收子账（按销售单查询）
/// * `posted_at` - 入账时间
///
/// # 返回
/// 返回 `(应收子账, 差额分录)`；差额为零时返回 `None`。
///
/// # 错误
/// 分录字段校验失败时返回错误。
fn build_receivable_delta(
    order: &SalesOrder,
    revision: &RevisionAggregate,
    current_gross: Amount,
    existing_account: Option<entities::receivable::ReceivableAccount>,
    posted_at: Instant,
) -> Result<
    Option<(
        entities::receivable::ReceivableAccount,
        entities::receivable::ReceivableEntry,
    )>,
> {
    let new_gross = revision_gross(revision)?;
    let delta = new_gross.to_decimal() - current_gross.to_decimal();
    if delta.is_zero() {
        return Ok(None);
    }
    let account = match existing_account {
        Some(account) => account,
        None => build_receivable_account(order, revision),
    };
    let entry = entities::receivable::ReceivableEntry::new(
        ReceivableEntryId::new(next_id()),
        entities::receivable::ReceivableEntryData {
            receivable_account_id: account.base.id.clone().into(),
            entry_type: entities::receivable::ReceivableEntryType::SalesChangeDelta,
            direction: if delta.is_sign_positive() {
                entities::receivable::EntryDirection::Increase
            } else {
                entities::receivable::EntryDirection::Decrease
            },
            amount: Amount::from_str(&delta.abs().to_string()).expect("差额必须为正数金额"),
            due_date: BusinessDate::today(),
            source_fact_type: "SALES_CHANGE".to_string(),
            source_document_id: order.base.id.clone(),
            source_revision_id: revision.revision.base.id.clone(),
            source_sequence: 1,
            posted_at,
        },
    )
    .map_err(Error::Logic)?;
    Ok(Some((account, entry)))
}
