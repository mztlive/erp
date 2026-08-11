//! 采购单查询与对象中心视图编排。

use std::collections::HashMap;

use database::{NoTransaction, PurchaseOrderExt};
use entities::purchase_order::{PurchaseOrder, SubmissionStatus};
use validator::Validate;

use super::dto::{
    PageView, PurchaseOrderCenterView, PurchaseOrderLineView, PurchaseOrderListItemView,
    PurchaseOrderListParams, PurchaseSalesAllocationView, TotalsView,
};
use super::view_mapping::{revision_line_to_view, revision_totals, submission_line_to_view};
use super::PurchaseOrderService;
use crate::errors::{Error, Result};

/// 采购单列表筛选条件类型（经 `PurchaseOrderExt` 关联类型跨 crate 可达）。
type PurchaseOrderFilter = <mongodb::Database as PurchaseOrderExt>::PurchaseOrderFilter;

impl PurchaseOrderService {
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
            sort_ascending: matches!(query.paging.sort_dir, super::dto::SortDir::Asc),
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
            .map(|change| super::dto::PurchaseChangeSummaryView {
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
                    lines.iter().map(revision_line_to_view).collect(),
                    revision_totals(&revision),
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
                    lines.iter().map(submission_line_to_view).collect(),
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
}
