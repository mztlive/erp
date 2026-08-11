//! 域 D13 `sales_order` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 建单（订单 + 稳定明细 + 工作副本 + 工作副本行 + 审计）：跨集合 →
//!   `database::Transactional::with_transaction`；
//! - 提交（提交快照 + 订单审核轨推进 + 审批记录/采购确认批次 + 待办 + 审计）：
//!   跨集合 → 同一事务模板；
//! - 保存草稿 / 作废：跨集合（工作副本行替换 + 头 CAS + 审计）→ 事务；
//! - 列表 / 详情：单集合无跨步骤原子性要求 → `&mut NoTransaction`。
//!
//! 跨域协作（P3 §2）：
//! - D08 `customer_accounts`：客户存在性校验；
//! - D12 `contracts`：合同存在性校验；
//! - D14 `sales_order_reviews` / `procurement_confirmations`：提交时按业务性质
//!   派发审批记录（卡券 → 销售领导审批）或采购二次确认批次（实物及服务）；
//! - D03 `work_items`：待办派发；
//! - D02 `audit_logs`：审计。
//!
//! 幂等：提交入口按「同一工作副本已形成提交」去重，重复提交返回既有提交
//! （AGENTS.md 外部依赖容错）；建单按 `order_no` 唯一索引兜底（409）。

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use database::{
    AccessControlExt, CatalogExt, ContractExt, CustomerExt, Executor, NoTransaction, ReceivableExt,
    SalesOrderExt, SalesReviewExt, Transactional, WorkItemExt,
};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{ProcurementConfirmationId, SalesOrderReviewId, WorkItemId};
use entities::money::Amount;
use entities::sales_order::{
    BusinessType, CloseStatus, CollectionProgress, CommercialStatus, FulfillmentProgress, GoodsLineFields,
    InvoiceProgress, LineType, OriginSystem, ReviewStatus, SalesOrder, SalesOrderData, SalesOrderId,
    SalesOrderLine, SalesOrderLineData, SalesOrderLineId, SalesOrderRevisionId, SalesOrderSubmission,
    SalesOrderSubmissionData, SalesOrderSubmissionId, SalesOrderSubmissionLine, SalesOrderSubmissionLineData,
    SalesOrderSubmissionLineId, SalesOrderWorkingCopy, SalesOrderWorkingCopyData, SalesOrderWorkingCopyId,
    SalesOrderWorkingCopyLine, SalesOrderWorkingCopyLineData, SalesOrderWorkingCopyLineId,
    SalesOrderWorkingCopyUpdate, VoucherLineDraft, WorkingPurpose,
};
use entities::sales_review::{
    ProcurementConfirmation, ProcurementConfirmationData, ProcurementConfirmationStatus, SalesOrderReview,
    SalesOrderReviewData, SalesReviewStage, SalesReviewStatus,
};
use entities::work_item::{WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

pub use self::dto::{
    CloseEligibilityView, CreateSalesOrderRequest, PageView, RevisionView, SalesOrderCreateIntent,
    SalesOrderDetailView, SalesOrderDraftLineRequest, SalesOrderDraftRequest, SalesOrderLineView,
    SalesOrderListParams, SalesOrderStageSummary, SalesOrderView, SalesOrderWorkingCopyLineView,
    SaveWorkingCopyRequest, SubmissionView, SubmitSalesOrderRequest, VoidSalesOrderRequest, WorkingCopyView,
};

/// 销售单列表筛选条件类型（经 `SalesOrderExt` 关联类型跨 crate 可达）。
type SalesOrderFilter = <mongodb::Database as SalesOrderExt>::SalesOrderFilter;

/// 销售单服务。
///
/// 提供销售单建单、草稿保存、提交、作废与查询编排。
pub struct SalesOrderService {
    db: Database,
}

impl SalesOrderService {
    /// 创建销售单服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 创建销售单（订单 + 稳定明细 + 首次提交工作副本原子形成；`intent=SUBMIT`
    /// 时随后立即提交）。
    ///
    /// 表头金额三元组由服务端按 §4.2 铁律 2 汇总**已舍入**的行金额，客户端不可
    /// 指定；跨域校验客户（D08）与合同（D12）存在性。`order_no` 唯一性由唯一
    /// 索引兜底（重复幂等键提交 → 409）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回销售单详情视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败或行字段组缺失
    /// * `NotFound` - 客户/合同不存在
    /// * `BusinessLogicError` - 客户已停用
    /// * `ConflictError` - order_no 重复
    pub async fn create_sales_order(
        &self,
        req: CreateSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        req.validate()?;
        self.ensure_sellable_draft_lines(&req.draft.lines).await?;
        let customer = self
            .db
            .customer_accounts()
            .find_by_id(&req.customer_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户不存在".to_string()))?;
        if !customer.is_active() {
            return Err(Error::BusinessLogicError(
                "客户已停用，禁止创建新销售单".to_string(),
            ));
        }
        if let Some(contract_id) = &req.contract_id {
            let contract = self
                .db
                .contracts()
                .find_by_id(contract_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("合同不存在".to_string()))?;
            if contract.customer_id != req.customer_id {
                return Err(Error::ValidationError(
                    "销售单引用的合同不属于所选客户".to_string(),
                ));
            }
        }

        let order = SalesOrder::new(
            SalesOrderId::new(next_id()),
            SalesOrderData {
                order_no: req.order_no,
                business_type: req.business_type,
                origin_system: entities::sales_order::OriginSystem::Erp,
                source_identity_id: None,
                customer_id: req.customer_id,
                contract_id: req.contract_id,
                settlement_party_id: req.settlement_party_id,
                source_status_code: None,
            },
            actor.id(),
        )?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let stable_lines = build_stable_lines(&order_id, &req.draft.lines)?;
        let (working_copy, working_copy_lines) =
            build_working_copy(&order, &stable_lines, &req.draft, 1, actor)?;

        let audit = actor
            .clone()
            .resource_log("sales_order.create", "sales_order", order.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let lines_for_tx = stable_lines.clone();
        let working_copy_for_tx = working_copy.clone();
        let working_copy_lines_for_tx = working_copy_lines.clone();
        let sellable_refs_for_tx = Self::sellable_working_copy_refs(&working_copy_lines)?;
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(&sellable_refs_for_tx, session)
                        .await?;
                    db.sales_orders().create(&order_for_tx, session).await?;
                    for line in &lines_for_tx {
                        db.sales_order_lines().create(line, session).await?;
                    }
                    db.sales_order_working_copies()
                        .create(&working_copy_for_tx, session)
                        .await?;
                    for line in &working_copy_lines_for_tx {
                        db.sales_order_working_copy_lines().create(line, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        if req.intent == SalesOrderCreateIntent::Submit {
            self.submit_sales_order(
                &order.base.id,
                SubmitSalesOrderRequest {
                    version: 1,
                    idempotency_key: req.idempotency_key,
                },
                actor,
            )
            .await?;
        }
        self.sales_order_detail(&order.base.id).await
    }

    /// 分页查询销售单列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn sales_order_list(&self, params: &SalesOrderListParams) -> Result<PageView<SalesOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesOrderFilter {
            order_no: query.order_no,
            customer_id: query.customer_id,
            commercial_status: query.commercial_status,
            review_status: query.review_status,
            business_type: query.business_type,
            created_by: query.created_by,
            my_todo: query.my_todo,
            exception_only: query.exception_only,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .sales_orders()
            .search_sales_orders(&filter, &mut NoTransaction)
            .await?;

        let owners = self
            .resolve_stage_owners_batch(
                &page
                    .items
                    .iter()
                    .map(|row| (row.id.clone(), row.review_status))
                    .collect::<Vec<_>>(),
            )
            .await?;

        let items = page
            .items
            .into_iter()
            .map(|row| {
                let (code, label, tone) = stage_code_label_tone(
                    row.commercial_status,
                    row.review_status,
                    row.close_status,
                    row.fulfillment_progress,
                );
                let (owner_role, owner_user_id, owner_user_name, due_at) =
                    owners.get(&row.id).cloned().unwrap_or_default();
                SalesOrderView {
                    id: row.id,
                    order_no: row.order_no,
                    business_type: row.business_type,
                    origin_system: row.origin_system,
                    customer_id: row.customer_id,
                    contract_id: row.contract_id,
                    commercial_status: row.commercial_status,
                    review_status: row.review_status,
                    fulfillment_progress: row.fulfillment_progress,
                    collection_progress: row.collection_progress,
                    invoice_progress: row.invoice_progress,
                    close_status: row.close_status,
                    effective_at: row.effective_at,
                    closed_at: row.closed_at,
                    version: row.version,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    stage: dto::SalesOrderStageSummary {
                        code,
                        label,
                        tone,
                        owner_role,
                        owner_user_id,
                        owner_user_name,
                        due_at,
                    },
                }
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询销售单详情（订单 + 稳定明细 + 草稿 + 提交历史 + 版本历史）。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单不存在
    pub async fn sales_order_detail(&self, id: &str) -> Result<SalesOrderDetailView> {
        let order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;

        let order_id = SalesOrderId::new(order.base.id.clone());

        let stable_lines = self
            .db
            .sales_order_lines()
            .list_lines_by_order(&order_id, &mut NoTransaction)
            .await?;

        let working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?;

        let working_copy_view = match working_copy {
            Some(copy) => Some(self.working_copy_view(&copy).await?),
            None => None,
        };

        let mut submissions = self
            .db
            .sales_order_submissions()
            .find_many(mongodb::bson::doc! { "sales_order_id": id }, &mut NoTransaction)
            .await?;

        // 新提交在前，便于前端取「当前商业内容」
        submissions.sort_by_key(|submission| std::cmp::Reverse(submission.submission_no));
        let submission_ids = submissions
            .iter()
            .map(|s| SalesOrderSubmissionId::new(s.base.id.clone()))
            .collect::<Vec<_>>();

        let submission_lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(&submission_ids, &mut NoTransaction)
            .await?;

        let mut lines_by_submission: HashMap<String, Vec<SalesOrderSubmissionLine>> = HashMap::new();

        for line in submission_lines {
            lines_by_submission
                .entry(line.submission_id.to_string())
                .or_default()
                .push(line);
        }

        let submission_views: Vec<SubmissionView> = submissions
            .into_iter()
            .map(|submission| {
                let mut lines = lines_by_submission
                    .remove(&submission.base.id)
                    .unwrap_or_default();
                lines.sort_by_key(|line| line.line_no);
                submission_view(submission, lines)
            })
            .collect();

        let revisions = self
            .db
            .sales_order_revisions()
            .list_by_order(&order_id, &mut NoTransaction)
            .await?;

        let owner_user_id = detail_owner_user_id(
            working_copy_view
                .as_ref()
                .map(|copy| copy.editor_user_id.as_str()),
            submission_views
                .first()
                .map(|submission| submission.submitted_by.as_str()),
            &order.stable.created_by,
        );

        let owner_user_name = self.account_name(&owner_user_id).await?;

        let (stage_owner_role, stage_owner_user_id, stage_due_at) = self
            .resolve_stage_owner(order.review_status, submission_ids.first())
            .await?;
        let stage_owner_user_name = match stage_owner_user_id.as_deref() {
            Some(user_id) => self.account_name(user_id).await?,
            None => None,
        };
        let (stage_code, stage_label, stage_tone) = stage_code_label_tone(
            order.stable.status,
            order.review_status,
            order.close_status,
            order.fulfillment_progress,
        );

        let receivable_accounts = self
            .db
            .receivable_accounts()
            .find_many(mongodb::bson::doc! { "sales_order_id": id }, &mut NoTransaction)
            .await?;
        let (settled_total, gross_total) =
            receivable_accounts
                .iter()
                .fold((zero_amount(), zero_amount()), |(settled, gross), account| {
                    (
                        settled.checked_add(account.settled_total),
                        gross.checked_add(account.gross_total),
                    )
                });
        let close_eligibility = compute_close_eligibility(
            order.business_type,
            order.stable.status,
            order.close_status,
            order.fulfillment_progress,
            order.collection_progress,
            order.invoice_progress,
            settled_total,
            gross_total,
        );

        let has_active_change_order = match order.stable.current_revision_id.as_ref() {
            Some(revision_id) => self
                .db
                .sales_change_orders()
                .find_in_progress_by_order_and_base(
                    &order_id,
                    &SalesOrderRevisionId::new(revision_id.clone()),
                    &mut NoTransaction,
                )
                .await?
                .is_some(),
            None => false,
        };
        let (can_start_sales_change_order, change_order_blocker) = compute_can_start_sales_change(
            order.origin_system,
            stage_code,
            stage_label,
            has_active_change_order,
        );

        Ok(SalesOrderDetailView {
            id: order.base.id.clone(),
            order_no: order.order_no.clone(),
            business_type: order.business_type,
            origin_system: order.origin_system,
            customer_id: order.customer_id.to_string(),
            contract_id: order.contract_id.as_ref().map(ToString::to_string),
            settlement_party_id: order.settlement_party_id.to_string(),
            commercial_status: order.stable.status,
            review_status: order.review_status,
            fulfillment_progress: order.fulfillment_progress,
            collection_progress: order.collection_progress,
            invoice_progress: order.invoice_progress,
            close_status: order.close_status,
            current_revision_id: order.stable.current_revision_id,
            effective_at: order.effective_at.map(|instant| instant.unix_secs() as u64),
            version: order.base.version,
            created_at: order.base.created_at,
            owner_user_id,
            owner_user_name,
            lines: stable_lines
                .into_iter()
                .map(|line| SalesOrderLineView {
                    id: line.base.id,
                    line_no: line.line_no,
                    line_status: line.line_status,
                })
                .collect(),
            working_copy: working_copy_view,
            submissions: submission_views,
            revisions: revisions
                .into_iter()
                .map(|revision| RevisionView {
                    id: revision.base.id,
                    revision_no: revision.revision.revision_no,
                    revision_source: revision.revision_source,
                    content_hash: revision.content_hash,
                    gross_amount: revision.gross_amount,
                    net_amount: revision.net_amount,
                    tax_amount: revision.tax_amount,
                    effective_at: revision.effective_at.unix_secs() as u64,
                    created_at: revision.base.created_at,
                })
                .collect(),
            stage: dto::SalesOrderStageSummary {
                code: stage_code,
                label: stage_label,
                tone: stage_tone,
                owner_role: stage_owner_role,
                owner_user_id: stage_owner_user_id,
                owner_user_name: stage_owner_user_name,
                due_at: stage_due_at,
            },
            close_eligibility,
            can_start_sales_change_order,
            change_order_blocker,
        })
    }

    /// 保存草稿（整表头覆盖 + 明细整批替换，乐观锁语义）。
    ///
    /// 期望版本 `req.version` 与工作副本当前版本不一致时返回冲突（409）；
    /// 行替换在事务内「软删旧行 + 写入新行」原子完成。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 保存请求（含期望版本与草稿内容）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回保存后的工作副本视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单或有效工作副本不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn save_working_copy(
        &self,
        id: &str,
        req: SaveWorkingCopyRequest,
        actor: &AuditActor,
    ) -> Result<WorkingCopyView> {
        req.validate()?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let mut working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("有效工作副本不存在".to_string()))?;
        if working_copy.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        self.ensure_sellable_draft_lines(&req.draft.lines).await?;

        let snapshot = header_snapshot(&req.draft)?;
        let stable_lines = self
            .db
            .sales_order_lines()
            .list_lines_by_order(&order_id, &mut NoTransaction)
            .await?;
        let lines = build_working_copy_lines(
            &order_id,
            &working_copy.base.id.clone().into(),
            &stable_lines,
            &req.draft.lines,
        )?;
        let (gross, net, tax) = line_totals(&lines);
        let next_version = working_copy.draft_version + 1;
        working_copy.update(
            SalesOrderWorkingCopyUpdate {
                content_hash: Some(draft_hash(&working_copy.base.id, next_version)),
                customer_id: Some(order.customer_id.clone()),
                contract_id: order.contract_id.clone(),
                settlement_party_id: Some(order.settlement_party_id.clone()),
                snapshot: Some(snapshot),
                project_name: req.draft.project_name.clone(),
                business_remark: req.draft.business_remark.clone(),
                voucher_category_sku_id: req.draft.voucher_category_sku_id.clone(),
                voucher_expiry_at: req
                    .draft
                    .voucher_expiry_at
                    .map(|secs| Instant::from_unix_secs(secs as i64)),
                gross_amount: Some(gross),
                net_amount: Some(net),
                tax_amount: Some(tax),
            },
            actor.id(),
        )?;
        working_copy.save_draft(
            draft_hash(&working_copy.base.id, next_version),
            req.draft.editor_user_id.clone(),
        )?;

        let old_lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&working_copy.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let audit = actor
            .clone()
            .resource_log("sales_order.save_draft", "sales_order", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let lines_for_tx = lines.clone();
        let sellable_refs_for_tx = Self::sellable_working_copy_refs(&lines)?;
        let working_copy = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(&sellable_refs_for_tx, session)
                        .await?;
                    for mut old in old_lines {
                        db.sales_order_working_copy_lines()
                            .soft_delete(&mut old, session)
                            .await?;
                    }
                    for line in &lines_for_tx {
                        db.sales_order_working_copy_lines().create(line, session).await?;
                    }
                    db.sales_order_working_copies()
                        .update(&mut working_copy, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SalesOrderWorkingCopy, crate::errors::Error>(working_copy)
                })
            })
            .await?;

        self.working_copy_view(&working_copy).await
    }

    /// 提交销售单（冻结提交快照并推进审核轨；重复提交幂等返回既有提交）。
    ///
    /// 跨集合事务内写入 `sales_order_submission(+_line)`、CAS 锁定工作副本、
    /// 推进销售单审核轨，并按业务性质派发：
    /// - 实物及服务 → 创建采购二次确认批次（`procurement_confirmation`，
    ///   待处理）+ `PROCUREMENT_CONFIRMATION` 待办（W07 队列）；
    /// - 卡券 → 创建 `SALES_LEADER` 阶段审批记录 + `CARD_SALES_MANAGER_APPROVAL`
    ///   待办。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 提交请求（含期望草稿版本与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交快照视图；已提交过的草稿幂等返回既有提交。
    ///
    /// # 错误
    /// * `NotFound` - 销售单或有效工作副本不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn submit_sales_order(
        &self,
        id: &str,
        req: SubmitSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SubmissionView> {
        req.validate()?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let mut working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("有效工作副本不存在".to_string()))?;
        if working_copy.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        if let Some(existing) = self
            .db
            .sales_order_submissions()
            .find_one_by_field(
                "working_copy_id",
                working_copy.base.id.as_str(),
                &mut NoTransaction,
            )
            .await?
        {
            let existing_id = SalesOrderSubmissionId::new(existing.base.id.clone());
            let existing_lines = self
                .db
                .sales_order_submission_lines()
                .list_lines_by_submissions(&[existing_id], &mut NoTransaction)
                .await?;
            return Ok(submission_view(existing, existing_lines));
        }

        let copy_id = SalesOrderWorkingCopyId::new(working_copy.base.id.clone());
        let copy_lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&copy_id, &mut NoTransaction)
            .await?;
        self.ensure_sellable_working_copy_lines(&copy_lines).await?;
        let existing_submissions = self
            .db
            .sales_order_submissions()
            .find_many(mongodb::bson::doc! { "sales_order_id": id }, &mut NoTransaction)
            .await?;
        let submission_no = existing_submissions
            .iter()
            .map(|submission| submission.submission_no)
            .max()
            .unwrap_or(0)
            + 1;
        let submission = build_submission(&working_copy, &copy_lines, submission_no, actor)?;
        let submission_lines = build_submission_lines(&submission, &copy_lines)?;
        let mut order_mut = order;
        order_mut.submit_for_review(actor.id())?;
        working_copy.submit()?;

        let (review, confirmation, work_item) = match order_mut.business_type {
            BusinessType::Voucher => {
                let review = SalesOrderReview::new(
                    SalesOrderReviewId::new(next_id()),
                    SalesOrderReviewData {
                        sales_order_id: order_id.clone(),
                        submission_id: submission.base.id.clone().into(),
                        review_stage: SalesReviewStage::SalesLeader,
                    },
                    actor.id(),
                )?;
                let item = WorkItem::new(
                    WorkItemId::new(next_id()),
                    WorkItemData {
                        work_item_type: WorkItemType::CardSalesManagerApproval,
                        business_object_type: "sales_order_review".to_string(),
                        business_object_id: review.base.id.clone(),
                        subject_version: Some(submission.base.id.clone()),
                        owner_role: Some("sales_leader".to_string()),
                        owner_user_id: None,
                        priority: WorkItemPriority::High,
                        due_at: None,
                        reason_code: Some("review_dispatched".to_string()),
                        impact_summary: Some("卡券销售提交待销售领导审批".to_string()),
                        completion_action: "DECIDE_REVIEW".to_string(),
                    },
                )?;
                (Some(review), None, Some(item))
            }
            BusinessType::GoodsService => {
                let confirmation = ProcurementConfirmation::new(
                    ProcurementConfirmationId::new(next_id()),
                    ProcurementConfirmationData {
                        sales_order_id: order_id.clone(),
                        submission_id: submission.base.id.clone().into(),
                        reject_reason_code: None,
                        comment: None,
                    },
                    actor.id(),
                )?;
                let item = WorkItem::new(
                    WorkItemId::new(next_id()),
                    WorkItemData {
                        work_item_type: WorkItemType::ProcurementConfirmation,
                        business_object_type: "procurement_confirmation".to_string(),
                        business_object_id: confirmation.base.id.clone(),
                        subject_version: Some(submission.base.id.clone()),
                        owner_role: Some("procurement".to_string()),
                        owner_user_id: None,
                        priority: WorkItemPriority::High,
                        due_at: None,
                        reason_code: Some("procurement_confirmation_dispatched".to_string()),
                        impact_summary: Some(format!("采购二次确认：销售提交 {}", submission.submission_no)),
                        completion_action: "DECIDE_CONFIRMATION".to_string(),
                    },
                )?;
                (None, Some(confirmation), Some(item))
            }
        };
        let audit = actor
            .clone()
            .resource_log("sales_order.submit", "sales_order", order_id.to_string())?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut wc_for_tx = working_copy.clone();
        let submission_for_tx = submission.clone();
        let lines_for_tx = submission_lines.clone();
        let confirmation_for_tx = confirmation.clone();
        let sellable_refs_for_tx = Self::sellable_working_copy_refs(&copy_lines)?;
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(&sellable_refs_for_tx, session)
                        .await?;
                    db.sales_order()
                        .submit_working_copy(&mut wc_for_tx, &submission_for_tx, &lines_for_tx, session)
                        .await?;
                    db.sales_orders().update(&mut order_mut, session).await?;
                    if let Some(review) = &review {
                        db.sales_order_reviews().create(review, session).await?;
                    }
                    if let Some(item) = &work_item {
                        db.work_items().create(item, session).await?;
                    }
                    if let Some(confirmation) = &confirmation_for_tx {
                        db.sales_review()
                            .create_procurement_confirmation_with_lines(confirmation, &[], session)
                            .await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(submission_view(submission, submission_lines))
    }

    /// 校验请求草稿中的实物及服务行仍引用公司商品池内的精确 SKU 修订。
    async fn ensure_sellable_draft_lines(&self, lines: &[SalesOrderDraftLineRequest]) -> Result<()> {
        let refs = lines
            .iter()
            .filter_map(|line| line.goods.as_ref())
            .map(|goods| (goods.sku_id.to_string(), goods.sku_revision_id.to_string()))
            .collect::<Vec<_>>();
        self.ensure_sellable_refs(&refs, &mut NoTransaction).await
    }

    /// 提交前重新校验已保存工作副本的精确 SKU 修订资格。
    async fn ensure_sellable_working_copy_lines(&self, lines: &[SalesOrderWorkingCopyLine]) -> Result<()> {
        let refs = Self::sellable_working_copy_refs(lines)?;
        self.ensure_sellable_refs(&refs, &mut NoTransaction).await
    }

    /// 从工作副本行提取必须成对存在的销售 SKU 与修订引用。
    fn sellable_working_copy_refs(lines: &[SalesOrderWorkingCopyLine]) -> Result<Vec<(String, String)>> {
        let refs = lines
            .iter()
            .filter(|line| line.line_type == LineType::GoodsService)
            .map(|line| {
                let sku_id = line
                    .sku_id
                    .as_ref()
                    .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU", line.line_no)))?;
                let revision_id = line
                    .sku_revision_id
                    .as_ref()
                    .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU 修订", line.line_no)))?;
                Ok((sku_id.to_string(), revision_id.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(refs)
    }

    /// 批量执行公司商品池资格校验并对缺失引用 fail-closed。
    async fn ensure_sellable_refs(
        &self,
        refs: &[(String, String)],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if refs.is_empty() {
            return Ok(());
        }
        let expected = refs.iter().cloned().collect::<HashSet<_>>();
        let qualified = self
            .db
            .catalog()
            .find_sellable_sku_refs(refs, BusinessDate::today(), executor)
            .await?
            .into_iter()
            .map(|row| (row.sku_id, row.sku_revision_id))
            .collect::<HashSet<_>>();
        let mut invalid = expected
            .difference(&qualified)
            .map(|(sku_id, _)| sku_id.clone())
            .collect::<Vec<_>>();
        invalid.sort();
        if invalid.is_empty() {
            Ok(())
        } else {
            Err(crate::catalog::sellable_sku_invalid_error(&invalid))
        }
    }

    /// 作废销售单草稿（主状态 `DRAFT → VOIDED`；放弃有效工作副本）。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 作废请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回作废后的销售单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn void_sales_order(
        &self,
        id: &str,
        req: VoidSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        req.validate()?;
        let mut order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        order.void(actor.id())?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let mut working_copy = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?;
        if let Some(copy) = &mut working_copy {
            copy.abandon()?;
        }
        let audit = actor
            .clone()
            .resource_log("sales_order.void", "sales_order", id.to_string())?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_orders().update(&mut order, session).await?;
                    if let Some(copy) = &mut working_copy {
                        db.sales_order_working_copies().update(copy, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.sales_order_detail(id).await
    }

    /// 构建工作副本行视图。
    ///
    /// # 参数
    /// * `copy` - 工作副本实体
    ///
    /// # 返回
    /// 返回行视图集合。
    ///
    /// # 错误
    /// 数据库读取失败时返回错误。
    async fn working_copy_view(&self, copy: &SalesOrderWorkingCopy) -> Result<WorkingCopyView> {
        let lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&copy.base.id.clone().into(), &mut NoTransaction)
            .await?;
        Ok(WorkingCopyView {
            id: copy.base.id.clone(),
            version: copy.base.version,
            working_purpose: copy.working_purpose,
            status: copy.stable.status,
            draft_version: copy.draft_version,
            content_hash: copy.content_hash.clone(),
            editor_user_id: copy.editor_user_id.clone(),
            business_type: copy.business_type,
            customer_name: copy.customer_snapshot.customer_name.clone(),
            contract_no: copy.contract_snapshot.as_ref().map(|s| s.contract_no.clone()),
            settlement_party_name: copy
                .settlement_party_snapshot
                .as_ref()
                .map(|s| s.settlement_party_name.clone()),
            payment_term_code: copy.payment_term_snapshot.payment_term_code.clone(),
            payment_term_name: copy.payment_term_snapshot.payment_term_name.clone(),
            invoice_type: copy.invoice_requirement_snapshot.invoice_type.clone(),
            tax_point: copy.invoice_requirement_snapshot.tax_point.clone(),
            project_name: copy.project_name.clone(),
            business_remark: copy.business_remark.clone(),
            voucher_category_sku_id: copy.voucher_category_sku_id.as_ref().map(ToString::to_string),
            voucher_expiry_at: copy.voucher_expiry_at.map(|instant| instant.unix_secs() as u64),
            gross_amount: copy.gross_amount,
            net_amount: copy.net_amount,
            tax_amount: copy.tax_amount,
            lines: lines.into_iter().map(working_copy_line_view).collect(),
        })
    }

    /// 查询销售单负责人账号姓名。
    ///
    /// # 参数
    /// * `user_id` - 负责人账号 ID
    ///
    /// # 返回
    /// 返回账号姓名；账号已不存在时返回 `None`。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    async fn account_name(&self, user_id: &str) -> Result<Option<String>> {
        Ok(self
            .db
            .accounts()
            .find_by_id(user_id, &mut NoTransaction)
            .await?
            .map(|account| account.name))
    }

    /// 解析当前审核轨阶段的责任角色/责任人/时限（详情页专用）。
    ///
    /// 按 `review_status` 找到对应的采购确认/审批记录，再按
    /// `(business_object_type, business_object_id)` 查找命中的有效待办
    /// （`WorkItemExt::list_active_by_object`）。找不到对应记录或待办时（例如
    /// 尚未提交、已生效、或 `PENDING_LOW_MARGIN_SUPERIOR` 当前无生产代码路径会
    /// 创建对应记录）返回全 `None`，不视为错误。
    ///
    /// # 参数
    /// * `review_status` - 销售单当前审核轨阶段
    /// * `latest_submission_id` - 最新一次提交；尚未提交过时为 `None`
    ///
    /// # 返回
    /// 返回 `(责任角色, 责任人账号, 时限)`。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    async fn resolve_stage_owner(
        &self,
        review_status: ReviewStatus,
        latest_submission_id: Option<&SalesOrderSubmissionId>,
    ) -> Result<(Option<String>, Option<String>, Option<u64>)> {
        let Some(submission_id) = latest_submission_id else {
            return Ok((None, None, None));
        };

        let business_object = match review_status {
            ReviewStatus::PendingProcurementConfirmation => self
                .db
                .procurement_confirmations()
                .find_pending_by_submission(submission_id, &mut NoTransaction)
                .await?
                .map(|confirmation| ("procurement_confirmation", confirmation.base.id)),
            ReviewStatus::PendingSalesLeader => self
                .db
                .sales_order_reviews()
                .find_by_submission_and_stage(
                    submission_id,
                    SalesReviewStage::SalesLeader,
                    &mut NoTransaction,
                )
                .await?
                .map(|review| ("sales_order_review", review.base.id)),
            ReviewStatus::PendingOperations => self
                .db
                .sales_order_reviews()
                .find_by_submission_and_stage(submission_id, SalesReviewStage::Operations, &mut NoTransaction)
                .await?
                .map(|review| ("sales_order_review", review.base.id)),
            ReviewStatus::PendingLowMarginSuperior => self
                .db
                .sales_order_reviews()
                .find_by_submission_and_stage(
                    submission_id,
                    SalesReviewStage::LowMarginSuperior,
                    &mut NoTransaction,
                )
                .await?
                .map(|review| ("sales_order_review", review.base.id)),
            _ => None,
        };

        let Some((business_object_type, business_object_id)) = business_object else {
            return Ok((None, None, None));
        };

        let items = self
            .db
            .work_items()
            .list_active_by_object(business_object_type, &business_object_id, &mut NoTransaction)
            .await?;

        Ok(match items.first() {
            Some(item) => (
                item.owner_role.clone(),
                item.owner_user_id.clone(),
                item.due_at.map(|instant| instant.unix_secs() as u64),
            ),
            None => (None, None, None),
        })
    }

    /// 批量解析本页销售单的当前阶段责任人/时限（列表专用，避免逐行查询）。
    ///
    /// 按 `sales_order_id` 直接查命中的采购确认/审批记录（不像
    /// [`Self::resolve_stage_owner`] 那样先定位最新提交——同一销售单同时只会
    /// 有一条在途确认/审批记录，列表场景不需要精确到"最新提交"这一层），
    /// 再批量查对应 work_item、批量解析涉及账号姓名。整页固定 3 次查询，不随
    /// 页大小线性增长。
    ///
    /// # 参数
    /// * `rows` - 本页销售单 `(id, review_status)`
    ///
    /// # 返回
    /// 返回按销售单 id 索引的 `(责任角色, 责任人账号, 责任人姓名, 时限)`；
    /// 审核轨不在途或无命中待办的订单不出现在返回的 map 中。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    async fn resolve_stage_owners_batch(
        &self,
        rows: &[(String, ReviewStatus)],
    ) -> Result<HashMap<String, (Option<String>, Option<String>, Option<String>, Option<u64>)>> {
        let pending_ids: Vec<String> = rows
            .iter()
            .filter(|(_, review)| {
                matches!(
                    review,
                    ReviewStatus::PendingProcurementConfirmation
                        | ReviewStatus::PendingSalesLeader
                        | ReviewStatus::PendingOperations
                        | ReviewStatus::PendingLowMarginSuperior
                )
            })
            .map(|(id, _)| id.clone())
            .collect();

        let mut owners = HashMap::new();
        if pending_ids.is_empty() {
            return Ok(owners);
        }

        let confirmations = self
            .db
            .procurement_confirmations()
            .find_many(
                mongodb::bson::doc! {
                    "sales_order_id": { "$in": &pending_ids },
                    "status": ProcurementConfirmationStatus::Pending.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        let reviews = self
            .db
            .sales_order_reviews()
            .find_many(
                mongodb::bson::doc! {
                    "sales_order_id": { "$in": &pending_ids },
                    "status": SalesReviewStatus::Pending.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;

        let mut business_object_by_order: HashMap<String, (&'static str, String)> = HashMap::new();
        for confirmation in &confirmations {
            business_object_by_order.insert(
                confirmation.sales_order_id.to_string(),
                ("procurement_confirmation", confirmation.base.id.clone()),
            );
        }
        for review in &reviews {
            business_object_by_order.insert(
                review.sales_order_id.to_string(),
                ("sales_order_review", review.base.id.clone()),
            );
        }
        if business_object_by_order.is_empty() {
            return Ok(owners);
        }

        let object_ids: Vec<String> = business_object_by_order
            .values()
            .map(|(_, id)| id.clone())
            .collect();
        let work_items = self
            .db
            .work_items()
            .find_many(
                mongodb::bson::doc! {
                    "business_object_type": { "$in": ["procurement_confirmation", "sales_order_review"] },
                    "business_object_id": { "$in": &object_ids },
                    "status": { "$in": [WorkItemStatus::Unclaimed.as_str(), WorkItemStatus::InProgress.as_str()] },
                },
                &mut NoTransaction,
            )
            .await?;
        let work_item_by_object: HashMap<String, &WorkItem> = work_items
            .iter()
            .map(|item| (item.business_object_id.clone(), item))
            .collect();

        let owner_user_ids: HashSet<String> = work_items
            .iter()
            .filter_map(|item| item.owner_user_id.clone())
            .collect();
        let mut names: HashMap<String, Option<String>> = HashMap::new();
        for user_id in owner_user_ids {
            let name = self.account_name(&user_id).await?;
            names.insert(user_id, name);
        }

        for (order_id, (_, object_id)) in &business_object_by_order {
            let item = work_item_by_object.get(object_id);
            let owner_role = item.and_then(|i| i.owner_role.clone());
            let owner_user_id = item.and_then(|i| i.owner_user_id.clone());
            let owner_user_name = owner_user_id
                .as_ref()
                .and_then(|user_id| names.get(user_id).cloned().flatten());
            let due_at = item.and_then(|i| i.due_at.map(|instant| instant.unix_secs() as u64));
            owners.insert(
                order_id.clone(),
                (owner_role, owner_user_id, owner_user_name, due_at),
            );
        }

        Ok(owners)
    }
}

/// 按“有效草稿负责人 → 最新提交人 → 建单人”确定销售单详情负责人。
fn detail_owner_user_id(
    working_copy_editor: Option<&str>,
    latest_submitter: Option<&str>,
    created_by: &str,
) -> String {
    working_copy_editor
        .or(latest_submitter)
        .unwrap_or(created_by)
        .to_string()
}

/// 销售单当前阶段 `(code, label, tone)`（服务端权威计算）。
///
/// 移植自 erp-client `api.ts::mapPrimaryStatus`；用真实枚举类型消掉了原实现里
/// 防御性的字符串兜底分支——后端只会产出这里覆盖到的枚举组合（`Draft` 恒配
/// `NotSubmitted`，`PendingReview` 恒配非 `NotSubmitted` 的审核轨阶段，见
/// `SalesOrder::submit_for_review`/`return_to_draft` 的不变式，两者不会交叉）。
fn stage_code_label_tone(
    commercial: CommercialStatus,
    review: ReviewStatus,
    close: CloseStatus,
    fulfillment: FulfillmentProgress,
) -> (&'static str, &'static str, &'static str) {
    if close == CloseStatus::Closed {
        return ("closed", "已关闭", "void");
    }
    if commercial == CommercialStatus::Voided {
        return ("voided", "已作废", "void");
    }
    if commercial == CommercialStatus::Effective {
        return if fulfillment == FulfillmentProgress::PartiallyFulfilled {
            ("fulfilling", "履约中", "info")
        } else {
            ("effective", "已生效", "success")
        };
    }
    match review {
        ReviewStatus::PendingProcurementConfirmation => ("awaiting_confirm", "待二次确认", "warning"),
        ReviewStatus::PendingLowMarginSuperior | ReviewStatus::Rejected => {
            ("awaiting_sales", "待销售处理", "warning")
        }
        ReviewStatus::PendingSalesLeader => ("awaiting_sales_lead", "待销售领导审批", "warning"),
        ReviewStatus::PendingOperations => ("awaiting_ops", "待运营审批", "warning"),
        ReviewStatus::Approved => ("effective", "已生效", "success"),
        ReviewStatus::NotSubmitted => ("draft", "草稿", "neutral"),
    }
}

/// 结案资格判定（服务端权威；移植自 erp-client `close-eligibility.ts::computeCloseEligibility`）。
///
/// 规则（W05 §5.3/§12）：非卡券以客户验收完成判定交付；卡券以履约期限到期判定
/// （不因已消费完提前算完成，`FulfillmentProgress::Completed` 由到期任务写入）；
/// 结案门槛为交付完成且回款收齐，开票进度不参与。回款是否收齐优先看
/// `collection_progress`，辅以应收子账合计兜底（两者任一满足即视为收齐，
/// 与前端原实现的双重判断保持一致）。
#[allow(clippy::too_many_arguments)]
fn compute_close_eligibility(
    business_type: BusinessType,
    commercial: CommercialStatus,
    close: CloseStatus,
    fulfillment: FulfillmentProgress,
    collection: CollectionProgress,
    invoice: InvoiceProgress,
    settled_total: Amount,
    gross_total: Amount,
) -> dto::CloseEligibilityView {
    let invoice_complete = invoice == InvoiceProgress::Completed;
    let fulfillment_done = fulfillment == FulfillmentProgress::Completed;
    let collection_settled = collection == CollectionProgress::Settled;

    if close == CloseStatus::Closed
        || commercial == CommercialStatus::Voided
        || commercial == CommercialStatus::Draft
    {
        let closed = close == CloseStatus::Closed;
        let (blockers, note) = if closed {
            (
                Vec::new(),
                "交付与回款都已完成，本单已自动结案。开票是否做完不影响结案。".to_string(),
            )
        } else if commercial == CommercialStatus::Voided {
            (
                vec!["本单已作废，不会再结案".to_string()],
                "作废单只保留历史记录，不能结案，也不能恢复。".to_string(),
            )
        } else {
            (
                vec!["草稿尚未生效，谈不上结案".to_string()],
                "草稿还没生效，先完成提交与确认。".to_string(),
            )
        };
        return dto::CloseEligibilityView {
            fulfillment_complete: closed || fulfillment_done,
            receivable_settled: closed || collection_settled,
            invoice_complete,
            eligible_to_close: closed,
            blockers,
            note,
        };
    }

    let fulfillment_complete = fulfillment_done;
    let receivable_settled = collection_settled || settled_total >= gross_total;
    let mut blockers = Vec::new();
    if !fulfillment_complete {
        blockers.push(
            if business_type == BusinessType::Voucher {
                "卡券还没到履约期限（持卡人是否消费完都不提前算交付完成）"
            } else {
                "客户验收还没做完"
            }
            .to_string(),
        );
    }
    if !receivable_settled {
        blockers.push("客户回款还没收齐".to_string());
    }
    let eligible_to_close = fulfillment_complete && receivable_settled;
    let note = if eligible_to_close {
        "交付和回款都齐了，系统会自动结案。发票开没开完都不挡结案，也无需人工点「关闭」。".to_string()
    } else {
        format!("还不能结案：{}。开票进度不参与是否结案。", blockers.join("；"))
    };

    dto::CloseEligibilityView {
        fulfillment_complete,
        receivable_settled,
        invoice_complete,
        eligible_to_close,
        blockers,
        note,
    }
}

/// 是否可以发起销售变更单（服务端权威；移植自 close-eligibility.ts::canStartSalesChange）。
///
/// 已生效单不可直接编辑；ERP 开单的已生效单可发起销售变更（商城开单同步的
/// 单据、尚在确认/审批中的单据、已有进行中变更单的单据均不可发起）。
fn compute_can_start_sales_change(
    origin_system: OriginSystem,
    stage_code: &str,
    stage_label: &str,
    has_active_change_order: bool,
) -> (bool, Option<String>) {
    if origin_system != OriginSystem::Erp {
        return (
            false,
            Some("这单由商城开单，商业数据同步中，本系统只能查看；改内容请在商城处理。".to_string()),
        );
    }
    if matches!(stage_code, "draft" | "voided" | "closed") {
        return (
            false,
            Some(format!("当前状态是「{stage_label}」，不能发起改单。")),
        );
    }
    if matches!(
        stage_code,
        "awaiting_sales" | "awaiting_confirm" | "awaiting_sales_lead" | "awaiting_ops"
    ) {
        return (
            false,
            Some("本单还在确认/审批中，请先处理完当前待办，再发起改单。".to_string()),
        );
    }
    if has_active_change_order {
        return (
            false,
            Some("已有一笔改单在处理中，请等它走完再发起新的。".to_string()),
        );
    }
    (true, None)
}

/// 汇总已舍入的行金额三元组（§4.2 铁律 2：表头只汇总已舍入的行金额）。
///
/// # 参数
/// * `lines` - 行实体（金额已在实体 `new` 内逐行舍入）
///
/// # 返回
/// 返回 `(含税合计, 不含税合计, 税额合计)`。
fn line_totals<T: LineAmounts>(lines: &[T]) -> (Amount, Amount, Amount) {
    let zero = zero_amount();
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

/// 行金额访问器（工作副本行与提交行共用）。
trait LineAmounts {
    /// 返回行含税金额。
    fn gross_amount(&self) -> Amount;
    /// 返回行不含税金额。
    fn net_amount(&self) -> Amount;
    /// 返回行税额。
    fn tax_amount(&self) -> Amount;
}

impl LineAmounts for SalesOrderWorkingCopyLine {
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

impl LineAmounts for SalesOrderSubmissionLine {
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

/// 返回静态零金额。
///
/// # 返回
/// 返回 `0.00` 金额值。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("静态零值必须合法")
}

/// 构建稳定明细行（订单创建时按草稿行号建立）。
///
/// # 参数
/// * `order_id` - 所属销售单
/// * `lines` - 草稿行请求
///
/// # 返回
/// 返回稳定明细行清单（行号升序）。
///
/// # 错误
/// 行号重复时返回错误。
fn build_stable_lines(
    order_id: &SalesOrderId,
    lines: &[SalesOrderDraftLineRequest],
) -> Result<Vec<SalesOrderLine>> {
    let mut stable = Vec::with_capacity(lines.len());
    for line in lines {
        if stable
            .iter()
            .any(|existing: &SalesOrderLine| existing.line_no == line.line_no)
        {
            return Err(Error::ValidationError(format!("行号 {} 重复", line.line_no)));
        }
        stable.push(SalesOrderLine::new(
            SalesOrderLineId::new(next_id()),
            order_id.clone(),
            SalesOrderLineData {
                line_no: line.line_no,
            },
        )?);
    }
    Ok(stable)
}

/// 构建工作副本（含行实体）。
///
/// 行金额由实体 `new` 逐行舍入计算，表头金额由服务端汇总已舍入的行金额
/// （§4.2 铁律 2）。
///
/// # 参数
/// * `order` - 所属销售单实体（已建）
/// * `stable_lines` - 稳定明细行（已建，行号与草稿行一一对应）
/// * `draft` - 草稿表头与明细请求
/// * `draft_version` - 初始草稿版本
/// * `actor` - 操作人
///
/// # 返回
/// 返回 `(工作副本实体, 工作副本行清单)`。
///
/// # 错误
/// 表头快照、行字段组或金额三元组校验失败时返回错误。
fn build_working_copy(
    order: &SalesOrder,
    stable_lines: &[SalesOrderLine],
    draft: &SalesOrderDraftRequest,
    draft_version: u32,
    actor: &AuditActor,
) -> Result<(SalesOrderWorkingCopy, Vec<SalesOrderWorkingCopyLine>)> {
    let order_id = SalesOrderId::new(order.base.id.clone());
    let working_copy_id = SalesOrderWorkingCopyId::new(next_id());
    // 行创建数据同时用于：① 头实体 `validate_line_list` 跨行断言；② 行实体落库。
    // 此前传空 Vec 会导致「销售单明细不能为空」——头校验与行集合分离的契约要求
    // 创建数据必须携带完整行摘要（即使行最终分集合存储）。
    let line_datas = build_working_copy_line_datas(stable_lines, &draft.lines)?;
    let lines = materialize_working_copy_lines(&working_copy_id, &line_datas)?;
    let (gross, net, tax) = line_totals(&lines);
    let snapshot = header_snapshot(draft)?;
    let working_copy = SalesOrderWorkingCopy::new(
        working_copy_id,
        SalesOrderWorkingCopyData {
            sales_order_id: order_id,
            working_purpose: WorkingPurpose::FirstSubmission,
            sales_change_order_id: None,
            base_revision_id: None,
            draft_version,
            content_hash: draft_hash(&order.base.id, draft_version),
            editor_user_id: draft.editor_user_id.clone(),
            business_type: order.business_type,
            customer_id: order.customer_id.clone(),
            contract_id: order.contract_id.clone(),
            settlement_party_id: order.settlement_party_id.clone(),
            snapshot,
            project_name: draft.project_name.clone(),
            business_remark: draft.business_remark.clone(),
            voucher_category_sku_id: draft.voucher_category_sku_id.clone(),
            voucher_expiry_at: draft
                .voucher_expiry_at
                .map(|secs| Instant::from_unix_secs(secs as i64)),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            lines: line_datas,
        },
        actor.id(),
    )?;
    Ok((working_copy, lines))
}

/// 将草稿行请求映射为工作副本行创建数据（稳定行号 → 稳定明细身份）。
///
/// # 参数
/// * `stable_lines` - 稳定明细行（行号与草稿行一一对应）
/// * `lines` - 草稿行请求
///
/// # 返回
/// 返回工作副本行创建数据清单。
///
/// # 错误
/// 行号无对应稳定明细时返回错误。
fn build_working_copy_line_datas(
    stable_lines: &[SalesOrderLine],
    lines: &[SalesOrderDraftLineRequest],
) -> Result<Vec<SalesOrderWorkingCopyLineData>> {
    let mut datas = Vec::with_capacity(lines.len());
    for line in lines {
        let stable_id = stable_lines
            .iter()
            .find(|stable| stable.line_no == line.line_no)
            .map(|stable| stable.base.id.clone())
            .ok_or_else(|| Error::ValidationError(format!("行号 {} 无对应稳定明细", line.line_no)))?;
        datas.push(SalesOrderWorkingCopyLineData {
            sales_order_line_id: SalesOrderLineId::new(stable_id),
            line_no: line.line_no,
            line_type: line.line_type,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
            goods: line.goods.clone(),
            voucher: line.voucher.clone(),
        });
    }
    Ok(datas)
}

/// 由行创建数据物化工作副本行实体。
///
/// # 参数
/// * `working_copy_id` - 所属工作副本 ID
/// * `line_datas` - 行创建数据
///
/// # 返回
/// 返回工作副本行实体清单（金额由实体逐行舍入计算）。
///
/// # 错误
/// 行字段组与行类型不一致、金额非法时返回错误。
fn materialize_working_copy_lines(
    working_copy_id: &SalesOrderWorkingCopyId,
    line_datas: &[SalesOrderWorkingCopyLineData],
) -> Result<Vec<SalesOrderWorkingCopyLine>> {
    let mut built = Vec::with_capacity(line_datas.len());
    for data in line_datas {
        built.push(SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new(next_id()),
            working_copy_id.clone(),
            data.clone(),
        )?);
    }
    Ok(built)
}

/// 构建工作副本行实体（草稿保存路径复用：稳定行 + 草稿请求 → 行实体）。
///
/// # 参数
/// * `_order_id` - 所属销售单（预留，稳定行 ID 由建单实体产生）
/// * `working_copy_id` - 所属工作副本 ID
/// * `stable_lines` - 稳定明细行（行号与草稿行一一对应）
/// * `lines` - 草稿行请求
///
/// # 返回
/// 返回工作副本行清单（金额由实体逐行舍入计算）。
///
/// # 错误
/// 行字段组与行类型不一致、金额非法时返回错误。
fn build_working_copy_lines(
    _order_id: &SalesOrderId,
    working_copy_id: &SalesOrderWorkingCopyId,
    stable_lines: &[SalesOrderLine],
    lines: &[SalesOrderDraftLineRequest],
) -> Result<Vec<SalesOrderWorkingCopyLine>> {
    let line_datas = build_working_copy_line_datas(stable_lines, lines)?;
    materialize_working_copy_lines(working_copy_id, &line_datas)
}

/// 构建表头快照入参。
///
/// # 参数
/// * `draft` - 草稿表头请求
///
/// # 返回
/// 返回表头快照入参。
fn header_snapshot(draft: &SalesOrderDraftRequest) -> Result<entities::sales_order::HeaderSnapshotData> {
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

/// 生成草稿内容指纹（服务端确定性派生，供幂等与历史查询）。
///
/// # 参数
/// * `id` - 业务对象 ID
/// * `version` - 草稿版本
///
/// # 返回
/// 返回 128 字符内的内容指纹。
fn draft_hash(id: &str, version: u32) -> String {
    format!("draft:{id}:{version}")
}

/// 从工作副本构建提交快照。
///
/// # 参数
/// * `working_copy` - 已迁移到 `Submitted` 的工作副本
/// * `lines` - 工作副本行
/// * `submission_no` - 提交序号
/// * `actor` - 提交人
///
/// # 返回
/// 返回提交快照实体。
///
/// # 错误
/// 提交字段校验失败时返回错误。
fn build_submission(
    working_copy: &SalesOrderWorkingCopy,
    lines: &[SalesOrderWorkingCopyLine],
    submission_no: u32,
    actor: &AuditActor,
) -> Result<SalesOrderSubmission> {
    let (gross, net, tax) = line_totals(lines);
    // 提交头 `validate_line_list` 需要行摘要；行实体另集存储，但创建数据必须非空。
    let mut line_datas = Vec::with_capacity(lines.len());
    for line in lines {
        line_datas.push(SalesOrderSubmissionLineData {
            sales_order_line_id: line.sales_order_line_id.clone(),
            line_no: line.line_no,
            line_type: line.line_type,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
            goods: working_copy_goods(line)?,
            voucher: working_copy_voucher(line)?,
        });
    }
    SalesOrderSubmission::new(
        SalesOrderSubmissionId::new(next_id()),
        SalesOrderSubmissionData {
            sales_order_id: working_copy.sales_order_id.clone(),
            submission_no,
            working_copy_id: working_copy.base.id.clone().into(),
            working_copy_version: working_copy.draft_version,
            business_type: working_copy.business_type,
            customer_id: working_copy.customer_id.clone(),
            contract_revision_id: None,
            settlement_party_id: working_copy.settlement_party_id.clone(),
            snapshot: entities::sales_order::HeaderSnapshotData {
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
            lines: line_datas,
        },
    )
    .map_err(Error::Logic)
}

/// 从工作副本行构建提交快照明细。
///
/// # 参数
/// * `submission` - 提交快照
/// * `lines` - 工作副本行
///
/// # 返回
/// 返回提交快照明细清单。
///
/// # 错误
/// 行字段组缺失或非法时返回错误。
fn build_submission_lines(
    submission: &SalesOrderSubmission,
    lines: &[SalesOrderWorkingCopyLine],
) -> Result<Vec<SalesOrderSubmissionLine>> {
    let mut built = Vec::with_capacity(lines.len());
    for line in lines {
        let goods = working_copy_goods(line)?;
        let voucher = working_copy_voucher(line)?;
        built.push(SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new(next_id()),
            submission.base.id.clone().into(),
            SalesOrderSubmissionLineData {
                sales_order_line_id: line.sales_order_line_id.clone(),
                line_no: line.line_no,
                line_type: line.line_type,
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

/// 从工作副本行还原实物及服务字段组。
///
/// # 参数
/// * `line` - 工作副本行
///
/// # 返回
/// 返回字段组；卡券行返回 `None`。
///
/// # 错误
/// 实物及服务行缺商品字段组时返回错误。
fn working_copy_goods(line: &SalesOrderWorkingCopyLine) -> Result<Option<GoodsLineFields>> {
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
    Ok(Some(GoodsLineFields {
        sku_id,
        sku_revision_id,
        welfare_scenario: line.welfare_scenario,
        fulfillment_mode,
        fulfillment_due_at,
        quantity,
        base_unit_code,
        unit_price_gross,
    }))
}

/// 从工作副本行还原卡券字段组。
///
/// # 参数
/// * `line` - 工作副本行
///
/// # 返回
/// 返回字段组；实物及服务行返回 `None`。
///
/// # 错误
/// 卡券行缺卡券字段组时返回错误。
fn working_copy_voucher(line: &SalesOrderWorkingCopyLine) -> Result<Option<VoucherLineDraft>> {
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
    let face_value_total = line
        .face_value_total
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少面额小计", line.line_no)))?;
    let transaction_amount = line
        .transaction_amount
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少成交金额", line.line_no)))?;
    let gift_amount = line
        .gift_amount
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少配赠金额", line.line_no)))?;
    Ok(Some(VoucherLineDraft {
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
    }))
}

/// 构造提交历史视图。
///
/// # 参数
/// * `submission` - 提交快照实体
/// * `lines` - 该提交下的明细行
///
/// # 返回
/// 返回视图。
fn submission_view(submission: SalesOrderSubmission, lines: Vec<SalesOrderSubmissionLine>) -> SubmissionView {
    SubmissionView {
        id: submission.base.id,
        submission_no: submission.submission_no,
        status: submission.stable.status,
        business_type: submission.business_type,
        customer_name: submission.customer_snapshot.customer_name.clone(),
        contract_no: submission
            .contract_snapshot
            .as_ref()
            .map(|s| s.contract_no.clone()),
        settlement_party_name: submission
            .settlement_party_snapshot
            .as_ref()
            .map(|s| s.settlement_party_name.clone()),
        payment_term_code: submission.payment_term_snapshot.payment_term_code.clone(),
        payment_term_name: submission.payment_term_snapshot.payment_term_name.clone(),
        invoice_type: submission.invoice_requirement_snapshot.invoice_type.clone(),
        tax_point: submission.invoice_requirement_snapshot.tax_point.clone(),
        project_name: submission.project_name.clone(),
        business_remark: submission.business_remark.clone(),
        voucher_category_sku_id: submission
            .voucher_category_sku_id
            .as_ref()
            .map(ToString::to_string),
        voucher_expiry_at: submission
            .voucher_expiry_at
            .map(|instant| instant.unix_secs() as u64),
        gross_amount: submission.gross_amount,
        net_amount: submission.net_amount,
        tax_amount: submission.tax_amount,
        submitted_by: submission.submitted_by,
        submitted_at: submission.submitted_at.unix_secs() as u64,
        created_at: submission.base.created_at,
        lines: lines.into_iter().map(submission_line_view).collect(),
    }
}

/// 构造工作副本行视图。
///
/// # 参数
/// * `line` - 工作副本行实体
///
/// # 返回
/// 返回视图。
fn working_copy_line_view(line: SalesOrderWorkingCopyLine) -> SalesOrderWorkingCopyLineView {
    SalesOrderWorkingCopyLineView {
        id: line.base.id,
        sales_order_line_id: line.sales_order_line_id.to_string(),
        line_no: line.line_no,
        line_type: line.line_type,
        gross_amount: line.gross_amount,
        net_amount: line.net_amount,
        tax_amount: line.tax_amount,
        sales_tax_rate: line.sales_tax_rate,
        item_name_snapshot: line.item_name_snapshot,
        spec_snapshot: line.spec_snapshot,
        unit_snapshot: line.unit_snapshot,
        sku_id: line.sku_id,
        sku_revision_id: line.sku_revision_id,
        welfare_scenario: line.welfare_scenario,
        fulfillment_mode: line.fulfillment_mode,
        fulfillment_due_at: line.fulfillment_due_at.map(|instant| instant.unix_secs() as u64),
        quantity: line.quantity,
        base_unit_code: line.base_unit_code,
        unit_price_gross: line.unit_price_gross,
        face_value: line.face_value,
        card_count: line.card_count,
        transaction_amount: line.transaction_amount,
        card_form: line.card_form,
    }
}

/// 构造提交明细行视图（与工作副本行视图同形）。
///
/// # 参数
/// * `line` - 提交行实体
///
/// # 返回
/// 返回视图。
fn submission_line_view(line: SalesOrderSubmissionLine) -> SalesOrderWorkingCopyLineView {
    SalesOrderWorkingCopyLineView {
        id: line.base.id,
        sales_order_line_id: line.sales_order_line_id.to_string(),
        line_no: line.line_no,
        line_type: line.line_type,
        gross_amount: line.gross_amount,
        net_amount: line.net_amount,
        tax_amount: line.tax_amount,
        sales_tax_rate: line.sales_tax_rate,
        item_name_snapshot: line.item_name_snapshot,
        spec_snapshot: line.spec_snapshot,
        unit_snapshot: line.unit_snapshot,
        sku_id: line.sku_id,
        sku_revision_id: line.sku_revision_id,
        welfare_scenario: line.welfare_scenario,
        fulfillment_mode: line.fulfillment_mode,
        fulfillment_due_at: line.fulfillment_due_at.map(|instant| instant.unix_secs() as u64),
        quantity: line.quantity,
        base_unit_code: line.base_unit_code,
        unit_price_gross: line.unit_price_gross,
        face_value: line.face_value,
        card_count: line.card_count,
        transaction_amount: line.transaction_amount,
        card_form: line.card_form,
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entities::money::Amount;
    use entities::sales_order::{
        BusinessType, CloseStatus, CollectionProgress, CommercialStatus, FulfillmentProgress,
        InvoiceProgress, OriginSystem, ReviewStatus,
    };

    use super::{
        compute_can_start_sales_change, compute_close_eligibility, detail_owner_user_id,
        stage_code_label_tone,
    };

    #[test]
    fn detail_owner_prefers_working_copy_then_latest_submission_then_creator() {
        assert_eq!(
            detail_owner_user_id(Some("editor"), Some("submitter"), "creator"),
            "editor"
        );
        assert_eq!(
            detail_owner_user_id(None, Some("submitter"), "creator"),
            "submitter"
        );
        assert_eq!(detail_owner_user_id(None, None, "creator"), "creator");
    }

    #[test]
    fn stage_closed_overrides_everything() {
        let (code, label, tone) = stage_code_label_tone(
            CommercialStatus::Effective,
            ReviewStatus::Approved,
            CloseStatus::Closed,
            FulfillmentProgress::Completed,
        );
        assert_eq!((code, label, tone), ("closed", "已关闭", "void"));
    }

    #[test]
    fn stage_voided() {
        let (code, label, tone) = stage_code_label_tone(
            CommercialStatus::Voided,
            ReviewStatus::NotSubmitted,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::NotStarted,
        );
        assert_eq!((code, label, tone), ("voided", "已作废", "void"));
    }

    #[test]
    fn stage_effective_distinguishes_fulfilling() {
        let fulfilling = stage_code_label_tone(
            CommercialStatus::Effective,
            ReviewStatus::Approved,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::PartiallyFulfilled,
        );
        assert_eq!(fulfilling, ("fulfilling", "履约中", "info"));

        let effective = stage_code_label_tone(
            CommercialStatus::Effective,
            ReviewStatus::Approved,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::NotStarted,
        );
        assert_eq!(effective, ("effective", "已生效", "success"));
    }

    #[test]
    fn stage_pending_review_branches() {
        let cases = [
            (
                ReviewStatus::PendingProcurementConfirmation,
                ("awaiting_confirm", "待二次确认", "warning"),
            ),
            (
                ReviewStatus::PendingLowMarginSuperior,
                ("awaiting_sales", "待销售处理", "warning"),
            ),
            (
                ReviewStatus::Rejected,
                ("awaiting_sales", "待销售处理", "warning"),
            ),
            (
                ReviewStatus::PendingSalesLeader,
                ("awaiting_sales_lead", "待销售领导审批", "warning"),
            ),
            (
                ReviewStatus::PendingOperations,
                ("awaiting_ops", "待运营审批", "warning"),
            ),
            (ReviewStatus::Approved, ("effective", "已生效", "success")),
            (ReviewStatus::NotSubmitted, ("draft", "草稿", "neutral")),
        ];
        for (review, expected) in cases {
            let actual = stage_code_label_tone(
                CommercialStatus::PendingReview,
                review,
                CloseStatus::NotSatisfied,
                FulfillmentProgress::NotStarted,
            );
            assert_eq!(actual, expected, "review={review:?}");
        }
    }

    #[test]
    fn close_eligibility_closed_branch_is_always_eligible() {
        let view = compute_close_eligibility(
            BusinessType::GoodsService,
            CommercialStatus::Effective,
            CloseStatus::Closed,
            FulfillmentProgress::NotStarted,
            CollectionProgress::NotCollected,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("0").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert!(view.eligible_to_close);
        assert!(view.fulfillment_complete);
        assert!(view.receivable_settled);
        assert!(view.blockers.is_empty());
    }

    #[test]
    fn close_eligibility_voided_and_draft_block_with_reason() {
        let voided = compute_close_eligibility(
            BusinessType::GoodsService,
            CommercialStatus::Voided,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::NotStarted,
            CollectionProgress::NotCollected,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("0").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert!(!voided.eligible_to_close);
        assert_eq!(voided.blockers, vec!["本单已作废，不会再结案".to_string()]);

        let draft = compute_close_eligibility(
            BusinessType::GoodsService,
            CommercialStatus::Draft,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::NotStarted,
            CollectionProgress::NotCollected,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("0").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert!(!draft.eligible_to_close);
        assert_eq!(draft.blockers, vec!["草稿尚未生效，谈不上结案".to_string()]);
    }

    #[test]
    fn close_eligibility_goods_vs_voucher_blocker_text() {
        let goods = compute_close_eligibility(
            BusinessType::GoodsService,
            CommercialStatus::Effective,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::NotStarted,
            CollectionProgress::NotCollected,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("0").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert_eq!(goods.blockers[0], "客户验收还没做完");

        let voucher = compute_close_eligibility(
            BusinessType::Voucher,
            CommercialStatus::Effective,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::NotStarted,
            CollectionProgress::NotCollected,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("0").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert_eq!(
            voucher.blockers[0],
            "卡券还没到履约期限（持卡人是否消费完都不提前算交付完成）"
        );
    }

    #[test]
    fn close_eligibility_receivable_settled_falls_back_to_amount_comparison() {
        // collection_progress 还没被标记 SETTLED，但应收子账合计已收齐——
        // 与前端原实现的双重判断保持一致。
        let view = compute_close_eligibility(
            BusinessType::GoodsService,
            CommercialStatus::Effective,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::Completed,
            CollectionProgress::PartiallyCollected,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("100").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert!(view.receivable_settled);
        assert!(view.eligible_to_close);
    }

    #[test]
    fn close_eligibility_invoice_progress_never_blocks() {
        let view = compute_close_eligibility(
            BusinessType::GoodsService,
            CommercialStatus::Effective,
            CloseStatus::NotSatisfied,
            FulfillmentProgress::Completed,
            CollectionProgress::Settled,
            InvoiceProgress::NotInvoiced,
            Amount::from_str("100").unwrap(),
            Amount::from_str("100").unwrap(),
        );
        assert!(view.eligible_to_close);
        assert!(!view.invoice_complete);
    }

    #[test]
    fn can_start_sales_change_blocks_mall_origin() {
        let (allowed, reason) =
            compute_can_start_sales_change(OriginSystem::Mall, "effective", "已生效", false);
        assert!(!allowed);
        assert!(reason.unwrap().contains("商城"));
    }

    #[test]
    fn can_start_sales_change_blocks_terminal_and_pending_stages() {
        for code in ["draft", "voided", "closed"] {
            let (allowed, reason) = compute_can_start_sales_change(OriginSystem::Erp, code, "任意", false);
            assert!(!allowed, "stage={code}");
            assert!(reason.is_some());
        }
        for code in [
            "awaiting_sales",
            "awaiting_confirm",
            "awaiting_sales_lead",
            "awaiting_ops",
        ] {
            let (allowed, _) = compute_can_start_sales_change(OriginSystem::Erp, code, "任意", false);
            assert!(!allowed, "stage={code}");
        }
    }

    #[test]
    fn can_start_sales_change_blocks_active_change_order_else_allows() {
        let (allowed, reason) =
            compute_can_start_sales_change(OriginSystem::Erp, "effective", "已生效", true);
        assert!(!allowed);
        assert!(reason.unwrap().contains("处理中"));

        let (allowed, reason) =
            compute_can_start_sales_change(OriginSystem::Erp, "effective", "已生效", false);
        assert!(allowed);
        assert!(reason.is_none());
    }
}
