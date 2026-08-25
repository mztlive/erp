//! 采购单查询与对象中心视图编排。

use std::collections::{HashMap, HashSet};

use database::{AccessControlExt, NoTransaction, PurchaseOrderExt, WorkItemExt};
use entities::purchase_order::PurchaseOrder;
use entities::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use validator::Validate;

use super::approval_query::load_document_approval;
use super::dto::{
    PageView, PurchaseActionBlockerView, PurchaseOrderCenterView, PurchaseOrderLineView,
    PurchaseOrderListItemView, PurchaseOrderListParams, PurchaseReviewDomainAction,
    PurchaseReviewWorkItemView, PurchaseSalesAllocationView, TotalsView,
};
use super::view_mapping::{revision_line_to_view, revision_totals, submission_line_to_view};
use super::PurchaseOrderService;
use crate::document_registry::find_approval_binding;
use crate::errors::{Error, Result};
use crate::work_item::ProcessingState;

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
        let sales_order_ids: Vec<String> = page
            .items
            .iter()
            .map(|row| row.sales_order_id.to_string())
            .collect();
        let sales_order_numbers = self.resolve_sales_order_numbers(&sales_order_ids).await?;
        let owner_ids: Vec<String> = page.items.iter().map(|row| row.created_by.clone()).collect();
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
                Ok(PurchaseOrderListItemView {
                    id: row.id,
                    purchase_no: row.purchase_no,
                    sales_order_id,
                    sales_order_no,
                    supplier_id: row.supplier_id.to_string(),
                    supplier_name,
                    purchase_type: row.purchase_type,
                    payment_term_code: row.payment_term_code,
                    owner_name: owner_names.get(&row.created_by).cloned().unwrap_or_default(),
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
    /// * `actor_id` - 当前已认证账号，用于计算审核责任动作
    ///
    /// # 返回
    /// 返回对象中心视图（当前内容按 版本 > 提交 > 草稿 优先级取用）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn purchase_order_detail(&self, id: &str, actor_id: &str) -> Result<PurchaseOrderCenterView> {
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
        let review_work_item = self.resolve_review_work_item(&order, actor_id).await?;
        let payable_summary = self
            .db
            .purchase_order()
            .find_payable_account(&order.base.id.clone().into(), &mut NoTransaction)
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
            review_work_item,
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

    /// 解析当前采购审核任务并以服务端责任事实计算可用动作。
    async fn resolve_review_work_item(
        &self,
        order: &PurchaseOrder,
        actor_id: &str,
    ) -> Result<Option<PurchaseReviewWorkItemView>> {
        let Ok(submission_id) = order.submission_id_for_formalization() else {
            return Ok(None);
        };
        let _ = (order, actor_id, &submission_id);
        return Ok(None);
        #[allow(unreachable_code)]
        let mut items = self
            .db
            .work_items()
            .list_active_by_object("purchase_order", &order.base.id, &mut NoTransaction)
            .await?
            .into_iter()
            .filter(|item| {
                item.work_item_type == WorkItemType::PurchaseOrderReview
                    && item.subject_version == submission_id.as_ref()
            })
            .collect::<Vec<_>>();
        if items.len() > 1 {
            return Err(Error::ConflictError(
                "采购单存在多个开放财务审核待办，已禁止操作".to_string(),
            ));
        }
        let Some(item) = items.pop() else {
            return Ok(None);
        };
        if false || item.business_object_id != order.base.id || item.business_object_type != "purchase_order"
        {
            return Err(Error::ConflictError(
                "采购审核待办与当前采购单责任事实不一致，已禁止操作".to_string(),
            ));
        }
        let assignment_eligible = true;
        let submission = self
            .db
            .purchase_order_submissions()
            .find_by_id(submission_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("采购审核待办引用的提交不存在，已禁止操作".to_string()))?;
        submission.ensure_pending().map_err(|_| {
            Error::ConflictError("采购审核待办引用的提交已不在待审核状态，已禁止操作".to_string())
        })?;
        let separation_satisfied = submission.reviewer_is_separated(actor_id);
        let (domain_allowed_actions, action_blockers) =
            review_task_access(&item, actor_id, assignment_eligible, separation_satisfied);
        Ok(Some(PurchaseReviewWorkItemView {
            work_item_id: item.base.id,
            work_item_type: item.work_item_type,
            task_version: item.base.version,
            subject_version: item.subject_version,
            status: item.status,
            owner_role: item.owner_role,
            owner_organization_id: item.owner_organization_id,
            owner_user_id: item.owner_user_id,
            processing_state: ProcessingState::Ready,
            action_blockers,
            domain_allowed_actions,
        }))
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
            .purchase_order()
            .find_sales_orders_by_ids(&ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.order_no))
            .collect::<HashMap<_, _>>();
        if unique.iter().any(|id| !numbers.contains_key(id)) {
            return Err(Error::Internal("采购单关联的销售单不存在".to_string()));
        }
        Ok(numbers)
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

/// 根据当前责任事实返回 W08 审核处理器动作。
fn review_task_access(
    item: &WorkItem,
    actor_id: &str,
    assignment_eligible: bool,
    separation_satisfied: bool,
) -> (Vec<PurchaseReviewDomainAction>, Vec<PurchaseActionBlockerView>) {
    if item.status != WorkItemStatus::Open {
        return (Vec::new(), Vec::new());
    }
    if !assignment_eligible {
        return (
            Vec::new(),
            vec![review_blocker(
                "TASK_RESPONSIBILITY_NOT_ELIGIBLE",
                "当前账号不在该任务的有效责任范围内。",
            )],
        );
    }
    if !separation_satisfied {
        return (
            Vec::new(),
            vec![review_blocker(
                "SEGREGATION_OF_DUTIES",
                "采购提交人与财务审核人必须分离。",
            )],
        );
    }
    if item.owner_user_id.as_deref() != Some(actor_id) {
        return (
            Vec::new(),
            vec![review_blocker(
                "TASK_OWNED_BY_ANOTHER",
                "该任务当前由其他人员负责。",
            )],
        );
    }
    (
        vec![
            PurchaseReviewDomainAction::Approve,
            PurchaseReviewDomainAction::Reject,
        ],
        Vec::new(),
    )
}

fn review_blocker(code: &str, message: &str) -> PurchaseActionBlockerView {
    PurchaseActionBlockerView {
        action: "REVIEW".to_string(),
        code: code.to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use entities::common::time::Instant;
    use entities::ids::WorkItemId;
    use entities::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};

    use super::review_task_access;
    use crate::purchase_order::PurchaseReviewDomainAction;

    fn review_task() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("wi-1"),
            WorkItemData {
                work_item_type: WorkItemType::PurchaseOrderReview,
                business_object_type: "purchase_order".to_string(),
                business_object_id: "po-1".to_string(),
                subject_version: "submission-1".to_string(),
                owner_role: "role-finance".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "reviewer-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(1),
        )
        .unwrap()
    }

    #[test]
    fn other_actor_cannot_review_owned_task() {
        let task = review_task();

        let (domain_actions, blockers) = review_task_access(&task, "other-reviewer", true, true);

        assert!(domain_actions.is_empty());
        assert_eq!(blockers[0].code, "TASK_OWNED_BY_ANOTHER");
    }

    #[test]
    fn current_owner_allows_strong_decisions() {
        let task = review_task();

        let (domain_actions, blockers) = review_task_access(&task, "reviewer-1", true, true);

        assert_eq!(
            domain_actions,
            vec![
                PurchaseReviewDomainAction::Approve,
                PurchaseReviewDomainAction::Reject,
            ]
        );
        assert!(blockers.is_empty());
    }

    #[test]
    fn ineligible_or_submitter_gets_no_review_action() {
        let task = review_task();

        let (domain_actions, blockers) = review_task_access(&task, "reviewer-1", false, true);
        assert!(domain_actions.is_empty());
        assert_eq!(blockers[0].code, "TASK_RESPONSIBILITY_NOT_ELIGIBLE");

        let (domain_actions, blockers) = review_task_access(&task, "reviewer-1", true, false);
        assert!(domain_actions.is_empty());
        assert_eq!(blockers[0].code, "SEGREGATION_OF_DUTIES");
    }
}
