// ---------------------------------------------------------------------
// 采购二次确认（W07）
// ---------------------------------------------------------------------

use database::{AccessControlExt, NoTransaction, SalesOrderExt, SalesReviewExt, Transactional};
use entities::ids::SalesOrderSubmissionId;
use entities::sales_review::{
    ProcurementConfirmation, ProcurementConfirmationLine, ProcurementConfirmationLineData,
};
use id_generator::next_id;
use validator::Validate;

use super::dto;
use super::{
    PageView, ProcurementConfirmationDetailView, ProcurementConfirmationFilter,
    ProcurementConfirmationLineView, ProcurementConfirmationListParams, ProcurementConfirmationView,
    SalesReviewService, SaveProcurementConfirmationLinesRequest,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl SalesReviewService {
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
