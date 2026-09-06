//! 应收往来子账列表、详情与创建编排。

use database::{ReceivableExt, SalesOrderExt};
use entities::receivable::{
    AccountReviewStatus, EntryDirection, ReceivableAccount, ReceivableAccountData, ReceivableEntry,
    ReceivableEntryData, ReceivableEntryType,
};
use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use erp_core::ids::{ReceivableAccountId, ReceivableEntryId, SalesOrderId, SalesOrderRevisionId};
use erp_core::money::Amount;
use erp_identity::Permission;
use erp_workflow::entity::work_item::{WorkItemStatus, WorkItemType};
use erp_workflow::WorkItemExt;
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use std::collections::{HashMap, HashSet};

use super::card_funds_review::{pending_review_status, validate_card_funds_reviewer_separation};
use super::dto::{
    CardFundsReviewActionBlockerView, CardFundsReviewAllowedAction, CardFundsReviewDetailParams,
    CardFundsReviewType, CreateReceivableAccountRequest, PageView, ReceivableAccountListParams,
    ReceivableAccountSummaryView, ReceivableAccountView, SortDir,
};
use super::mapping::{
    card_funds_review_chain, card_funds_snapshot_of, invoice_fact_views, load_card_funds_snapshot,
    map_chain_error, receipt_fact_views, zero_amount,
};
use super::{card_funds_task, invoice_task, ReceivableAccountFilter, ReceivableService};
use crate::errors::{Error, Result};
use crate::workflow_compose::work_item_service;
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::SharedRbacService;
use erp_workflow::service::work_item::WorkItemAllowedAction;

impl ReceivableService {
    // -----------------------------------------------------------------------
    // 应收往来子账
    // -----------------------------------------------------------------------

