//! 应收往来子账列表、详情与创建编排。

use erp_core::ids::{ReceivableAccountId, ReceivableEntryId, SalesOrderId};
use erp_core::money::Amount;
use erp_finance::entity::receivable::{EntryDirection, ReceivableEntry};
use erp_finance::repository::ReceivableExt;
use erp_identity::Permission;
use erp_party::PartyExt;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::entity::work_item::{WorkItemStatus, WorkItemType};
use erp_workflow::WorkItemExt;
use persistence_core::NoTransaction;
use validator::Validate;

use std::collections::{HashMap, HashSet};

use super::separation::validate_card_funds_reviewer_separation;
use super::snapshot::pending_review_status;
use super::snapshot::{
    card_funds_review_chain, card_funds_snapshot_of, invoice_fact_views, load_card_funds_snapshot,
    map_chain_error, receipt_fact_views, zero_amount,
};
use super::ReceivableReadService;
use crate::finance::dto::ReceivableAccountView;
use crate::ports::work_item_authorization::WorkItemAuthorizationReadPort;
use crate::{Error, Result};
use application_core::AuditActor;
use erp_finance::dto::receivable::{
    CardFundsReviewActionBlockerView, CardFundsReviewAllowedAction, CardFundsReviewDetailParams,
    CardFundsReviewType, PageView, ReceivableAccountListParams, ReceivableAccountSummaryView, SortDir,
};
use erp_finance::repository::ReceivableAccountFilter;
use erp_identity::SharedRbacService;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::service::work_item::WorkItemAllowedAction;
use std::future::Future;

