//! 采购单查询与对象中心视图编排。

use std::collections::{HashMap, HashSet};

use database::{AccessControlExt, NoTransaction, PayableExt, PurchaseOrderExt, SalesOrderExt, SupplierExt};
use entities::purchase_order::PurchaseOrder;
use validator::Validate;

use super::approval_query::load_document_approval;
use super::dto::{
    PageView, PurchaseOrderCenterView, PurchaseOrderLineView, PurchaseOrderListItemView,
    PurchaseOrderListParams, PurchaseSalesAllocationView, TotalsView,
};
use super::view_mapping::{revision_line_to_view, revision_totals, submission_line_to_view};
use super::PurchaseOrderService;
use crate::document_registry::find_approval_binding;
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

        let supplier_ids = page
            .items
            .iter()
            .map(|row| row.supplier_id.clone())
            .collect::<Vec<_>>();
        let supplier_names = self
            .db
            .supplier()
            .current_legal_names_by_account_ids(&supplier_ids, &mut NoTransaction)
            .await?;
        let sales_order_ids: Vec<String> = page
            .items
            .iter()
            .map(|row| row.sales_order_id.to_string())
            .collect();
        let sales_order_numbers = self.resolve_sales_order_numbers(&sales_order_ids).await?;
        let owner_ids: Vec<String> = page
            .items
            .iter()
            .filter_map(|row| {
                row.owner_user_id
                    .as_deref()
                    .filter(|owner| !owner.trim().is_empty())
                    .map(str::to_string)
            })
            .collect();
        let owner_names = self.resolve_account_names(&owner_ids).await?;
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
            .map(|row| -> Result<PurchaseOrderListItemView> {
                let sales_order_id = row.sales_order_id.to_string();
                let sales_order_no = sales_order_numbers
                    .get(&sales_order_id)
                    .cloned()
                    .ok_or_else(|| Error::Internal("采购单关联的销售单不存在".to_string()))?;
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
                let owner_user_id = row
                    .owner_user_id
                    .as_deref()
                    .filter(|owner| !owner.trim().is_empty())
                    .map(str::to_string);
                Ok(PurchaseOrderListItemView {
                    id: row.id,
                    purchase_no: row.purchase_no,
                    sales_order_id,
                    sales_order_no,
                    supplier_id: row.supplier_id.to_string(),
                    supplier_name,
                    purchase_type: row.purchase_type,
                    payment_term_code: row.payment_term_code,
                    owner_name: owner_names
                        .get(owner_user_id.as_deref().unwrap_or_default())
                        .cloned()
                        .or_else(|| owner_user_id.as_ref().map(|_| "责任账号不可用".to_string()))
                        .unwrap_or_else(|| "未指定".to_string()),
                    owner_user_id,
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
                })
            })
            .collect::<Result<Vec<_>>>()?;

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
        let sales_order_id = order.sales_order_id.to_string();
        let sales_order_no = self
            .resolve_sales_order_numbers(std::slice::from_ref(&sales_order_id))
            .await?
            .remove(&sales_order_id)
            .ok_or_else(|| Error::Internal("采购单关联的销售单不存在".to_string()))?;
        let owner_user_id = order.current_owner_user_id()?.to_string();
        let owner_name = self
            .resolve_account_names(std::slice::from_ref(&owner_user_id))
            .await?
            .remove(&owner_user_id)
            .unwrap_or_else(|| "责任账号不可用".to_string());

        let (content_source, lines, totals) = self.resolve_current_content(&order).await?;
        let allocations = self.resolve_allocations(&order).await?;
        let changes = self
            .db
            .purchase_order()
            .list_changes_by_order(&order.base.id.clone().into(), &mut NoTransaction)
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
        let payable_summary = self
            .db
            .payable_accounts()
            .find_by_purchase_order(&order.base.id.clone().into(), &mut NoTransaction)
            .await?
            .map(|account| super::dto::PurchaseOrderPayableSummaryView {
                payable_open_amount: account.open_total,
                paid_allocated_amount: account.settled_total,
                purchase_invoice_allocated_amount: account.invoiced_total,
            });
        let binding = match find_approval_binding(&self.db, &order.base.id, &mut NoTransaction).await {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };

        Ok(PurchaseOrderCenterView {
            id: order.base.id.clone(),
            purchase_no: order.purchase_no.clone(),
            status: order.stable.status,
            review_status: order.review_status,
            version: order.base.version,
            sales_order_id,
            sales_order_no,
            supplier_id: order.supplier_id.to_string(),
            supplier_name,
            purchase_type: order.purchase_type,
            payment_term_code: order.payment_term_code.clone(),
            fulfillment_responsibility: order.fulfillment_responsibility,
            owner_user_id,
            owner_name,
            target_warehouse_id: order.target_warehouse_id.as_ref().map(ToString::to_string),
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
            payable_summary,
            approval: load_document_approval(
                &self.db,
                order.base.id.as_ref(),
                binding.as_ref(),
                order.stable.status,
            )
            .await?,
            created_at: order.base.created_at,
        })
    }

    /// 批量解析账号展示姓名。
    ///
    /// 用于采购单列表「负责人」列：把 `created_by` 解析为账号姓名，避免把账号 ID 直接展示给用户。
    ///
    /// # 参数
    /// * `account_ids` - 账号 ID 列表（可重复；空串会被忽略）
    ///
    /// # 返回
    /// 返回账号 ID → 姓名映射；账号不存在时不写入该键。
    ///
    /// # 错误
    /// * `RepositoryError` - 数据库查询失败
    pub(super) async fn resolve_account_names(
        &self,
        account_ids: &[String],
    ) -> Result<HashMap<String, String>> {
        let mut unique = Vec::new();
        let mut seen = HashSet::new();
        for account_id in account_ids {
            let trimmed = account_id.trim();
            if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
                continue;
            }
            unique.push(trimmed.to_string());
        }
        if unique.is_empty() {
            return Ok(HashMap::new());
        }
        let accounts = self
            .db
            .accounts()
            .list_by_ids(&unique, &mut NoTransaction)
            .await?;
        Ok(accounts
            .into_iter()
            .map(|account| (account.base.id, account.name))
            .collect())
    }

    /// 批量解析采购单来源销售单的业务单号。
    ///
    /// 内部 ID 只作为路由与关联键；任何缺失的来源销售单都视为数据完整性错误，
    /// 不得回退为把 ID 当作业务单号返回。
    ///
    /// # 参数
    /// * `sales_order_ids` - 销售单稳定身份字符串，可重复
    ///
    /// # 返回
    /// 返回销售单 ID → 业务单号映射。
    ///
    /// # 错误
    /// * `RepositoryError` - 数据库查询失败
    /// * `Internal` - 采购单引用的销售单不存在
    async fn resolve_sales_order_numbers(
        &self,
        sales_order_ids: &[String],
    ) -> Result<HashMap<String, String>> {
        let unique = sales_order_ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect::<HashSet<_>>();
        if unique.is_empty() {
            return Ok(HashMap::new());
        }
        let ids = unique
            .iter()
            .cloned()
            .map(entities::ids::SalesOrderId::new)
            .collect::<Vec<_>>();
        let numbers = self
            .db
            .sales_orders()
            .find_orders_by_ids(&ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.order_no))
            .collect::<HashMap<_, _>>();
        if unique.iter().any(|id| !numbers.contains_key(id)) {
            return Err(Error::Internal("采购单关联的销售单不存在".to_string()));
        }
        Ok(numbers)
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
        let submission_ids = pointer_ids
            .iter()
            .cloned()
            .map(entities::ids::PurchaseOrderSubmissionId::new)
            .collect::<Vec<_>>();
        let submissions = self
            .db
            .purchase_order()
            .find_submissions_by_ids(&submission_ids, &mut NoTransaction)
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
        let revision_ids = pointer_ids
            .iter()
            .cloned()
            .map(entities::ids::PurchaseOrderRevisionId::new)
            .collect::<Vec<_>>();
        let revisions = self
            .db
            .purchase_order()
            .find_revisions_by_ids(&revision_ids, &mut NoTransaction)
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
                    .purchase_order()
                    .list_revision_lines(&revision_id.clone().into(), &mut NoTransaction)
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
                    .purchase_order()
                    .list_submission_lines(&submission_id.clone().into(), &mut NoTransaction)
                    .await?;
                let source = submission.content_source();
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
            .purchase_order()
            .list_revision_lines(&revision_id.clone().into(), &mut NoTransaction)
            .await?;
        let line_ids = lines
            .iter()
            .map(|line| entities::ids::PurchaseOrderRevisionLineId::new(line.base.id.clone()))
            .collect::<Vec<_>>();
        if line_ids.is_empty() {
            return Ok(Vec::new());
        }
        let allocations = self
            .db
            .purchase_line_sales_allocations()
            .find_by_purchase_revision_line_ids(&line_ids, &mut NoTransaction)
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