    /// 分页查询应收往来子账列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4），禁止任意字段透传。
    ///
    /// # 参数
    /// * `params` - 查询参数（`customer_id`/`counterparty_party_id`/`status`/
    ///   `sales_order_id`/`review_status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn receivable_account_list(
        &self,
        params: &ReceivableAccountListParams,
    ) -> Result<PageView<ReceivableAccountSummaryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ReceivableAccountFilter {
            keyword: query.q,
            account_id: query.account_id,
            customer_id: query.customer_id,
            counterparty_party_id: query.counterparty_party_id,
            status: query.status,
            sales_order_id: query.sales_order_id,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .receivable_accounts()
            .search_receivable_accounts(&filter, &mut NoTransaction)
            .await?;
        let account_ids = page
            .items
            .iter()
            .map(|row| ReceivableAccountId::new(row.id.clone()))
            .collect::<Vec<_>>();
        let mut entries_by_account = HashMap::<String, Vec<ReceivableEntry>>::new();
        let entries = self
            .db
            .receivable_entries()
            .find_entries_by_accounts(&account_ids, &mut NoTransaction)
            .await?;
        let decrease_entry_ids = entries
            .iter()
            .filter(|entry| entry.direction == EntryDirection::Decrease)
            .map(|entry| ReceivableEntryId::new(entry.base.id.clone()))
            .collect::<Vec<_>>();
        let mut offset_by_increase = HashMap::<String, Amount>::new();
        for offset in self
            .db
            .receivable_entry_offsets()
            .find_offsets_by_decreases(&decrease_entry_ids, &mut NoTransaction)
            .await?
        {
            let total = offset_by_increase
                .entry(offset.increase_entry_id.to_string())
                .or_insert_with(zero_amount);
            *total = total.checked_add(offset.offset_amount);
        }
        for entry in entries {
            entries_by_account
                .entry(entry.receivable_account_id.to_string())
                .or_default()
                .push(entry);
        }
        for entries in entries_by_account.values_mut() {
            entries.sort_unstable_by_key(|entry| entry.source_sequence);
        }

        let sales_order_ids = page
            .items
            .iter()
            .map(|row| SalesOrderId::new(row.sales_order_id.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let sales_orders = self
            .db
            .sales_orders()
            .find_orders_by_ids(&sales_order_ids, &mut NoTransaction)
            .await?;
        let revision_ids = sales_orders
            .iter()
            .map(|order| {
                order
                    .stable
                    .current_revision_id
                    .clone()
                    .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前正式版本".to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        let revisions = self
            .db
            .sales_order_revisions()
            .find_revisions_by_ids(&revision_ids, &mut NoTransaction)
            .await?;
        let sales_order_by_id = sales_orders
            .into_iter()
            .map(|order| (order.base.id.clone(), order))
            .collect::<HashMap<_, _>>();
        let revision_by_id = revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<HashMap<_, _>>();

        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            let order = sales_order_by_id
                .get(&row.sales_order_id)
                .ok_or_else(|| Error::NotFound("应收账户来源销售单不存在".to_string()))?;
            let revision_id = order
                .stable
                .current_revision_id
                .as_ref()
                .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前正式版本".to_string()))?;
            let revision = revision_by_id
                .get(revision_id)
                .ok_or_else(|| Error::NotFound("来源销售单当前正式版本不存在".to_string()))?;
            let entries = entries_by_account
                .remove(&row.id)
                .unwrap_or_default()
                .into_iter()
                .map(|entry| crate::receivable::dto::ReceivableEntryView {
                    offset_total: offset_by_increase
                        .get(&entry.base.id)
                        .copied()
                        .unwrap_or_else(zero_amount),
                    id: entry.base.id,
                    entry_type: entry.entry_type,
                    direction: entry.direction,
                    amount: entry.amount,
                    due_date: entry.due_date,
                    source_document_id: entry.source_document_id,
                    source_sequence: entry.source_sequence,
                    posted_at: entry.posted_at,
                })
                .collect();
            views.push(ReceivableAccountSummaryView {
                id: row.id,
                sales_order_id: row.sales_order_id,
                sales_order_no: order.order_no.clone(),
                account_seq: row.account_seq,
                customer_id: row.customer_id,
                customer_name: revision.customer_snapshot.customer_name.clone(),
                counterparty_party_id: row.counterparty_party_id,
                counterparty_party_name: revision
                    .settlement_party_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.settlement_party_name.clone()),
                review_status: row.review_status,
                gross_total: row.gross_total,
                settled_total: row.settled_total,
                open_total: row.open_total,
                invoiceable_total: row.invoiceable_total,
                invoiced_total: row.invoiced_total,
                open_invoiceable_total: row.open_invoiceable_total,
                status: row.stable.status(),
                version: row.version,
                created_at: row.created_at,
                entries,
            });
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询应收往来子账详情（子账 + 分录 + 抵销 + 复核链）。
    ///
    /// # 参数
    /// * `id` - 应收往来子账 ID
    ///
    /// # 返回
    /// 返回完整台账视图。
    ///
    /// # 错误
    /// * `NotFound` - 子账不存在
    /// * `RepositoryError` - 数据库查询失败
    pub async fn receivable_account_detail(&self, id: &str) -> Result<ReceivableAccountView> {
        self.receivable_account_view(id.to_string()).await
    }

    /// 按当前操作人投影 W13 正式任务与领域动作。
    ///
    /// 通用任务只负责建立/表达处理责任；`CONFIRM_ZERO` / `APPROVE` /
    /// `REJECT` 及票款登记入口均由当前账户、复核类型、正式事实、
    /// 岗位分离和 RBAC 在服务端独立计算。
    ///
    /// # 错误
    /// 应收账户、任务或当前版本不存在，任务与对象不匹配，或授权
    /// 事实无法读取时返回错误。
    pub async fn receivable_account_detail_with_actions(
        &self,
        id: &str,
        params: &CardFundsReviewDetailParams,
        actor: &AuditActor,
        rbac: SharedRbacService,
    ) -> Result<ReceivableAccountView> {
        let mut view = self.receivable_account_view(id.to_string()).await?;
        let Some(work_item_id) = params
            .work_item_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(view);
        };
        let formal = work_item_service(self.db.clone(), rbac.clone())
            .authorize_work_item(work_item_id, actor)
            .await?;
        let review_type = match formal.item.work_item_type {
            WorkItemType::CardFundsReview => CardFundsReviewType::Opening,
            WorkItemType::CardFundsDeltaReview => CardFundsReviewType::SyncDelta,
            _ => {
                return Err(Error::BusinessLogicError(
                    "正式任务不是 W13 卡券票款复核".to_string(),
                ));
            }
        };
        if formal.item.business_object_type != "receivable_account"
            || formal.item.business_object_id != id
            || formal.item.subject_version != view.current_sales_order_revision_id
            || false
        {
            return Err(Error::BusinessLogicError(
                "正式任务与当前应收账户或销售版本不匹配".to_string(),
            ));
        }
        view.work_item = None;
        view.active_review_type = Some(review_type);
        if !formal.allowed_actions.contains(&WorkItemAllowedAction::Process) {
            block_card_funds_actions(
                &mut view,
                "CURRENT_RESPONSIBILITY_REQUIRED",
                "当前账号不是开放任务的当前责任人",
            );
            return Ok(view);
        }

        let work_item = self
            .db
            .work_items()
            .find_by_id(work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("卡券票款复核任务不存在".to_string()))?;
        if work_item.status != WorkItemStatus::Open || !work_item.is_owned_by(actor.id()) {
            block_card_funds_actions(
                &mut view,
                "CURRENT_RESPONSIBILITY_REQUIRED",
                "当前账号不是开放任务的当前责任人",
            );
            return Ok(view);
        }
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
        let snapshot = load_card_funds_snapshot(&self.db, &account, &mut NoTransaction).await?;
        let expected_status = pending_review_status(review_type);
        if account.review_status != expected_status {
            block_card_funds_review_decisions(
                &mut view,
                "REVIEW_STATUS_CHANGED",
                "应收账户已不在当前复核类型的待处理状态",
            );
        } else if validate_card_funds_reviewer_separation(
            &self.db,
            &account,
            &snapshot,
            &work_item,
            actor.id(),
            &mut NoTransaction,
        )
        .await
        .is_err()
        {
            block_card_funds_review_decisions(
                &mut view,
                "ACTOR_INELIGIBLE_OR_SOD",
                "当前账号不再具备复核资格，或与已登记票款事实的经办人冲突",
            );
        } else {
            view.allowed_actions.push(CardFundsReviewAllowedAction::Reject);
            let has_receipt_facts = !snapshot.receipt_allocations.is_empty();
            let has_invoice_facts = !snapshot.invoice_allocations.is_empty();
            if review_type == CardFundsReviewType::Opening
                && !has_receipt_facts
                && !has_invoice_facts
                && account.settled_total == zero_amount()
                && account.invoiced_total == zero_amount()
            {
                view.allowed_actions
                    .push(CardFundsReviewAllowedAction::ConfirmZero);
            } else {
                push_card_funds_blocker(
                    &mut view,
                    CardFundsReviewAllowedAction::ConfirmZero,
                    if review_type == CardFundsReviewType::Opening {
                        "RECORDED_FACTS_NOT_ZERO"
                    } else {
                        "NOT_OPENING_REVIEW"
                    },
                    if review_type == CardFundsReviewType::Opening {
                        "已存在正式回款/发票事实或净额不为零，不能从零起算"
                    } else {
                        "从零起算仅适用于期初复核"
                    },
                );
            }
            if has_receipt_facts || has_invoice_facts {
                if review_type != CardFundsReviewType::SyncDelta || !snapshot.reviews.is_empty() {
                    view.allowed_actions.push(CardFundsReviewAllowedAction::Approve);
                } else {
                    push_card_funds_blocker(
                        &mut view,
                        CardFundsReviewAllowedAction::Approve,
                        "REVIEW_BASELINE_MISSING",
                        "同步差额复核缺少已完成的期初复核基线",
                    );
                }
            } else {
                push_card_funds_blocker(
                    &mut view,
                    CardFundsReviewAllowedAction::Approve,
                    "RECORDED_FACTS_REQUIRED",
                    "「已登记事实已核对」必须存在正式回款或销项发票事实",
                );
            }
        }

        let subject = erp_identity::subject(actor.kind(), actor.id());
        let has_counterparty_name = view
            .counterparty_party_name
            .as_deref()
            .is_some_and(|name| !name.trim().is_empty());
        project_registration_action(
            &mut view,
            CardFundsReviewAllowedAction::RegisterReceipt,
            has_counterparty_name,
            rbac.enforce(
                &subject,
                &Permission::parse("customer_receipt:create")
                    .map_err(|error| Error::Internal(error.to_string()))?,
            )
            .await?,
        );
        project_registration_action(
            &mut view,
            CardFundsReviewAllowedAction::RegisterInvoice,
            has_counterparty_name,
            rbac.enforce(
                &subject,
                &Permission::parse("invoice:create").map_err(|error| Error::Internal(error.to_string()))?,
            )
            .await?,
        );
        Ok(view)
    }

    /// 建立应收往来子账与原始应收分录（跨集合事务写入）。
    ///
    /// 校验来源销售单存在（D13 Repository），同事务写入子账与分录，
    /// 保证「子账 + 原始应收」原子可见（数据模型 §6.8）。业务幂等唯一
    /// `(receivable_account_id, source_fact_type, source_document_id,
    /// source_revision_id, entry_type, source_sequence)` 由唯一索引保证，
    /// 重复提交落入 409。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建子账的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源销售单不存在
    /// * `ConflictError` - 业务唯一键重复（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_receivable_account(
        &self,
        req: CreateReceivableAccountRequest,
        actor: &AuditActor,
    ) -> Result<ReceivableAccountView> {
        req.validate()?;
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&req.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        let review_status =
            AccountReviewStatus::resolve_initial(req.review_status, sales_order.business_type)
                .map_err(|error| Error::ValidationError(error.to_string()))?;
        if review_status == AccountReviewStatus::OpeningPending
            && sales_order.stable.current_revision_id.as_deref()
                != Some(req.source_sales_order_revision_id.as_str())
        {
            return Err(Error::ConflictError(
                "卡券票款复核必须绑定来源销售单的当前正式版本".to_string(),
            ));
        }

        let account_id = ReceivableAccountId::new(next_id());
        let entry_id = ReceivableEntryId::new(next_id());
        let posted_at = Instant::now();
        let account = ReceivableAccount::new(
            account_id.clone(),
            ReceivableAccountData {
                sales_order_id: req.sales_order_id.clone().into(),
                account_seq: req.account_seq,
                customer_id: req.customer_id.clone(),
                counterparty_party_id: req.counterparty_party_id.clone(),
                source_sales_order_revision_id: SalesOrderRevisionId::new(
                    &req.source_sales_order_revision_id,
                ),
                review_status,
                reviewed_by: None,
                reviewed_at: None,
                review_evidence_reference: None,
                gross_total: req.gross_total,
                settled_total: zero_amount(),
                invoiceable_total: req.invoiceable_total.unwrap_or(req.gross_total),
                invoiced_total: zero_amount(),
            },
            actor.id(),
        )?;
        let entry = ReceivableEntry::new(
            entry_id,
            ReceivableEntryData {
                receivable_account_id: account_id.clone(),
                entry_type: ReceivableEntryType::Original,
                direction: EntryDirection::Increase,
                amount: account.gross_total,
                due_date: req.due_date,
                source_fact_type: "sales_order".to_string(),
                source_document_id: req.sales_order_id.clone(),
                source_revision_id: req.source_sales_order_revision_id,
                source_sequence: req.source_sequence,
                posted_at,
            },
        )?;
        let audit = actor.clone().resource_log(
            "receivable_account.create",
            "receivable_account",
            account_id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.receivable()
                        .create_receivable_with_entry(&account, &entry, session)
                        .await?;
                    card_funds_task::ensure_initial_card_funds_review_task(&db, &account, session).await?;
                    invoice_task::ensure_sales_invoice_task(&db, &account, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.receivable_account_detail(&account_id).await
    }

    /// 装配应收往来子账详情视图。
    ///
    /// # 参数
    /// * `id` - 子账 ID
    ///
    /// # 返回
    /// 返回完整台账视图。
    ///
    /// # 错误
    /// * `NotFound` - 子账不存在
    pub(super) async fn receivable_account_view(&self, id: String) -> Result<ReceivableAccountView> {
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
        let snapshot = load_card_funds_snapshot(&self.db, &account, &mut NoTransaction).await?;
        let offsets = snapshot
            .entries
            .iter()
            .filter(|entry| entry.direction == EntryDirection::Decrease)
            .map(|entry| entry.base.id.clone().into())
            .collect::<Vec<ReceivableEntryId>>();
        let mut offset_map: std::collections::HashMap<String, Amount> = std::collections::HashMap::new();
        for offset in self
            .db
            .receivable_entry_offsets()
            .find_offsets_by_decreases(&offsets, &mut NoTransaction)
            .await?
        {
            let key = offset.increase_entry_id.to_string();
            let total = offset_map.entry(key).or_insert_with(zero_amount);
            *total = total.checked_add(offset.offset_amount);
        }
        let entry_views = snapshot
            .entries
            .iter()
            .map(|entry| {
                let offset_total = offset_map
                    .get(&entry.base.id)
                    .copied()
                    .unwrap_or_else(zero_amount);
                crate::receivable::dto::ReceivableEntryView {
                    id: entry.base.id.clone(),
                    entry_type: entry.entry_type,
                    direction: entry.direction,
                    amount: entry.amount,
                    due_date: entry.due_date,
                    source_document_id: entry.source_document_id.clone(),
                    source_sequence: entry.source_sequence,
                    posted_at: entry.posted_at,
                    offset_total,
                }
            })
            .collect();
        let reviews = snapshot
            .reviews
            .iter()
            .map(|review| crate::receivable::dto::FundsReviewView {
                id: review.base.id.clone(),
                review_no: review.review_no,
                review_type: review.review_type,
                review_result: review.review_result,
                reviewed_by: review.reviewed_by.clone(),
                reviewed_at: review.reviewed_at,
                evidence_reference: review.evidence_reference.clone(),
            })
            .collect();
        let chain = card_funds_review_chain(&snapshot.reviews)?;
        let review_chain_tail_id = chain.tail_id().map(str::to_string);
        let next_review_no = chain.next_review_no().map_err(map_chain_error)?;
        let review_chain_version = chain.version().to_string();
        let funds_fact_version = card_funds_snapshot_of(&snapshot)?.fact_version(&account);
        let receipt_facts = receipt_fact_views(&snapshot);
        let invoice_facts = invoice_fact_views(&snapshot);

        Ok(ReceivableAccountView {
            id: account.base.id.clone(),
            sales_order_id: account.sales_order_id.to_string(),
            sales_order_no: snapshot.sales_order_no.clone(),
            sales_order_revision_no: snapshot.sales_order_revision_no,
            sales_order_snapshot_at: snapshot.sales_order_snapshot_at,
            account_seq: account.account_seq,
            source_sales_order_revision_id: account.source_sales_order_revision_id.to_string(),
            current_sales_order_revision_id: snapshot.current_sales_order_revision_id.clone(),
            customer_id: account.customer_id.to_string(),
            customer_name: snapshot.customer_name.clone(),
            counterparty_party_id: account.counterparty_party_id.to_string(),
            counterparty_party_name: snapshot.counterparty_party_name.clone(),
            review_status: account.review_status,
            gross_total: account.gross_total,
            settled_total: account.settled_total,
            open_total: account.open_total,
            invoiceable_total: account.invoiceable_total,
            invoiced_total: account.invoiced_total,
            open_invoiceable_total: account.open_invoiceable_total,
            status: account.stable.status(),
            version: account.base.version,
            account_domain_version: account.base.version.to_string(),
            review_chain_tail_id,
            review_chain_version,
            next_review_no,
            funds_fact_version,
            receipt_facts,
            invoice_facts,
            created_at: account.base.created_at,
            entries: entry_views,
            reviews,
            work_item: None,
            active_review_type: None,
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
        })
    }
}

fn push_card_funds_blocker(
    view: &mut ReceivableAccountView,
    action: CardFundsReviewAllowedAction,
    code: &str,
    message: &str,
) {
    view.action_blockers.push(CardFundsReviewActionBlockerView {
        action: action.as_str().to_string(),
        code: code.to_string(),
        message: message.to_string(),
    });
}

fn block_card_funds_review_decisions(view: &mut ReceivableAccountView, code: &str, message: &str) {
    for action in [
        CardFundsReviewAllowedAction::ConfirmZero,
        CardFundsReviewAllowedAction::Approve,
        CardFundsReviewAllowedAction::Reject,
    ] {
        push_card_funds_blocker(view, action, code, message);
    }
}

fn block_card_funds_actions(view: &mut ReceivableAccountView, code: &str, message: &str) {
    block_card_funds_review_decisions(view, code, message);
    for action in [
        CardFundsReviewAllowedAction::RegisterReceipt,
        CardFundsReviewAllowedAction::RegisterInvoice,
    ] {
        push_card_funds_blocker(view, action, code, message);
    }
}

fn project_registration_action(
    view: &mut ReceivableAccountView,
    action: CardFundsReviewAllowedAction,
    has_counterparty_name: bool,
    permitted: bool,
) {
    if !has_counterparty_name {
        push_card_funds_blocker(
            view,
            action,
            "COUNTERPARTY_NAME_MISSING",
            "当前销售版本缺少收款/开票往来主体名称，禁止以内部 ID 伪装名称登记事实",
        );
    } else if !permitted {
        push_card_funds_blocker(
            view,
            action,
            "REGISTRATION_PERMISSION_REQUIRED",
            "当前账号没有登记该类票款事实的权限",
        );
    } else {
        view.allowed_actions.push(action);
    }
}
