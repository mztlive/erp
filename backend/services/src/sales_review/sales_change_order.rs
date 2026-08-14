// ---------------------------------------------------------------------
// 销售变更单（W05 变更轨；§8.1.3 本批部分）
// ---------------------------------------------------------------------

use std::str::FromStr;

use database::{AccessControlExt, NoTransaction, SalesOrderExt, SalesReviewExt, Transactional, WorkItemExt};
use entities::common::time::Instant;
use entities::ids::{
    SalesChangeOrderId, SalesChangeReviewId, SalesChangeSubmissionId, SalesChangeSubmissionLineId,
    SalesOrderId, SalesOrderLineId, SalesOrderWorkingCopyId, WorkItemId,
};
use entities::money::Amount;
use entities::sales_order::WorkingPurpose;
use entities::sales_review::{
    SalesChangeOrder, SalesChangeOrderData, SalesChangeReview, SalesChangeReviewData, SalesChangeReviewStage,
    SalesChangeSubmission, SalesChangeSubmissionData, SalesChangeSubmissionLine,
    SalesChangeSubmissionLineData,
};
use entities::work_item::{
    AssignmentMode, AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};
use id_generator::next_id;
use validator::Validate;

use super::dto;
use super::sales_change_mapping::{change_copy_goods, change_copy_voucher, convert_line_type};
use super::{
    CreateSalesChangeOrderRequest, PageView, SalesChangeDraftRequest, SalesChangeLineRequest,
    SalesChangeOrderDetailView, SalesChangeOrderFilter, SalesChangeOrderListParams, SalesChangeOrderView,
    SalesReviewService, SubmitSalesChangeRequest, VoidSalesChangeOrderRequest,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl SalesReviewService {
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
        if order.commercial_status != entities::sales_order::CommercialStatus::Effective {
            return Err(Error::BusinessLogicError(
                "只有已生效的销售单才能发起变更".to_string(),
            ));
        }
        let base_revision_id = order
            .stable
            .current_revision_id
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本，无法发起变更".to_string()))?;
        let base_revision = self
            .db
            .sales_order_revisions()
            .find_by_id(&base_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单当前版本不存在".to_string()))?;
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
        if order.business_type == entities::sales_order::BusinessType::Voucher {
            return Err(Error::BusinessLogicError(
                "卡券销售变更缺少原正式版本冻结的目标商城或应收到期日，禁止创建变更单".to_string(),
            ));
        }
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
                contract_revision_id: order
                    .contract_id
                    .as_ref()
                    .and_then(|_| base_revision.contract_revision_id.clone()),
                settlement_party_id: order.settlement_party_id.clone(),
                snapshot: change_header_snapshot(&req.draft)?,
                project_name: req.draft.project_name.clone(),
                business_remark: req.draft.business_remark.clone(),
                voucher_category_sku_id: req.draft.voucher_category_sku_id.clone(),
                voucher_expiry_at: req
                    .draft
                    .voucher_expiry_at
                    .map(|secs| Instant::from_unix_secs(secs as i64)),
                target_mall_id: None,
                receivable_due_date: None,
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

        let review_stage = if working_copy.business_type == entities::sales_order::BusinessType::GoodsService
        {
            SalesChangeReviewStage::ProcurementImpact
        } else {
            SalesChangeReviewStage::OperationsImpact
        };
        let review = SalesChangeReview::new(
            SalesChangeReviewId::new(next_id()),
            SalesChangeReviewData {
                sales_change_submission_id: submission.base.id.clone().into(),
                review_stage,
            },
            actor.id(),
        )?;
        let work_item = WorkItem::new(
            WorkItemId::new(next_id()),
            WorkItemData {
                work_item_type: WorkItemType::SalesChangeImpactReview,
                approval_step_instance_id: None,
                business_object_type: "sales_change_review".to_string(),
                business_object_id: review.base.id.clone(),
                subject_version: submission.base.id.clone(),
                assignment_mode: AssignmentMode::Pool,
                owner_role: match review_stage {
                    SalesChangeReviewStage::ProcurementImpact => "role-procurement",
                    SalesChangeReviewStage::OperationsImpact => "role-operations",
                    SalesChangeReviewStage::FinanceReview => unreachable!("初始影响确认不会直接进入财务复核"),
                }
                .to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: None,
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: Some("change_impact_dispatched".to_string()),
                impact_summary: Some("销售变更履约影响确认".to_string()),
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
            contract_revision_id: working_copy.contract_revision_id.clone(),
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