impl ReceivableReadService {
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
        let (keyword_sales_order_ids, keyword_party_ids) = if let Some(keyword) = query.q.as_deref() {
            let sales_ids = self
                .db
                .sales_orders()
                .matching_ids_by_number(keyword, &mut NoTransaction)
                .await?;
            let party_ids = self
                .db
                .party()
                .matching_current_party_ids_by_name(keyword, &mut NoTransaction)
                .await?;
            (sales_ids, party_ids)
        } else {
            (Vec::new(), Vec::new())
        };
        let filter = ReceivableAccountFilter {
            keyword: query.q,
            keyword_sales_order_ids,
            keyword_party_ids,
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
                .map(|entry| erp_finance::dto::receivable::ReceivableEntryView {
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
        task_auth: &dyn WorkItemAuthorizationReadPort,
    ) -> Result<ReceivableAccountView> {
        let (mut view, task) = load_review_task(
            self.receivable_account_view(id.to_string()),
            |work_item_id| async move {
                self.db
                    .work_items()
                    .find_by_id(&work_item_id, &mut NoTransaction)
                    .await?
                    .ok_or_else(|| Error::NotFound("卡券票款复核任务不存在".to_string()))
            },
            id,
            params.work_item_id.as_deref(),
            actor,
            task_auth,
        )
        .await?;
        let Some(ReviewTask {
            work_item,
            review_type,
        }) = task
        else {
            return Ok(view);
        };
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
    pub async fn receivable_account_view(&self, id: String) -> Result<ReceivableAccountView> {
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
                erp_finance::dto::receivable::ReceivableEntryView {
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
            .map(|review| erp_finance::dto::receivable::FundsReviewView {
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

/// 详情已授权且仍开放归属于当前处理人的原始任务。
struct ReviewTask {
    work_item: WorkItem,
    review_type: CardFundsReviewType,
}

/// 按原顺序读取详情、授权和重读任务；不合并两个任务快照。
async fn load_review_task<V, F, T>(
    detail: V,
    load_task: F,
    id: &str,
    requested_task_id: Option<&str>,
    actor: &AuditActor,
    task_auth: &dyn WorkItemAuthorizationReadPort,
) -> Result<(ReceivableAccountView, Option<ReviewTask>)>
where
    V: Future<Output = Result<ReceivableAccountView>>,
    F: FnOnce(String) -> T,
    T: Future<Output = Result<WorkItem>>,
{
    let mut view = detail.await?;
    let Some(work_item_id) = requested_task_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok((view, None));
    };
    let formal = task_auth.authorize(work_item_id, actor).await?;
    let review_type = match formal.work_item_type {
        WorkItemType::CardFundsReview => CardFundsReviewType::Opening,
        WorkItemType::CardFundsDeltaReview => CardFundsReviewType::SyncDelta,
        _ => {
            return Err(Error::BusinessLogicError(
                "正式任务不是 W13 卡券票款复核".to_string(),
            ));
        }
    };
    if formal.business_object_type != "receivable_account"
        || formal.business_object_id != id
        || formal.subject_version != view.current_sales_order_revision_id
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
        return Ok((view, None));
    }

    let work_item = load_task(work_item_id.to_string()).await?;
    if work_item.status != WorkItemStatus::Open || !work_item.is_owned_by(actor.id()) {
        block_card_funds_actions(
            &mut view,
            "CURRENT_RESPONSIBILITY_REQUIRED",
            "当前账号不是开放任务的当前责任人",
        );
        return Ok((view, None));
    }
    Ok((
        view,
        Some(ReviewTask {
            work_item,
            review_type,
        }),
    ))
}

#[cfg(test)]
mod authorization_tests {
    use super::*;
    use crate::ports::work_item_authorization::AuthorizedTaskFact;
    use async_trait::async_trait;
    use erp_core::{common::time::Instant, ids::WorkItemId, AccountKind};
    use erp_finance::entity::receivable::{AccountReviewStatus, ReceivableAccountStatus};
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority};
    use std::sync::{Arc, Mutex};

    struct RecordingAuthority {
        trace: Arc<Mutex<Vec<String>>>,
        fact: AuthorizedTaskFact,
        denied: bool,
    }

    #[async_trait]
    impl WorkItemAuthorizationReadPort for RecordingAuthority {
        async fn authorize(&self, id: &str, actor: &AuditActor) -> erp_workflow::Result<AuthorizedTaskFact> {
            self.trace
                .lock()
                .unwrap()
                .push(format!("authorize:{id}:{}", actor.id()));
            if self.denied {
                return Err(erp_workflow::Error::Forbidden("authority-denied".into()));
            }
            Ok(self.fact.clone())
        }
    }

    fn authority(trace: Arc<Mutex<Vec<String>>>) -> RecordingAuthority {
        RecordingAuthority {
            trace,
            fact: AuthorizedTaskFact {
                work_item_type: WorkItemType::CardFundsReview,
                business_object_type: "receivable_account".into(),
                business_object_id: "account-1".into(),
                subject_version: "revision-1".into(),
                allowed_actions: vec![WorkItemAllowedAction::Process],
            },
            denied: false,
        }
    }

    fn view() -> ReceivableAccountView {
        ReceivableAccountView {
            id: "account-1".into(),
            sales_order_id: "sales-1".into(),
            sales_order_no: "SO-1".into(),
            sales_order_revision_no: 1,
            sales_order_snapshot_at: 1,
            account_seq: 1,
            source_sales_order_revision_id: "revision-1".into(),
            current_sales_order_revision_id: "revision-1".into(),
            customer_id: "customer-1".into(),
            customer_name: "客户".into(),
            counterparty_party_id: "party-1".into(),
            counterparty_party_name: Some("往来主体".into()),
            review_status: AccountReviewStatus::OpeningPending,
            gross_total: zero_amount(),
            settled_total: zero_amount(),
            open_total: zero_amount(),
            invoiceable_total: zero_amount(),
            invoiced_total: zero_amount(),
            open_invoiceable_total: zero_amount(),
            status: ReceivableAccountStatus::Open,
            version: 1,
            account_domain_version: "1".into(),
            review_chain_tail_id: None,
            review_chain_version: "0".into(),
            next_review_no: 1,
            funds_fact_version: "facts-1".into(),
            receipt_facts: Vec::new(),
            invoice_facts: Vec::new(),
            created_at: 1,
            entries: Vec::new(),
            reviews: Vec::new(),
            work_item: None,
            active_review_type: None,
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
        }
    }

    fn task() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("task-1"),
            WorkItemData {
                work_item_type: WorkItemType::CardFundsReview,
                business_object_type: "receivable_account".into(),
                business_object_id: "account-1".into(),
                subject_version: "revision-1".into(),
                owner_role: "role-finance".into(),
                owner_organization_id: "organization-1".into(),
                owner_user_id: "actor-1".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(1_700_000_000),
        )
        .unwrap()
    }

    /// 驱动生产 helper；两次仓储读取由实际参数位置记录，测试不复制业务判断。
    async fn run(
        authority: &RecordingAuthority,
        requested: Option<&str>,
        detail: Result<ReceivableAccountView>,
        raw_task: Result<WorkItem>,
    ) -> Result<(ReceivableAccountView, Option<ReviewTask>)> {
        let detail_trace = Arc::clone(&authority.trace);
        let raw_trace = Arc::clone(&authority.trace);
        let actor = AuditActor::new("actor-1".into(), "处理人".into(), AccountKind::Admin);
        load_review_task(
            async move {
                detail_trace.lock().unwrap().push("detail".into());
                detail
            },
            move |id| async move {
                raw_trace.lock().unwrap().push(format!("raw:{id}"));
                raw_task
            },
            "account-1",
            requested,
            &actor,
            authority,
        )
        .await
    }

    #[tokio::test]
    async fn empty_task_id_keeps_detail_and_skips_authority_and_raw_reload() {
        for requested in [None, Some(""), Some("  ")] {
            let trace = Arc::new(Mutex::new(Vec::new()));
            let authority = authority(Arc::clone(&trace));
            let expected = view();
            let (actual, task) = run(&authority, requested, Ok(expected.clone()), Ok(task()))
                .await
                .unwrap();
            assert_eq!(actual, expected);
            assert!(task.is_none());
            assert_eq!(*trace.lock().unwrap(), ["detail"]);
        }
    }

    #[tokio::test]
    async fn detail_failure_precedes_authority() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let authority = authority(Arc::clone(&trace));
        let result = run(
            &authority,
            Some("task-1"),
            Err(Error::NotFound("detail".into())),
            Ok(task()),
        )
        .await;
        assert!(matches!(result, Err(Error::NotFound(message)) if message == "detail"));
        assert_eq!(*trace.lock().unwrap(), ["detail"]);
    }

    #[tokio::test]
    async fn authority_error_precedes_formal_binding_checks_and_preserves_type() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut authority = authority(Arc::clone(&trace));
        authority.denied = true;
        authority.fact.work_item_type = WorkItemType::BusinessException;
        let result = run(&authority, Some(" task-1 "), Ok(view()), Ok(task())).await;
        assert!(matches!(result, Err(Error::Forbidden(message)) if message == "authority-denied"));
        assert_eq!(*trace.lock().unwrap(), ["detail", "authorize:task-1:actor-1"]);
    }

    #[tokio::test]
    async fn formal_type_object_identity_and_subject_checks_precede_raw_reload() {
        for mismatch in 0..4 {
            let trace = Arc::new(Mutex::new(Vec::new()));
            let mut authority = authority(Arc::clone(&trace));
            match mismatch {
                0 => authority.fact.work_item_type = WorkItemType::BusinessException,
                1 => authority.fact.business_object_type = "sales_order".into(),
                2 => authority.fact.business_object_id = "account-2".into(),
                _ => authority.fact.subject_version = "revision-2".into(),
            }
            let expected = if mismatch == 0 {
                "正式任务不是 W13 卡券票款复核"
            } else {
                "正式任务与当前应收账户或销售版本不匹配"
            };
            let result = run(&authority, Some("task-1"), Ok(view()), Ok(task())).await;
            assert!(matches!(result, Err(Error::BusinessLogicError(message)) if message == expected));
            assert_eq!(*trace.lock().unwrap(), ["detail", "authorize:task-1:actor-1"]);
        }
    }

    #[tokio::test]
    async fn missing_process_action_blocks_all_five_actions_without_raw_reload() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut authority = authority(Arc::clone(&trace));
        authority.fact.allowed_actions = vec![WorkItemAllowedAction::View];
        let (view, task) = run(&authority, Some("task-1"), Ok(view()), Ok(task()))
            .await
            .unwrap();
        assert!(task.is_none());
        assert_eq!(view.active_review_type, Some(CardFundsReviewType::Opening));
        assert!(view.allowed_actions.is_empty());
        assert_eq!(view.action_blockers.len(), 5);
        assert!(view
            .action_blockers
            .iter()
            .all(|blocker| blocker.code == "CURRENT_RESPONSIBILITY_REQUIRED"));
        assert_eq!(*trace.lock().unwrap(), ["detail", "authorize:task-1:actor-1"]);
    }

    #[tokio::test]
    async fn raw_reload_failure_is_preserved_after_authorization() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let authority = authority(Arc::clone(&trace));
        let result = run(
            &authority,
            Some("task-1"),
            Ok(view()),
            Err(Error::NotFound("raw-task".into())),
        )
        .await;
        assert!(matches!(result, Err(Error::NotFound(message)) if message == "raw-task"));
        assert_eq!(
            *trace.lock().unwrap(),
            ["detail", "authorize:task-1:actor-1", "raw:task-1"]
        );
    }

