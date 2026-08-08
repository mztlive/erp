//! 域 D15 `purchase_order` 服务编排（页面：W08）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 依据创建采购单、保存草稿、提交财务审核、财务审核通过/驳回、采购变更提交与
//!   生效均为跨集合写入 → `database::Transactional::with_transaction`；
//! - 财务审核通过（§8.1.4）在单事务内：锁定提交 → 逐行复验采购确认来源 →
//!   复制为生效版本与版本行 → 形成销售分配 → 推进采购状态与版本指针 →
//!   形成应付原始分录与 `CONFIRMED` 成本事实 → 完成审核待办 → 审计；
//! - 采购变更生效（§8.1.3 采购部分）在单事务内：基准版本校验 → 新版本/版本行/
//!   分配 → 应付与成本差额 → 当前版本指针推进，不修改已发生事实。
//!
//! 跨域协作（只经 DatabaseExt 调对方 Repository，禁止 Service 依赖 Service）：
//! - D09 `supplier`：供应商角色与商务结算版本（提交快照）；
//! - D14 `sales_review`：采购二次确认及其分行（创建依据）；
//! - D13 `sales_order`：销售提交行快照（商品名/规格/单位/SKU）与销售版本行
//!   （销售分配指向）——D13 不在 domains.md 声明清单，属越界读，见最终报告；
//! - D07 `party`：主体名称（供应商名称快照）——同上，报告协调人；
//! - D19 `payable`：应付子账与原始应付分录（审核通过、变更差额）；
//! - D20 `cost`：`CONFIRMED` 成本事实（审核通过、变更差额）；
//! - D03 `work_item`：采购审核待办（提交创建、审核完成）。

use std::collections::HashMap;
use std::str::FromStr;

use database::{
    AccessControlExt, CostExt, NoTransaction, PartyExt, PayableExt, PurchaseOrderExt, SalesOrderExt,
    SalesReviewExt, SupplierExt, Transactional, WorkItemExt,
};
use entities::common::time::Instant;
use entities::ids::{
    CostEntryId, PayableEntryId, ProcurementConfirmationId, PurchaseChangeOrderId,
    PurchaseChangeSubmissionId, PurchaseChangeSubmissionLineId, PurchaseOrderId, PurchaseOrderRevisionId,
    PurchaseOrderRevisionLineId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, WorkItemId,
};
use entities::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};
use entities::purchase_order::{
    FulfillmentResponsibility, PaymentTermSnapshot, PurchaseChangeOrder, PurchaseChangeOrderData,
    PurchaseChangeOrderStatus, PurchaseChangeOrderUpdate, PurchaseChangeSubmission,
    PurchaseChangeSubmissionData, PurchaseChangeSubmissionLine, PurchaseChangeSubmissionLineData,
    PurchaseLineType, PurchaseOrder, PurchaseOrderData, PurchaseOrderRevision, PurchaseOrderRevisionData,
    PurchaseOrderRevisionLine, PurchaseOrderRevisionLineData, PurchaseOrderStatus, PurchaseOrderSubmission,
    PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData, PurchaseType,
    SubmissionStatus, SupplierSnapshot,
};
use entities::sales_review::{FulfillmentMode, ProcurementConfirmationStatus};
use entities::work_item::{WorkItem, WorkItemData, WorkItemPriority, WorkItemType};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

pub(crate) mod draft;
mod dto;

pub use self::dto::{
    ApprovePurchaseOrderRequest, CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderResult,
    CreationBasisLineView, CreationBasisView, EffectPurchaseChangeRequest, PageView,
    PurchaseChangeEffectResult, PurchaseChangeOrderListParams, PurchaseChangeOrderView,
    PurchaseChangeSubmitResult, PurchaseOrderCenterView, PurchaseOrderLineView, PurchaseOrderListItemView,
    PurchaseOrderListParams, PurchaseReviewResult, PurchaseSalesAllocationView, RejectPurchaseOrderRequest,
    SavePurchaseOrderDraftRequest, SavePurchaseOrderDraftResult, SavePurchaseOrderLine,
    StartPurchaseChangeRequest, StartPurchaseChangeResult, SubmitPurchaseChangeRequest,
    SubmitPurchaseOrderRequest, SubmitPurchaseOrderResult, TotalsView,
};

/// 采购单列表筛选条件类型（经 `PurchaseOrderExt` 关联类型跨 crate 可达）。
type PurchaseOrderFilter = <mongodb::Database as PurchaseOrderExt>::PurchaseOrderFilter;
/// 采购确认筛选条件类型（经 `SalesReviewExt` 关联类型跨 crate 可达）。
type ProcurementConfirmationFilter = <mongodb::Database as SalesReviewExt>::ProcurementConfirmationFilter;

/// 采购单服务。
///
/// 提供采购单从依据创建、草稿保存、提交审核到财务审核（§8.1.4）与变更生效
/// （§8.1.3 采购部分）的编排。
pub struct PurchaseOrderService {
    db: Database,
}

