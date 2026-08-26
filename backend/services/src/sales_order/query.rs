//! 销售单查询用例：列表、详情、工作副本视图与阶段责任人解析。

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use database::{
    AccessControlExt, NoTransaction, PurchaseOrderExt, ReceivableExt, SalesOrderExt, SalesReviewExt,
    WorkItemExt,
};
use entities::ids::{SalesOrderId, SalesOrderRevisionId, SalesOrderSubmissionId};
use entities::sales_order::{
    BusinessType, ReviewStatus, SalesOrderRevision, SalesOrderRevisionLine, SalesOrderSubmissionLine,
    SalesOrderWorkingCopy, WorkingPurpose,
};
use entities::Permission;
use validator::Validate;

use super::adapter::document_type_for_sales_create;
use super::approval_query::load_document_approval;
use super::dto;
use super::dto::{
    ActiveCardSalesApprovalView, PageView, PurchaseCreationAccessView, RevisionView, SalesOrderDetailView,
    SalesOrderLineView, SalesOrderListParams, SalesOrderView, SalesProcurementCoverageView, SubmissionView,
    WorkingCopyView,
};
use super::mapper::{revision_view, submission_view, working_copy_line_view};
use super::pricing::zero_amount;
use super::status::{
    compute_can_start_sales_change, compute_close_eligibility, detail_owner_user_id, stage_code_label_tone,
    CloseEligibilityInputs,
};
use super::SalesOrderService;
use crate::document_registry::find_approval_binding;
use crate::{
    audit::AuditActor,
    errors::{Error, Result},
    iam::subject,
};

/// 销售单列表筛选条件类型（经 `SalesOrderExt` 关联类型跨 crate 可达）。
type SalesOrderFilter = <mongodb::Database as SalesOrderExt>::SalesOrderFilter;

/// 构造尚无当前销售版本时的零采购覆盖视图。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回目标、覆盖、剩余和进度均为零的视图。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 零值只用于未生效销售单，不掩盖已生效销售单的当前版本缺失。
fn empty_sales_procurement_coverage() -> SalesProcurementCoverageView {
    SalesProcurementCoverageView {
        total_quantity: entities::money::Quantity::from_str("0").expect("零数量合法"),
        covered_quantity: entities::money::Quantity::from_str("0").expect("零数量合法"),
        remaining_quantity: entities::money::Quantity::from_str("0").expect("零数量合法"),
        progress: entities::money::Rate::from_str("0").expect("零进度合法"),
    }
}

/// 按销售版本 ID 分组公共行，并在组内按行号升序。
///
/// # 参数
/// * `lines` - 一次批量查出的公共行版本
///
/// # 返回
/// 返回以版本 ID 为键的行清单。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 组内必须按 `line_no` 升序，详情页明细顺序与落库行号一致。
fn group_revision_lines(lines: Vec<SalesOrderRevisionLine>) -> HashMap<String, Vec<SalesOrderRevisionLine>> {
    let mut grouped: HashMap<String, Vec<SalesOrderRevisionLine>> = HashMap::new();
    for line in lines {
        grouped
            .entry(line.sales_order_revision_id.to_string())
            .or_default()
            .push(line);
    }
    for group in grouped.values_mut() {
        group.sort_by_key(|line| line.line_no);
    }
    grouped
}

/// 在本单版本列表内解析每个版本的前一版本号。
///
/// # 参数
/// * `revisions` - 同一销售单的正式版本
///
/// # 返回
/// 返回「当前版本 ID → 前一版本号」；找不到前一版本时不写入。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 只在本单已加载的版本列表内解析，不另查仓储。
fn previous_revision_numbers(revisions: &[SalesOrderRevision]) -> HashMap<String, u32> {
    let by_id: HashMap<&str, u32> = revisions
        .iter()
        .map(|row| (row.base.id.as_str(), row.revision.revision_no))
        .collect();
    let mut out = HashMap::new();
    for row in revisions {
        let Some(previous_id) = row.previous_revision_id.as_ref() else {
            continue;
        };
        let Some(previous_no) = by_id.get(previous_id.as_ref()) else {
            continue;
        };
        out.insert(row.base.id.clone(), *previous_no);
    }
    out
}

