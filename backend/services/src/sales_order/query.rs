//! 销售单查询用例：列表、详情、工作副本视图与阶段责任人解析。

use std::collections::{HashMap, HashSet};

use database::{
    AccessControlExt, ApprovalExt, NoTransaction, ReceivableExt, SalesOrderExt, SalesReviewExt, WorkItemExt,
};
use entities::ids::{ApprovalInstanceId, SalesOrderId, SalesOrderRevisionId, SalesOrderSubmissionId};
use entities::sales_order::{
    CommercialStatus, ReviewStatus, SalesOrderSubmissionLine, SalesOrderWorkingCopy, WorkingPurpose,
};
use entities::sales_review::ProcurementConfirmationStatus;
use entities::work_item::{WorkItem, WorkItemStatus};
use entities::Permission;
use validator::Validate;

use super::dto;
use super::dto::{
    ActiveCardSalesApprovalView, ActiveLowMarginManagerConfirmationView, CardSalesApprovalAllowedAction,
    LowMarginManagerAllowedAction, OpenProcurementRejectionView, PageView, ProcurementRejectionAllowedAction,
    RevisionView, SalesOrderDetailView, SalesOrderLineView, SalesOrderListParams, SalesOrderView,
    SubmissionView, WorkingCopyView,
};
use super::mapper::{submission_view, working_copy_line_view};
use super::pricing::zero_amount;
use super::status::{
    compute_can_start_sales_change, compute_close_eligibility, detail_owner_user_id, stage_code_label_tone,
};
use super::SalesOrderService;
use crate::{
    approval::{CARD_SALES_APPROVAL, OPERATIONS_APPROVAL, SALES_MANAGER_APPROVAL},
    audit::AuditActor,
    errors::{Error, Result},
    work_item::{ProcessingBlockerView, ProcessingState, WorkItemPartyView},
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
            .resolve_stage_owner(&order.base.id, order.review_status, submission_ids.first())
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
        order: &entities::sales_order::SalesOrder,
        submission_id: &SalesOrderSubmissionId,
        submission: Option<&SubmissionView>,
        actor: &AuditActor,
    ) -> Result<Option<ActiveCardSalesApprovalView>> {
        if order.business_type != entities::sales_order::BusinessType::Voucher
            || !matches!(
                order.review_status,
                ReviewStatus::PendingSalesLeader | ReviewStatus::PendingOperations
            )
        {
            return Ok(None);
        }
        let Some(instance) = self
            .db
            .approval_instances()
            .find_non_terminal_by_subject(
                CARD_SALES_APPROVAL,
                "sales_order",
                &order.base.id,
                submission_id.as_ref(),
                &mut NoTransaction,
            )
            .await?
        else {
            return Ok(None);
        };
        let step = self
            .db
            .approval_step_instances()
            .find_current_by_instance(&ApprovalInstanceId::new(&instance.base.id), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("活动审批实例缺少当前步骤".to_string()))?;
        let work_item = self
            .db
            .work_items()
            .find_one(
                mongodb::bson::doc! {
                    "approval_step_instance_id": &step.base.id,
                    "status": WorkItemStatus::Open.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        let blocker = if instance.status == entities::approval::ApprovalInstanceStatus::Blocked
            || step.status == entities::approval::ApprovalStepStatus::Blocked
        {
            Some(ProcessingBlockerView {
                code: instance
                    .blocker_code
                    .clone()
                    .or(step.blocker_code.clone())
                    .unwrap_or_else(|| "APPROVAL_BLOCKED".to_string()),
                message: "审批当前受阻，请等待管理员恢复。".to_string(),
            })
        } else {
            None
        };
        let mut allowed_actions = match (work_item.as_ref(), self.rbac.as_ref()) {
            (Some(item), Some(rbac)) if blocker.is_none() => {
                match crate::work_item::WorkItemService::new(self.db.clone(), rbac.clone())
                    .work_item_detail(&item.base.id, actor)
                    .await
                {
                    Ok(task) => task
                        .allowed_actions
                        .into_iter()
                        .filter_map(|action| match action {
                            crate::work_item::WorkItemAllowedAction::StartProcessing => {
                                Some(CardSalesApprovalAllowedAction::StartProcessing)
                            }
                            crate::work_item::WorkItemAllowedAction::Process
                                if task.owner_user_id.as_deref() == Some(actor.id()) =>
                            {
                                Some(CardSalesApprovalAllowedAction::Approve)
                            }
                            _ => None,
                        })
                        .flat_map(|action| match action {
                            CardSalesApprovalAllowedAction::Approve => vec![
                                CardSalesApprovalAllowedAction::Approve,
                                CardSalesApprovalAllowedAction::Reject,
                                CardSalesApprovalAllowedAction::Terminate,
                            ],
                            other => vec![other],
                        })
                        .collect(),
                    Err(Error::Forbidden(_) | Error::NotFound(_)) => Vec::new(),
                    Err(error) => return Err(error),
                }
            }
            _ => Vec::new(),
        };
        if self
            .can_cancel_card_sales_approval(order, &instance, &step, work_item.as_ref(), submission, actor)
            .await?
        {
            allowed_actions.push(CardSalesApprovalAllowedAction::Cancel);
        }
        let owner_user = match work_item.as_ref().and_then(|item| item.owner_user_id.as_deref()) {
            Some(user_id) => Some(WorkItemPartyView {
                id: user_id.to_string(),
                display_name: self
                    .account_name(user_id)
                    .await?
                    .unwrap_or_else(|| "当前处理人".to_string()),
            }),
            None => None,
        };
        let expected_review_status = match step.step_key.as_str() {
            SALES_MANAGER_APPROVAL => "PENDING_SALES_LEAD",
            OPERATIONS_APPROVAL => "PENDING_OPERATIONS",
            _ => return Err(Error::Internal("审批当前步骤未注册销售工作面".to_string())),
        };
        let summary = submission
            .map(|submission| {
                format!(
                    "提交第 {} 版，含税金额 {}，{} 条明细",
                    submission.submission_no,
                    submission.gross_amount,
                    submission.lines.len()
                )
            })
            .unwrap_or_else(|| "冻结销售提交".to_string());
        Ok(Some(ActiveCardSalesApprovalView {
            approval_instance_id: instance.base.id,
            instance_version: instance.base.version,
            approval_step_instance_id: step.base.id,
            step_version: step.base.version,
            work_item_id: work_item.as_ref().map(|item| item.base.id.clone()),
            task_version: work_item.as_ref().map(|item| item.base.version),
            work_item_type: work_item.as_ref().map(|item| item.work_item_type),
            work_item_status: work_item.as_ref().map(|item| item.status),
            processing_state: if blocker.is_some() {
                ProcessingState::ApprovalBlocked
            } else {
                ProcessingState::Ready
            },
            processing_blocker: blocker.clone(),
            assignment_mode: work_item.as_ref().map(|item| item.assignment_mode),
            owner_user,
            subject_version: instance.subject_version,
            sales_order_submission_id: submission_id.to_string(),
            submission_no: submission.map(|item| item.submission_no).unwrap_or_default(),
            frozen_submission_summary: summary,
            expected_review_status: expected_review_status.to_string(),
            allowed_actions,
            action_blockers: blocker.into_iter().collect(),
        }))
    }

    /// 判定原提交人是否仍可撤回卡券销售审批。
    ///
    /// 只有领导首步尚未形成任何决定事实时允许撤回；运营步骤即使仍开放，也因
    /// 已存在领导不可变决定而失败关闭。阻塞首步可以没有待办，活动首步必须存在
    /// 与实例、步骤、销售单和冻结提交一致的开放待办。
    async fn can_cancel_card_sales_approval(
        &self,
        order: &entities::sales_order::SalesOrder,
        instance: &entities::approval::ApprovalInstance,
        step: &entities::approval::ApprovalStepInstance,
        work_item: Option<&WorkItem>,
        submission: Option<&SubmissionView>,
        actor: &AuditActor,
    ) -> Result<bool> {
        let Some(rbac) = self.rbac.as_ref() else {
            return Ok(false);
        };
        let permission =
            Permission::parse("sales_order:cancel_approval").expect("卡券审批撤回权限常量必须合法");
        if !rbac
            .enforce(&crate::iam::subject(actor.kind(), actor.id()), &permission)
            .await?
        {
            return Ok(false);
        }
        let Some(submission) = submission else {
            return Ok(false);
        };
        let subject_matches = instance.definition_key == CARD_SALES_APPROVAL
            && instance.definition_version == crate::approval::CARD_SALES_APPROVAL_VERSION
            && instance.business_object_type == "sales_order"
            && instance.business_object_id == order.base.id
            && instance.subject_version == submission.id
            && instance
                .current_step_instance_id
                .as_ref()
                .map(ToString::to_string)
                .as_deref()
                == Some(step.base.id.as_str())
            && step.approval_instance_id.to_string() == instance.base.id;
        let submitter_matches = instance.started_by == actor.id()
            && submission.submitted_by == actor.id()
            && submission.status == entities::sales_order::SubmissionStatus::InReview;
        let order_is_editable_after_cancel = order.commercial_status == CommercialStatus::PendingReview
            && order.review_status == ReviewStatus::PendingSalesLeader
            && order.stable.current_revision_id.is_none();
        let task_matches = cancel_work_item_matches(instance, step, work_item);
        if !subject_matches
            || !submitter_matches
            || !order_is_editable_after_cancel
            || !task_matches
            || !cancel_step_policy_allows(
                instance.status,
                step.status,
                &step.step_key,
                step.decision.is_some() || step.decided_by.is_some() || step.decided_at.is_some(),
            )
        {
            return Ok(false);
        }
        let reviews = self
            .db
            .sales_order_reviews()
            .find_many(
                mongodb::bson::doc! { "submission_id": submission.id.as_str() },
                &mut NoTransaction,
            )
            .await?;
        Ok(reviews.is_empty())
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
        order_id: &SalesOrderId,
        commercial_status: CommercialStatus,
        actor: Option<&AuditActor>,
    ) -> Result<Option<OpenProcurementRejectionView>> {
        if commercial_status != CommercialStatus::Draft {
            return Ok(None);
        }

        let pending = self
            .db
            .procurement_confirmations()
            .find_pending_by_sales_order(order_id, &mut NoTransaction)
            .await?;
        if pending.is_some() {
            return Ok(None);
        }

        let Some(rejected) = self
            .db
            .procurement_confirmations()
            .find_latest_rejected_by_sales_order(order_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };

        let handled_by = rejected.handled_by;
        let handled_by_name = match handled_by.as_deref() {
            Some(user_id) => self.account_name(user_id).await?,
            None => None,
        };

        let allowed_actions = match actor {
            Some(actor) => {
                let copy = self
                    .db
                    .sales_order_working_copies()
                    .find_active_by_order_and_purpose(
                        order_id,
                        WorkingPurpose::FirstSubmission,
                        &mut NoTransaction,
                    )
                    .await?;
                if copy
                    .as_ref()
                    .is_some_and(|copy| copy.editor_user_id == actor.id())
                {
                    vec![
                        ProcurementRejectionAllowedAction::ResubmitChangedTerms,
                        ProcurementRejectionAllowedAction::RequestLowMarginAcceptance,
                        ProcurementRejectionAllowedAction::VoidAfterRejection,
                    ]
                } else {
                    Vec::new()
                }
            }
            None => Vec::new(),
        };
        Ok(Some(OpenProcurementRejectionView {
            procurement_confirmation_id: rejected.base.id,
            submission_id: rejected.submission_id.to_string(),
            reject_reason_code: rejected.reject_reason_code.map(|code| code.as_str().to_string()),
            comment: rejected.comment,
            handled_by,
            handled_by_name,
            handled_at: rejected.handled_at.map(|instant| instant.unix_secs() as u64),
            allowed_actions,
        }))
    }

    /// 解析当前销售单唯一活动低毛利确认，并把通用责任动作映射为领域动作。
    async fn resolve_active_low_margin_manager_confirmation(
        &self,
        order: &entities::sales_order::SalesOrder,
        actor: Option<&AuditActor>,
    ) -> Result<Option<ActiveLowMarginManagerConfirmationView>> {
        let Some(actor) = actor else {
            return Ok(None);
        };
        let Some(rbac) = self.rbac.as_ref() else {
            return Ok(None);
        };
        let item = self
            .db
            .work_items()
            .list_active_by_object("sales_order", &order.base.id, &mut NoTransaction)
            .await?
            .into_iter()
            .find(|item| {
                item.work_item_type == entities::work_item::WorkItemType::LowMarginManagerConfirmation
            });
        let Some(item) = item else {
            return Ok(None);
        };
        let confirmation = self
            .db
            .low_margin_manager_confirmations()
            .find_one_by_field(
                "low_margin_submission_id",
                &item.subject_version,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("低毛利上级确认事实不存在".to_string()))?;
        let formal = match crate::work_item::WorkItemService::new(self.db.clone(), rbac.clone())
            .work_item_detail(&item.base.id, actor)
            .await
        {
            Ok(formal) => Some(formal),
            Err(Error::Forbidden(_) | Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let mut allowed_actions = Vec::new();
        if formal.as_ref().is_some_and(|formal| {
            formal
                .allowed_actions
                .contains(&crate::work_item::WorkItemAllowedAction::StartProcessing)
        }) {
            allowed_actions.push(LowMarginManagerAllowedAction::StartProcessing);
        }
        if formal.as_ref().is_some_and(|formal| {
            formal
                .allowed_actions
                .contains(&crate::work_item::WorkItemAllowedAction::Process)
        }) {
            allowed_actions.extend([
                LowMarginManagerAllowedAction::Approve,
                LowMarginManagerAllowedAction::Reject,
            ]);
        }
        Ok(Some(ActiveLowMarginManagerConfirmationView {
            confirmation_id: confirmation.base.id,
            work_item_id: item.base.id,
            task_version: item.base.version.to_string(),
            subject_version: item.subject_version,
            low_margin_submission_id: confirmation.low_margin_submission_id.to_string(),
            rejected_procurement_confirmation_id: confirmation
                .rejected_procurement_confirmation_id
                .to_string(),
            acceptance_reason: confirmation.acceptance_reason,
            evidence_reference_ids: confirmation.evidence_reference_ids,
            owner_user: formal.as_ref().and_then(|formal| formal.owner_user.clone()),
            allowed_actions,
            action_blockers: formal.map_or_else(Vec::new, |formal| formal.action_blockers),
        }))
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
    async fn resolve_open_rejection_order_ids(&self, draft_order_ids: &[String]) -> Result<HashSet<String>> {
        if draft_order_ids.is_empty() {
            return Ok(HashSet::new());
        }

        let rejected = self
            .db
            .procurement_confirmations()
            .find_many(
                mongodb::bson::doc! {
                    "sales_order_id": { "$in": draft_order_ids },
                    "status": ProcurementConfirmationStatus::Rejected.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;
        let pending = self
            .db
            .procurement_confirmations()
            .find_many(
                mongodb::bson::doc! {
                    "sales_order_id": { "$in": draft_order_ids },
                    "status": ProcurementConfirmationStatus::Pending.as_str(),
                },
                &mut NoTransaction,
            )
            .await?;

        let pending_ids: HashSet<String> = pending
            .into_iter()
            .map(|row| row.sales_order_id.to_string())
            .collect();
        let mut open = HashSet::new();
        for row in rejected {
            let order_id = row.sales_order_id.to_string();
            if !pending_ids.contains(&order_id) {
                open.insert(order_id);
            }
        }
        Ok(open)
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
        sales_order_id: &str,
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
            ReviewStatus::PendingSalesLeader | ReviewStatus::PendingOperations => {
                Some(("sales_order", sales_order_id.to_string()))
            }
            ReviewStatus::PendingLowMarginSuperior => None,
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
                Some(item.owner_role.clone()),
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
        let mut business_object_by_order: HashMap<String, (&'static str, String)> = HashMap::new();
        for confirmation in &confirmations {
            business_object_by_order.insert(
                confirmation.sales_order_id.to_string(),
                ("procurement_confirmation", confirmation.base.id.clone()),
            );
        }
        for (order_id, review_status) in rows {
            if matches!(
                review_status,
                ReviewStatus::PendingSalesLeader | ReviewStatus::PendingOperations
            ) {
                business_object_by_order.insert(order_id.clone(), ("sales_order", order_id.clone()));
            }
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
                    "business_object_type": { "$in": ["procurement_confirmation", "sales_order"] },
                    "business_object_id": { "$in": &object_ids },
                    "status": WorkItemStatus::Open.as_str(),
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
            let owner_role = item.map(|i| i.owner_role.clone());
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

/// 校验活动或阻塞领导步骤绑定的可关闭待办身份。
fn cancel_work_item_matches(
    instance: &entities::approval::ApprovalInstance,
    step: &entities::approval::ApprovalStepInstance,
    work_item: Option<&WorkItem>,
) -> bool {
    if instance.status == entities::approval::ApprovalInstanceStatus::Blocked && work_item.is_none() {
        return true;
    }
    work_item.is_some_and(|item| {
        item.status == WorkItemStatus::Open
            && item.work_item_type == entities::work_item::WorkItemType::CardSalesManagerApproval
            && item.approval_step_instance_id.as_deref() == Some(step.base.id.as_str())
            && item.business_object_type == instance.business_object_type
            && item.business_object_id == instance.business_object_id
            && item.subject_version == instance.subject_version
    })
}

/// 只允许尚无决定事实的销售领导首步进入撤回策略。
fn cancel_step_policy_allows(
    instance_status: entities::approval::ApprovalInstanceStatus,
    step_status: entities::approval::ApprovalStepStatus,
    step_key: &str,
    has_decision: bool,
) -> bool {
    matches!(
        (instance_status, step_status),
        (
            entities::approval::ApprovalInstanceStatus::Running,
            entities::approval::ApprovalStepStatus::Active
        ) | (
            entities::approval::ApprovalInstanceStatus::Blocked,
            entities::approval::ApprovalStepStatus::Blocked
        )
    ) && step_key == SALES_MANAGER_APPROVAL
        && !has_decision
}

#[cfg(test)]
mod card_sales_cancel_policy_tests {
    use entities::approval::{ApprovalInstanceStatus, ApprovalStepStatus};

    use super::{cancel_step_policy_allows, OPERATIONS_APPROVAL, SALES_MANAGER_APPROVAL};

    #[test]
    fn submitter_policy_allows_only_undecided_manager_step() {
        assert!(cancel_step_policy_allows(
            ApprovalInstanceStatus::Running,
            ApprovalStepStatus::Active,
            SALES_MANAGER_APPROVAL,
            false,
        ));
        assert!(cancel_step_policy_allows(
            ApprovalInstanceStatus::Blocked,
            ApprovalStepStatus::Blocked,
            SALES_MANAGER_APPROVAL,
            false,
        ));
    }

    #[test]
    fn irreversible_decision_or_operations_step_forbids_cancel() {
        assert!(!cancel_step_policy_allows(
            ApprovalInstanceStatus::Running,
            ApprovalStepStatus::Active,
            SALES_MANAGER_APPROVAL,
            true,
        ));
        assert!(!cancel_step_policy_allows(
            ApprovalInstanceStatus::Running,
            ApprovalStepStatus::Active,
            OPERATIONS_APPROVAL,
            false,
        ));
    }
}