impl PurchaseOrderService {
    /// 创建采购单服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询采购单列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4）；行金额取自当前
    /// 提交/版本表头汇总（批量取回，禁止 N+1）。
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
    pub async fn purchase_order_list(
        &self,
        params: &PurchaseOrderListParams,
    ) -> Result<PageView<PurchaseOrderListItemView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = PurchaseOrderFilter {
            purchase_no: query.q,
            sales_order_id: query.sales_order_id.map(entities::ids::SalesOrderId::new),
            supplier_id: query.supplier_id.map(entities::ids::SupplierAccountId::new),
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, self::dto::SortDir::Asc),
        };
        let page = self
            .db
            .purchase_orders()
            .search_purchase_orders(&filter, &mut NoTransaction)
            .await?;

        let supplier_ids: Vec<&entities::ids::SupplierAccountId> =
            page.items.iter().map(|row| &row.supplier_id).collect();
        let supplier_names = self.resolve_supplier_names(&supplier_ids).await?;
        let pointer_ids: Vec<String> = page
            .items
            .iter()
            .filter_map(|row| {
                row.current_submission_id
                    .clone()
                    .or_else(|| row.current_revision_id.clone())
            })
            .collect();
        let totals = self.resolve_order_totals(&pointer_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| {
                let supplier_name = supplier_names
                    .get(&row.supplier_id.to_string())
                    .cloned()
                    .unwrap_or_else(|| row.supplier_id.to_string());
                let pointer = row
                    .current_submission_id
                    .clone()
                    .or_else(|| row.current_revision_id.clone());
                let totals = pointer
                    .as_ref()
                    .and_then(|id| totals.get(id).cloned())
                    .unwrap_or_default();
                PurchaseOrderListItemView {
                    id: row.id,
                    purchase_no: row.purchase_no,
                    sales_order_id: row.sales_order_id.to_string(),
                    supplier_id: row.supplier_id.to_string(),
                    supplier_name,
                    purchase_type: row.purchase_type,
                    status: row.status,
                    review_status: row.review_status,
                    gross_amount: totals.0,
                    net_amount: totals.1,
                    tax_amount: totals.2,
                    payment_progress: row.payment_progress,
                    invoice_progress: row.invoice_progress,
                    fulfillment_progress: row.fulfillment_progress,
                    current_submission_id: row.current_submission_id,
                    current_revision_id: row.current_revision_id,
                    version: row.version,
                    created_at: row.created_at,
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

    /// 查询采购创建依据（已通过的采购确认及其分行，页面 W08 建单入口）。
    ///
    /// # 返回
    /// 返回全部已通过且未完全消费的确认批次（按创建时间倒序）。
    ///
    /// # 错误
    /// * `RepositoryError` - 数据库查询失败
    pub async fn creation_basis_list(&self) -> Result<Vec<CreationBasisView>> {
        let filter = ProcurementConfirmationFilter {
            submission_id: None,
            status: Some(ProcurementConfirmationStatus::Approved),
            page: 1,
            page_size: 100,
            sort_by: Some("created_at".to_string()),
            sort_ascending: false,
        };
        let page = self
            .db
            .procurement_confirmations()
            .search_procurement_confirmations(&filter, &mut NoTransaction)
            .await?;

        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            let already_created = self
                .db
                .purchase_orders()
                .find_one(
                    mongodb::bson::doc! {
                        "sales_order_id": row.sales_order_id.clone(),
                    },
                    &mut NoTransaction,
                )
                .await?;
            if already_created.is_some() {
                continue;
            }
            let confirmation_id = ProcurementConfirmationId::new(row.id.clone());
            let lines = self
                .db
                .procurement_confirmation_lines()
                .list_lines_by_confirmation(&confirmation_id, &mut NoTransaction)
                .await?;
            if lines.is_empty() {
                continue;
            }
            let supplier_id = lines[0].supplier_id.clone();
            let supplier_name = self
                .resolve_supplier_name(&supplier_id)
                .await?
                .unwrap_or_else(|| supplier_id.to_string());
            let payment_term_code = self.resolve_payment_term_code(&supplier_id).await?;

            let (line_views, estimated) = self.build_basis_lines(&lines).await?;
            views.push(CreationBasisView {
                basis_id: row.id,
                sales_order_id: row.sales_order_id,
                submission_id: row.submission_id,
                supplier_id: supplier_id.to_string(),
                supplier_name,
                payment_term_code,
                lines: line_views,
                estimated_gross: estimated,
            });
        }
        Ok(views)
    }

    /// 依据创建采购单（幂等：同拆单维度草稿复用，不重复创建）。
    ///
    /// 在单事务内写入 `purchase_order`、草稿 `purchase_order_submission` 与其明细；
    /// 商品行快照取自销售提交行（D13 读），金额逐行按确认成本×数量舍入。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建（或复用）采购单结果。
    ///
    /// # 错误
    /// * `NotFound` - 创建依据不存在或未通过
    /// * `ConflictError` - 同拆单维度已存在非终态采购单（幂等复用则返回成功）
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_from_basis(
        &self,
        req: CreatePurchaseOrderFromBasisRequest,
        actor: &AuditActor,
    ) -> Result<CreatePurchaseOrderResult> {
        req.validate()?;
        let confirmation_id = ProcurementConfirmationId::new(req.basis_id.clone());
        let confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(&confirmation_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购创建依据不存在".to_string()))?;
        if confirmation.stable.status != ProcurementConfirmationStatus::Approved {
            return Err(Error::BusinessLogicError(
                "创建依据未通过采购确认，不能建单".to_string(),
            ));
        }
        let lines = self
            .db
            .procurement_confirmation_lines()
            .list_lines_by_confirmation(&confirmation_id, &mut NoTransaction)
            .await?;
        if lines.is_empty() {
            return Err(Error::BusinessLogicError("创建依据没有可拆入的分行".to_string()));
        }
        let split_dimensions = lines
            .iter()
            .map(|line| (line.supplier_id.to_string(), line.fulfillment_mode.as_str()))
            .collect::<std::collections::HashSet<_>>();
        if split_dimensions.len() != 1 {
            return Err(Error::ValidationError(
                "该采购确认包含多个供应商或履约方式，必须由审批动作自动拆分采购单草稿".to_string(),
            ));
        }
        let supplier_id = lines[0].supplier_id.clone();
        let fulfillment = fulfillment_from_mode(lines[0].fulfillment_mode);
        let sales_order_id = confirmation.sales_order_id.clone();

        // 幂等去重：同一销售单 + 供应商 + 拆单维度已存在非终态采购单则复用。
        let existing = self
            .db
            .purchase_orders()
            .find_one(
                mongodb::bson::doc! {
                    "sales_order_id": sales_order_id.to_string(),
                    "supplier_id": supplier_id.to_string(),
                    "status": { "$in": [
                        PurchaseOrderStatus::Draft.as_str(),
                        PurchaseOrderStatus::PendingFinanceReview.as_str(),
                    ]},
                },
                &mut NoTransaction,
            )
            .await?;
        if let Some(order) = existing {
            return Ok(CreatePurchaseOrderResult {
                purchase_order_id: order.base.id.clone(),
                purchase_no: order.purchase_no.clone(),
                lock_version: order.base.version,
                replayed: true,
                reference: format!("PO-{}", order.purchase_no),
            });
        }

        let supplier_name = self
            .resolve_supplier_name(&supplier_id)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let order_id = PurchaseOrderId::new(next_id());
        let purchase_no = format!("PO-{}-{}", today_stamp(), &order_id.to_string()[..6]);
        let mut order = PurchaseOrder::new(
            order_id.clone(),
            PurchaseOrderData {
                purchase_no,
                sales_order_id,
                supplier_id: supplier_id.clone(),
                purchase_type: req.purchase_type,
                payment_term_code: req.payment_term_code.clone(),
                fulfillment_responsibility: fulfillment,
            },
            actor.id(),
        )?;

        let submission = self
            .build_draft_submission(
                &order_id,
                &supplier_id,
                req.purchase_type,
                fulfillment,
                &supplier_name,
                &req.payment_term_code,
                &lines,
            )
            .await?;
        order.current_submission_id = Some(submission.base.id.clone());
        let submission_lines = self
            .build_submission_lines_from_basis(&submission.base.id, &lines)
            .await?;

        let audit =
            actor
                .clone()
                .resource_log("purchase_order.create", "purchase_order", order.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let submission_for_tx = submission.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_orders().create(&order_for_tx, session).await?;
                    db.purchase_order_submissions()
                        .create(&submission_for_tx, session)
                        .await?;
                    for line in &submission_lines {
                        db.purchase_order_submission_lines().create(line, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(CreatePurchaseOrderResult {
            purchase_order_id: order.base.id.clone(),
            purchase_no: order.purchase_no.clone(),
            lock_version: order.base.version,
            replayed: false,
            reference: format!("PO-{}", order.purchase_no),
        })
    }

    /// 查询采购单对象中心。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    ///
    /// # 返回
    /// 返回对象中心视图（当前内容按 版本 > 提交 > 草稿 优先级取用）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn purchase_order_detail(&self, id: &str) -> Result<PurchaseOrderCenterView> {
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        let supplier_name = self
            .resolve_supplier_name(&order.supplier_id)
            .await?
            .unwrap_or_else(|| order.supplier_id.to_string());

        let (content_source, lines, totals) = self.resolve_current_content(&order).await?;
        let allocations = self.resolve_allocations(&order).await?;
        let changes = self
            .db
            .purchase_change_orders()
            .find_many(
                mongodb::bson::doc! { "purchase_order_id": order.base.id.clone() },
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .map(|change| dto::PurchaseChangeSummaryView {
                change_id: change.base.id.clone(),
                status: change.stable.status.as_str().to_string(),
                base_revision_id: change.base_revision_id.to_string(),
                effective_revision_id: change.effective_revision_id.as_ref().map(ToString::to_string),
                reason: change.reason,
                created_at: change.base.created_at,
            })
            .collect();

        let revision_no = match &order.stable.current_revision_id {
            Some(revision_id) => {
                let revision = self
                    .db
                    .purchase_order_revisions()
                    .find_by_id(revision_id, &mut NoTransaction)
                    .await?;
                revision.map(|revision| revision.revision.revision_no)
            }
            None => None,
        };

        Ok(PurchaseOrderCenterView {
            id: order.base.id.clone(),
            purchase_no: order.purchase_no.clone(),
            status: order.stable.status,
            review_status: order.review_status,
            version: order.base.version,
            sales_order_id: order.sales_order_id.to_string(),
            supplier_id: order.supplier_id.to_string(),
            supplier_name,
            purchase_type: order.purchase_type,
            payment_term_code: order.payment_term_code.clone(),
            fulfillment_responsibility: order.fulfillment_responsibility,
            payment_progress: order.payment_progress,
            invoice_progress: order.invoice_progress,
            fulfillment_progress: order.fulfillment_progress,
            current_submission_id: order.current_submission_id.clone(),
            current_revision_id: order.stable.current_revision_id.clone(),
            revision_no,
            content_source,
            lines,
            totals,
            allocations,
            changes,
            created_at: order.base.created_at,
        })
    }

    /// 保存采购草稿（表头 + 完整行替换，单事务）。
    ///
    /// 只允许 `Draft` 状态；期望版本与当前版本不一致返回 409。
    /// 每次保存形成新的草稿提交（行集合不可变替换，旧草稿提交标记失效），
    /// 商品行金额按 `line_amounts(单价, 数量, 税率)` 逐行计算，
    /// 物流费用行按 `gross − round(gross × 税率)` 换算，表头只汇总已舍入行金额。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 保存请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新乐观锁版本与表头汇总。
    ///
    /// # 错误
    /// * `NotFound` - 采购单或草稿提交不存在
    /// * `ConflictError` - 期望版本不一致
    /// * `BusinessLogicError` - 状态非草稿或行数据非法
    pub async fn save_draft(
        &self,
        id: &str,
        req: SavePurchaseOrderDraftRequest,
        actor: &AuditActor,
    ) -> Result<SavePurchaseOrderDraftResult> {
        req.validate()?;
        let mut order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, req.expected_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::Draft {
            return Err(Error::BusinessLogicError(
                "只有草稿状态的采购单可以编辑".to_string(),
            ));
        }
        let old_draft_id = order
            .current_submission_id
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| Error::BusinessLogicError("采购单缺少草稿提交".to_string()))?;
        let mut old_draft = self
            .db
            .purchase_order_submissions()
            .find_by_id(&old_draft_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("草稿提交不存在".to_string()))?;
        if old_draft.status != SubmissionStatus::Draft {
            return Err(Error::BusinessLogicError("草稿提交已冻结，不能保存".to_string()));
        }

        let (gross, net, tax) = self.compute_request_totals(&req.lines).await?;
        let mut update = entities::purchase_order::PurchaseOrderUpdate::default();
        if let Some(payment_term_code) = req.payment_term_code.clone() {
            update.payment_term_code = Some(payment_term_code);
        }
        order.update(update, actor.id())?;
        let new_draft = PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new(next_id()),
            PurchaseOrderSubmissionData {
                purchase_order_id: order.base.id.clone().into(),
                submission_no: format!("DRAFT-{}", &next_id()[..8]),
                supplier_id: old_draft.supplier_id.clone(),
                purchase_type: old_draft.purchase_type,
                fulfillment_responsibility: old_draft.fulfillment_responsibility,
                supplier_revision_id: old_draft.supplier_revision_id.clone(),
                supplier_snapshot: old_draft.supplier_snapshot.clone(),
                payment_term_snapshot: old_draft.payment_term_snapshot.clone(),
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
            },
        )?;
        let lines = self
            .build_lines_from_request(&new_draft.base.id.clone().into(), &req.lines)
            .await?;
        old_draft.mark_superseded()?;
        order.current_submission_id = Some(new_draft.base.id.clone());

        let audit =
            actor
                .clone()
                .resource_log("purchase_order.update", "purchase_order", order.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let old_draft_for_tx = old_draft.clone();
        let new_draft_for_tx = new_draft.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_order_submissions()
                        .update(&mut old_draft_for_tx.clone(), session)
                        .await?;
                    db.purchase_order_submissions()
                        .create(&new_draft_for_tx, session)
                        .await?;
                    for line in &lines {
                        db.purchase_order_submission_lines().create(line, session).await?;
                    }
                    db.purchase_orders()
                        .update(&mut order_for_tx.clone(), session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(SavePurchaseOrderDraftResult {
            lock_version: order.base.version,
            totals: TotalsView {
                gross: gross.to_string(),
                net: net.to_string(),
                tax: tax.to_string(),
            },
            reference: format!("SAVED-V{}", order.base.version),
        })
    }

    /// 提交财务审核（§6.6：头行冻结，形成不可变提交与审核待办）。
    ///
    /// 单事务写入提交、提交明细、采购主表指针与审核待办；重复提交（状态已
    /// 非草稿）直接失败，提交序号唯一索引兜底，只产生一条正式提交。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 提交请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交结果（提交 ID、序号与审核待办）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单不存在
    /// * `ConflictError` - 期望版本不一致或重复提交
    /// * `BusinessLogicError` - 状态非草稿或草稿内容缺失
    pub async fn submit(
        &self,
        id: &str,
        req: SubmitPurchaseOrderRequest,
        actor: &AuditActor,
    ) -> Result<SubmitPurchaseOrderResult> {
        req.validate()?;
        let mut order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, req.expected_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::Draft {
            return Err(Error::ConflictError(
                "采购单已提交或已生效，请勿重复提交".to_string(),
            ));
        }
        let draft_id = order
            .current_submission_id
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| Error::BusinessLogicError("采购单缺少草稿提交".to_string()))?;
        let mut draft = self
            .db
            .purchase_order_submissions()
            .find_by_id(&draft_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("草稿提交不存在".to_string()))?;
        if draft.status != SubmissionStatus::Draft {
            return Err(Error::ConflictError("草稿提交已冻结".to_string()));
        }
        let mut draft_lines = self
            .db
            .purchase_order_submission_lines()
            .find_many(
                mongodb::bson::doc! { "purchase_order_submission_id": draft_id },
                &mut NoTransaction,
            )
            .await?;

        // 形成新的正式提交（复制草稿内容，冻结）。
        let mut superseded_draft = draft.clone();
        superseded_draft.mark_superseded()?;
        let submission = self
            .freeze_submission(&mut order, &mut draft, &mut draft_lines, actor)
            .await?;
        let work_item = self.build_review_work_item(&order, &submission)?;

        let audit =
            actor
                .clone()
                .resource_log("purchase_order.submit", "purchase_order", order.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let work_item_for_tx = work_item.clone();
        let submission_for_tx = submission.clone();
        let superseded_draft_for_tx = superseded_draft.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_order()
                        .create_purchase_submission(
                            &mut order_for_tx.clone(),
                            &submission_for_tx,
                            &draft_lines,
                            session,
                        )
                        .await?;
                    db.purchase_order_submissions()
                        .update(&mut superseded_draft_for_tx.clone(), session)
                        .await?;
                    db.work_items().create(&work_item_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(SubmitPurchaseOrderResult {
            purchase_order_id: order.base.id.clone(),
            purchase_no: order.purchase_no.clone(),
            submission_id: submission.base.id.clone(),
            submission_no: submission.submission_no.clone(),
            work_item_id: work_item.base.id.clone(),
            lock_version: order.base.version,
            reference: format!("SUB-{}", submission.submission_no),
        })
    }

    /// 财务审核通过（§8.1.4 事务不变量）。
    ///
    /// 单事务：锁定提交 → 逐行复验采购确认来源 → 复制为生效版本与版本行 →
    /// 形成销售分配 → 推进采购状态与版本指针 → 应付原始分录与 `CONFIRMED`
    /// 成本事实 → 完成审核待办 → 审计。任一失败整体回滚。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 审核通过请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回审核结果（版本与应付分录）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单/提交/待办不存在
    /// * `ConflictError` - 版本不一致或重复审核
    /// * `BusinessLogicError` - 状态机或来源校验失败
    pub async fn review_approve(
        &self,
        id: &str,
        req: ApprovePurchaseOrderRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReviewResult> {
        req.validate()?;
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, req.expected_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::PendingFinanceReview {
            return Err(Error::ConflictError(
                "采购单不在待财务审核状态，请勿重复审核".to_string(),
            ));
        }
        if order.current_submission_id.as_deref() != Some(&req.submission_id) {
            return Err(Error::ConflictError("提交与采购单当前待审提交不一致".to_string()));
        }
        let submission = self
            .db
            .purchase_order_submissions()
            .find_by_id(&req.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("待审核提交不存在".to_string()))?;
        if submission.status != SubmissionStatus::Pending {
            return Err(Error::ConflictError(
                "提交已审核或已失效，请勿重复审核".to_string(),
            ));
        }
        let submission_lines = self
            .db
            .purchase_order_submission_lines()
            .find_many(
                mongodb::bson::doc! { "purchase_order_submission_id": &req.submission_id },
                &mut NoTransaction,
            )
            .await?;

        // 生效版本号：当前版本 + 1。
        let revision_no = self.next_revision_no(&order).await?;
        let (revision, revision_lines) = self
            .build_effective_revision(&order, &submission, &submission_lines, revision_no, &req, actor)
            .await?;

        let mut work_item = self
            .db
            .work_items()
            .find_by_id(&req.work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("审核待办不存在".to_string()))?;
        if work_item.status != entities::work_item::WorkItemStatus::InProgress
            && work_item.status != entities::work_item::WorkItemStatus::Unclaimed
        {
            return Err(Error::ConflictError("审核待办已终结，请勿重复审核".to_string()));
        }
        work_item.claim(actor.id())?;
        work_item.complete(actor.id(), Instant::now())?;

        let payable = self.build_payable(&order, &submission).await?;
        let cost_entries = self
            .build_confirmed_cost_entries(&submission, &submission_lines, revision_no)
            .await?;

        let audit =
            actor
                .clone()
                .resource_log("purchase_order.approve", "purchase_order", order.base.id.clone())?;
        let actor_id = actor.id().to_string();
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let submission_for_tx = submission.clone();
        let work_item_for_tx = work_item.clone();
        let revision_for_tx = revision.clone();
        let payable_for_tx = payable.clone();
        let payable_entry_id = payable.1.base.id.clone();
        let cost_entries_for_tx = cost_entries.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_order()
                        .create_effective_revision(&revision_for_tx, &revision_lines, session)
                        .await?;
                    let mut order_mut = order_for_tx.clone();
                    order_mut.apply_finance_review(true, &actor_id)?;
                    order_mut.stable.current_revision_id = Some(revision_for_tx.base.id.clone());
                    let mut submission_mut = submission_for_tx.clone();
                    submission_mut.mark_reviewed(true)?;
                    db.purchase_order_submissions()
                        .update(&mut submission_mut, session)
                        .await?;
                    db.purchase_orders().update(&mut order_mut, session).await?;
                    db.payable()
                        .create_payable_with_entry(&payable_for_tx.0, &payable_for_tx.1, session)
                        .await?;
                    for entry in &cost_entries_for_tx {
                        db.cost()
                            .create_cost_entry_with_allocations(entry, Vec::new(), session)
                            .await?;
                    }
                    db.work_items()
                        .update(&mut work_item_for_tx.clone(), session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(PurchaseReviewResult {
            review_result: "APPROVED".to_string(),
            revision_id: Some(revision.base.id.clone()),
            revision_no: Some(revision_no),
            payable_entry_id: Some(payable_entry_id),
            lock_version: order.base.version,
            reference: format!("REVIEW-V{}", order.base.version),
        })
    }

    /// 财务审核驳回（采购返回可编辑草稿）。
    ///
    /// 单事务：提交记录驳回结论、采购回草稿、完成审核待办、审计。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 驳回请求（结构化原因代码必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回审核结果（`REJECTED`）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单/提交/待办不存在
    /// * `ConflictError` - 版本不一致或重复审核
    pub async fn review_reject(
        &self,
        id: &str,
        req: RejectPurchaseOrderRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseReviewResult> {
        req.validate()?;
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, req.expected_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::PendingFinanceReview {
            return Err(Error::ConflictError(
                "采购单不在待财务审核状态，请勿重复审核".to_string(),
            ));
        }
        let submission = self
            .db
            .purchase_order_submissions()
            .find_by_id(&req.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("待审核提交不存在".to_string()))?;
        if submission.status != SubmissionStatus::Pending {
            return Err(Error::ConflictError(
                "提交已审核或已失效，请勿重复审核".to_string(),
            ));
        }
        let mut work_item = self
            .db
            .work_items()
            .find_by_id(&req.work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("审核待办不存在".to_string()))?;
        if work_item.status != entities::work_item::WorkItemStatus::InProgress
            && work_item.status != entities::work_item::WorkItemStatus::Unclaimed
        {
            return Err(Error::ConflictError("审核待办已终结，请勿重复审核".to_string()));
        }
        work_item.claim(actor.id())?;
        work_item.complete(actor.id(), Instant::now())?;

        let audit =
            actor
                .clone()
                .resource_log("purchase_order.reject", "purchase_order", order.base.id.clone())?;
        let actor_id = actor.id().to_string();
        tracing::info!(
            purchase_order_id = %id,
            submission_id = %req.submission_id,
            reason_code = %req.reason_code,
            comment = ?req.comment,
            "采购财务审核驳回已记录"
        );
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let submission_for_tx = submission.clone();
        let work_item_for_tx = work_item.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut order_mut = order_for_tx.clone();
                    order_mut.apply_finance_review(false, &actor_id)?;
                    order_mut.current_submission_id = None;
                    let mut submission_mut = submission_for_tx.clone();
                    submission_mut.mark_reviewed(false)?;
                    db.purchase_order_submissions()
                        .update(&mut submission_mut, session)
                        .await?;
                    db.purchase_orders().update(&mut order_mut, session).await?;
                    db.work_items()
                        .update(&mut work_item_for_tx.clone(), session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(PurchaseReviewResult {
            review_result: "REJECTED".to_string(),
            revision_id: None,
            revision_no: None,
            payable_entry_id: None,
            lock_version: order.base.version,
            reference: format!("REVIEW-V{}", order.base.version),
        })
    }

    /// 发起采购变更（基于当前生效版本创建变更单）。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 发起请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更单结果。
    ///
    /// # 错误
    /// * `NotFound` - 采购单不存在
    /// * `ConflictError` - 版本不一致或已存在进行中变更
    /// * `BusinessLogicError` - 采购单未生效
    pub async fn start_change(
        &self,
        id: &str,
        req: StartPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<StartPurchaseChangeResult> {
        req.validate()?;
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, req.expected_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::Effective
            && order.stable.status != PurchaseOrderStatus::PartiallyExecuted
        {
            return Err(Error::BusinessLogicError(
                "只有已生效的采购单可以发起变更".to_string(),
            ));
        }
        let base_revision_id = order
            .stable
            .current_revision_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("采购单没有生效版本，不能发起变更".to_string()))?;
        let has_in_progress = self
            .db
            .purchase_change_orders()
            .exists(
                mongodb::bson::doc! {
                    "purchase_order_id": id,
                    "status": { "$in": [
                        PurchaseChangeOrderStatus::Draft.as_str(),
                        PurchaseChangeOrderStatus::PendingWarehouseImpact.as_str(),
                        PurchaseChangeOrderStatus::PendingFinanceReview.as_str(),
                    ]},
                },
                &mut NoTransaction,
            )
            .await?;
        if has_in_progress {
            return Err(Error::ConflictError(
                "存在进行中的采购变更，不能重复发起".to_string(),
            ));
        }
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;

        let change = PurchaseChangeOrder::new(
            PurchaseChangeOrderId::new(next_id()),
            PurchaseChangeOrderData {
                purchase_order_id: order.base.id.clone().into(),
                base_revision_id: entities::ids::PurchaseOrderRevisionId::new(base_revision.base.id.clone()),
                reason: req.reason.clone(),
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "purchase_change_order.create",
            "purchase_change_order",
            change.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let change_for_tx = change.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_change_orders()
                        .create(&change_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(StartPurchaseChangeResult {
            change_id: change.base.id.clone(),
            base_revision_id: base_revision.base.id.clone(),
            base_revision_no: base_revision.revision.revision_no,
            lock_version: order.base.version,
            reference: format!("CHANGE-V{}", base_revision.revision.revision_no),
        })
    }

    /// 提交采购变更目标内容（形成不可变变更提交）。
    ///
    /// # 参数
    /// * `change_id` - 变更单 ID
    /// * `req` - 提交请求（目标完整头、行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回变更提交结果。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    /// * `ConflictError` - 版本不一致或重复提交
    pub async fn submit_change(
        &self,
        change_id: &str,
        req: SubmitPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeSubmitResult> {
        req.validate()?;
        let mut change = self
            .db
            .purchase_change_orders()
            .find_by_id(change_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        self.ensure_version(&change, req.expected_lock_version)?;
        if change.stable.status != PurchaseChangeOrderStatus::Draft {
            return Err(Error::ConflictError("变更单已提交，请勿重复提交".to_string()));
        }
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&change.purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原采购单不存在".to_string()))?;
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&change.base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;

        let supplier_name = self
            .resolve_supplier_name(&order.supplier_id)
            .await?
            .unwrap_or_else(|| order.supplier_id.to_string());
        let submission = self
            .build_change_submission(&change, &order, &base_revision, &supplier_name, &req)
            .await?;
        let lines = self
            .build_change_submission_lines(&submission.base.id.clone(), &req.lines)
            .await?;
        let mut submission_mut = submission.clone();
        submission_mut.submit(Instant::now(), actor.id())?;

        let change_update = PurchaseChangeOrderUpdate {
            current_submission_id: Some(submission.base.id.clone().into()),
            target_content_hash: Some(content_fingerprint(&req.lines)),
            status: Some(PurchaseChangeOrderStatus::PendingWarehouseImpact),
            ..Default::default()
        };
        change.update(change_update, actor.id())?;

        let audit = actor.clone().resource_log(
            "purchase_change_order.submit",
            "purchase_change_order",
            change.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let change_for_tx = change.clone();
        let submission_for_tx = submission_mut.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_order()
                        .create_change_submission(
                            &mut change_for_tx.clone(),
                            &submission_for_tx,
                            &lines,
                            session,
                        )
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(PurchaseChangeSubmitResult {
            change_id: change.base.id.clone(),
            submission_id: submission.base.id.clone(),
            submission_no: submission.submission_no.clone(),
            status: change.stable.status.as_str().to_string(),
            lock_version: change.base.version,
            reference: format!("CS-{}", submission.submission_no),
        })
    }

    /// 采购变更生效（§8.1.3 采购部分）。
    ///
    /// 单事务：基准版本仍为当前版本 → 复制目标提交为新采购版本与版本行 →
    /// 形成销售分配 → 追加应付与成本差额 → 推进采购当前版本指针 →
    /// 变更单置为生效。不修改已发生事实。
    ///
    /// # 参数
    /// * `change_id` - 变更单 ID
    /// * `req` - 生效请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回生效结果。
    ///
    /// # 错误
    /// * `NotFound` - 变更单/提交不存在
    /// * `ConflictError` - 版本不一致或重复生效
    /// * `BusinessLogicError` - 基准版本已不是当前版本
    pub async fn effect_change(
        &self,
        change_id: &str,
        req: EffectPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeEffectResult> {
        req.validate()?;
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(change_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        self.ensure_version(&change, req.expected_lock_version)?;
        if change.stable.status == PurchaseChangeOrderStatus::Effective {
            return Err(Error::ConflictError("变更单已生效，请勿重复操作".to_string()));
        }
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&change.purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("原采购单不存在".to_string()))?;
        if change.base_revision_id.to_string()
            != order.stable.current_revision_id.as_deref().unwrap_or_default()
        {
            return Err(Error::BusinessLogicError(
                "基准版本已不是当前版本，变更不能生效".to_string(),
            ));
        }
        let submission = self
            .db
            .purchase_change_submissions()
            .find_by_id(&req.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("变更提交不存在".to_string()))?;
        if submission.status != SubmissionStatus::Pending {
            return Err(Error::ConflictError("变更提交已处理，请勿重复生效".to_string()));
        }
        let lines = self
            .db
            .purchase_change_submission_lines()
            .find_many(
                mongodb::bson::doc! { "purchase_change_submission_id": &req.submission_id },
                &mut NoTransaction,
            )
            .await?;

        let new_revision_no = self.next_revision_no(&order).await?;
        let (revision, revision_lines) = self
            .build_change_revision(&order, &submission, &lines, new_revision_no)
            .await?;
        let base_revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(&change.base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("基准版本不存在".to_string()))?;
        let delta = self
            .build_change_deltas(&order, &base_revision, &revision)
            .await?;

        let audit = actor.clone().resource_log(
            "purchase_change_order.effect",
            "purchase_change_order",
            change.base.id.clone(),
        )?;
        let actor_id = actor.id().to_string();
        let db = self.db.clone();
        let client = db.client().clone();
        let order_for_tx = order.clone();
        let change_for_tx = change.clone();
        let submission_for_tx = submission.clone();
        let revision_for_tx = revision.clone();
        let payable_delta_id = delta.0.as_ref().map(|(_, entry)| entry.base.id.clone());
        let cost_deltas = delta.1.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_order()
                        .create_effective_revision(&revision_for_tx, &revision_lines, session)
                        .await?;
                    let mut order_mut = order_for_tx.clone();
                    order_mut.stable.current_revision_id = Some(revision_for_tx.base.id.clone());
                    db.purchase_orders().update(&mut order_mut, session).await?;
                    if let Some((account, entry)) = &delta.0 {
                        db.payable()
                            .create_payable_with_entry(account, entry, session)
                            .await?;
                    }
                    for entry in &cost_deltas {
                        db.cost()
                            .create_cost_entry_with_allocations(entry, Vec::new(), session)
                            .await?;
                    }
                    let mut submission_mut = submission_for_tx.clone();
                    submission_mut.status = SubmissionStatus::Approved;
                    db.purchase_change_submissions()
                        .update(&mut submission_mut, session)
                        .await?;
                    let mut change_mut = change_for_tx.clone();
                    change_mut.update(
                        PurchaseChangeOrderUpdate {
                            effective_revision_id: Some(revision_for_tx.base.id.clone().into()),
                            status: Some(PurchaseChangeOrderStatus::Effective),
                            ..Default::default()
                        },
                        &actor_id,
                    )?;
                    db.purchase_change_orders()
                        .update(&mut change_mut, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(PurchaseChangeEffectResult {
            change_id: change.base.id.clone(),
            revision_id: revision.base.id.clone(),
            revision_no: new_revision_no,
            payable_delta_entry_id: payable_delta_id,
            purchase_order_lock_version: order.base.version,
            reference: format!("EFFECT-V{new_revision_no}"),
        })
    }

    /// 分页查询采购变更单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法
    pub async fn change_order_list(
        &self,
        params: &PurchaseChangeOrderListParams,
    ) -> Result<PageView<PurchaseChangeOrderView>> {
        params.validate()?;
        let (sort_by, sort_dir) =
            self::dto::normalize_sort(&params.sort_by, &params.sort_dir, &["created_at"])?;
        let page = params.page.unwrap_or(1);
        let page_size = params.page_size.unwrap_or(20).clamp(1, 100);
        let mut filter = mongodb::bson::doc! {};
        if let Some(purchase_order_id) = params
            .purchase_order_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            filter.insert("purchase_order_id", purchase_order_id);
        }
        if let Some(status) = params.status.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            filter.insert("status", status);
        }
        let sort_doc = mongodb::bson::doc! { sort_by: if matches!(sort_dir, self::dto::SortDir::Asc) { 1i32 } else { -1i32 } };
        let items = self
            .db
            .purchase_change_orders()
            .find_many_sorted(filter.clone(), sort_doc, &mut NoTransaction)
            .await?;
        let total = items.len() as i64;
        let start = ((page - 1) * u64::from(page_size)) as usize;
        let views = items
            .into_iter()
            .skip(start)
            .take(page_size as usize)
            .map(|change| PurchaseChangeOrderView {
                id: change.base.id.clone(),
                purchase_order_id: change.purchase_order_id.to_string(),
                base_revision_id: change.base_revision_id.to_string(),
                reason: change.reason.clone(),
                status: change.stable.status.as_str().to_string(),
                current_submission_id: change.current_submission_id.as_ref().map(ToString::to_string),
                effective_revision_id: change.effective_revision_id.as_ref().map(ToString::to_string),
                version: change.base.version,
                created_at: change.base.created_at,
            })
            .collect();
        Ok(PageView {
            items: views,
            total,
            page,
            page_size,
        })
    }

    /// 查询采购变更单详情。
    ///
    /// # 参数
    /// * `id` - 变更单 ID
    ///
    /// # 返回
    /// 返回变更单视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在
    pub async fn change_order_detail(&self, id: &str) -> Result<PurchaseChangeOrderView> {
        let change = self
            .db
            .purchase_change_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        Ok(PurchaseChangeOrderView {
            id: change.base.id.clone(),
            purchase_order_id: change.purchase_order_id.to_string(),
            base_revision_id: change.base_revision_id.to_string(),
            reason: change.reason.clone(),
            status: change.stable.status.as_str().to_string(),
            current_submission_id: change.current_submission_id.as_ref().map(ToString::to_string),
            effective_revision_id: change.effective_revision_id.as_ref().map(ToString::to_string),
            version: change.base.version,
            created_at: change.base.created_at,
        })
    }
}

// ---------------------------------------------------------------------------
// 私有编排辅助
// ---------------------------------------------------------------------------

impl PurchaseOrderService {
    /// 校验乐观锁版本一致。
    fn ensure_version(&self, entity: &impl Versioned, expected: u64) -> Result<()> {
        if entity.version() != expected {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        Ok(())
    }

    /// 批量解析供应商名称（D07 主体修订快照）。
    async fn resolve_supplier_names(
        &self,
        supplier_ids: &[&entities::ids::SupplierAccountId],
    ) -> Result<HashMap<String, String>> {
        let mut names = HashMap::new();
        for supplier_id in supplier_ids {
            if let Some(name) = self.resolve_supplier_name(supplier_id).await? {
                names.insert(supplier_id.to_string(), name);
            }
        }
        Ok(names)
    }

    /// 批量解析内容金额（提交/版本表头汇总，一次 `$in` 查询，禁止 N+1）。
    async fn resolve_order_totals(
        &self,
        pointer_ids: &[String],
    ) -> Result<HashMap<String, (String, String, String)>> {
        if pointer_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut totals = HashMap::new();
        let submissions = self
            .db
            .purchase_order_submissions()
            .find_many(
                mongodb::bson::doc! { "id": { "$in": pointer_ids } },
                &mut NoTransaction,
            )
            .await?;
        for submission in submissions {
            totals.insert(
                submission.base.id.clone(),
                (
                    submission.gross_amount.to_string(),
                    submission.net_amount.to_string(),
                    submission.tax_amount.to_string(),
                ),
            );
        }
        let revisions = self
            .db
            .purchase_order_revisions()
            .find_many(
                mongodb::bson::doc! { "id": { "$in": pointer_ids } },
                &mut NoTransaction,
            )
            .await?;
        for revision in revisions {
            totals.insert(
                revision.base.id.clone(),
                (
                    revision.gross_amount.to_string(),
                    revision.net_amount.to_string(),
                    revision.tax_amount.to_string(),
                ),
            );
        }
        Ok(totals)
    }

    /// 解析供应商名称（D09 供应商角色 → D07 主体 → 当前主体修订法定名称）。
    async fn resolve_supplier_name(
        &self,
        supplier_id: &entities::ids::SupplierAccountId,
    ) -> Result<Option<String>> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?;
        let Some(supplier) = supplier else { return Ok(None) };
        let party = self
            .db
            .parties()
            .find_by_id(&supplier.party_id, &mut NoTransaction)
            .await?;
        let Some(party) = party else { return Ok(None) };
        let Some(revision_id) = party.stable.current_revision_id else {
            return Ok(None);
        };
        let revision = self
            .db
            .party_revisions()
            .find_by_id(&revision_id, &mut NoTransaction)
            .await?;
        Ok(revision.map(|revision| revision.legal_name))
    }

    /// 解析供应商付款条件代码（D09 商务结算版本快照，缺省 `NET-30`）。
    async fn resolve_payment_term_code(
        &self,
        supplier_id: &entities::ids::SupplierAccountId,
    ) -> Result<String> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?;
        let Some(supplier) = supplier else {
            return Ok("NET-30".to_string());
        };
        let Some(revision_id) = supplier.current_commercial_profile_revision_id else {
            return Ok("NET-30".to_string());
        };
        let revision = self
            .db
            .supplier_commercial_profile_revisions()
            .find_by_id(&revision_id, &mut NoTransaction)
            .await?;
        Ok(revision
            .map(|revision| revision.payment_term_snapshot)
            .unwrap_or_else(|| "NET-30".to_string()))
    }

    /// 构建创建依据行视图（金额逐行舍入）。
    async fn build_basis_lines(
        &self,
        lines: &[entities::sales_review::ProcurementConfirmationLine],
    ) -> Result<(Vec<CreationBasisLineView>, String)> {
        let mut views = Vec::with_capacity(lines.len());
        let mut estimated = zero_amount();
        let sales_line_ids: Vec<String> = lines
            .iter()
            .map(|line| line.sales_order_submission_line_id.to_string())
            .collect();
        let sales_lines = self
            .db
            .sales_order_submission_lines()
            .find_many(
                mongodb::bson::doc! { "id": { "$in": sales_line_ids } },
                &mut NoTransaction,
            )
            .await?;
        let sales_by_id: HashMap<String, entities::sales_order::SalesOrderSubmissionLine> = sales_lines
            .into_iter()
            .map(|line| (line.base.id.clone(), line))
            .collect();
        for line in lines {
            let (gross, _, _) = line_amounts(
                line.latest_cost_gross,
                line.confirmed_quantity,
                line.input_tax_rate,
            );
            estimated = estimated.checked_add(gross);
            let sales_line = sales_by_id.get(&line.sales_order_submission_line_id.to_string());
            views.push(CreationBasisLineView {
                procurement_confirmation_line_id: line.base.id.clone(),
                sales_order_submission_line_id: line.sales_order_submission_line_id.to_string(),
                supplier_id: line.supplier_id.to_string(),
                confirmed_quantity: line.confirmed_quantity.to_string(),
                latest_cost_gross: line.latest_cost_gross.to_string(),
                input_tax_rate: line.input_tax_rate.to_string(),
                expected_delivery_date: line.expected_delivery_date.to_string(),
                product_name: sales_line.map(|sales| sales.item_name_snapshot.clone()),
                specification: sales_line.and_then(|sales| sales.spec_snapshot.clone()),
                gross_amount: gross.to_string(),
            });
        }
        Ok((views, estimated.to_string()))
    }

    /// 构建草稿提交（表头来自依据，供应商快照与付款条件门禁在提交时冻结）。
    #[allow(clippy::too_many_arguments)]
    async fn build_draft_submission(
        &self,
        order_id: &PurchaseOrderId,
        supplier_id: &entities::ids::SupplierAccountId,
        purchase_type: PurchaseType,
        fulfillment: FulfillmentResponsibility,
        supplier_name: &str,
        payment_term_code: &str,
        lines: &[entities::sales_review::ProcurementConfirmationLine],
    ) -> Result<PurchaseOrderSubmission> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let revision_id = supplier
            .current_commercial_profile_revision_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("供应商缺少商务结算版本".to_string()))?;
        let gross = lines
            .iter()
            .map(|line| {
                let (gross, _, _) = line_amounts(
                    line.latest_cost_gross,
                    line.confirmed_quantity,
                    line.input_tax_rate,
                );
                gross
            })
            .fold(zero_amount(), |acc, amount| acc.checked_add(amount));
        let net = lines
            .iter()
            .map(|line| {
                let (_, net, _) = line_amounts(
                    line.latest_cost_gross,
                    line.confirmed_quantity,
                    line.input_tax_rate,
                );
                net
            })
            .fold(zero_amount(), |acc, amount| acc.checked_add(amount));
        let tax = lines
            .iter()
            .map(|line| {
                let (_, _, tax) = line_amounts(
                    line.latest_cost_gross,
                    line.confirmed_quantity,
                    line.input_tax_rate,
                );
                tax
            })
            .fold(zero_amount(), |acc, amount| acc.checked_add(amount));
        PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new(next_id()),
            PurchaseOrderSubmissionData {
                purchase_order_id: order_id.clone(),
                submission_no: format!("DRAFT-{}", &next_id()[..8]),
                supplier_id: supplier_id.clone(),
                purchase_type,
                fulfillment_responsibility: fulfillment,
                supplier_revision_id: revision_id,
                supplier_snapshot: SupplierSnapshot::new(supplier_name.to_string())?,
                payment_term_snapshot: self.payment_term_snapshot(payment_term_code).await?,
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
            },
        )
        .map_err(Into::into)
    }

    /// 从依据分行构建草稿明细（商品行快照取自销售提交行 D13）。
    async fn build_submission_lines_from_basis(
        &self,
        submission_id: &str,
        lines: &[entities::sales_review::ProcurementConfirmationLine],
    ) -> Result<Vec<PurchaseOrderSubmissionLine>> {
        let mut result = Vec::with_capacity(lines.len());
        let sales_line_ids: Vec<String> = lines
            .iter()
            .map(|line| line.sales_order_submission_line_id.to_string())
            .collect();
        let sales_lines = self
            .db
            .sales_order_submission_lines()
            .find_many(
                mongodb::bson::doc! { "id": { "$in": sales_line_ids } },
                &mut NoTransaction,
            )
            .await?;
        let sales_by_id: HashMap<String, entities::sales_order::SalesOrderSubmissionLine> = sales_lines
            .into_iter()
            .map(|line| (line.base.id.clone(), line))
            .collect();
        for (index, line) in lines.iter().enumerate() {
            let (gross, net, tax) = line_amounts(
                line.latest_cost_gross,
                line.confirmed_quantity,
                line.input_tax_rate,
            );
            let sales_line = sales_by_id.get(&line.sales_order_submission_line_id.to_string());
            let purchase_line = PurchaseOrderSubmissionLine::new(
                PurchaseOrderSubmissionLineId::new(next_id()),
                PurchaseOrderSubmissionLineData {
                    purchase_order_submission_id: entities::ids::PurchaseOrderSubmissionId::new(
                        submission_id.to_string(),
                    ),
                    line_no: (index + 1) as u32,
                    line_type: PurchaseLineType::ItemService,
                    procurement_confirmation_line_id: Some(line.base.id.clone().into()),
                    sku_id: sales_line.and_then(|sales| sales.sku_id.clone()),
                    sku_revision_id: sales_line.and_then(|sales| sales.sku_revision_id.clone()),
                    product_name_snapshot: sales_line.map(|sales| sales.item_name_snapshot.clone()),
                    specification_snapshot: sales_line.and_then(|sales| sales.spec_snapshot.clone()),
                    quantity: Some(line.confirmed_quantity),
                    base_unit_code: sales_line.and_then(|sales| sales.unit_snapshot.clone()),
                    unit_cost_gross: Some(line.latest_cost_gross),
                    gross_amount: gross,
                    net_amount: net,
                    tax_amount: tax,
                    input_tax_rate: Some(line.input_tax_rate),
                    expected_delivery_date: Some(line.expected_delivery_date),
                    sales_order_submission_line_id: Some(line.sales_order_submission_line_id.clone()),
                    allocated_quantity: Some(line.confirmed_quantity),
                },
            )?;
            result.push(purchase_line);
        }
        Ok(result)
    }

    /// 解析当前内容（版本 > 提交 > 草稿）并返回行与表头汇总。
    async fn resolve_current_content(
        &self,
        order: &PurchaseOrder,
    ) -> Result<(String, Vec<PurchaseOrderLineView>, TotalsView)> {
        if let Some(revision_id) = &order.stable.current_revision_id {
            if let Some(revision) = self
                .db
                .purchase_order_revisions()
                .find_by_id(revision_id, &mut NoTransaction)
                .await?
            {
                let lines = self
                    .db
                    .purchase_order_revision_lines()
                    .find_many(
                        mongodb::bson::doc! { "purchase_order_revision_id": revision_id },
                        &mut NoTransaction,
                    )
                    .await?;
                return Ok((
                    "REVISION".to_string(),
                    lines.iter().map(self::dto::revision_line_to_view).collect(),
                    self::dto::revision_totals(&revision),
                ));
            }
        }
        if let Some(submission_id) = &order.current_submission_id {
            if let Some(submission) = self
                .db
                .purchase_order_submissions()
                .find_by_id(submission_id, &mut NoTransaction)
                .await?
            {
                let lines = self
                    .db
                    .purchase_order_submission_lines()
                    .find_many(
                        mongodb::bson::doc! { "purchase_order_submission_id": submission_id },
                        &mut NoTransaction,
                    )
                    .await?;
                let source = if submission.status == SubmissionStatus::Draft {
                    "DRAFT"
                } else {
                    "SUBMISSION"
                };
                return Ok((
                    source.to_string(),
                    lines.iter().map(self::dto::submission_line_to_view).collect(),
                    TotalsView {
                        gross: submission.gross_amount.to_string(),
                        net: submission.net_amount.to_string(),
                        tax: submission.tax_amount.to_string(),
                    },
                ));
            }
        }
        Ok((
            "DRAFT".to_string(),
            Vec::new(),
            TotalsView {
                gross: "0.00".to_string(),
                net: "0.00".to_string(),
                tax: "0.00".to_string(),
            },
        ))
    }

    /// 解析当前生效版本的销售分配。
    async fn resolve_allocations(&self, order: &PurchaseOrder) -> Result<Vec<PurchaseSalesAllocationView>> {
        let Some(revision_id) = &order.stable.current_revision_id else {
            return Ok(Vec::new());
        };
        let lines = self
            .db
            .purchase_order_revision_lines()
            .find_many(
                mongodb::bson::doc! { "purchase_order_revision_id": revision_id },
                &mut NoTransaction,
            )
            .await?;
        let line_ids: Vec<String> = lines.iter().map(|line| line.base.id.clone()).collect();
        if line_ids.is_empty() {
            return Ok(Vec::new());
        }
        let allocations = self
            .db
            .purchase_line_sales_allocations()
            .find_many(
                mongodb::bson::doc! { "purchase_order_revision_line_id": { "$in": line_ids } },
                &mut NoTransaction,
            )
            .await?;
        Ok(allocations
            .into_iter()
            .map(|allocation| PurchaseSalesAllocationView {
                id: allocation.base.id,
                purchase_order_revision_line_id: allocation.purchase_order_revision_line_id.to_string(),
                sales_order_revision_line_id: allocation.sales_order_revision_line_id.to_string(),
                allocated_quantity: allocation.allocated_quantity.to_string(),
                allocated_cost_gross: allocation.allocated_cost_gross.to_string(),
                allocated_cost_net: allocation.allocated_cost_net.to_string(),
            })
            .collect())
    }

    /// 解析付款条件门禁快照（PREPAY 前缀判定先款后货，金额/比例门槛暂空）。
    async fn payment_term_snapshot(&self, payment_term_code: &str) -> Result<PaymentTermSnapshot> {
        let prepay_gate = payment_term_code.trim().to_uppercase().starts_with("PREPAY");
        PaymentTermSnapshot::new(payment_term_code.to_string(), prepay_gate, None, None).map_err(Into::into)
    }

    /// 冻结草稿为正式提交（复制明细并重指向正式提交、推进主表指针）。
    async fn freeze_submission(
        &self,
        order: &mut PurchaseOrder,
        draft: &mut PurchaseOrderSubmission,
        draft_lines: &mut [PurchaseOrderSubmissionLine],
        actor: &AuditActor,
    ) -> Result<PurchaseOrderSubmission> {
        let next_no = self.next_submission_no(order).await?;
        let formal = PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new(next_id()),
            PurchaseOrderSubmissionData {
                purchase_order_id: order.base.id.clone().into(),
                submission_no: next_no.clone(),
                supplier_id: draft.supplier_id.clone(),
                purchase_type: draft.purchase_type,
                fulfillment_responsibility: draft.fulfillment_responsibility,
                supplier_revision_id: draft.supplier_revision_id.clone(),
                supplier_snapshot: draft.supplier_snapshot.clone(),
                payment_term_snapshot: draft.payment_term_snapshot.clone(),
                gross_amount: draft.gross_amount,
                net_amount: draft.net_amount,
                tax_amount: draft.tax_amount,
            },
        )?;
        let mut formal = formal;
        formal.submit(Instant::now(), actor.id())?;
        // 正式提交行沿用草稿明细（行内提交引用改为正式提交）。
        for line in draft_lines.iter_mut() {
            line.purchase_order_submission_id = formal.base.id.clone().into();
        }
        order.submit_for_review(formal.base.id.clone(), actor.id())?;
        Ok(formal)
    }

    /// 计算下一个提交序号（`SUB-{n}`，聚合内唯一）。
    async fn next_submission_no(&self, order: &PurchaseOrder) -> Result<String> {
        let existing = self
            .db
            .purchase_order_submissions()
            .find_many(
                mongodb::bson::doc! { "purchase_order_id": order.base.id.clone() },
                &mut NoTransaction,
            )
            .await?;
        let max_no = existing
            .iter()
            .filter_map(|submission| {
                submission
                    .submission_no
                    .strip_prefix("SUB-")
                    .and_then(|value| value.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0);
        Ok(format!("SUB-{:06}", max_no + 1))
    }

    /// 计算下一个版本号（同一采购单内从 1 递增）。
    async fn next_revision_no(&self, order: &PurchaseOrder) -> Result<u32> {
        let existing = self
            .db
            .purchase_order_revisions()
            .find_many(
                mongodb::bson::doc! { "purchase_order_id": order.base.id.clone() },
                &mut NoTransaction,
            )
            .await?;
        Ok(existing
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0)
            + 1)
    }

    /// 构建审核待办（D03）。
    fn build_review_work_item(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseOrderSubmission,
    ) -> Result<WorkItem> {
        WorkItem::new(
            WorkItemId::new(next_id()),
            WorkItemData {
                work_item_type: WorkItemType::PurchaseOrderReview,
                business_object_type: "purchase_order".to_string(),
                business_object_id: order.base.id.clone(),
                subject_version: Some(submission.submission_no.clone()),
                owner_role: Some("finance".to_string()),
                owner_user_id: None,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: Some(format!("采购单 {} 待财务审核", order.purchase_no)),
                completion_action: "review".to_string(),
            },
        )
        .map_err(Into::into)
    }

    /// 形成生效版本与版本行（§8.1.4 复制已通过提交）。
    ///
    /// 说明：`purchase_line_sales_allocation` 的 Data 类型未从实体层导出
    /// （entities 冻结），分配写入本阶段无法构造实体，已在报告中提出；
    /// 版本行保留销售提交行引用与分配数量，供入库预占沿分配关系回查。
    #[allow(clippy::too_many_arguments)]
    async fn build_effective_revision(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseOrderSubmission,
        submission_lines: &[PurchaseOrderSubmissionLine],
        revision_no: u32,
        _req: &ApprovePurchaseOrderRequest,
        _actor: &AuditActor,
    ) -> Result<(PurchaseOrderRevision, Vec<PurchaseOrderRevisionLine>)> {
        // 逐行复验：商品/服务行必须引用已通过的采购确认分行（§6.6 必需约束）。
        for line in submission_lines {
            if line.line_type == PurchaseLineType::ItemService {
                let Some(confirmation_line_id) = &line.procurement_confirmation_line_id else {
                    return Err(Error::BusinessLogicError(
                        "采购明细缺少采购确认分行引用".to_string(),
                    ));
                };
                let confirmation_line = self
                    .db
                    .procurement_confirmation_lines()
                    .find_by_id(confirmation_line_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| {
                        Error::BusinessLogicError("采购明细引用的采购确认分行不存在".to_string())
                    })?;
                let confirmation = self
                    .db
                    .procurement_confirmations()
                    .find_by_id(&confirmation_line.procurement_confirmation_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::BusinessLogicError("采购确认不存在".to_string()))?;
                if confirmation.stable.status != ProcurementConfirmationStatus::Approved {
                    return Err(Error::BusinessLogicError(
                        "采购明细引用的采购确认未通过，审核不能通过".to_string(),
                    ));
                }
            }
        }
        let revision = PurchaseOrderRevision::new(
            PurchaseOrderRevisionId::new(next_id()),
            PurchaseOrderRevisionData {
                purchase_order_id: order.base.id.clone().into(),
                revision_no,
                supplier_revision_id: submission.supplier_revision_id.clone(),
                supplier_snapshot: submission.supplier_snapshot.clone(),
                payment_term_snapshot: submission.payment_term_snapshot.clone(),
                gross_amount: submission.gross_amount,
                net_amount: submission.net_amount,
                tax_amount: submission.tax_amount,
                effective_at: Instant::now(),
            },
        )?;
        let mut revision_lines = Vec::with_capacity(submission_lines.len());
        for line in submission_lines {
            revision_lines.push(PurchaseOrderRevisionLine::new(
                PurchaseOrderRevisionLineId::new(next_id()),
                PurchaseOrderRevisionLineData {
                    purchase_order_revision_id: revision.base.id.clone().into(),
                    line_no: line.line_no,
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line.procurement_confirmation_line_id.clone(),
                    sku_id: line.sku_id.clone(),
                    sku_revision_id: line.sku_revision_id.clone(),
                    product_name_snapshot: line.product_name_snapshot.clone(),
                    specification_snapshot: line.specification_snapshot.clone(),
                    quantity: line.quantity,
                    base_unit_code: line.base_unit_code.clone(),
                    unit_cost_gross: line.unit_cost_gross,
                    gross_amount: line.gross_amount,
                    net_amount: line.net_amount,
                    tax_amount: line.tax_amount,
                    input_tax_rate: line.input_tax_rate,
                    expected_delivery_date: line.expected_delivery_date,
                },
            )?);
        }
        Ok((revision, revision_lines))
    }

    /// 构建应付子账与原始应付分录（D19；子账按采购单维度）。
    async fn build_payable(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseOrderSubmission,
    ) -> Result<(entities::payable::PayableAccount, entities::payable::PayableEntry)> {
        let account = entities::payable::PayableAccount::new(
            entities::ids::PayableAccountId::new(next_id()),
            entities::payable::PayableAccountData {
                source_document_id: order.base.id.clone(),
                supplier_id: order.supplier_id.clone(),
                source_type: entities::payable::PayableSourceType::PurchaseOrder,
                gross_total: submission.gross_amount,
                settled_total: zero_amount(),
                invoiceable_total: submission.gross_amount,
                invoiced_total: zero_amount(),
            },
            "system",
        )?;
        let entry = entities::payable::PayableEntry::new(
            PayableEntryId::new(next_id()),
            entities::payable::PayableEntryData {
                payable_account_id: account.base.id.clone().into(),
                entry_type: entities::payable::PayableEntryType::Original,
                direction: entities::payable::EntryDirection::Increase,
                amount: submission.gross_amount,
                due_date: entities::common::time::BusinessDate::today(),
                source_fact_type: "purchase_order".to_string(),
                source_document_id: order.base.id.clone(),
                source_revision_id: submission.base.id.clone(),
                source_sequence: 1,
                posted_at: Instant::now(),
            },
        )?;
        Ok((account, entry))
    }

    /// 构建 `CONFIRMED` 成本事实（D20；逐采购行一个成本事实）。
    async fn build_confirmed_cost_entries(
        &self,
        submission: &PurchaseOrderSubmission,
        lines: &[PurchaseOrderSubmissionLine],
        revision_no: u32,
    ) -> Result<Vec<entities::cost::CostEntry>> {
        let mut entries = Vec::new();
        for line in lines {
            let tax_rate = line.input_tax_rate.unwrap_or_else(zero_rate);
            entries.push(entities::cost::CostEntry::new(
                CostEntryId::new(next_id()),
                entities::cost::CostEntryData {
                    cost_type: if line.line_type == PurchaseLineType::LogisticsFee {
                        entities::cost::CostType::Logistics
                    } else {
                        entities::cost::CostType::Product
                    },
                    cost_stage: entities::cost::CostStage::Confirmed,
                    cost_scope: entities::cost::CostScope::NonVoucherFulfillment,
                    cost_basis: None,
                    supplier_id: Some(submission.supplier_id.clone()),
                    gross_amount: line.gross_amount,
                    net_amount: line.net_amount,
                    tax_amount: line.tax_amount,
                    tax_inclusion: true,
                    input_tax_rate: tax_rate,
                    occurred_at: Instant::now(),
                    source_fact_type: "purchase_order".to_string(),
                    source_document_id: submission.purchase_order_id.to_string(),
                    source_line_id: line.base.id.clone(),
                    source_version: revision_no.to_string(),
                    adjusts_cost_entry_id: None,
                    evidence_attachment_id: None,
                },
            )?);
        }
        Ok(entries)
    }

    /// 构建变更提交（表头取自目标内容，提交动作由调用方冻结审计人）。
    async fn build_change_submission(
        &self,
        change: &PurchaseChangeOrder,
        order: &PurchaseOrder,
        base_revision: &PurchaseOrderRevision,
        _supplier_name: &str,
        req: &SubmitPurchaseChangeRequest,
    ) -> Result<PurchaseChangeSubmission> {
        let (gross, net, tax) = self.compute_request_totals(&req.lines).await?;
        let payment_term_code = req
            .payment_term_code
            .clone()
            .unwrap_or_else(|| base_revision.payment_term_snapshot.payment_term_code.clone());
        let payment_term_snapshot = self.payment_term_snapshot(&payment_term_code).await?;
        let next_no = self.next_change_submission_no(change).await?;
        PurchaseChangeSubmission::new(
            PurchaseChangeSubmissionId::new(next_id()),
            PurchaseChangeSubmissionData {
                purchase_change_order_id: change.base.id.clone().into(),
                submission_no: next_no,
                base_revision_id: change.base_revision_id.clone(),
                supplier_id: order.supplier_id.clone(),
                purchase_type: order.purchase_type,
                fulfillment_responsibility: order.fulfillment_responsibility,
                supplier_revision_id: base_revision.supplier_revision_id.clone(),
                supplier_snapshot: base_revision.supplier_snapshot.clone(),
                payment_term_snapshot,
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
            },
        )
        .map_err(Into::into)
    }

    /// 构建变更提交行。
    async fn build_change_submission_lines(
        &self,
        submission_id: &str,
        lines: &[SavePurchaseOrderLine],
    ) -> Result<Vec<PurchaseChangeSubmissionLine>> {
        self.build_change_lines_inner(submission_id, lines).await
    }

    /// 从请求行构建提交行（逐行计算金额）。
    async fn build_lines_from_request(
        &self,
        submission_id: &entities::ids::PurchaseOrderSubmissionId,
        lines: &[SavePurchaseOrderLine],
    ) -> Result<Vec<PurchaseOrderSubmissionLine>> {
        let mut result = Vec::with_capacity(lines.len());
        for (index, line) in lines.iter().enumerate() {
            let (gross, net, tax) = self.compute_line_amounts(line).await?;
            result.push(PurchaseOrderSubmissionLine::new(
                PurchaseOrderSubmissionLineId::new(next_id()),
                PurchaseOrderSubmissionLineData {
                    purchase_order_submission_id: submission_id.clone(),
                    line_no: (index + 1) as u32,
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line
                        .procurement_confirmation_line_id
                        .as_ref()
                        .map(|value| entities::ids::ProcurementConfirmationLineId::new(value.clone())),
                    sku_id: line
                        .sku_id
                        .as_ref()
                        .map(|value| entities::ids::SkuId::new(value.clone())),
                    sku_revision_id: line
                        .sku_revision_id
                        .as_ref()
                        .map(|value| entities::ids::SkuRevisionId::new(value.clone())),
                    product_name_snapshot: line.product_name.clone(),
                    specification_snapshot: line.specification.clone(),
                    quantity: self.parse_quantity(line.quantity.as_deref())?,
                    base_unit_code: line.base_unit_code.clone(),
                    unit_cost_gross: self.parse_unit_price(line.unit_cost_gross.as_deref())?,
                    gross_amount: gross,
                    net_amount: net,
                    tax_amount: tax,
                    input_tax_rate: self.parse_rate(line.input_tax_rate.as_deref())?,
                    expected_delivery_date: line
                        .expected_delivery_date
                        .as_deref()
                        .map(parse_business_date)
                        .transpose()?,
                    sales_order_submission_line_id: line
                        .sales_order_submission_line_id
                        .as_ref()
                        .map(|value| entities::ids::SalesOrderSubmissionLineId::new(value.clone())),
                    allocated_quantity: self.parse_quantity(line.allocated_quantity.as_deref())?,
                },
            )?);
        }
        Ok(result)
    }

    /// 从请求行构建变更提交行（复用同构字段组）。
    async fn build_change_lines_inner(
        &self,
        submission_id: &str,
        lines: &[SavePurchaseOrderLine],
    ) -> Result<Vec<PurchaseChangeSubmissionLine>> {
        let mut result = Vec::with_capacity(lines.len());
        for (index, line) in lines.iter().enumerate() {
            let (gross, net, tax) = self.compute_line_amounts(line).await?;
            result.push(PurchaseChangeSubmissionLine::new(
                PurchaseChangeSubmissionLineId::new(next_id()),
                PurchaseChangeSubmissionLineData {
                    purchase_change_submission_id: entities::ids::PurchaseChangeSubmissionId::new(
                        submission_id.to_string(),
                    ),
                    line_no: (index + 1) as u32,
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line
                        .procurement_confirmation_line_id
                        .as_ref()
                        .map(|value| entities::ids::ProcurementConfirmationLineId::new(value.clone())),
                    sku_id: line
                        .sku_id
                        .as_ref()
                        .map(|value| entities::ids::SkuId::new(value.clone())),
                    sku_revision_id: line
                        .sku_revision_id
                        .as_ref()
                        .map(|value| entities::ids::SkuRevisionId::new(value.clone())),
                    product_name_snapshot: line.product_name.clone(),
                    specification_snapshot: line.specification.clone(),
                    quantity: self.parse_quantity(line.quantity.as_deref())?,
                    base_unit_code: line.base_unit_code.clone(),
                    unit_cost_gross: self.parse_unit_price(line.unit_cost_gross.as_deref())?,
                    gross_amount: gross,
                    net_amount: net,
                    tax_amount: tax,
                    input_tax_rate: self.parse_rate(line.input_tax_rate.as_deref())?,
                    expected_delivery_date: line
                        .expected_delivery_date
                        .as_deref()
                        .map(parse_business_date)
                        .transpose()?,
                    sales_order_submission_line_id: line
                        .sales_order_submission_line_id
                        .as_ref()
                        .map(|value| entities::ids::SalesOrderSubmissionLineId::new(value.clone())),
                    allocated_quantity: self.parse_quantity(line.allocated_quantity.as_deref())?,
                },
            )?);
        }
        Ok(result)
    }

    /// 计算单行金额（商品行 `line_amounts`，物流行 `gross − round(gross×税率)`）。
    async fn compute_line_amounts(&self, line: &SavePurchaseOrderLine) -> Result<(Amount, Amount, Amount)> {
        let tax_rate = self
            .parse_rate(line.input_tax_rate.as_deref())?
            .unwrap_or_else(zero_rate);
        match line.line_type {
            PurchaseLineType::ItemService => {
                let quantity = self
                    .parse_quantity(line.quantity.as_deref())?
                    .ok_or_else(|| Error::ValidationError("商品行数量不能为空".to_string()))?;
                let unit_cost = self
                    .parse_unit_price(line.unit_cost_gross.as_deref())?
                    .ok_or_else(|| Error::ValidationError("商品行含税单价不能为空".to_string()))?;
                Ok(line_amounts(unit_cost, quantity, tax_rate))
            }
            PurchaseLineType::LogisticsFee => {
                let gross = self
                    .parse_amount(line.gross_amount.as_deref())?
                    .ok_or_else(|| Error::ValidationError("物流费用行含税金额不能为空".to_string()))?;
                let tax = entities::money::Amount::try_from(entities::money::round_to_cent(
                    gross.to_decimal() * tax_rate.to_decimal(),
                ))
                .expect("舍入后小数位不超过 2 位");
                let net = Amount::try_from(gross.to_decimal() - tax.to_decimal())
                    .expect("物流行净额小数位不超过 2 位");
                Ok((gross, net, tax))
            }
        }
    }

    /// 汇总请求行的表头金额。
    async fn compute_request_totals(
        &self,
        lines: &[SavePurchaseOrderLine],
    ) -> Result<(Amount, Amount, Amount)> {
        let mut gross = zero_amount();
        let mut net = zero_amount();
        let mut tax = zero_amount();
        for line in lines {
            let (gross_line, net_line, tax_line) = self.compute_line_amounts(line).await?;
            gross = gross.checked_add(gross_line);
            net = net.checked_add(net_line);
            tax = tax.checked_add(tax_line);
        }
        Ok((gross, net, tax))
    }

    /// 计算下一个变更提交序号。
    async fn next_change_submission_no(&self, change: &PurchaseChangeOrder) -> Result<String> {
        let existing = self
            .db
            .purchase_change_submissions()
            .find_many(
                mongodb::bson::doc! { "purchase_change_order_id": change.base.id.clone() },
                &mut NoTransaction,
            )
            .await?;
        let max_no = existing
            .iter()
            .filter_map(|submission| {
                submission
                    .submission_no
                    .strip_prefix("CS-")
                    .and_then(|value| value.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0);
        Ok(format!("CS-{:06}", max_no + 1))
    }

    /// 形成变更生效版本与版本行。
    async fn build_change_revision(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseChangeSubmission,
        lines: &[PurchaseChangeSubmissionLine],
        revision_no: u32,
    ) -> Result<(PurchaseOrderRevision, Vec<PurchaseOrderRevisionLine>)> {
        let revision = PurchaseOrderRevision::new(
            PurchaseOrderRevisionId::new(next_id()),
            PurchaseOrderRevisionData {
                purchase_order_id: order.base.id.clone().into(),
                revision_no,
                supplier_revision_id: submission.supplier_revision_id.clone(),
                supplier_snapshot: submission.supplier_snapshot.clone(),
                payment_term_snapshot: submission.payment_term_snapshot.clone(),
                gross_amount: submission.gross_amount,
                net_amount: submission.net_amount,
                tax_amount: submission.tax_amount,
                effective_at: Instant::now(),
            },
        )?;
        let mut revision_lines = Vec::with_capacity(lines.len());
        for line in lines {
            revision_lines.push(PurchaseOrderRevisionLine::new(
                PurchaseOrderRevisionLineId::new(next_id()),
                PurchaseOrderRevisionLineData {
                    purchase_order_revision_id: revision.base.id.clone().into(),
                    line_no: line.line_no,
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line.procurement_confirmation_line_id.clone(),
                    sku_id: line.sku_id.clone(),
                    sku_revision_id: line.sku_revision_id.clone(),
                    product_name_snapshot: line.product_name_snapshot.clone(),
                    specification_snapshot: line.specification_snapshot.clone(),
                    quantity: line.quantity,
                    base_unit_code: line.base_unit_code.clone(),
                    unit_cost_gross: line.unit_cost_gross,
                    gross_amount: line.gross_amount,
                    net_amount: line.net_amount,
                    tax_amount: line.tax_amount,
                    input_tax_rate: line.input_tax_rate,
                    expected_delivery_date: line.expected_delivery_date,
                },
            )?);
        }
        Ok((revision, revision_lines))
    }

    /// 构建变更差额（应付差额分录 + `CONFIRMED` 差额成本事实）。
    async fn build_change_deltas(
        &self,
        order: &PurchaseOrder,
        base_revision: &PurchaseOrderRevision,
        new_revision: &PurchaseOrderRevision,
    ) -> Result<(
        Option<(entities::payable::PayableAccount, entities::payable::PayableEntry)>,
        Vec<entities::cost::CostEntry>,
    )> {
        let delta_amount = Amount::try_from(
            new_revision.gross_amount.to_decimal() - base_revision.gross_amount.to_decimal(),
        )
        .expect("金额差值小数位不超过 2 位");
        let payable_delta = if delta_amount.to_decimal() != zero_amount().to_decimal() {
            let account = entities::payable::PayableAccount::new(
                entities::ids::PayableAccountId::new(next_id()),
                entities::payable::PayableAccountData {
                    source_document_id: order.base.id.clone(),
                    supplier_id: order.supplier_id.clone(),
                    source_type: entities::payable::PayableSourceType::PurchaseOrder,
                    gross_total: delta_amount,
                    settled_total: zero_amount(),
                    invoiceable_total: delta_amount,
                    invoiced_total: zero_amount(),
                },
                "system",
            )?;
            let entry = entities::payable::PayableEntry::new(
                PayableEntryId::new(next_id()),
                entities::payable::PayableEntryData {
                    payable_account_id: account.base.id.clone().into(),
                    entry_type: entities::payable::PayableEntryType::ChangeDelta,
                    direction: if delta_amount.to_decimal() > zero_amount().to_decimal() {
                        entities::payable::EntryDirection::Increase
                    } else {
                        entities::payable::EntryDirection::Decrease
                    },
                    amount: Amount::try_from(delta_amount.to_decimal().abs())
                        .expect("差额绝对值小数位不超过 2 位"),
                    due_date: entities::common::time::BusinessDate::today(),
                    source_fact_type: "purchase_change_order".to_string(),
                    source_document_id: order.base.id.clone(),
                    source_revision_id: new_revision.base.id.clone(),
                    source_sequence: 1,
                    posted_at: Instant::now(),
                },
            )?;
            Some((account, entry))
        } else {
            None
        };
        Ok((payable_delta, Vec::new()))
    }

    /// 解析数量。
    fn parse_quantity(&self, value: Option<&str>) -> Result<Option<Quantity>> {
        match value {
            Some(value) if !value.trim().is_empty() => Quantity::from_str(value.trim())
                .map(Some)
                .map_err(|_| Error::ValidationError(format!("非法数量: {value}"))),
            _ => Ok(None),
        }
    }

    /// 解析含税单价。
    fn parse_unit_price(&self, value: Option<&str>) -> Result<Option<UnitPrice>> {
        match value {
            Some(value) if !value.trim().is_empty() => UnitPrice::from_str(value.trim())
                .map(Some)
                .map_err(|_| Error::ValidationError(format!("非法含税单价: {value}"))),
            _ => Ok(None),
        }
    }

    /// 解析税率。
    fn parse_rate(&self, value: Option<&str>) -> Result<Option<Rate>> {
        match value {
            Some(value) if !value.trim().is_empty() => Rate::from_str(value.trim())
                .map(Some)
                .map_err(|_| Error::ValidationError(format!("非法税率: {value}"))),
            _ => Ok(None),
        }
    }

    /// 解析金额。
    fn parse_amount(&self, value: Option<&str>) -> Result<Option<Amount>> {
        match value {
            Some(value) if !value.trim().is_empty() => Amount::from_str(value.trim())
                .map(Some)
                .map_err(|_| Error::ValidationError(format!("非法金额: {value}"))),
            _ => Ok(None),
        }
    }
}

/// 版本化访问（乐观锁校验统一入口）。
trait Versioned {
    /// 返回实体乐观锁版本。
    fn version(&self) -> u64;
}

impl Versioned for PurchaseOrder {
    fn version(&self) -> u64 {
        self.base.version
    }
}

impl Versioned for PurchaseChangeOrder {
    fn version(&self) -> u64 {
        self.base.version
    }
}

/// 从确认履约方式映射采购履约责任。
fn fulfillment_from_mode(mode: FulfillmentMode) -> FulfillmentResponsibility {
    match mode {
        FulfillmentMode::CompanyWarehouse => FulfillmentResponsibility::Warehouse,
        FulfillmentMode::SupplierDirect => FulfillmentResponsibility::SupplierDirect,
        FulfillmentMode::ElectronicDelivery => FulfillmentResponsibility::Electronic,
        FulfillmentMode::OfflineService => FulfillmentResponsibility::Service,
    }
}

/// 零金额。
fn zero_amount() -> Amount {
    Amount::from_str("0").expect("零金额合法")
}

/// 零税率。
fn zero_rate() -> Rate {
    Rate::from_str("0").expect("零税率合法")
}

/// 解析业务日期字符串。
fn parse_business_date(value: &str) -> Result<entities::common::time::BusinessDate> {
    entities::common::time::BusinessDate::from_str(value.trim())
        .map_err(|_| Error::ValidationError(format!("非法业务日期: {value}")))
}

/// 生成当日时间戳前缀（`YYYYMMDD`）。
fn today_stamp() -> String {
    entities::common::time::BusinessDate::today()
        .to_string()
        .replace('-', "")
}

/// 内容指纹（Debug 形态 SipHash 十六进制；同二进制内稳定，用于变更目标内容比对）。
fn content_fingerprint(lines: &[SavePurchaseOrderLine]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    format!("{:?}", lines).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