impl SalesOrderService {
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
    #[tracing::instrument(
        name = "sales_order.list",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "list")
    )]
    pub async fn sales_order_list(&self, params: &SalesOrderListParams) -> Result<PageView<SalesOrderView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesOrderFilter {
            order_no: query.order_no,
            customer_id: query.customer_id,
            contract_id: query.contract_id,
            origin_system: query.origin_system,
            commercial_status: query.commercial_status,
            review_status: query.review_status,
            business_type: query.business_type,
            fulfillment_progress: query.fulfillment_progress,
            collection_progress: query.collection_progress,
            invoice_progress: query.invoice_progress,
            close_status: query.close_status,
            created_from: query.created_from,
            created_to: query.created_to,
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
                    .map(|row| (row.id.clone(), row.business_type, row.review_status))
                    .collect::<Vec<_>>(),
            )
            .await?;
        let owner_names = self
            .resolve_account_names_batch(
                &page
                    .items
                    .iter()
                    .map(|row| row.created_by.clone())
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
                let (owner_role, stage_owner_user_id, stage_owner_user_name, due_at) =
                    owners.get(&row.id).cloned().unwrap_or_default();
                let owner_user_id = row.created_by.clone();
                let owner_user_name = owner_names.get(&owner_user_id).cloned();
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
                    owner_user_id,
                    owner_user_name,
                    stage: dto::SalesOrderStageSummary {
                        code,
                        label,
                        tone,
                        owner_role,
                        owner_user_id: stage_owner_user_id,
                        owner_user_name: stage_owner_user_name,
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
    #[tracing::instrument(
        name = "sales_order.detail",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "detail")
    )]
    pub async fn sales_order_detail(
        &self,
        id: &str,
        actor: Option<&AuditActor>,
    ) -> Result<SalesOrderDetailView> {
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

        let submissions = self
            .db
            .sales_order_submissions()
            .list_by_order_newest_first(&order_id, &mut NoTransaction)
            .await?;
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

        let revisions = self.load_revision_views(&order_id).await?;

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
        let purchase_order_count = self
            .db
            .purchase_orders()
            .count_active_by_sales_order(&order_id, &mut NoTransaction)
            .await?;
        let purchase_coverage = self.sales_procurement_coverage(&order).await?;
        let purchase_creation_access = self
            .purchase_creation_access(&order, &purchase_coverage, actor)
            .await?;

        let active_card_sales_approval = match (actor, submission_ids.first()) {
            (Some(actor), Some(submission_id)) => {
                self.resolve_active_card_sales_approval(
                    &order,
                    submission_id,
                    submission_views.first(),
                    actor,
                )
                .await?
            }
            _ => None,
        };

        let (stage_owner_role, stage_owner_user_id, stage_due_at) = self
            .resolve_stage_owner(
                &SalesOrderId::new(order.base.id.clone()),
                order.business_type,
                order.review_status,
            )
            .await?;
        let stage_owner_user_name = match stage_owner_user_id.as_deref() {
            Some(user_id) => self.account_name(user_id).await?,
            None => None,
        };
        let (stage_code, stage_label, stage_tone) = stage_code_label_tone(
            order.commercial_status,
            order.review_status,
            order.close_status,
            order.fulfillment_progress,
        );

        let receivable_accounts = self
            .db
            .receivable_accounts()
            .list_by_sales_order(&order_id, &mut NoTransaction)
            .await?;
        let (settled_total, invoiced_total, gross_total) = receivable_accounts.iter().fold(
            (zero_amount(), zero_amount(), zero_amount()),
            |(settled, invoiced, gross), account| {
                (
                    settled.checked_add(account.settled_total),
                    invoiced.checked_add(account.invoiced_total),
                    gross.checked_add(account.gross_total),
                )
            },
        );
        let close_eligibility = compute_close_eligibility(CloseEligibilityInputs {
            business_type: order.business_type,
            commercial: order.commercial_status,
            close: order.close_status,
            fulfillment: order.fulfillment_progress,
            collection: order.collection_progress,
            invoice: order.invoice_progress,
            settled_total,
            gross_total,
        });

        let has_active_change_order = match order.current_revision_id() {
            Some(revision_id) => {
                self.db
                    .sales_change_orders()
                    .has_in_progress_by_order_and_base(
                        &order_id,
                        &SalesOrderRevisionId::new(revision_id),
                        &mut NoTransaction,
                    )
                    .await?
            }
            None => false,
        };
        let (can_start_sales_change_order, change_order_blocker) = compute_can_start_sales_change(
            order.origin_system,
            stage_code,
            stage_label,
            has_active_change_order,
        );

        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .ok()
            .flatten();
        let approval = Some(
            load_document_approval(
                &self.db,
                order.business_type,
                id,
                binding.as_ref(),
                order.commercial_status,
                order.review_status,
            )
            .await?,
        );

        Ok(SalesOrderDetailView {
            id: order.base.id.clone(),
            order_no: order.order_no.clone(),
            business_type: order.business_type,
            origin_system: order.origin_system,
            customer_id: order.customer_id.to_string(),
            contract_id: order.contract_id.as_ref().map(ToString::to_string),
            settlement_party_id: order.settlement_party_id.to_string(),
            commercial_status: order.commercial_status,
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
            purchase_order_count,
            purchase_coverage,
            purchase_creation_access,
            settled_total,
            invoiced_total,
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
            revisions,
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
            active_card_sales_approval,
            approval,
        })
    }

    /// 组装销售单正式版本历史视图（含当时表头快照与明细摘要）。
    ///
    /// # 参数
    /// * `order_id` - 稳定销售单
    ///
    /// # 返回
    /// 返回按版本号倒序的版本视图。
    ///
    /// # 错误
    /// * `RepositoryError` - 查询正式版本或版本行失败
    ///
    /// # 关键业务约束
    /// 版本行按版本 ID 一次批量取出，禁止按版本循环查询。
    async fn load_revision_views(&self, order_id: &SalesOrderId) -> Result<Vec<RevisionView>> {
        let revisions = self
            .db
            .sales_order_revisions()
            .list_by_order(order_id, &mut NoTransaction)
            .await?;
        let revision_ids = revisions
            .iter()
            .map(|row| SalesOrderRevisionId::new(row.base.id.clone()))
            .collect::<Vec<_>>();
        let lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revisions(&revision_ids, &mut NoTransaction)
            .await?;
        let mut lines_by_revision = group_revision_lines(lines);
        let previous_nos = previous_revision_numbers(&revisions);
        Ok(revisions
            .into_iter()
            .map(|revision| {
                let lines = lines_by_revision.remove(&revision.base.id).unwrap_or_default();
                let previous_revision_no = previous_nos.get(&revision.base.id).copied();
                revision_view(revision, lines, previous_revision_no)
            })
            .collect())
    }

    /// 计算当前账号从销售单继续执行供给分配的访问投影。
    ///
    /// # 参数
    /// * `order` - 当前销售稳定单
    /// * `coverage` - 按销售与采购当前版本计算的采购覆盖
    /// * `actor` - 当前已认证账号；内部无账号上下文时为空
    ///
    /// # 返回
    /// 返回账号状态、静态供给分配权限与开放责任任务共同决定的访问投影。
    ///
    /// # 错误
    /// 账号、任务或 RBAC 查询失败，以及权限值对象不合法时返回错误。
    ///
    /// # 关键业务约束
    /// `allowed` 只在账号仍可登录、身份未变化、拥有 `purchase_order:create`
    /// 且持有该销售单开放供给分配任务时为真，与 basis/create 接口的认证和
    /// 授权边界一致。
    async fn purchase_creation_access(
        &self,
        order: &entities::sales_order::SalesOrder,
        coverage: &SalesProcurementCoverageView,
        actor: Option<&AuditActor>,
    ) -> Result<PurchaseCreationAccessView> {
        if let Some(message) = order.procurement_creation_blocker(coverage.remaining_quantity) {
            return Ok(blocked_purchase_creation_access(message));
        }
        let Some(actor) = actor else {
            return Ok(blocked_purchase_creation_access("当前调用缺少供给分配责任上下文"));
        };
        if let Some(message) = self.purchase_creation_actor_blocker(actor).await? {
            return Ok(blocked_purchase_creation_access(message));
        }
        let task_count = self
            .purchase_creation_task_count(&order.base.id, actor.id())
            .await?;
        if task_count == 0 {
            return Ok(blocked_purchase_creation_access(
                "当前账号不是该销售单供给分配任务负责人",
            ));
        }
        Ok(PurchaseCreationAccessView {
            allowed: true,
            task_count,
            blocker: None,
        })
    }

    /// 重验当前账号的登录状态、身份与供给分配权限。
    ///
    /// # 参数
    /// * `actor` - JWT 已认证但需要按当前账号和 RBAC 事实重验的操作人
    ///
    /// # 返回
    /// 账号与权限均有效时返回 `None`；否则返回可直接下发的明确 blocker。
    ///
    /// # 错误
    /// 账号或 RBAC 查询失败，以及权限值对象不合法时返回错误。
    ///
    /// # 关键业务约束
    /// 必须使用当前账号记录和规范 Casbin 主体，不能仅信任请求中的历史认证
    /// 快照。
    async fn purchase_creation_actor_blocker(&self, actor: &AuditActor) -> Result<Option<&'static str>> {
        let account = self
            .db
            .accounts()
            .find_by_id(actor.id(), &mut NoTransaction)
            .await?;
        let Some(account) = account.filter(|account| account.kind == actor.kind() && account.can_login())
        else {
            return Ok(Some("当前账号不存在、已停用或身份已变化，不能分配供给"));
        };
        let permission = Permission::parse("purchase_order:create")?;
        let allowed = self
            .require_rbac()?
            .enforce(&subject(account.kind, &account.base.id), &permission)
            .await?;
        Ok((!allowed).then_some("当前账号缺少 purchase_order:create 权限"))
    }

    /// 统计当前账号在指定销售单下拥有的开放供给分配任务。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单稳定主键
    /// * `actor_id` - 已通过当前账号与采购建单权限重验的账号主键
    ///
    /// # 返回
    /// 返回该账号拥有的开放供给分配任务数量。
    ///
    /// # 错误
    /// 工作项查询失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 只统计任务仓储认定为开放且由当前账号负责的供给分配任务。
    async fn purchase_creation_task_count(&self, sales_order_id: &str, actor_id: &str) -> Result<usize> {
        Ok(self
            .db
            .work_items()
            .list_open_procurement_owned_by(actor_id, Some(sales_order_id), None, &mut NoTransaction)
            .await?
            .len())
    }

    /// 计算销售单当前版本采购目标、覆盖、剩余与进度。
    ///
    /// # 参数
    /// * `order` - 销售稳定单
    ///
    /// # 返回
    /// 有当前版本时返回按采购当前指针计算的覆盖视图；草稿无当前版本时返回零值。
    ///
    /// # 错误
    /// 当前版本、采购指针或覆盖数量不一致，以及仓储读取失败时返回错误。
    ///
    /// # 关键业务约束
    /// 正式采购只读取当前采购版本及其 allocation，草稿类只读取当前提交。
    async fn sales_procurement_coverage(
        &self,
        order: &entities::sales_order::SalesOrder,
    ) -> Result<SalesProcurementCoverageView> {
        if order.current_revision_id().is_none() {
            return Ok(empty_sales_procurement_coverage());
        }
        let coverage = crate::purchase_order::coverage::load_sales_procurement_coverage(
            &self.db,
            order,
            &mut NoTransaction,
        )
        .await?;
        Ok(SalesProcurementCoverageView {
            total_quantity: coverage.summary.total_quantity,
            covered_quantity: coverage.summary.covered_quantity,
            remaining_quantity: coverage.summary.remaining_quantity,
            progress: coverage.summary.progress,
        })
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
    pub(super) async fn working_copy_view(&self, copy: &SalesOrderWorkingCopy) -> Result<WorkingCopyView> {
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
            contract_revision_id: copy.contract_revision_id.as_ref().map(ToString::to_string),
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
            target_mall_id: copy.target_mall_id.as_ref().map(ToString::to_string),
            receivable_due_date: copy.receivable_due_date,
            gross_amount: copy.gross_amount,
            net_amount: copy.net_amount,
            tax_amount: copy.tax_amount,
            lines: lines.into_iter().map(working_copy_line_view).collect(),
        })
    }

    /// 构建当前操作人可安全执行的卡券审批工作面投影。
    async fn resolve_active_card_sales_approval(
        &self,
        _order: &entities::sales_order::SalesOrder,
        _submission_id: &SalesOrderSubmissionId,
        _submission: Option<&SubmissionView>,
        _actor: &AuditActor,
    ) -> Result<Option<ActiveCardSalesApprovalView>> {
        Ok(None)
    }

    /// 按账号 ID 查询展示姓名。
    ///
    /// 用于销售单负责人、阶段责任人和采购驳回处理人，避免把账号 ID 下发给页面。
    ///
    /// # 参数
    /// * `user_id` - 账号 ID
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

    /// 解析当前审核轨阶段的责任角色、责任人和时限（详情页专用）。
    ///
    /// # 参数
    /// * `sales_order_id` - 销售单稳定身份
    /// * `business_type` - 业务性质，用于确定审批任务对象类型
    /// * `review_status` - 当前审核轨状态
    ///
    /// # 返回
    /// 返回 `(责任角色, 责任人账号, 时限)`；非在途状态或无开放审批任务时均为空。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    async fn resolve_stage_owner(
        &self,
        sales_order_id: &SalesOrderId,
        business_type: BusinessType,
        review_status: ReviewStatus,
    ) -> Result<(Option<String>, Option<String>, Option<u64>)> {
        if !review_status.has_active_review_task() {
            return Ok((None, None, None));
        }
        let object_type = document_type_for_sales_create(business_type).as_str().to_string();
        let tasks = self
            .db
            .work_items()
            .list_active_approval_by_objects(&[(object_type, sales_order_id.to_string())], &mut NoTransaction)
            .await?;
        let Some(task) = tasks.into_iter().next() else {
            return Ok((None, None, None));
        };
        Ok((
            Some(task.owner_role),
            task.owner_user_id,
            task.due_at.map(|due_at| due_at.unix_secs() as u64),
        ))
    }

    /// 批量解析本页销售单的当前阶段责任人和时限。
    ///
    /// # 参数
    /// * `rows` - 本页销售单 `(id, 业务性质, 审核轨状态)` 集合
    ///
    /// # 返回
    /// 返回按销售单 ID 索引的 `(责任角色, 责任人账号, 责任人姓名, 时限)`；
    /// 非在途状态或无开放审批任务的销售单不进入结果。
    ///
    /// # 错误
    /// 工作项或账号批量查询失败时返回仓储错误。
    async fn resolve_stage_owners_batch(
        &self,
        rows: &[(String, BusinessType, ReviewStatus)],
    ) -> Result<HashMap<String, (Option<String>, Option<String>, Option<String>, Option<u64>)>> {
        let business_objects = rows
            .iter()
            .filter(|(_, _, review_status)| review_status.has_active_review_task())
            .map(|(id, business_type, _)| {
                (
                    document_type_for_sales_create(*business_type)
                        .as_str()
                        .to_string(),
                    id.clone(),
                )
            })
            .collect::<Vec<_>>();
        let tasks = self
            .db
            .work_items()
            .list_active_approval_by_objects(&business_objects, &mut NoTransaction)
            .await?;
        let owner_names = self
            .resolve_account_names_batch(
                &tasks
                    .iter()
                    .filter_map(|task| task.owner_user_id.clone())
                    .collect::<Vec<_>>(),
            )
            .await?;
        Ok(tasks
            .into_iter()
            .map(|task| {
                let owner_name = task
                    .owner_user_id
                    .as_ref()
                    .and_then(|owner_id| owner_names.get(owner_id).cloned());
                (
                    task.business_object_id,
                    (
                        Some(task.owner_role),
                        task.owner_user_id,
                        owner_name,
                        task.due_at.map(|due_at| due_at.unix_secs() as u64),
                    ),
                )
            })
            .collect())
    }

    /// 批量解析账号展示姓名。
    ///
    /// # 参数
    /// * `account_ids` - 账号 ID 集合，允许重复或为空
    ///
    /// # 返回
    /// 返回按账号 ID 索引的展示姓名；已删除或不存在的账号不会进入结果。
    ///
    /// # 错误
    /// 账号仓储查询失败时返回仓储错误。
    async fn resolve_account_names_batch(&self, account_ids: &[String]) -> Result<HashMap<String, String>> {
        let unique_ids = account_ids
            .iter()
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let accounts = self
            .db
            .accounts()
            .list_by_ids(&unique_ids, &mut NoTransaction)
            .await?;
        Ok(accounts
            .into_iter()
            .map(|account| (account.base.id, account.name))
            .collect())
    }
}