    #[tokio::test]
    async fn raw_reload_rechecks_open_status_and_current_owner() {
        for closed in [true, false] {
            let trace = Arc::new(Mutex::new(Vec::new()));
            let authority = authority(Arc::clone(&trace));
            let mut raw = task();
            if closed {
                raw.status = WorkItemStatus::Closed;
            } else {
                raw.owner_user_id = Some("other-actor".into());
            }
            let (view, task) = run(&authority, Some("task-1"), Ok(view()), Ok(raw))
                .await
                .unwrap();
            assert!(task.is_none());
            assert_eq!(view.action_blockers.len(), 5);
            assert!(view
                .action_blockers
                .iter()
                .all(|blocker| blocker.code == "CURRENT_RESPONSIBILITY_REQUIRED"));
            assert_eq!(
                *trace.lock().unwrap(),
                ["detail", "authorize:task-1:actor-1", "raw:task-1"]
            );
        }
    }

    #[tokio::test]
    async fn authorized_review_preserves_opening_and_delta_type_and_raw_task() {
        for (kind, review_type) in [
            (WorkItemType::CardFundsReview, CardFundsReviewType::Opening),
            (WorkItemType::CardFundsDeltaReview, CardFundsReviewType::SyncDelta),
        ] {
            let trace = Arc::new(Mutex::new(Vec::new()));
            let mut authority = authority(Arc::clone(&trace));
            authority.fact.work_item_type = kind;
            let mut raw = task();
            raw.base.version = 7;
            let (view, authorized) = run(&authority, Some(" task-1 "), Ok(view()), Ok(raw))
                .await
                .unwrap();
            let authorized = authorized.unwrap();
            assert_eq!(authorized.review_type, review_type);
            assert_eq!(authorized.work_item.base.version, 7);
            assert_eq!(view.active_review_type, Some(review_type));
            assert!(view.action_blockers.is_empty());
            assert_eq!(
                *trace.lock().unwrap(),
                ["detail", "authorize:task-1:actor-1", "raw:task-1"]
            );
        }
    }
}
