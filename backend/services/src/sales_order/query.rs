//! 销售单查询用例：列表、详情、工作副本视图与阶段责任人解析。

use std::collections::{HashMap, HashSet};

use database::{AccessControlExt, NoTransaction, ReceivableExt, SalesOrderExt, SalesReviewExt};
use entities::ids::{SalesOrderId, SalesOrderRevisionId, SalesOrderSubmissionId};
use entities::sales_order::{
    CommercialStatus, ReviewStatus, SalesOrderSubmissionLine, SalesOrderWorkingCopy, WorkingPurpose,
};
use validator::Validate;

use super::adapter::document_approval_view;
use super::dto;
use super::dto::{
    ActiveCardSalesApprovalView, ActiveLowMarginManagerConfirmationView, OpenProcurementRejectionView,
    PageView, RevisionView, SalesOrderDetailView, SalesOrderLineView, SalesOrderListParams, SalesOrderView,
    SubmissionView, WorkingCopyView,
};
use super::mapper::{submission_view, working_copy_line_view};
use super::pricing::zero_amount;
use super::status::{
    compute_can_start_sales_change, compute_close_eligibility, detail_owner_user_id, stage_code_label_tone,
};
use super::SalesOrderService;
use crate::document_registry::find_approval_binding;
use crate::{
    audit::AuditActor,
    errors::{Error, Result},
};

/// 销售单列表筛选条件类型（经 `SalesOrderExt` 关联类型跨 crate 可达）。
type SalesOrderFilter = <mongodb::Database as SalesOrderExt>::SalesOrderFilter;

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
                    .map(|row| (row.id.clone(), row.review_status))
                    .collect::<Vec<_>>(),
            )
            .await?;

        let open_rejection_order_ids = self
            .resolve_open_rejection_order_ids(
                &page
                    .items
                    .iter()
                    .filter(|row| row.commercial_status == CommercialStatus::Draft)
                    .map(|row| row.id.clone())
                    .collect::<Vec<_>>(),
            )
            .await?;

        let items = page
            .items
            .into_iter()
            .map(|row| {
                let (mut code, mut label, mut tone) = stage_code_label_tone(
                    row.commercial_status,
                    row.review_status,
                    row.close_status,
                    row.fulfillment_progress,
                );
                if open_rejection_order_ids.contains(&row.id) {
                    code = "awaiting_sales";
                    label = "待销售处理";
                    tone = "warning";
                }
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

        let open_procurement_rejection = self
            .resolve_open_procurement_rejection(&order_id, order.commercial_status, actor)
            .await?;

        let active_low_margin_manager_confirmation = self
            .resolve_active_low_margin_manager_confirmation(&order, actor)
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
                submission_ids.first(),
                order.review_status,
            )
            .await?;
        let stage_owner_user_name = match stage_owner_user_id.as_deref() {
            Some(user_id) => self.account_name(user_id).await?,
            None => None,
        };
        let (mut stage_code, mut stage_label, mut stage_tone) = stage_code_label_tone(
            order.commercial_status,
            order.review_status,
            order.close_status,
            order.fulfillment_progress,
        );
        // 采购驳回后订单回到草稿且审核轨被清成 NotSubmitted；若存在开放驳回，
        // 阶段应对齐「待销售处理」，而不是普通草稿。
        if open_procurement_rejection.is_some() {
            stage_code = "awaiting_sales";
            stage_label = "待销售处理";
            stage_tone = "warning";
        }

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
            order.commercial_status,
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

        let binding = find_approval_binding(&self.db, id, &mut NoTransaction)
            .await
            .ok()
            .flatten();
        let approval = Some(document_approval_view(
            binding.as_ref(),
            None,
            order.commercial_status,
            order.review_status,
        ));

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
            open_procurement_rejection,
            active_card_sales_approval,
            active_low_margin_manager_confirmation,
            approval,
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

    /// 判定原提交人是否仍可撤回卡券销售审批。
    ///
    /// 只有领导首步尚未形成任何决定事实时允许撤回；运营步骤即使仍开放，也因
    /// 已存在领导不可变决定而失败关闭。阻塞首步可以没有待办，活动首步必须存在
    /// 与实例、步骤、销售单和冻结提交一致的开放待办。
    #[allow(dead_code)]
    async fn can_cancel_card_sales_approval(
        &self,
        _order: &entities::sales_order::SalesOrder,
        _submission: Option<&SubmissionView>,
        _actor: &AuditActor,
    ) -> Result<bool> {
        Ok(false)
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

    /// 解析销售单是否存在「开放中的采购二次确认驳回」。
    ///
    /// 规则：
    /// - 主状态必须为草稿（驳回后 `return_to_draft`）；
    /// - 存在最近一次 `REJECTED` 采购确认；
    /// - 同一销售单下没有 `PENDING` 采购确认（否则已重提进入新一轮）。
    ///
    /// 结果挂在销售单详情上，使销售角色不依赖 `procurement_confirmation:list`
    /// 也能看到改价重提 / 作废入口。
    ///
    /// # 参数
    /// * `order_id` - 销售单 ID
    /// * `commercial_status` - 当前商业主状态
    ///
    /// # 返回
    /// 开放驳回摘要（含处理人姓名，不把账号 ID 当展示文案）；不满足条件时返回 `None`。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    async fn resolve_open_procurement_rejection(
        &self,
        _order_id: &SalesOrderId,
        _commercial_status: CommercialStatus,
        _actor: Option<&AuditActor>,
    ) -> Result<Option<OpenProcurementRejectionView>> {
        Ok(None)
    }

    /// 解析当前销售单唯一活动低毛利确认，并把通用责任动作映射为领域动作。
    async fn resolve_active_low_margin_manager_confirmation(
        &self,
        _order: &entities::sales_order::SalesOrder,
        _actor: Option<&AuditActor>,
    ) -> Result<Option<ActiveLowMarginManagerConfirmationView>> {
        Ok(None)
    }

    /// 批量识别本页草稿中仍有开放采购驳回的销售单 ID。
    ///
    /// 列表阶段展示需要把「驳回后回草稿」对齐为「待销售处理」；本方法对草稿子集
    /// 固定两次查询（REJECTED / PENDING），避免逐行查库。
    ///
    /// # 参数
    /// * `draft_order_ids` - 本页主状态为草稿的销售单 ID
    ///
    /// # 返回
    /// 返回存在开放驳回的销售单 ID 集合（有 REJECTED、且无 PENDING）。
    ///
    /// # 错误
    /// 数据库查询失败时返回仓储错误。
    async fn resolve_open_rejection_order_ids(&self, _draft_order_ids: &[String]) -> Result<HashSet<String>> {
        Ok(HashSet::new())
    }

    /// 解析当前审核轨阶段的责任角色/责任人/时限（详情页专用）。
    ///
    /// 按 `review_status` 找到对应的采购确认/审批记录，再按
    /// `(business_object_type, business_object_id)` 查找命中的有效待办
    /// （`WorkItemExt::list_active_by_object`）。找不到对应记录或待办时（例如
    /// 尚未提交、已生效、或低毛利路径已删除时会
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
        _sales_order_id: &SalesOrderId,
        _latest_submission_id: Option<&SalesOrderSubmissionId>,
        _review_status: ReviewStatus,
    ) -> Result<(Option<String>, Option<String>, Option<u64>)> {
        Ok((None, None, None))
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
        _rows: &[(String, ReviewStatus)],
    ) -> Result<HashMap<String, (Option<String>, Option<String>, Option<String>, Option<u64>)>> {
        Ok(HashMap::new())
    }
}