/// 构造禁止创建采购单的稳定访问投影。
///
/// # 参数
/// * `message` - 可直接展示给调用方的明确业务阻塞说明
///
/// # 返回
/// 返回 `allowed = false`、任务数为零且携带 blocker 的访问投影。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 禁止分支不得泄露任何开放任务数量，避免把未授权任务事实下发给调用方。
fn blocked_purchase_creation_access(message: &str) -> PurchaseCreationAccessView {
    PurchaseCreationAccessView {
        allowed: false,
        task_count: 0,
        blocker: Some(message.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use entities::sales_order::ReviewStatus;

    use super::blocked_purchase_creation_access;

    #[test]
    fn unified_and_legacy_pending_reviews_require_open_tasks() {
        for status in [
            ReviewStatus::InApproval,
            ReviewStatus::PendingProcurementConfirmation,
            ReviewStatus::PendingLowMarginSuperior,
            ReviewStatus::PendingSalesLeader,
            ReviewStatus::PendingOperations,
        ] {
            assert!(status.has_active_review_task());
        }
    }

    #[test]
    fn terminal_review_states_do_not_require_open_tasks() {
        for status in [
            ReviewStatus::NotSubmitted,
            ReviewStatus::Approved,
            ReviewStatus::Rejected,
        ] {
            assert!(!status.has_active_review_task());
        }
    }

    /// 采购创建访问投影必须在查询责任任务前重验当前账号和采购建单权限。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无；账号状态、规范主体、权限或检查顺序缺失时测试失败。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 详情页 `allowed` 必须与 basis/create 接口的当前认证和 RBAC 边界一致。
    #[test]
    fn purchase_creation_access_revalidates_account_and_permission_before_tasks() {
        let source = include_str!("query.rs");
        let access = source
            .split_once("async fn purchase_creation_access")
            .expect("必须存在采购创建访问投影")
            .1;
        let actor_gate = access
            .find("purchase_creation_actor_blocker(actor)")
            .expect("必须先重验账号与权限");
        let task_query = access
            .find("purchase_creation_task_count")
            .expect("必须查询开放采购任务");
        assert!(actor_gate < task_query);

        let actor_check = source
            .split_once("async fn purchase_creation_actor_blocker")
            .expect("必须存在账号与权限重验 helper")
            .1;
        let account = actor_check.find(".accounts()").expect("必须加载当前账号");
        let can_login = actor_check
            .find("account.can_login()")
            .expect("必须检查当前账号可登录");
        let permission = actor_check
            .find("Permission::parse(\"purchase_order:create\")")
            .expect("必须解析采购建单权限");
        let enforce = actor_check
            .find(".enforce(&subject(account.kind, &account.base.id), &permission)")
            .expect("必须用规范账号主体重验权限");
        assert!(account < can_login);
        assert!(can_login < permission);
        assert!(permission < enforce);
        let inactive_blocker = "当前账号不存在、已停用或身份已变化，不能分配供给";
        assert!(actor_check.contains(inactive_blocker));
        assert!(actor_check.contains("当前账号缺少 purchase_order:create 权限"));
    }

    /// 禁止访问投影必须返回明确 blocker 且隐藏任务数量。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无；禁止投影仍允许创建、泄露任务数或缺少说明时测试失败。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 未授权账号不能从销售详情推断其名下或他人的采购任务数量。
    #[test]
    fn blocked_purchase_creation_access_is_explicit_and_hides_tasks() {
        let view = blocked_purchase_creation_access("当前账号缺少 purchase_order:create 权限");

        assert!(!view.allowed);
        assert_eq!(view.task_count, 0);
        assert_eq!(
            view.blocker.as_deref(),
            Some("当前账号缺少 purchase_order:create 权限")
        );
    }
}
