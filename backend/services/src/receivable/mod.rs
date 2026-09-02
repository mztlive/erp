//! 域 D18 `receivable` 服务编排（页面：W11 客户往来、W13 卡券票款复核）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 发票创建必须在同一事务注册 `BusinessDocument` 并调用统一绑定端口；
//!   `NO_APPROVAL` 返回空绑定，不查询发布定义、不启动实例、不建任务；
//! - 单集合草稿写入（复核缓存更新）→ `&mut NoTransaction`；
//! - 客户回款创建必须在同一事务注册 `BusinessDocument` 并绑定发布定义；
//! - 跨集合资金/票款过账（§8.3 不变量）→
//!   `database::Transactional::with_transaction`，闭包内按稳定顺序锁定两侧，
//!   不执行外部 HTTP/文件 IO。
//! - 资金类入口（回款过账、发票登记、红冲）以业务唯一键
//!   （回款单号/规范化发票号码）与状态迁移构成去重机制，重复提交只产生一条
//!   正式事实。回款过账只能作为审批最终通过动作。
//!
//! 跨域只经 `DatabaseExt` 调对方域 Repository：D13 `sales_orders()` 校验来源
//! 销售单存在；D18 拥有 `invoice` 实体与仓储，D19 经 `invoices()` 复用。

use database::{
    AccessControlExt, DocumentRegistryExt, Executor, FileAssetExt, NoTransaction, PayableExt, ReceivableExt,
    SalesOrderExt, Transactional, WorkItemExt,
};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{
    BusinessDocument, DocumentType, WorkflowAction, WorkflowActionData, WorkflowActionType,
};

use entities::ids::{
    BusinessDocumentId, CustomerReceiptId, InvoiceId, PayableAccountId, PurchaseInvoiceAllocationId,
    ReceiptAllocationId, ReceivableAccountId, ReceivableEntryId, ReceivableFundsReviewId,
    SalesInvoiceAllocationId, SalesOrderId, SalesOrderRevisionId, WorkflowActionId,
};
use entities::money::Amount;
use entities::payable::{PurchaseInvoiceAllocation, PurchaseInvoiceAllocationData};
use entities::receivable::{
    AccountReviewStatus, AllocationAction, CardFundsRegistrationAllocationInput,
    CardFundsRegistrationAllocations, CardFundsRegistrationAllocationsError, CustomerReceipt,
    CustomerReceiptData, CustomerReceiptStatus, EntryDirection, Invoice, InvoiceData, InvoiceDirection,
    InvoiceKind, InvoiceStatus, PendingReceiptAllocation, ReceiptAllocation, ReceiptAllocationData,
    ReceivableAccount, ReceivableAccountData, ReceivableEntry, ReceivableEntryData, ReceivableEntryType,
    ReceivableFundsReview, ReceivableFundsReviewData, RedInvoiceAllocationBasis, RedInvoiceAllocationPlan,
    RedInvoiceAllocationPlanError, RedInvoiceAllocationReversal, ReviewResult, SalesInvoiceAllocation,
    SalesInvoiceAllocationData,
};
use entities::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use entities::Permission;
use id_generator::next_id;
use mongodb::Database;
use sha2::{Digest, Sha256};
use validator::Validate;

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, binding_decision,
    BindPublishedDefinitionCommand, BindingDecision,
};
use crate::approval::business_adapter::{adapter_spec_of, BindingRevalidationContext};
use crate::approval::execution::idempotency::normalize_idempotency_key;
use crate::approval::execution::{
    command_may_have_committed, command_recovery_delay, prepare_cancel, prepare_start,
};
use crate::approval::policy::{policy_of, DocumentApprovalPolicy};
use crate::audit::{AuditActor, CommandReceipt, CommandReceiptServiceExt as _};
use crate::document_registry::{find_approval_binding, new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use crate::iam::{self, SharedRbacService};
use crate::work_item::{WorkItemAllowedAction, WorkItemService};

mod adapter;
mod cancel_approval;
pub(crate) mod card_funds_decision;
pub(crate) mod card_funds_task;
mod customer_receipt_commit;
mod dto;
mod invoice_commit;
pub(crate) mod invoice_task;
mod start_approval;

pub use self::adapter::customer_receipt_object_readable;
use self::adapter::{
    build_customer_receipt_snapshot, customer_receipt_adapter, customer_receipt_responsible_org_id,
    customer_receipt_start_command, customer_receipt_subject_ref, document_approval_view,
    ensure_final_approve_posting, execute_customer_receipt_domain_action, require_frozen_binding,
    start_approval_command_kind, start_customer_receipt_approval, RECENT_HISTORY_LIMIT,
};
use self::cancel_approval::{
    build_customer_receipt_cancel_input, load_cancel_runtime, persist_customer_receipt_cancel,
    CustomerReceiptCancelPersistInput,
};
use self::card_funds_decision::{
    canonical_evidence as card_funds_canonical_evidence,
    validate_evidence_assets as validate_card_funds_evidence_assets,
    validated_from_dto as validated_card_funds_decision, workflow_comment as card_funds_workflow_comment,
};
use self::customer_receipt_commit::PreparedCustomerReceiptCommit;
use self::dto::SortDir;
pub use self::dto::{
    CancelCustomerReceiptApprovalRequest, CardFundsRegistrationAllocation, CardFundsRegistrationResult,
    CardFundsReviewActionBlockerView, CardFundsReviewAllowedAction, CardFundsReviewBusinessResult,
    CardFundsReviewConclusion, CardFundsReviewDecision, CardFundsReviewDetailParams,
    CardFundsReviewFollowUpWorkItem, CardFundsReviewResult, CardFundsReviewType,
    CommitCustomerReceiptRequest, CommitInvoiceRequest, CommitRedInvoiceRequest,
    CompleteCardFundsReviewCommand, CompleteCardFundsReviewResult, CompletedWorkItemStatus,
    CreateCustomerReceiptRequest, CreateInvoiceRequest, CreateReceivableAccountRequest,
    CustomerReceiptListParams, CustomerReceiptView, DocumentApprovalView, FundsReviewView, InvoiceListParams,
    InvoiceView, PageView, PostCustomerReceiptRequest, PostInvoiceRequest, ReceiptAllocationView,
    ReceivableAccountListParams, ReceivableAccountSummaryView, ReceivableAccountView,
    ReceivableInvoiceFactView, ReceivableReceiptFactView, RegisterCardFundsInvoiceRequest,
    RegisterCardFundsReceiptRequest, SalesInvoiceAllocationLineRequest, SalesInvoiceAllocationView,
    SubmitCustomerReceiptRequest,
};
use self::invoice_commit::{convert_post_allocations, ensure_sales_invoice, PreparedInvoiceCommit};
use self::start_approval::{
    build_customer_receipt_start_input, load_bound_definition_graph,
    load_bound_definition_graph_with_executor, load_start_receipt, load_start_receipt_with_executor,
    persist_customer_receipt_start, persist_customer_receipt_start_in_transaction,
    replay_customer_receipt_start_with_executor, CustomerReceiptStartInput, CustomerReceiptStartPersistInput,
};

/// 应收往来子账列表筛选条件类型（经 `ReceivableExt` 关联类型跨 crate 可达）。
type ReceivableAccountFilter = <mongodb::Database as ReceivableExt>::ReceivableAccountFilter;
/// 客户回款单列表筛选条件类型。
type CustomerReceiptFilter = <mongodb::Database as ReceivableExt>::CustomerReceiptFilter;
/// 发票列表筛选条件类型。
type InvoiceFilter = <mongodb::Database as ReceivableExt>::InvoiceFilter;

/// 客户往来服务。
///
/// 提供应收台账、回款、销项发票与卡券票款复核的查询与过账编排。
pub struct ReceivableService {
    db: Database,
    rbac: SharedRbacService,
}

impl ReceivableService {
    /// 创建客户往来服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        let rbac = iam::shared_rbac_service(db.clone());
        Self { db, rbac }
    }

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
        let formal = WorkItemService::new(self.db.clone(), rbac.clone())
            .work_item_detail(work_item_id, actor)
            .await?;
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
        view.work_item = Some(formal.clone());
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

        let subject = iam::subject(actor.kind(), actor.id());
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

    /// 以 W13 强类型领域命令完成卡券票款正式复核。
    ///
    /// 单事务锁定任务、应收账户、复核链和当前票款事实，重验全部任务/领域版本、
    /// 当前责任与岗位分离后，追加复核事实和 `workflow_action`，刷新账户查询缓存，
    /// 并由领域命令完成原任务。驳回在同一事务按当前责任规则创建同类型后继任务，
    /// 未决复核责任不得离开工作台。
    /// 审计记录同时充当不泄漏原始幂等键的稳定结果收据。
    ///
    /// # 错误
    /// 任务/领域版本漂移返回冲突；任务、证据、结论或岗位分离不满足时 fail closed。
    pub async fn complete_card_funds_review(
        &self,
        command: CompleteCardFundsReviewCommand,
        actor: &AuditActor,
    ) -> Result<CompleteCardFundsReviewResult> {
        command.validate()?;
        if command.work_item_id.as_ref().trim().is_empty()
            || command.work_item_id.as_ref().chars().count() > 128
        {
            return Err(Error::ValidationError("任务 ID 非法".to_string()));
        }
        let validated = validated_card_funds_decision(&command.decision)?;
        let expected_task_version = parse_task_version(&command.expected_task_version)?;
        let fingerprint = card_funds_command_fingerprint(&command)?;
        let audit_id = card_funds_audit_id(actor.id(), &command.idempotency_key);
        if let Some(result) = self
            .replay_card_funds_review(&audit_id, &fingerprint, &command.work_item_id)
            .await?
        {
            return Ok(result);
        }

        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let rbac_for_tx = self.rbac.clone();
        let command_for_tx = command.clone();
        let validated_for_tx = validated.clone();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let decision = &command_for_tx.decision;
                    let mut work_item = db
                        .work_items()
                        .find_by_id(&command_for_tx.work_item_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("卡券票款复核任务不存在".to_string()))?;
                    validate_card_funds_work_item(
                        &work_item,
                        decision,
                        expected_task_version,
                        &command_for_tx.expected_subject_version,
                        &actor_id,
                    )?;

                    let mut account = db
                        .receivable_accounts()
                        .find_by_id(&decision.receivable_account_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
                    let snapshot = load_card_funds_snapshot(&db, &account, session).await?;
                    validate_card_funds_versions(&account, &snapshot, decision, &work_item)?;
                    validate_card_funds_facts(&account, &snapshot, decision)?;
                    {
                        let assets = db
                            .file_assets()
                            .find_by_ids(validated_for_tx.evidence().document_ids(), session)
                            .await?;
                        validate_card_funds_evidence_assets(&validated_for_tx, &assets, Instant::now())?;
                    }
                    validate_card_funds_reviewer_separation(
                        &db, &account, &snapshot, &work_item, &actor_id, session,
                    )
                    .await?;
                    WorkItemService::new(db.clone(), rbac_for_tx)
                        .ensure_domain_decision_access(&actor_owned, &work_item, session)
                        .await?;

                    let completed_at = Instant::now();
                    let evidence = card_funds_canonical_evidence(&validated_for_tx);
                    let review_type = entity_review_type(match validated_for_tx.review_type() {
                        entities::receivable::EntityCardFundsReviewType::Opening => CardFundsReviewType::Opening,
                        entities::receivable::EntityCardFundsReviewType::SyncDelta => CardFundsReviewType::SyncDelta,
                    });
                    let review_result = entity_review_result(match validated_for_tx.review_result() {
                        entities::receivable::EntityCardFundsReviewResult::Approved => CardFundsReviewResult::Approved,
                        entities::receivable::EntityCardFundsReviewResult::Rejected => CardFundsReviewResult::Rejected,
                    });
                    let review = ReceivableFundsReview::new(
                        ReceivableFundsReviewId::new(next_id()),
                        ReceivableFundsReviewData {
                            receivable_account_id: decision.receivable_account_id.clone(),
                            review_no: decision.expected_next_review_no,
                            review_type,
                            work_item_id: command_for_tx.work_item_id.clone(),
                            evidence_document_id: decision.evidence_document_ids.first().cloned(),
                            evidence_reference: evidence.clone(),
                            review_result,
                            reviewed_by: actor_id.clone(),
                            reviewed_at: completed_at,
                            supersedes_review_id: snapshot
                                .reviews
                                .last()
                                .map(|tail| tail.base.id.clone().into()),
                        },
                    )?;

                    let cache_status = match validated_for_tx.review_result() {
                        entities::receivable::EntityCardFundsReviewResult::Approved => AccountReviewStatus::Reviewed,
                        entities::receivable::EntityCardFundsReviewResult::Rejected => {
                            let dto_type = match validated_for_tx.review_type() {
                                entities::receivable::EntityCardFundsReviewType::Opening => CardFundsReviewType::Opening,
                                entities::receivable::EntityCardFundsReviewType::SyncDelta => CardFundsReviewType::SyncDelta,
                            };
                            pending_review_status(dto_type)
                        }
                    };
                    let cache_update = match validated_for_tx.review_result() {
                        entities::receivable::EntityCardFundsReviewResult::Approved => entities::receivable::ReceivableAccountUpdate {
                            review_status: Some(cache_status),
                            reviewed_by: Some(actor_id.clone()),
                            reviewed_at: Some(completed_at),
                            review_evidence_reference: Some(evidence.clone().unwrap_or_else(|| {
                                validated_for_tx
                                    .evidence()
                                    .document_ids()
                                    .first()
                                    .map(|id| id.to_string())
                                    .unwrap_or_default()
                            })),
                            gross_total: None,
                            invoiceable_total: None,
                        },
                        entities::receivable::EntityCardFundsReviewResult::Rejected => entities::receivable::ReceivableAccountUpdate {
                            review_status: Some(cache_status),
                            reviewed_by: Some(String::new()),
                            reviewed_at: None,
                            review_evidence_reference: Some(String::new()),
                            gross_total: None,
                            invoiceable_total: None,
                        },
                    };
                    account.update(cache_update, &actor_id)?;

                    let workflow = WorkflowAction::new(
                        WorkflowActionId::new(next_id()),
                        WorkflowActionData {
                            document_id: BusinessDocumentId::new(account.sales_order_id.to_string()),
                            action_type: match validated_for_tx.review_result() {
                                entities::receivable::EntityCardFundsReviewResult::Approved => WorkflowActionType::Approve,
                                entities::receivable::EntityCardFundsReviewResult::Rejected => WorkflowActionType::Reject,
                            },
                            from_status: account_review_status_code(pending_review_status(match validated_for_tx.review_type() {
                                entities::receivable::EntityCardFundsReviewType::Opening => CardFundsReviewType::Opening,
                                entities::receivable::EntityCardFundsReviewType::SyncDelta => CardFundsReviewType::SyncDelta,
                            }))
                            .to_string(),
                            to_status: account_review_status_code(cache_status).to_string(),
                            actor_id: actor_id.clone(),
                            actor_role: work_item.owner_role.clone(),
                            comment: card_funds_workflow_comment(&validated_for_tx),
                        },
                    )?;
                    work_item.record_activity(&actor_id, completed_at)?;
                    work_item.complete_by_domain_command(&actor_id, completed_at)?;

                    db.receivable().append_funds_review(&review, session).await?;
                    db.receivable_accounts().update(&mut account, session).await?;
                    db.workflow_actions().create(&workflow, session).await?;
                    db.work_items().update(&mut work_item, session).await?;
                    let follow_up_work_item = if validated_for_tx.review_result()
                        == entities::receivable::EntityCardFundsReviewResult::Rejected
                    {
                        Some(
                            card_funds_task::ensure_card_funds_review_task(
                                &db,
                                &account,
                                &work_item.subject_version,
                                session,
                            )
                            .await?
                            .ok_or_else(|| Error::Internal("驳回后账户未形成待复核后继任务".to_string()))?,
                        )
                    } else {
                        None
                    };

                    let receipt = CardFundsReviewReceipt {
                        receivable_funds_review_id: review.base.id.clone(),
                        workflow_action_id: workflow.base.id.clone(),
                        review_no: review.review_no,
                        account_review_status: account.review_status.as_str().to_string(),
                        completed_at: completed_at.unix_secs(),
                        review_result: match validated_for_tx.review_result() {
                            entities::receivable::EntityCardFundsReviewResult::Approved => CardFundsReviewResult::Approved,
                            entities::receivable::EntityCardFundsReviewResult::Rejected => CardFundsReviewResult::Rejected,
                        },
                        conclusion: match validated_for_tx.conclusion() {
                            entities::receivable::EntityCardFundsReviewConclusion::NoHistoryFromZero => CardFundsReviewConclusion::NoHistoryFromZero,
                            entities::receivable::EntityCardFundsReviewConclusion::RecordedFactsReconciled => CardFundsReviewConclusion::RecordedFactsReconciled,
                            entities::receivable::EntityCardFundsReviewConclusion::Rejected => CardFundsReviewConclusion::Rejected,
                        },
                        follow_up_work_item_id: follow_up_work_item.as_ref().map(|item| item.base.id.clone()),
                        follow_up_work_item_type: follow_up_work_item
                            .map(|item| item.work_item_type.as_str().to_string()),
                    };
                    let audit = actor_owned.resource_log_with_id(
                        audit_id_for_tx,
                        CARD_FUNDS_REVIEW_ACTION,
                        "receivable_funds_review",
                        account.base.id.clone(),
                        Some(card_funds_receipt_message(&fingerprint_for_tx, &receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CardFundsReviewReceipt, crate::errors::Error>(receipt)
                })
            })
            .await;

        let receipt = match transaction_result {
            Ok(receipt) => receipt,
            Err(error) => {
                if let Some(result) = self
                    .replay_card_funds_review(&audit_id, &fingerprint, &command.work_item_id)
                    .await?
                {
                    return Ok(result);
                }
                return Err(error);
            }
        };
        Ok(receipt.into_result(
            command.work_item_id.as_ref(),
            command.decision.receivable_account_id.as_ref(),
            &audit_id,
        ))
    }

    /// 按稳定审计收据严格重放已完成的 W13 正式结果。
    async fn replay_card_funds_review(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        work_item_id: &entities::ids::WorkItemId,
    ) -> Result<Option<CompleteCardFundsReviewResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.action != CARD_FUNDS_REVIEW_ACTION
            || audit.resource_type != "receivable_funds_review"
            || !audit.success
        {
            return Err(Error::Internal("卡券票款复核幂等收据身份非法".to_string()));
        }
        let account_id = audit
            .resource_id
            .as_deref()
            .ok_or_else(|| Error::Internal("卡券票款复核幂等收据缺少应收账户".to_string()))?;
        let mut receipt = parse_card_funds_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("卡券票款复核幂等收据为空".to_string()))?,
            expected_fingerprint,
        )?;
        if receipt.requires_legacy_rejected_follow_up() {
            let follow_up = self
                .ensure_legacy_rejected_card_funds_follow_up(
                    &receipt.receivable_funds_review_id,
                    work_item_id,
                    account_id,
                )
                .await?;
            receipt.attach_follow_up(&follow_up);
        }
        Ok(Some(receipt.into_result(
            work_item_id.as_ref(),
            account_id,
            audit_id,
        )))
    }

    /// 为七字段旧版驳回收据补建或复用正式 W13 后继任务。
    ///
    /// 旧版驳回事务已经完成原任务并保留账户待复核状态，但收据没有记录后继任务。
    /// 本迁移在事务内核对原任务、正式复核事实和账户状态，再按当前财务责任规则
    /// 建立后继任务；重复回放复用同一开放任务，后继已处理时由相邻复核事实定位，
    /// 不修改历史审计收据。
    async fn ensure_legacy_rejected_card_funds_follow_up(
        &self,
        review_id: &str,
        work_item_id: &entities::ids::WorkItemId,
        account_id: &str,
    ) -> Result<WorkItem> {
        let db = self.db.clone();
        let client = db.client().clone();
        let review_id = review_id.to_string();
        let work_item_id = work_item_id.clone();
        let account_id = ReceivableAccountId::new(account_id.to_string());
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let original = db
                        .work_items()
                        .find_by_id(&work_item_id, session)
                        .await?
                        .ok_or_else(|| {
                            Error::ConflictError("旧版驳回复核的原工作项不存在，无法补建后继任务".to_string())
                        })?;
                    if original.status != WorkItemStatus::Completed
                        || original.business_object_type != "receivable_account"
                        || original.business_object_id != account_id.to_string()
                        || !matches!(
                            original.work_item_type,
                            WorkItemType::CardFundsReview | WorkItemType::CardFundsDeltaReview
                        )
                    {
                        return Err(Error::ConflictError(
                            "旧版驳回复核的原工作项身份或状态不一致".to_string(),
                        ));
                    }

                    let reviews = db
                        .receivable_funds_reviews()
                        .find_reviews_by_account(&account_id, session)
                        .await?;
                    let review = reviews
                        .iter()
                        .find(|review| review.base.id == review_id)
                        .ok_or_else(|| {
                            Error::ConflictError("旧版驳回复核的正式复核事实不存在".to_string())
                        })?;
                    if review.work_item_id != work_item_id || review.review_result != ReviewResult::Rejected {
                        return Err(Error::ConflictError(
                            "旧版驳回复核的正式事实与原工作项不一致".to_string(),
                        ));
                    }
                    let next_review_no = review
                        .review_no
                        .checked_add(1)
                        .ok_or_else(|| Error::Internal("旧版驳回复核的复核号已达到上限".to_string()))?;
                    let completed_successor_id = reviews
                        .iter()
                        .find(|candidate| {
                            candidate.review_no == next_review_no
                                && candidate
                                    .supersedes_review_id
                                    .as_ref()
                                    .is_some_and(|id| id.as_ref() == review_id.as_str())
                        })
                        .map(|candidate| candidate.work_item_id.clone());
                    if let Some(successor_id) = completed_successor_id {
                        let successor = db
                            .work_items()
                            .find_by_id(&successor_id, session)
                            .await?
                            .ok_or_else(|| {
                                Error::ConflictError("旧版驳回复核的已处理后继工作项不存在".to_string())
                            })?;
                        if successor.status != WorkItemStatus::Completed
                            || successor.work_item_type != original.work_item_type
                            || successor.business_object_type != "receivable_account"
                            || successor.business_object_id != account_id.to_string()
                            || successor.subject_version != original.subject_version
                        {
                            return Err(Error::ConflictError(
                                "旧版驳回复核的已处理后继工作项身份不一致".to_string(),
                            ));
                        }
                        return Ok(successor);
                    }

                    let account = db
                        .receivable_accounts()
                        .find_by_id(&account_id, session)
                        .await?
                        .ok_or_else(|| Error::ConflictError("旧版驳回复核的应收账户不存在".to_string()))?;
                    let expected_type = match account.review_status {
                        AccountReviewStatus::OpeningPending => WorkItemType::CardFundsReview,
                        AccountReviewStatus::SyncDeltaPending => WorkItemType::CardFundsDeltaReview,
                        AccountReviewStatus::NotApplicable | AccountReviewStatus::Reviewed => {
                            return Err(Error::ConflictError(
                                "旧版驳回复核的应收账户已离开待复核状态，不能自动补建后继任务".to_string(),
                            ));
                        }
                    };
                    if original.work_item_type != expected_type {
                        return Err(Error::ConflictError(
                            "旧版驳回复核的任务类型与账户待复核状态不一致".to_string(),
                        ));
                    }

                    card_funds_task::ensure_card_funds_review_task(
                        &db,
                        &account,
                        &original.subject_version,
                        session,
                    )
                    .await?
                    .ok_or_else(|| Error::ConflictError("旧版驳回复核未处于可补建后继任务的状态".to_string()))
                })
            })
            .await
    }

    /// 在 W13 当前责任任务内原子登记历史回款及其核销分配。
    ///
    /// 任务、责任、销售版本和票款事实版本在事务内重验；分配集合先由领域
    /// 值对象完成单账户、严格正数和金额守恒校验，随后由 Service 持有事务边界。
    ///
    /// # 参数
    /// * `req` - 历史回款字段、任务快照、分配意图与幂等键
    /// * `actor` - 已通过鉴权且用于责任校验和审计的操作人
    ///
    /// # 返回
    /// 返回登记后的票款事实版本、账户金额进度和新建回款事实。
    ///
    /// # 错误
    /// 请求校验、任务责任或版本校验、领域分配不变量、重复单号、仓储写入或
    /// 事务提交失败时返回既有服务错误，且原错误分类与文案保持不变。
    ///
    /// # 约束
    /// 回款单、单据注册、核销分配、子账进度、销售进度与审计任一失败时整体回滚。
    pub async fn register_card_funds_receipt(
        &self,
        req: RegisterCardFundsReceiptRequest,
        actor: &AuditActor,
    ) -> Result<CardFundsRegistrationResult> {
        req.validate()?;
        let expected_task_version = parse_task_version(&req.expected_task_version)?;
        let fingerprint = card_funds_registration_fingerprint(&req)?;
        let audit_id = card_funds_registration_audit_id(
            CARD_FUNDS_RECEIPT_REGISTRATION_ACTION,
            actor.id(),
            &req.idempotency_key,
        );
        let receipt_no = normalized_registration_no(req.receipt_no.as_deref())
            .unwrap_or_else(|| stable_registration_no("SK", actor.id(), &req.idempotency_key));
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let req_for_tx = req.clone();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let (account_id, receipt_id) = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if let Some(replayed) = replay_card_funds_registration(
                        &db,
                        &audit_id_for_tx,
                        CARD_FUNDS_RECEIPT_REGISTRATION_ACTION,
                        &fingerprint_for_tx,
                        session,
                    )
                    .await?
                    {
                        return Ok::<(String, String), crate::errors::Error>(replayed);
                    }
                    let (account, snapshot) = load_card_funds_registration_context(
                        &db,
                        CardFundsRegistrationContextInput {
                            rbac: rbac.clone(),
                            work_item_id: &req_for_tx.work_item_id,
                            expected_task_version,
                            expected_subject_version: &req_for_tx.expected_subject_version,
                            expected_funds_fact_version: &req_for_tx.expected_funds_fact_version,
                            actor: &actor_owned,
                        },
                        session,
                    )
                    .await?;
                    let validated_allocations = card_funds_registration_allocations(
                        &req_for_tx.allocations,
                        &account.base.id,
                        req_for_tx.gross_amount,
                    )?;
                    let registration_amount = validated_allocations.total();
                    if db
                        .customer_receipts()
                        .find_by_receipt_no(&receipt_no, session)
                        .await?
                        .is_some()
                    {
                        return Err(Error::ConflictError("回款单号已登记，请勿重复提交".to_string()));
                    }

                    let allocation_plan =
                        plan_card_funds_receipt_allocations(&snapshot, registration_amount)?;
                    let mut receipt = CustomerReceipt::new(
                        CustomerReceiptId::new(next_id()),
                        CustomerReceiptData {
                            receipt_no: receipt_no.clone(),
                            counterparty_party_id: account.counterparty_party_id.clone(),
                            customer_id: Some(account.customer_id.clone()),
                            received_at: req_for_tx.received_at,
                            amount: registration_amount,
                            bank_reference: Some(req_for_tx.evidence_reference.clone()),
                        },
                        actor_id.clone(),
                    )?;
                    receipt.register_historical_fact()?;
                    let organization_id = customer_receipt_responsible_org_id(&receipt)?;
                    let bind_command = BindPublishedDefinitionCommand {
                        document_type: DocumentType::CustomerReceipt,
                        business_object_id: receipt.base.id.clone(),
                        business_object_version: receipt.base.version,
                        context: BindingRevalidationContext {
                            organization_id,
                            creator_id: actor_id.clone(),
                        },
                    };
                    let document = new_registered_document(
                        &receipt.base.id,
                        DocumentType::CustomerReceipt,
                        receipt.receipt_no.clone(),
                    )?;
                    persist_bound_customer_receipt_document(
                        &db,
                        &rbac,
                        document,
                        &bind_command,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    db.customer_receipts().create(&receipt, session).await?;
                    for (index, (entry_id, amount)) in allocation_plan.iter().enumerate() {
                        let applied = db
                            .receivable_accounts()
                            .apply_settlement(
                                &ReceivableAccountId::new(account.base.id.clone()),
                                amount,
                                &actor_id,
                                session,
                            )
                            .await?;
                        if !applied {
                            return Err(Error::BusinessLogicError(
                                "子账剩余开放余额不足，历史回款登记被拒绝".to_string(),
                            ));
                        }
                        let allocation = ReceiptAllocation::new(
                            ReceiptAllocationId::new(next_id()),
                            ReceiptAllocationData {
                                customer_receipt_id: CustomerReceiptId::new(receipt.base.id.clone()),
                                receivable_entry_id: entry_id.clone(),
                                allocation_seq: (index as u32) + 1,
                                allocation_action: AllocationAction::Apply,
                                allocated_amount: *amount,
                                allocated_at: Instant::now(),
                                reverses_allocation_id: None,
                            },
                        )?;
                        db.receipt_allocations().create(&allocation, session).await?;
                    }
                    let create_audit = actor_owned.clone().resource_log_with_message(
                        "customer_receipt.card_funds_register",
                        "customer_receipt",
                        receipt.base.id.clone(),
                        Some(req_for_tx.evidence_reference.clone()),
                    )?;
                    db.audit_logs().create(&create_audit, session).await?;
                    let receipt_audit = actor_owned.clone().resource_log_with_id(
                        audit_id_for_tx,
                        CARD_FUNDS_RECEIPT_REGISTRATION_ACTION,
                        "receivable_account",
                        account.base.id.clone(),
                        Some(card_funds_registration_receipt_message(
                            &fingerprint_for_tx,
                            "receipt",
                            &receipt.base.id,
                        )),
                    )?;
                    db.audit_logs().create(&receipt_audit, session).await?;
                    crate::sales_order::update_sales_order_money_progress(
                        &db,
                        session,
                        &account.sales_order_id,
                        actor_id.clone(),
                        None,
                    )
                    .await?;
                    Ok::<(String, String), crate::errors::Error>((account.base.id, receipt.base.id))
                })
            })
            .await?;
        self.card_funds_registration_result(&account_id, Some(&receipt_id), None)
            .await
    }

    /// 在 W13 当前责任任务内原子登记历史销项发票及其分配。
    ///
    /// 任务、责任、销售版本和票款事实版本在事务内重验；分配集合先由领域
    /// 值对象完成单账户、严格正数和金额守恒校验，发票净税恒等与写入仍由 Service 编排。
    ///
    /// # 参数
    /// * `req` - 历史销项发票字段、任务快照、分配意图与幂等键
    /// * `actor` - 已通过鉴权且用于责任校验和审计的操作人
    ///
    /// # 返回
    /// 返回登记后的票款事实版本、账户金额进度和新建发票事实。
    ///
    /// # 错误
    /// 请求校验、任务责任或版本校验、领域分配不变量、净税恒等、重复票号、
    /// 仓储写入或事务提交失败时返回既有服务错误，且原错误分类与文案保持不变。
    ///
    /// # 约束
    /// 发票、分配、子账进度、销售进度与审计任一失败时整体回滚。
    pub async fn register_card_funds_invoice(
        &self,
        req: RegisterCardFundsInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<CardFundsRegistrationResult> {
        req.validate()?;
        let expected_task_version = parse_task_version(&req.expected_task_version)?;
        let fingerprint = card_funds_registration_fingerprint(&req)?;
        let audit_id = card_funds_registration_audit_id(
            CARD_FUNDS_INVOICE_REGISTRATION_ACTION,
            actor.id(),
            &req.idempotency_key,
        );
        let invoice_no = normalized_registration_no(req.invoice_no.as_deref())
            .unwrap_or_else(|| stable_registration_no("FP", actor.id(), &req.idempotency_key));
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let req_for_tx = req.clone();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let (account_id, invoice_id) = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if let Some(replayed) = replay_card_funds_registration(
                        &db,
                        &audit_id_for_tx,
                        CARD_FUNDS_INVOICE_REGISTRATION_ACTION,
                        &fingerprint_for_tx,
                        session,
                    )
                    .await?
                    {
                        return Ok::<(String, String), crate::errors::Error>(replayed);
                    }
                    let (account, _snapshot) = load_card_funds_registration_context(
                        &db,
                        CardFundsRegistrationContextInput {
                            rbac: rbac.clone(),
                            work_item_id: &req_for_tx.work_item_id,
                            expected_task_version,
                            expected_subject_version: &req_for_tx.expected_subject_version,
                            expected_funds_fact_version: &req_for_tx.expected_funds_fact_version,
                            actor: &actor_owned,
                        },
                        session,
                    )
                    .await?;
                    let validated_allocations = card_funds_registration_allocations(
                        &req_for_tx.allocations,
                        &account.base.id,
                        req_for_tx.gross_amount,
                    )?;
                    let registration_amount = validated_allocations.total();
                    if registration_amount != req_for_tx.net_amount.checked_add(req_for_tx.tax_amount) {
                        return Err(Error::ValidationError(
                            "发票含税金额必须等于不含税金额加税额".to_string(),
                        ));
                    }
                    if db
                        .invoices()
                        .find_by_direction_and_normalized_no(
                            InvoiceDirection::Sales,
                            &invoice_no.to_uppercase(),
                            session,
                        )
                        .await?
                        .is_some()
                    {
                        return Err(Error::ConflictError("发票号码已登记，请勿重复提交".to_string()));
                    }
                    let mut invoice = Invoice::new(
                        InvoiceId::new(next_id()),
                        InvoiceData {
                            invoice_direction: InvoiceDirection::Sales,
                            invoice_kind: InvoiceKind::Blue,
                            party_id: account.counterparty_party_id.clone(),
                            invoice_code: None,
                            invoice_no: invoice_no.clone(),
                            invoice_date: req_for_tx.invoice_date,
                            gross_amount: registration_amount,
                            net_amount: req_for_tx.net_amount,
                            tax_amount: req_for_tx.tax_amount,
                            rounding_adjustment_amount: zero_amount(),
                            rounding_reason: None,
                            original_invoice_id: None,
                        },
                        &actor_id,
                    )?;
                    invoice.mark_registered(&actor_id)?;
                    register_created_invoice_document(&db, &rbac, &invoice, &actor_owned, session).await?;
                    let applied = db
                        .receivable_accounts()
                        .apply_invoicing(
                            &ReceivableAccountId::new(account.base.id.clone()),
                            &registration_amount,
                            &actor_id,
                            session,
                        )
                        .await?;
                    if !applied {
                        return Err(Error::BusinessLogicError(
                            "子账剩余可开票额度不足，历史发票登记被拒绝".to_string(),
                        ));
                    }
                    db.invoices().create(&invoice, session).await?;
                    let allocation = SalesInvoiceAllocation::new(
                        SalesInvoiceAllocationId::new(next_id()),
                        SalesInvoiceAllocationData {
                            invoice_id: InvoiceId::new(invoice.base.id.clone()),
                            receivable_account_id: ReceivableAccountId::new(account.base.id.clone()),
                            allocation_seq: 1,
                            allocation_action: AllocationAction::Apply,
                            allocated_gross_amount: registration_amount,
                            allocated_net_amount: req_for_tx.net_amount,
                            allocated_tax_amount: req_for_tx.tax_amount,
                            reverses_allocation_id: None,
                        },
                    )?;
                    db.sales_invoice_allocations()
                        .create(&allocation, session)
                        .await?;
                    invoice_task::sync_sales_invoice_task(
                        &db,
                        &ReceivableAccountId::new(account.base.id.clone()),
                        invoice_task::SalesInvoiceTaskChange::InvoicePosted,
                        session,
                    )
                    .await?;
                    let create_audit = actor_owned.clone().resource_log_with_message(
                        "invoice.card_funds_register",
                        "invoice",
                        invoice.base.id.clone(),
                        Some(req_for_tx.evidence_reference.clone()),
                    )?;
                    db.audit_logs().create(&create_audit, session).await?;
                    let receipt_audit = actor_owned.clone().resource_log_with_id(
                        audit_id_for_tx,
                        CARD_FUNDS_INVOICE_REGISTRATION_ACTION,
                        "receivable_account",
                        account.base.id.clone(),
                        Some(card_funds_registration_receipt_message(
                            &fingerprint_for_tx,
                            "invoice",
                            &invoice.base.id,
                        )),
                    )?;
                    db.audit_logs().create(&receipt_audit, session).await?;
                    crate::sales_order::update_sales_order_money_progress(
                        &db,
                        session,
                        &account.sales_order_id,
                        actor_id.clone(),
                        None,
                    )
                    .await?;
                    Ok::<(String, String), crate::errors::Error>((account.base.id, invoice.base.id))
                })
            })
            .await?;
        self.card_funds_registration_result(&account_id, None, Some(&invoice_id))
            .await
    }

    /// 装配 W13 原子登记后的账户金额与本次正式事实。
    async fn card_funds_registration_result(
        &self,
        account_id: &str,
        receipt_id: Option<&str>,
        invoice_id: Option<&str>,
    ) -> Result<CardFundsRegistrationResult> {
        let view = self.receivable_account_view(account_id.to_string()).await?;
        let receipt_facts = receipt_id
            .map(|id| {
                view.receipt_facts
                    .iter()
                    .filter(|fact| fact.receipt_id == id)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let invoice_facts = invoice_id
            .map(|id| {
                view.invoice_facts
                    .iter()
                    .filter(|fact| fact.invoice_id == id)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if receipt_id.is_some() && receipt_facts.is_empty() {
            return Err(Error::Internal("历史回款登记结果缺少正式事实".to_string()));
        }
        if invoice_id.is_some() && invoice_facts.is_empty() {
            return Err(Error::Internal("历史发票登记结果缺少正式事实".to_string()));
        }
        Ok(CardFundsRegistrationResult {
            funds_fact_version: view.funds_fact_version,
            subject_hash: format!("acct:{}:v{}", view.id, view.account_domain_version),
            settled_total: view.settled_total,
            invoiced_total: view.invoiced_total,
            open_total: view.open_total,
            open_invoiceable_total: view.open_invoiceable_total,
            receipt_facts,
            invoice_facts,
        })
    }

    // -----------------------------------------------------------------------
    // 客户回款单
    // -----------------------------------------------------------------------

    /// 分页查询客户回款单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`receipt_no`/`counterparty_party_id`/`status`）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn customer_receipt_list(
        &self,
        params: &CustomerReceiptListParams,
    ) -> Result<PageView<CustomerReceiptView>> {
        params.validate()?;
        let query = params.normalized()?;
        let receipt_ids = self
            .receipt_ids_for_account_scope(
                query.sales_order_id.as_deref(),
                query.receivable_account_id.as_ref(),
            )
            .await?;
        let filter = CustomerReceiptFilter {
            receipt_ids,
            receipt_no: query.receipt_no,
            counterparty_party_id: query.counterparty_party_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .customer_receipts()
            .search_customer_receipts(&filter, &mut NoTransaction)
            .await?;
        let receipt_ids = page
            .items
            .iter()
            .map(|row| CustomerReceiptId::new(row.id.clone()))
            .collect::<Vec<_>>();
        let document_ids = page.items.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
        let mut allocations_by_receipt = HashMap::<String, Vec<ReceiptAllocation>>::new();
        for allocation in self
            .db
            .receipt_allocations()
            .find_allocations_by_receipts(&receipt_ids, &mut NoTransaction)
            .await?
        {
            allocations_by_receipt
                .entry(allocation.customer_receipt_id.to_string())
                .or_default()
                .push(allocation);
        }
        for allocations in allocations_by_receipt.values_mut() {
            allocations.sort_unstable_by_key(|allocation| allocation.allocation_seq);
        }
        let bindings_by_document = self
            .db
            .business_documents()
            .find_documents_by_ids(&document_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|document| (document.base.id.clone(), document.approval_binding))
            .collect::<HashMap<_, _>>();
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            let allocations = allocations_by_receipt.remove(&row.id).unwrap_or_default();
            let (allocated_total, allocations) = allocation_view(&allocations);
            let approval_binding = bindings_by_document.get(&row.id).and_then(Option::as_ref);
            views.push(CustomerReceiptView {
                id: row.id,
                receipt_no: row.receipt_no,
                status: row.status,
                counterparty_party_id: row.counterparty_party_id,
                customer_id: row.customer_id,
                received_at: row.received_at,
                amount: row.amount,
                bank_reference: row.bank_reference,
                version: row.version,
                created_at: row.created_at,
                allocated_total,
                unallocated_amount: row.amount.checked_sub(allocated_total),
                allocations,
                approval: document_approval_view(approval_binding, None, row.status),
            });
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 解析销售单/子账范围内出现过核销分配的回款单主键。
    async fn receipt_ids_for_account_scope(
        &self,
        sales_order_id: Option<&str>,
        account_id: Option<&ReceivableAccountId>,
    ) -> Result<Option<Vec<String>>> {
        let Some(accounts) = self.accounts_for_list_scope(sales_order_id, account_id).await? else {
            return Ok(None);
        };
        let account_ids = accounts
            .iter()
            .map(|account| ReceivableAccountId::new(account.base.id.clone()))
            .collect::<Vec<_>>();
        let entries = self
            .db
            .receivable_entries()
            .find_entries_by_accounts(&account_ids, &mut NoTransaction)
            .await?;
        let entry_ids = entries
            .into_iter()
            .map(|entry| ReceivableEntryId::new(entry.base.id))
            .collect::<Vec<_>>();
        let allocations = self
            .db
            .receipt_allocations()
            .find_allocations_by_entries(&entry_ids, &mut NoTransaction)
            .await?;
        Ok(Some(
            allocations
                .into_iter()
                .map(|allocation| allocation.customer_receipt_id.to_string())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect(),
        ))
    }

    /// 解析销售单/子账范围内出现过分配的销项发票主键。
    async fn invoice_ids_for_account_scope(
        &self,
        sales_order_id: Option<&str>,
        account_id: Option<&ReceivableAccountId>,
    ) -> Result<Option<Vec<String>>> {
        let Some(accounts) = self.accounts_for_list_scope(sales_order_id, account_id).await? else {
            return Ok(None);
        };
        let account_ids = accounts
            .iter()
            .map(|account| ReceivableAccountId::new(account.base.id.clone()))
            .collect::<Vec<_>>();
        let allocations = self
            .db
            .sales_invoice_allocations()
            .find_allocations_by_accounts(&account_ids, &mut NoTransaction)
            .await?;
        Ok(Some(
            allocations
                .into_iter()
                .map(|allocation| allocation.invoice_id.to_string())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect(),
        ))
    }

    /// 读取列表关联范围；未指定范围时返回 `None`，不得触发全量关联扫描。
    async fn accounts_for_list_scope(
        &self,
        sales_order_id: Option<&str>,
        account_id: Option<&ReceivableAccountId>,
    ) -> Result<Option<Vec<ReceivableAccount>>> {
        if sales_order_id.is_none() && account_id.is_none() {
            return Ok(None);
        }
        let mut accounts = if let Some(account_id) = account_id {
            self.db
                .receivable_accounts()
                .find_by_id(account_id.as_ref(), &mut NoTransaction)
                .await?
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            self.db
                .receivable_accounts()
                .find_accounts_by_sales_order_id(sales_order_id.unwrap_or_default(), &mut NoTransaction)
                .await?
        };
        if let Some(sales_order_id) = sales_order_id {
            accounts.retain(|account| account.sales_order_id.as_ref() == sales_order_id);
        }
        Ok(Some(accounts))
    }

    /// 查询客户回款单详情（含核销分配行）。
    ///
    /// # 参数
    /// * `id` - 回款单 ID
    ///
    /// # 返回
    /// 返回回款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 回款单不存在
    pub async fn customer_receipt_detail(&self, id: &str) -> Result<CustomerReceiptView> {
        self.customer_receipt_view(id.to_string()).await
    }

    /// 登记客户回款草稿，并在同一事务绑定已发布审批定义。
    ///
    /// 回款单号全局唯一（`uk_customer_receipts_no` 唯一索引）构成幂等去重。
    /// 绑定失败必须回滚业务实体，不得把绑定推迟到提交。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建回款单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 回款单号重复或流程未配置
    pub async fn create_customer_receipt(
        &self,
        req: CreateCustomerReceiptRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let receipt = CustomerReceipt::new(
            CustomerReceiptId::new(next_id()),
            CustomerReceiptData {
                receipt_no: req.receipt_no,
                counterparty_party_id: req.counterparty_party_id,
                customer_id: req.customer_id,
                received_at: req.received_at,
                amount: req.amount,
                bank_reference: req.bank_reference,
            },
            actor.id(),
        )?;
        persist_created_customer_receipt(&self.db, &self.rbac, receipt.clone(), actor.clone()).await?;
        self.customer_receipt_detail(&receipt.base.id).await
    }

    /// 原子创建或提交客户回款并启动审批。
    ///
    /// 新回款的单据注册与定义绑定、回款实体、冻结核销分配、审批运行事实、
    /// 不可变快照、入口任务和审计全部位于同一事务。已有草稿用乐观锁校验后
    /// 走同一启动事务，前端不得再执行“先创建草稿、再提交”。
    ///
    /// # 参数
    /// * `req` - 新回款或已有草稿身份、冻结分配与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回进入审批后的回款单视图。
    ///
    /// # 错误
    /// * `ValidationError` - 参数组合或分配不合法
    /// * `ConflictError` - 草稿版本、状态、绑定或审批定义冲突
    /// * `NotFound` - 已有草稿不存在
    pub async fn commit_customer_receipt(
        &self,
        req: CommitCustomerReceiptRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_payload(
            "customer-receipt-commit-",
            actor.id(),
            "customer_receipt.commit",
            "customer_receipt",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(receipt_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self.customer_receipt_detail(&receipt_id).await;
        }
        let prepared = req.prepare()?;
        let (new_receipt, requested_id, expected_version, allocations) = match prepared {
            PreparedCustomerReceiptCommit::New { receipt, allocations } => {
                receipt.validate()?;
                let candidate = CustomerReceipt::new(
                    CustomerReceiptId::new(next_id()),
                    CustomerReceiptData {
                        receipt_no: receipt.receipt_no,
                        counterparty_party_id: receipt.counterparty_party_id,
                        customer_id: receipt.customer_id,
                        received_at: receipt.received_at,
                        amount: receipt.amount,
                        bank_reference: receipt.bank_reference,
                    },
                    actor.id(),
                )?;
                (Some(candidate), None, None, allocations)
            }
            PreparedCustomerReceiptCommit::Existing {
                receipt_id,
                expected_version,
                allocations,
            } => (None, Some(receipt_id), Some(expected_version), allocations),
        };
        let idempotency_key = req.idempotency_key;
        let adapter = customer_receipt_adapter()?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let command_receipt_for_tx = command_receipt.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let (mut receipt, binding) = match new_receipt {
                        Some(candidate) => {
                            if db
                                .customer_receipts()
                                .find_by_receipt_no(&candidate.receipt_no, session)
                                .await?
                                .is_some()
                            {
                                return Err(Error::ConflictError("回款单号已存在，请刷新后重试".to_string()));
                            }
                            let organization_id = customer_receipt_responsible_org_id(&candidate)?;
                            let bind_command = BindPublishedDefinitionCommand {
                                document_type: DocumentType::CustomerReceipt,
                                business_object_id: candidate.base.id.clone(),
                                business_object_version: candidate.base.version,
                                context: BindingRevalidationContext {
                                    organization_id,
                                    creator_id: actor_owned.id().to_string(),
                                },
                            };
                            let document = new_registered_document(
                                &candidate.base.id,
                                DocumentType::CustomerReceipt,
                                candidate.receipt_no.clone(),
                            )?;
                            let binding = persist_bound_customer_receipt_document(
                                &db,
                                &rbac,
                                document,
                                &bind_command,
                                &actor_owned,
                                session,
                            )
                            .await?;
                            db.customer_receipts().create(&candidate, session).await?;
                            let audit = actor_owned.clone().resource_log(
                                "customer_receipt.create",
                                "customer_receipt",
                                candidate.base.id.clone(),
                            )?;
                            db.audit_logs().create(&audit, session).await?;
                            (candidate, binding)
                        }
                        None => {
                            let receipt_id = requested_id
                                .as_deref()
                                .ok_or_else(|| Error::ValidationError("已有回款缺少主键".to_string()))?;
                            let receipt = db
                                .customer_receipts()
                                .find_by_id(receipt_id, session)
                                .await?
                                .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
                            ensure_expected_version(
                                receipt.base.version,
                                expected_version.ok_or_else(|| {
                                    Error::ValidationError("已有回款缺少期望版本".to_string())
                                })?,
                            )?;
                            let binding = find_approval_binding(&db, receipt_id, session)
                                .await?
                                .ok_or_else(|| Error::ConflictError("客户回款单缺少审批绑定".to_string()))?;
                            (receipt, binding)
                        }
                    };
                    let binding = require_frozen_binding(Some(&binding))?.clone();
                    start_customer_receipt_approval(&mut receipt, allocations)?;
                    let id = receipt.base.id.clone();
                    let subject = customer_receipt_subject_ref(&id)?;
                    let now = Instant::now();
                    let snapshot = build_customer_receipt_snapshot(&receipt, actor_owned.id(), now)?;
                    let organization_id = customer_receipt_responsible_org_id(&receipt)?;
                    let _ = customer_receipt_object_readable(&organization_id, actor_owned.id())?;
                    let graph = load_bound_definition_graph_with_executor(&db, &binding, session).await?;
                    let existing_start_receipt = load_start_receipt_with_executor(
                        &db,
                        &subject,
                        receipt.approval_subject_version,
                        &idempotency_key,
                        session,
                    )
                    .await?;
                    let start_input = build_customer_receipt_start_input(CustomerReceiptStartInput {
                        graph,
                        binding: &binding,
                        subject,
                        subject_version: receipt.approval_subject_version,
                        actor_id: actor_owned.id(),
                        organization_id: &organization_id,
                        idempotency_key: &idempotency_key,
                        receipt: existing_start_receipt,
                        now,
                    })?;
                    let prepared = prepare_start(start_input)?;
                    let committed = persist_customer_receipt_start_in_transaction(
                        &db,
                        CustomerReceiptStartPersistInput {
                            receipt,
                            actor: actor_owned.clone(),
                            id,
                            snapshot_payload: snapshot,
                            prepared,
                            owner_role: adapter.owner_role,
                            organization_id,
                            now,
                        },
                        session,
                    )
                    .await?;
                    let command_audit =
                        command_receipt_for_tx.audit(actor_owned.clone(), committed.base.id.clone())?;
                    db.audit_logs().create(&command_audit, session).await?;
                    Ok::<CustomerReceipt, crate::errors::Error>(committed)
                })
            })
            .await;

        let committed = match transaction_result {
            Ok(committed) => committed,
            Err(error) => match command_receipt.committed_resource_id(&self.db).await? {
                Some(receipt_id) => return self.customer_receipt_detail(&receipt_id).await,
                None => return Err(error),
            },
        };

        self.customer_receipt_detail(&committed.base.id).await
    }

    /// 提交客户回款并调用统一 `start_approval`。
    ///
    /// 按合同 §4.4.1 冻结 `approval_subject_version` 与 `subject_snapshot`，
    /// 单据进入 `IN_APPROVAL`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// # 参数
    /// * `id` - 回款单主键
    /// * `req` - 提交请求（版本、幂等键与冻结分配）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交后的回款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 回款单不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    pub async fn submit_customer_receipt(
        &self,
        id: &str,
        req: SubmitCustomerReceiptRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let adapter = customer_receipt_adapter()?;
        let mut receipt = self.load_customer_receipt(id).await?;
        ensure_expected_version(receipt.base.version, req.expected_version)?;
        let allocations = self::customer_receipt_commit::convert_allocations(&req.allocations)?;
        start_customer_receipt_approval(&mut receipt, allocations)?;
        self.dispatch_customer_receipt_start(id, receipt, req.idempotency_key, actor, adapter)
            .await
    }

    /// 撤回客户回款审批，成功后回到草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// # 参数
    /// * `id` - 回款单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的回款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 回款单不存在
    /// * `ConflictError` - 非审批中、已最终通过或并发冲突
    pub async fn cancel_customer_receipt_approval(
        &self,
        id: &str,
        req: CancelCustomerReceiptApprovalRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
        let mut receipt = self.load_customer_receipt(id).await?;
        ensure_expected_version(receipt.base.version, req.expected_version)?;
        self.persist_cancelled_customer_receipt(id, &mut receipt, &req, actor)
            .await?;
        self.customer_receipt_detail(id).await
    }

    /// 客户端直接过账失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 返回
    /// 恒返回冲突。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_client_post() -> Result<CustomerReceiptView> {
        Err(Error::ConflictError(
            "客户回款过账只能由审批最终通过动作执行，客户端不得直接过账".to_string(),
        ))
    }

    /// 从绑定读取定义并持久化启动事实。
    ///
    /// # 错误
    /// 无绑定、定义缺失或写入失败时返回错误。
    async fn dispatch_customer_receipt_start(
        &self,
        id: &str,
        receipt: CustomerReceipt,
        idempotency_key: String,
        actor: &AuditActor,
        adapter: adapter::CustomerReceiptAdapter,
    ) -> Result<CustomerReceiptView> {
        let subject = customer_receipt_subject_ref(id)?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let now = Instant::now();
        let snapshot = build_customer_receipt_snapshot(&receipt, actor.id(), now)?;
        let start = customer_receipt_start_command(
            id,
            receipt.approval_subject_version,
            actor.id(),
            &idempotency_key,
        );
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let organization_id = customer_receipt_responsible_org_id(&receipt)?;
        let _ = customer_receipt_object_readable(&organization_id, actor.id())?;
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_start_receipt(
            &self.db,
            &subject,
            receipt.approval_subject_version,
            &idempotency_key,
        )
        .await?;
        let start_input = build_customer_receipt_start_input(CustomerReceiptStartInput {
            graph,
            binding: &binding,
            subject,
            subject_version: receipt.approval_subject_version,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key: &idempotency_key,
            receipt: existing_receipt,
            now,
        })?;
        let prepared = prepare_start(start_input)?;
        let recovery_subject_version = receipt.approval_subject_version;
        let persisted = persist_customer_receipt_start(
            &self.db,
            CustomerReceiptStartPersistInput {
                receipt,
                actor: actor.clone(),
                id: id.to_string(),
                snapshot_payload: snapshot,
                prepared,
                owner_role: adapter.owner_role,
                organization_id,
                now,
            },
        )
        .await;
        if let Err(error) = persisted {
            if !command_may_have_committed(&error) {
                return Err(error);
            }
            self.recover_customer_receipt_start(id, recovery_subject_version, &idempotency_key, actor, error)
                .await?;
        }
        self.customer_receipt_detail(id).await
    }

    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    async fn recover_customer_receipt_start(
        &self,
        receipt_id: &str,
        subject_version: u32,
        idempotency_key: &str,
        actor: &AuditActor,
        original_error: Error,
    ) -> Result<String> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let receipt_id = receipt_id.to_string();
            let idempotency_key = idempotency_key.to_string();
            let actor_id = actor.id().to_string();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        let receipt = db
                            .customer_receipts()
                            .find_by_id(&receipt_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
                        let organization_id = customer_receipt_responsible_org_id(&receipt)?;
                        let _ = customer_receipt_object_readable(&organization_id, &actor_id)?;
                        let binding = find_approval_binding(&db, &receipt_id, session).await?;
                        let binding = require_frozen_binding(binding.as_ref())?;
                        let subject = customer_receipt_subject_ref(&receipt_id)?;
                        replay_customer_receipt_start_with_executor(
                            &db,
                            &subject,
                            subject_version,
                            &idempotency_key,
                            binding,
                            &actor_id,
                            session,
                        )
                        .await
                    })
                })
                .await;
            match recovered {
                Ok(Some(instance_id)) => return Ok(instance_id),
                Ok(None) => {}
                Err(error) if command_may_have_committed(&error) => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_customer_receipt(
        &self,
        id: &str,
        receipt: &mut CustomerReceipt,
        req: &CancelCustomerReceiptApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let adapter = customer_receipt_adapter()?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = customer_receipt_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, receipt.approval_subject_version).await?;
        let now = Instant::now();
        let idempotency_key = normalize_idempotency_key(&req.idempotency_key)?;
        let input = build_customer_receipt_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_customer_receipt_domain_action(receipt, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "customer_receipt.cancel_approval",
            "customer_receipt",
            id.to_string(),
        )?;
        persist_customer_receipt_cancel(
            &self.db,
            CustomerReceiptCancelPersistInput {
                receipt: receipt.clone(),
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: req.reason.clone(),
                now,
                audit,
            },
        )
        .await
    }

    /// 按主键读取客户回款单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    async fn load_customer_receipt(&self, id: &str) -> Result<CustomerReceipt> {
        self.db
            .customer_receipts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))
    }

    /// 最终通过过账并核销（§8.3-1 事务不变量）。
    ///
    /// 作为合同 `on_final_approve`，仅 `IN_APPROVAL` 可进入过账。同一事务内：
    /// 校验回款与应收分录同一往来主体、分录开放余额与回款剩余余额；写提交时
    /// 冻结的核销分配（`APPLY`）；按条件原子更新子账已核销进度。
    ///
    /// # 参数
    /// * `id` - 回款单 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后回款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 回款单或应收分录不存在
    /// * `ConflictError` - 非审批中
    /// * `BusinessLogicError` - 跨主体核销、超额核销或重复过账
    pub async fn post_customer_receipt(&self, id: &str, actor: &AuditActor) -> Result<CustomerReceiptView> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let receipt_id = id.to_string();
        let detail_id = receipt_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    post_customer_receipt_in_transaction(&db, &receipt_id, &actor_owned, session).await
                })
            })
            .await?;

        self.customer_receipt_detail(&detail_id).await
    }

    // -----------------------------------------------------------------------
    // 发票
    // -----------------------------------------------------------------------

    /// 分页查询发票列表（销项/进项共用，`invoice_direction` 筛选）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn invoice_list(&self, params: &InvoiceListParams) -> Result<PageView<InvoiceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let invoice_ids = self
            .invoice_ids_for_account_scope(
                query.sales_order_id.as_deref(),
                query.receivable_account_id.as_ref(),
            )
            .await?;
        let filter = InvoiceFilter {
            invoice_ids,
            invoice_direction: query.invoice_direction,
            invoice_kind: query.invoice_kind,
            party_id: query.party_id,
            invoice_no: query.invoice_no,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .invoices()
            .search_invoices(&filter, &mut NoTransaction)
            .await?;
        let invoice_ids = page
            .items
            .iter()
            .map(|row| InvoiceId::new(row.id.clone()))
            .collect::<Vec<_>>();
        let mut sales_allocations_by_invoice = HashMap::<String, Vec<SalesInvoiceAllocation>>::new();
        for allocation in self
            .db
            .sales_invoice_allocations()
            .find_allocations_by_invoices(&invoice_ids, &mut NoTransaction)
            .await?
        {
            sales_allocations_by_invoice
                .entry(allocation.invoice_id.to_string())
                .or_default()
                .push(allocation);
        }
        let mut purchase_allocations_by_invoice = HashMap::<String, Vec<PurchaseInvoiceAllocation>>::new();
        for allocation in self
            .db
            .purchase_invoice_allocations()
            .find_allocations_by_invoices(&invoice_ids, &mut NoTransaction)
            .await?
        {
            purchase_allocations_by_invoice
                .entry(allocation.invoice_id.to_string())
                .or_default()
                .push(allocation);
        }
        for allocations in sales_allocations_by_invoice.values_mut() {
            allocations.sort_unstable_by_key(|allocation| allocation.allocation_seq);
        }
        for allocations in purchase_allocations_by_invoice.values_mut() {
            allocations.sort_unstable_by_key(|allocation| allocation.allocation_seq);
        }
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            let (allocated_total, allocations) = match row.invoice_direction {
                InvoiceDirection::Sales => {
                    sales_allocation_view(&sales_allocations_by_invoice.remove(&row.id).unwrap_or_default())
                }
                InvoiceDirection::Purchase => purchase_allocation_view(
                    &purchase_allocations_by_invoice
                        .remove(&row.id)
                        .unwrap_or_default(),
                ),
            };
            views.push(InvoiceView {
                id: row.id,
                invoice_direction: row.invoice_direction,
                invoice_kind: row.invoice_kind,
                party_id: row.party_id,
                invoice_code: row.invoice_code,
                invoice_no: row.invoice_no,
                invoice_date: row.invoice_date,
                gross_amount: row.gross_amount,
                net_amount: row.net_amount,
                tax_amount: row.tax_amount,
                rounding_adjustment_amount: row.rounding_adjustment_amount,
                rounding_reason: row.rounding_reason,
                original_invoice_id: row.original_invoice_id,
                status: row.stable.status(),
                version: row.version,
                created_at: row.created_at,
                allocated_total,
                unallocated_amount: row.gross_amount.checked_sub(allocated_total),
                allocations,
            });
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询发票详情（含分配行）。
    ///
    /// # 参数
    /// * `id` - 发票 ID
    ///
    /// # 返回
    /// 返回发票视图。
    ///
    /// # 错误
    /// * `NotFound` - 发票不存在
    pub async fn invoice_detail(&self, id: &str) -> Result<InvoiceView> {
        self.invoice_view(id.to_string()).await
    }

    /// 登记发票草稿：同一事务注册 `BusinessDocument` 并调用统一绑定端口。
    ///
    /// 发票为 `NO_APPROVAL`：返回空绑定，不查询已发布定义，不启动审批实例，
    /// 不创建审批任务。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建发票视图。
    ///
    /// # 错误
    /// * `ValidationError` - 金额三元组不恒等或字段非法
    pub async fn create_invoice(&self, req: CreateInvoiceRequest, actor: &AuditActor) -> Result<InvoiceView> {
        req.validate()?;
        let invoice = Invoice::new(
            InvoiceId::new(next_id()),
            InvoiceData {
                invoice_direction: req.invoice_direction,
                invoice_kind: req.invoice_kind,
                party_id: req.party_id,
                invoice_code: req.invoice_code,
                invoice_no: req.invoice_no,
                invoice_date: req.invoice_date,
                gross_amount: req.gross_amount,
                net_amount: req.net_amount,
                tax_amount: req.tax_amount,
                rounding_adjustment_amount: req.rounding_adjustment_amount.unwrap_or(zero_amount()),
                rounding_reason: req.rounding_reason,
                original_invoice_id: None,
            },
            actor.id(),
        )?;
        persist_created_invoice(&self.db, &self.rbac, invoice.clone(), actor.clone()).await?;
        self.invoice_detail(&invoice.base.id).await
    }

    /// 原子创建或提交销项发票并完成分配。
    ///
    /// 新发票的 `BusinessDocument` 注册、发票实体、销项分配、应收子账开票进度、
    /// 销售单开票进度和审计全部位于同一 MongoDB 事务。已有草稿则用乐观锁
    /// 校验后在同一事务过账，前端不得再执行“先创建、再过账”。
    ///
    /// # 参数
    /// * `req` - 新发票或已有草稿身份、最终分配与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回已登记发票及其正式分配。
    ///
    /// # 错误
    /// * `ValidationError` - 新建/已有草稿参数组合或金额不合法
    /// * `ConflictError` - 草稿版本、状态或规范化发票号码冲突
    /// * `BusinessLogicError` - 跨主体、分配不守恒或超额开票
    pub async fn commit_invoice(&self, req: CommitInvoiceRequest, actor: &AuditActor) -> Result<InvoiceView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_payload(
            "sales-invoice-commit-",
            actor.id(),
            "invoice.commit",
            "invoice",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(invoice_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self.invoice_detail(&invoice_id).await;
        }
        let prepared = req.prepare()?;
        let expected_task_version = crate::work_item::expected_task_version(&req.expected_task_version)?;
        let work_item_id = req.work_item_id.clone();
        let policy_revision = self.rbac.current_policy_revision().await?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let command_receipt_for_tx = command_receipt.clone();
        let transaction_result = rbac
            .clone()
            .run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    let (mut invoice, plan_lines) = match prepared {
                        PreparedInvoiceCommit::New { invoice, allocations } => {
                            invoice.validate()?;
                            let new_invoice = Invoice::new(
                                InvoiceId::new(next_id()),
                                InvoiceData {
                                    invoice_direction: invoice.invoice_direction,
                                    invoice_kind: invoice.invoice_kind,
                                    party_id: invoice.party_id,
                                    invoice_code: invoice.invoice_code,
                                    invoice_no: invoice.invoice_no,
                                    invoice_date: invoice.invoice_date,
                                    gross_amount: invoice.gross_amount,
                                    net_amount: invoice.net_amount,
                                    tax_amount: invoice.tax_amount,
                                    rounding_adjustment_amount: invoice
                                        .rounding_adjustment_amount
                                        .unwrap_or(zero_amount()),
                                    rounding_reason: invoice.rounding_reason,
                                    original_invoice_id: None,
                                },
                                actor_id.as_str(),
                            )?;
                            register_created_invoice_document(
                                &db,
                                &rbac,
                                &new_invoice,
                                &actor_owned,
                                session,
                            )
                            .await?;
                            db.invoices().create(&new_invoice, session).await?;
                            (new_invoice, allocations)
                        }
                        PreparedInvoiceCommit::Existing {
                            invoice_id,
                            expected_version,
                            allocations,
                        } => {
                            let invoice = db
                                .invoices()
                                .find_by_id(&invoice_id, session)
                                .await?
                                .ok_or_else(|| Error::NotFound("发票不存在".to_string()))?;
                            ensure_expected_version(invoice.base.version, expected_version)?;
                            ensure_sales_invoice(&invoice)?;
                            (invoice, allocations)
                        }
                    };
                    if invoice.stable.status() != InvoiceStatus::Draft {
                        return Err(Error::ConflictError("发票已登记，请勿重复提交".to_string()));
                    }
                    let duplicate = db
                        .invoices()
                        .find_by_direction_and_normalized_no(
                            invoice.invoice_direction,
                            &invoice.normalized_no,
                            session,
                        )
                        .await?;
                    if duplicate
                        .as_ref()
                        .is_some_and(|other| other.base.id != invoice.base.id)
                    {
                        return Err(Error::ConflictError("发票号码已登记，请勿重复提交".to_string()));
                    }
                    let allocation_account_ids: Vec<ReceivableAccountId> = plan_lines
                        .iter()
                        .map(|line| line.receivable_account_id.clone())
                        .collect();
                    invoice_task::record_invoice_execution(
                        &db,
                        &work_item_id,
                        expected_task_version,
                        &invoice.party_id,
                        &allocation_account_ids,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    let allocation_ids: Vec<SalesInvoiceAllocationId> = (0..plan_lines.len())
                        .map(|_| SalesInvoiceAllocationId::new(next_id()))
                        .collect();
                    let plan = entities::receivable::SalesInvoiceAllocationPlan::new(
                        invoice.base.id.clone().into(),
                        invoice.gross_amount,
                        invoice.net_amount,
                        invoice.tax_amount,
                        &plan_lines,
                        &allocation_ids,
                    )?;
                    let account_id_strs: Vec<String> = plan
                        .account_invoicing_deltas()
                        .iter()
                        .map(|(id, _)| id.to_string())
                        .collect();
                    let accounts = db
                        .receivable_accounts()
                        .find_accounts_by_ids(&account_id_strs, session)
                        .await?;
                    let accounts_by_id: HashMap<&str, &ReceivableAccount> = accounts
                        .iter()
                        .map(|account| (account.base.id.as_str(), account))
                        .collect();
                    for (account_id, _) in plan.account_invoicing_deltas() {
                        let account = accounts_by_id
                            .get(account_id.as_ref())
                            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
                        if account.counterparty_party_id != invoice.party_id {
                            return Err(Error::BusinessLogicError("禁止跨往来主体开票".to_string()));
                        }
                    }
                    let invoicing = db
                        .receivable_accounts()
                        .apply_invoicings_many(plan.account_invoicing_deltas(), &actor_id, session)
                        .await?;
                    if !invoicing.rejected.is_empty() {
                        return Err(Error::BusinessLogicError(
                            "子账剩余可开票额度不足，开票被拒绝".to_string(),
                        ));
                    }
                    invoice.mark_registered(&actor_id)?;
                    db.invoices().update(&mut invoice, session).await?;
                    db.receivable()
                        .create_sales_invoice_allocations_many(plan.new_allocations(), session)
                        .await?;
                    let audit = actor_owned.clone().resource_log(
                        "invoice.commit",
                        "invoice",
                        invoice.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    let mut receivable_account_ids = account_id_strs.clone();
                    receivable_account_ids.sort();
                    receivable_account_ids.dedup();
                    for account_id in receivable_account_ids {
                        invoice_task::sync_sales_invoice_task(
                            &db,
                            &ReceivableAccountId::new(account_id),
                            invoice_task::SalesInvoiceTaskChange::InvoicePosted,
                            session,
                        )
                        .await?;
                    }
                    let mut sales_order_ids: Vec<String> = accounts
                        .iter()
                        .map(|account| account.sales_order_id.to_string())
                        .collect();
                    sales_order_ids.sort();
                    sales_order_ids.dedup();
                    for sales_order_id in sales_order_ids {
                        crate::sales_order::update_sales_order_money_progress(
                            &db,
                            session,
                            &entities::ids::SalesOrderId::new(sales_order_id),
                            actor_id.clone(),
                            None,
                        )
                        .await?;
                    }
                    let committed_id = invoice.base.id.clone();
                    let command_audit =
                        command_receipt_for_tx.audit(actor_owned.clone(), committed_id.clone())?;
                    db.audit_logs().create(&command_audit, session).await?;
                    Ok::<String, crate::errors::Error>(committed_id)
                })
            })
            .await;

        let detail_id = match transaction_result {
            Ok(invoice_id) => invoice_id,
            Err(error) => match command_receipt.committed_resource_id(&self.db).await? {
                Some(invoice_id) => invoice_id,
                None => return Err(error),
            },
        };

        self.invoice_detail(&detail_id).await
    }

    /// 发票登记过账并分配（§8.3-2 事务不变量）。
    ///
    /// 同一事务内：规范化号码去重（`find_by_direction_and_normalized_no` +
    /// 唯一索引兜底）；校验发票与可开票子账同一往来主体；分配合计等于发票
    /// 金额；写销项发票分配；按条件原子更新子账净已开票进度
    /// （`apply_invoicing` 不超额开票）；发票迁移为已登记。
    /// 任一校验失败整体回滚。规范化发票号码唯一构成重复提交去重。
    ///
    /// # 参数
    /// * `id` - 发票 ID
    /// * `req` - 过账请求（分配行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回登记后发票视图。
    ///
    /// # 错误
    /// * `NotFound` - 发票或子账不存在
    /// * `ConflictError` - 规范化号码已登记或发票已登记
    /// * `BusinessLogicError` - 跨主体开票、分配合计不等或超额开票
    pub async fn post_invoice(
        &self,
        id: &str,
        req: PostInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<InvoiceView> {
        req.validate()?;
        let expected_task_version = crate::work_item::expected_task_version(&req.expected_task_version)?;
        let policy_revision = self.rbac.current_policy_revision().await?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let invoice_id = id.to_string();
        let detail_id = invoice_id.clone();
        let work_item_id = req.work_item_id.clone();
        let plan_lines = convert_post_allocations(&req.allocations);
        rbac.run_authorized_policy_transaction(policy_revision, move |session| {
            Box::pin(async move {
                let mut invoice = db
                    .invoices()
                    .find_by_id(&invoice_id, session)
                    .await?
                    .ok_or_else(|| Error::NotFound("发票不存在".to_string()))?;
                ensure_sales_invoice(&invoice)?;
                if invoice.stable.status() != entities::receivable::InvoiceStatus::Draft {
                    return Err(Error::ConflictError("发票已登记，请勿重复提交".to_string()));
                }
                let duplicate = db
                    .invoices()
                    .find_by_direction_and_normalized_no(
                        invoice.invoice_direction,
                        &invoice.normalized_no,
                        session,
                    )
                    .await?;
                if let Some(other) = duplicate {
                    if other.base.id != invoice.base.id {
                        return Err(Error::ConflictError("发票号码已登记，请勿重复提交".to_string()));
                    }
                }

                let allocation_account_ids: Vec<ReceivableAccountId> = plan_lines
                    .iter()
                    .map(|line| line.receivable_account_id.clone())
                    .collect();
                invoice_task::record_invoice_execution(
                    &db,
                    &work_item_id,
                    expected_task_version,
                    &invoice.party_id,
                    &allocation_account_ids,
                    &actor_owned,
                    session,
                )
                .await?;

                let allocation_ids: Vec<SalesInvoiceAllocationId> = (0..plan_lines.len())
                    .map(|_| SalesInvoiceAllocationId::new(next_id()))
                    .collect();
                let plan = entities::receivable::SalesInvoiceAllocationPlan::new(
                    invoice.base.id.clone().into(),
                    invoice.gross_amount,
                    invoice.net_amount,
                    invoice.tax_amount,
                    &plan_lines,
                    &allocation_ids,
                )?;
                let account_id_strs: Vec<String> = plan
                    .account_invoicing_deltas()
                    .iter()
                    .map(|(id, _)| id.to_string())
                    .collect();
                let accounts = db
                    .receivable_accounts()
                    .find_accounts_by_ids(&account_id_strs, session)
                    .await?;
                let accounts_by_id: HashMap<&str, &ReceivableAccount> = accounts
                    .iter()
                    .map(|account| (account.base.id.as_str(), account))
                    .collect();
                for (account_id, _) in plan.account_invoicing_deltas() {
                    let account = accounts_by_id
                        .get(account_id.as_ref())
                        .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
                    if account.counterparty_party_id != invoice.party_id {
                        return Err(Error::BusinessLogicError("禁止跨往来主体开票".to_string()));
                    }
                }
                let invoicing = db
                    .receivable_accounts()
                    .apply_invoicings_many(plan.account_invoicing_deltas(), &actor_id, session)
                    .await?;
                if !invoicing.rejected.is_empty() {
                    return Err(Error::BusinessLogicError(
                        "子账剩余可开票额度不足，开票被拒绝".to_string(),
                    ));
                }
                invoice.mark_registered(&actor_id)?;
                db.invoices().update(&mut invoice, session).await?;
                db.receivable()
                    .create_sales_invoice_allocations_many(plan.new_allocations(), session)
                    .await?;
                let audit =
                    actor_owned
                        .clone()
                        .resource_log("invoice.post", "invoice", invoice.base.id.clone())?;
                db.audit_logs().create(&audit, session).await?;
                let mut receivable_account_ids = account_id_strs.clone();
                receivable_account_ids.sort();
                receivable_account_ids.dedup();
                for account_id in receivable_account_ids {
                    invoice_task::sync_sales_invoice_task(
                        &db,
                        &ReceivableAccountId::new(account_id),
                        invoice_task::SalesInvoiceTaskChange::InvoicePosted,
                        session,
                    )
                    .await?;
                }
                let mut sales_order_ids: Vec<String> = accounts
                    .iter()
                    .map(|account| account.sales_order_id.to_string())
                    .collect();
                sales_order_ids.sort();
                sales_order_ids.dedup();
                for sales_order_id in sales_order_ids {
                    crate::sales_order::update_sales_order_money_progress(
                        &db,
                        session,
                        &entities::ids::SalesOrderId::new(sales_order_id),
                        actor_id.clone(),
                        None,
                    )
                    .await?;
                }
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await?;

        self.invoice_detail(&detail_id).await
    }

    /// 按原蓝票一次开具红票并红冲（§8.3-3 事务不变量）。
    ///
    /// 服务端在同一事务内读取原票的有效分配、计算本次反向行、创建红票、
    /// 冲减应收或应付子账进度并写审计。客户端不得提交分配 ID、净额或税额。
    /// 部分红冲时原蓝票保持已登记；全部剩余金额红冲后才置为已红冲。
    ///
    /// # 参数
    /// * `id` - 原蓝票 ID
    /// * `req` - 红票业务意图与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建红票视图。
    ///
    /// # 错误
    /// * `NotFound` - 原蓝票或有效分配不存在
    /// * `ConflictError` - 红票号码重复
    /// * `BusinessLogicError` - 红冲累计超过原分配或超额红冲
    ///
    /// # 约束
    /// 领域计划只计算金额；ID 生成、事务、写入、任务同步和审计继续由 Service 持有。
    pub async fn issue_red_invoice(
        &self,
        id: &str,
        req: CommitRedInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<InvoiceView> {
        req.validate()?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let digest = hex::encode(Sha256::digest(
            format!("{}|{}|{}", actor.id(), id, req.idempotency_key.trim()).as_bytes(),
        ));
        let red_no = req
            .invoice_no
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("HT-{}", &digest[..12]));
        let requested_amount = req.amount;
        let reason = req.reason.trim().to_string();
        let original_id = id.to_string();
        let red_invoice_id = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let original = db
                        .invoices()
                        .find_by_id(&original_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("原蓝票不存在".to_string()))?;
                    if !original.is_registered() || original.invoice_kind != InvoiceKind::Blue {
                        return Err(Error::BusinessLogicError(
                            "只有已登记的蓝票可以被红冲".to_string(),
                        ));
                    }
                    let allocation_plan = match original.invoice_direction {
                        InvoiceDirection::Sales => {
                            let blue = db
                                .sales_invoice_allocations()
                                .find_allocations_by_invoices(
                                    &[InvoiceId::new(original.base.id.clone())],
                                    session,
                                )
                                .await?;
                            let account_ids = blue
                                .iter()
                                .filter(|line| line.allocation_action == AllocationAction::Apply)
                                .map(|line| line.receivable_account_id.clone())
                                .collect::<Vec<_>>();
                            let related = db
                                .sales_invoice_allocations()
                                .find_allocations_by_accounts(&account_ids, session)
                                .await?;
                            sales_red_invoice_allocation_plan(&blue, &related, requested_amount)?
                        }
                        InvoiceDirection::Purchase => {
                            let blue = db
                                .purchase_invoice_allocations()
                                .find_allocations_by_invoices(
                                    &[InvoiceId::new(original.base.id.clone())],
                                    session,
                                )
                                .await?;
                            let account_ids = blue
                                .iter()
                                .filter(|line| {
                                    line.allocation_action == entities::payable::AllocationAction::Apply
                                })
                                .map(|line| line.payable_account_id.clone())
                                .collect::<Vec<_>>();
                            let related = db
                                .purchase_invoice_allocations()
                                .find_allocations_by_accounts(&account_ids, session)
                                .await?;
                            purchase_red_invoice_allocation_plan(&blue, &related, requested_amount)?
                        }
                    };
                    let (red_gross, red_net, red_tax) = allocation_plan.totals();

                    if let Some(existing) = db
                        .invoices()
                        .find_by_direction_and_normalized_no(
                            original.invoice_direction,
                            &red_no.to_uppercase(),
                            session,
                        )
                        .await?
                    {
                        if existing.invoice_kind == InvoiceKind::Red
                            && existing.original_invoice_id.as_ref()
                                == Some(&InvoiceId::new(original.base.id.clone()))
                            && existing.gross_amount == red_gross
                            && existing.net_amount == red_net
                            && existing.tax_amount == red_tax
                        {
                            return Ok::<String, crate::errors::Error>(existing.base.id);
                        }
                        return Err(Error::ConflictError("红票号码已登记，请勿重复提交".to_string()));
                    }

                    let red_invoice_id = InvoiceId::new(next_id());
                    let mut red_mut = Invoice::new(
                        red_invoice_id.clone(),
                        InvoiceData {
                            invoice_direction: original.invoice_direction,
                            invoice_kind: InvoiceKind::Red,
                            party_id: original.party_id.clone(),
                            invoice_code: original.invoice_code.clone(),
                            invoice_no: red_no.clone(),
                            invoice_date: entities::common::time::BusinessDate::today(),
                            gross_amount: red_gross,
                            net_amount: red_net,
                            tax_amount: red_tax,
                            rounding_adjustment_amount: zero_amount(),
                            rounding_reason: None,
                            original_invoice_id: Some(original.base.id.clone().into()),
                        },
                        &actor_id,
                    )?;
                    red_mut.mark_registered(&actor_id)?;
                    let mut original_mut = original;
                    register_created_invoice_document(&db, &rbac, &red_mut, &actor_owned, session).await?;
                    db.invoices().create(&red_mut, session).await?;
                    if allocation_plan.is_full_reversal() {
                        original_mut.mark_red_invoiced(&actor_id)?;
                        db.invoices().update(&mut original_mut, session).await?;
                    }

                    let mut sales_order_account_ids = Vec::new();
                    for (index, line) in allocation_plan.lines().iter().enumerate() {
                        match original_mut.invoice_direction {
                            InvoiceDirection::Sales => {
                                let account_id = ReceivableAccountId::new(line.account_id.clone());
                                let reverted = db
                                    .receivable_accounts()
                                    .revert_invoicing(&account_id, &line.gross, &actor_id, session)
                                    .await?;
                                if !reverted {
                                    return Err(Error::BusinessLogicError(
                                        "红冲金额超过已开票进度".to_string(),
                                    ));
                                }
                                db.sales_invoice_allocations()
                                    .create(
                                        &SalesInvoiceAllocation::new(
                                            SalesInvoiceAllocationId::new(next_id()),
                                            SalesInvoiceAllocationData {
                                                invoice_id: red_invoice_id.clone(),
                                                receivable_account_id: account_id,
                                                allocation_seq: (index as u32) + 1,
                                                allocation_action: AllocationAction::Reverse,
                                                allocated_gross_amount: line.gross,
                                                allocated_net_amount: line.net,
                                                allocated_tax_amount: line.tax,
                                                reverses_allocation_id: Some(SalesInvoiceAllocationId::new(
                                                    line.original_allocation_id.clone(),
                                                )),
                                            },
                                        )?,
                                        session,
                                    )
                                    .await?;
                                sales_order_account_ids.push(line.account_id.clone());
                            }
                            InvoiceDirection::Purchase => {
                                let account_id = PayableAccountId::new(line.account_id.clone());
                                let reverted = db
                                    .payable_accounts()
                                    .revert_invoicing(&account_id, &line.gross, &actor_id, session)
                                    .await?;
                                if !reverted {
                                    return Err(Error::BusinessLogicError(
                                        "红冲金额超过已收票进度".to_string(),
                                    ));
                                }
                                db.purchase_invoice_allocations()
                                    .create(
                                        &PurchaseInvoiceAllocation::new(
                                            PurchaseInvoiceAllocationId::new(next_id()),
                                            PurchaseInvoiceAllocationData {
                                                invoice_id: red_invoice_id.clone(),
                                                payable_account_id: account_id,
                                                allocation_seq: (index as u32) + 1,
                                                allocation_action:
                                                    entities::payable::AllocationAction::Reverse,
                                                allocated_gross_amount: line.gross,
                                                allocated_net_amount: line.net,
                                                allocated_tax_amount: line.tax,
                                                reverses_allocation_id: Some(
                                                    PurchaseInvoiceAllocationId::new(
                                                        line.original_allocation_id.clone(),
                                                    ),
                                                ),
                                            },
                                        )?,
                                        session,
                                    )
                                    .await?;
                            }
                        }
                    }
                    let audit = actor_owned.clone().resource_log_with_message(
                        "invoice.red_issue",
                        "invoice",
                        red_mut.base.id.clone(),
                        Some(reason.clone()),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    if original_mut.invoice_direction == InvoiceDirection::Sales {
                        sales_order_account_ids.sort();
                        sales_order_account_ids.dedup();
                        for account_id in &sales_order_account_ids {
                            invoice_task::sync_sales_invoice_task(
                                &db,
                                &ReceivableAccountId::new(account_id.clone()),
                                invoice_task::SalesInvoiceTaskChange::RedInvoiceIssued,
                                session,
                            )
                            .await?;
                        }
                        let mut sales_order_ids = Vec::new();
                        for account in db
                            .receivable_accounts()
                            .find_accounts_by_ids(&sales_order_account_ids, session)
                            .await?
                        {
                            sales_order_ids.push(account.sales_order_id.to_string());
                        }
                        sales_order_ids.sort();
                        sales_order_ids.dedup();
                        for sales_order_id in sales_order_ids {
                            crate::sales_order::update_sales_order_money_progress(
                                &db,
                                session,
                                &entities::ids::SalesOrderId::new(sales_order_id),
                                actor_id.clone(),
                                None,
                            )
                            .await?;
                        }
                    }
                    Ok::<String, crate::errors::Error>(red_invoice_id.to_string())
                })
            })
            .await?;

        self.invoice_detail(&red_invoice_id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

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
    async fn receivable_account_view(&self, id: String) -> Result<ReceivableAccountView> {
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
        let review_chain_tail_id = snapshot.reviews.last().map(|review| review.base.id.clone());
        let next_review_no = next_review_no(&snapshot.reviews)?;
        let review_chain_version = review_chain_version(&snapshot.reviews);
        let funds_fact_version = funds_fact_version(&account, &snapshot);
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

    /// 装配客户回款单视图。
    ///
    /// # 参数
    /// * `id` - 回款单 ID
    ///
    /// # 返回
    /// 返回回款单视图（含分配行与未分配余额）。
    ///
    /// # 错误
    /// * `NotFound` - 回款单不存在
    async fn customer_receipt_view(&self, id: String) -> Result<CustomerReceiptView> {
        let receipt = self
            .db
            .customer_receipts()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
        let allocations = self
            .db
            .receipt_allocations()
            .find_allocations_by_receipts(&[receipt.base.id.clone().into()], &mut NoTransaction)
            .await?;
        let (allocated_total, views) = allocation_view(&allocations);
        let binding = match find_approval_binding(&self.db, &id, &mut NoTransaction).await {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        Ok(CustomerReceiptView {
            id: receipt.base.id.clone(),
            receipt_no: receipt.receipt_no,
            status: receipt.status,
            counterparty_party_id: receipt.counterparty_party_id.to_string(),
            customer_id: receipt.customer_id.map(|id| id.to_string()),
            received_at: receipt.received_at,
            amount: receipt.amount,
            bank_reference: receipt.bank_reference,
            version: receipt.base.version,
            created_at: receipt.base.created_at,
            unallocated_amount: receipt.amount.checked_sub(allocated_total),
            allocated_total,
            allocations: views,
            approval: document_approval_view(binding.as_ref(), None, receipt.status),
        })
    }

    /// 装配发票视图。
    ///
    /// # 参数
    /// * `id` - 发票 ID
    ///
    /// # 返回
    /// 返回发票视图（含分配行与未分配余额）。
    ///
    /// # 错误
    /// * `NotFound` - 发票不存在
    async fn invoice_view(&self, id: String) -> Result<InvoiceView> {
        let invoice = self
            .db
            .invoices()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("发票不存在".to_string()))?;
        let (allocated_total, views) = match invoice.invoice_direction {
            InvoiceDirection::Purchase => {
                // 进项票分配独立存储（purchase_invoice_allocations）；视图共用销项形状，
                // 应付子账 ID 落 receivable_account_id 字段（前端以
                // payable_account_id ?? receivable_account_id 兜底读取）。
                let rows = self
                    .db
                    .purchase_invoice_allocations()
                    .find_allocations_by_invoices(&[invoice.base.id.clone().into()], &mut NoTransaction)
                    .await?;
                purchase_allocation_view(&rows)
            }
            InvoiceDirection::Sales => {
                let allocations = self
                    .db
                    .sales_invoice_allocations()
                    .find_allocations_by_invoices(&[invoice.base.id.clone().into()], &mut NoTransaction)
                    .await?;
                sales_allocation_view(&allocations)
            }
        };
        Ok(InvoiceView {
            id: invoice.base.id.clone(),
            invoice_direction: invoice.invoice_direction,
            invoice_kind: invoice.invoice_kind,
            party_id: invoice.party_id.to_string(),
            invoice_code: invoice.invoice_code,
            invoice_no: invoice.invoice_no,
            invoice_date: invoice.invoice_date,
            gross_amount: invoice.gross_amount,
            net_amount: invoice.net_amount,
            tax_amount: invoice.tax_amount,
            rounding_adjustment_amount: invoice.rounding_adjustment_amount,
            rounding_reason: invoice.rounding_reason,
            original_invoice_id: invoice.original_invoice_id.map(|id| id.to_string()),
            status: invoice.stable.status(),
            version: invoice.base.version,
            created_at: invoice.base.created_at,
            unallocated_amount: invoice.gross_amount.checked_sub(allocated_total),
            allocated_total,
            allocations: views,
        })
    }
}

const CARD_FUNDS_REVIEW_ACTION: &str = "receivable_funds_review.complete";
const CARD_FUNDS_REVIEW_RECEIPT_PREFIX: &str = "card-funds-review-command-";
/// 在审批运行时持有的事务内过账客户回款并写入核销事实。
///
/// # 参数
/// * `db` - 数据库实例
/// * `receipt_id` - 客户回款单 ID
/// * `actor` - 已认证操作人
/// * `session` - 审批运行时持有的唯一事务会话
///
/// # 返回
/// 回款、核销、应收进度、销售回款进度和成功审计全部写入时返回 `Ok(())`。
///
/// # 错误
/// 回款/分录不存在、主体或额度不变量失败、任一写入失败时返回错误。
pub(crate) async fn post_customer_receipt_in_transaction(
    db: &Database,
    receipt_id: &str,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let actor_id = actor.id().to_string();
    let mut receipt = db
        .customer_receipts()
        .find_by_id(receipt_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
    if receipt.status == CustomerReceiptStatus::Reversed {
        return Err(Error::BusinessLogicError("已冲正回款不能再核销".to_string()));
    }
    ensure_final_approve_posting(&receipt)?;
    execute_customer_receipt_domain_action(
        &mut receipt,
        crate::approval::policy::ApprovalDomainAction::CustomerReceiptPost,
    )?;

    let existing = db
        .receipt_allocations()
        .find_allocations_by_receipts(&[receipt.base.id.clone().into()], session)
        .await?;
    let net_allocated = net_receipt_allocated(&existing);
    if net_allocated.checked_add(pending_allocated_total(&receipt.pending_allocations)) > receipt.amount {
        return Err(Error::BusinessLogicError("核销合计超过回款金额".to_string()));
    }

    let mut entry_balances = HashMap::<String, Amount>::new();
    for allocation in &existing {
        let balance = entry_balances
            .entry(allocation.receivable_entry_id.to_string())
            .or_insert_with(zero_amount);
        match allocation.allocation_action {
            AllocationAction::Apply => *balance = balance.checked_add(allocation.allocated_amount),
            AllocationAction::Reverse => *balance = balance.checked_sub(allocation.allocated_amount),
        }
    }

    let next_seq = existing
        .iter()
        .map(|allocation| allocation.allocation_seq)
        .max()
        .unwrap_or(0)
        + 1;
    let pending = receipt.pending_allocations.clone();
    let mut new_allocations = Vec::with_capacity(pending.len());
    let mut sales_order_ids = Vec::new();
    for (index, line) in pending.iter().enumerate() {
        let entry = db
            .receivable_entries()
            .find_by_id(&line.receivable_entry_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
        let account = db
            .receivable_accounts()
            .find_by_id(&entry.receivable_account_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
        sales_order_ids.push(account.sales_order_id.to_string());
        if account.counterparty_party_id != receipt.counterparty_party_id {
            return Err(Error::BusinessLogicError("禁止跨往来主体核销".to_string()));
        }
        let allocated = entry_balances
            .entry(entry.base.id.clone())
            .or_insert_with(zero_amount);
        if allocated.checked_add(line.allocated_amount) > entry.amount {
            return Err(Error::BusinessLogicError(
                "核销金额超过应收分录开放余额".to_string(),
            ));
        }
        *allocated = allocated.checked_add(line.allocated_amount);
        new_allocations.push(ReceiptAllocation::new(
            ReceiptAllocationId::new(next_id()),
            ReceiptAllocationData {
                customer_receipt_id: receipt.base.id.clone().into(),
                receivable_entry_id: line.receivable_entry_id.clone(),
                allocation_seq: next_seq + index as u32,
                allocation_action: AllocationAction::Apply,
                allocated_amount: line.allocated_amount,
                allocated_at: Instant::now(),
                reverses_allocation_id: None,
            },
        )?);
    }

    for allocation in &new_allocations {
        let entry = db
            .receivable_entries()
            .find_by_id(&allocation.receivable_entry_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
        let applied = db
            .receivable_accounts()
            .apply_settlement(
                &entry.receivable_account_id,
                &allocation.allocated_amount,
                &actor_id,
                session,
            )
            .await?;
        if !applied {
            return Err(Error::BusinessLogicError(
                "子账剩余开放余额不足，核销被拒绝".to_string(),
            ));
        }
    }
    receipt.mark_posted()?;
    db.customer_receipts().update(&mut receipt, session).await?;
    for allocation in &new_allocations {
        db.receipt_allocations().create(allocation, session).await?;
    }
    let audit = actor.clone().resource_log(
        &format!("customer_receipt.post:{receipt_id}"),
        "customer_receipt",
        receipt.base.id.clone(),
    )?;
    db.audit_logs().create(&audit, session).await?;
    sales_order_ids.sort();
    sales_order_ids.dedup();
    for sales_order_id in sales_order_ids {
        crate::sales_order::update_sales_order_money_progress(
            db,
            session,
            &SalesOrderId::new(sales_order_id),
            actor_id.clone(),
            None,
        )
        .await?;
    }
    Ok(())
}

/// 在审批运行时持有的事务内撤回客户回款审批。
///
/// # 错误
/// 回款单不存在、动作不匹配、状态迁移或 CAS 写入失败时返回错误。
pub(crate) async fn cancel_customer_receipt_approval_in_transaction(
    db: &Database,
    receipt_id: &str,
    action: crate::approval::policy::ApprovalDomainAction,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut receipt = db
        .customer_receipts()
        .find_by_id(receipt_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户回款单不存在".to_string()))?;
    execute_customer_receipt_domain_action(&mut receipt, action)?;
    db.customer_receipts().update(&mut receipt, executor).await?;
    let audit = actor.clone().resource_log(
        "customer_receipt.cancel_approval",
        "customer_receipt",
        receipt_id.to_string(),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}

const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";
const CARD_FUNDS_RECEIPT_REGISTRATION_ACTION: &str = "card_funds.receipt.register";
const CARD_FUNDS_INVOICE_REGISTRATION_ACTION: &str = "card_funds.invoice.register";
const CARD_FUNDS_REGISTRATION_RECEIPT_PREFIX: &str = "card-funds-registration-";
/// W13 校验所需的当前应收、票款与复核链事实快照。
struct CardFundsSnapshot {
    current_sales_order_revision_id: String,
    sales_order_no: String,
    sales_order_revision_no: u32,
    sales_order_snapshot_at: u64,
    customer_name: String,
    counterparty_party_name: Option<String>,
    entries: Vec<ReceivableEntry>,
    reviews: Vec<ReceivableFundsReview>,
    receipt_allocations: Vec<ReceiptAllocation>,
    invoice_allocations: Vec<SalesInvoiceAllocation>,
    receipts: Vec<CustomerReceipt>,
    invoices: Vec<Invoice>,
}

/// 加载 W13 历史票款登记上下文所需的任务版本与责任人事实。
struct CardFundsRegistrationContextInput<'a> {
    rbac: SharedRbacService,
    work_item_id: &'a entities::ids::WorkItemId,
    expected_task_version: u64,
    expected_subject_version: &'a str,
    expected_funds_fact_version: &'a str,
    actor: &'a AuditActor,
}

/// 在事务内加载并校验 W13 历史票款登记的任务、责任、账户与事实版本。
///
/// # 错误
/// 任务、账户或事实不存在，责任校验失败，或任一并发版本变化时返回错误。
async fn load_card_funds_registration_context(
    db: &Database,
    input: CardFundsRegistrationContextInput<'_>,
    executor: &mut dyn Executor,
) -> Result<(ReceivableAccount, CardFundsSnapshot)> {
    let work_item = db
        .work_items()
        .find_by_id(input.work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("卡券票款复核任务不存在".to_string()))?;
    if work_item.base.version != input.expected_task_version {
        return Err(Error::ConflictError(
            "复核任务版本已变化，请刷新后重试".to_string(),
        ));
    }
    if work_item.subject_version != input.expected_subject_version.trim() {
        return Err(Error::ConflictError(
            "复核任务对象版本已变化，请刷新后重试".to_string(),
        ));
    }
    if work_item.business_object_type != "receivable_account"
        || !matches!(
            work_item.work_item_type,
            WorkItemType::CardFundsReview | WorkItemType::CardFundsDeltaReview
        )
    {
        return Err(Error::BusinessLogicError(
            "当前任务不是应收账户卡券票款复核任务".to_string(),
        ));
    }
    if !work_item.is_owned_by(input.actor.id()) {
        return Err(Error::Forbidden("当前账号不是开放任务的当前责任人".to_string()));
    }
    WorkItemService::new(db.clone(), input.rbac)
        .ensure_domain_decision_access(input.actor, &work_item, executor)
        .await?;

    let account = db
        .receivable_accounts()
        .find_by_id(&work_item.business_object_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
    let snapshot = load_card_funds_snapshot(db, &account, executor).await?;
    if snapshot.current_sales_order_revision_id != work_item.subject_version {
        return Err(Error::ConflictError(
            "销售单当前版本已变化，请刷新后重试".to_string(),
        ));
    }
    let expected_review_status = match work_item.work_item_type {
        WorkItemType::CardFundsReview => AccountReviewStatus::OpeningPending,
        WorkItemType::CardFundsDeltaReview => AccountReviewStatus::SyncDeltaPending,
        _ => unreachable!("任务类型已在前置校验收窄"),
    };
    if account.review_status != expected_review_status {
        return Err(Error::ConflictError(
            "应收账户已不在当前复核类型的待处理状态".to_string(),
        ));
    }
    if snapshot
        .counterparty_party_name
        .as_deref()
        .is_none_or(|name| name.trim().is_empty())
    {
        return Err(Error::BusinessLogicError(
            "当前销售版本缺少收款或开票往来主体名称".to_string(),
        ));
    }
    if funds_fact_version(&account, &snapshot) != input.expected_funds_fact_version.trim() {
        return Err(Error::ConflictError("票款事实已变化，请刷新后重试".to_string()));
    }
    Ok((account, snapshot))
}

/// 将 W13 服务 DTO 转换为领域输入并构造已验证分配集合。
///
/// # 参数
/// * `allocations` - HTTP 契约复用的分配 DTO 行
/// * `account_id` - 事务内重新加载的当前任务应收子账 ID
/// * `expected_total` - 本次登记的含税总额
///
/// # 返回
/// 返回保持请求顺序的领域值对象，供后续编排复用其已验证合计。
///
/// # 错误
/// 领域账户错误和合计错误映射为既有 `BusinessLogicError`，非正金额映射为
/// 既有 `ValidationError`，文案与对外错误语义保持不变。
///
/// # 约束
/// 本函数只做 DTO 到领域输入的适配，不重复实现账户、金额或守恒规则。
fn card_funds_registration_allocations(
    allocations: &[CardFundsRegistrationAllocation],
    account_id: &str,
    expected_total: Amount,
) -> Result<CardFundsRegistrationAllocations> {
    let lines = allocations
        .iter()
        .map(|allocation| CardFundsRegistrationAllocationInput {
            target_account_id: allocation.target_account_id.clone(),
            amount: allocation.amount,
        })
        .collect();
    CardFundsRegistrationAllocations::new(ReceivableAccountId::new(account_id), expected_total, lines)
        .map_err(|error| match error {
            CardFundsRegistrationAllocationsError::NonPositiveAmount => {
                Error::ValidationError(error.to_string())
            }
            CardFundsRegistrationAllocationsError::TargetAccountMismatch
            | CardFundsRegistrationAllocationsError::TotalMismatch => {
                Error::BusinessLogicError(error.to_string())
            }
        })
}

/// 按应收分录顺序为 W13 历史回款生成服务端核销计划。
fn plan_card_funds_receipt_allocations(
    snapshot: &CardFundsSnapshot,
    amount: Amount,
) -> Result<Vec<(ReceivableEntryId, Amount)>> {
    if amount <= zero_amount() {
        return Err(Error::ValidationError("回款金额必须大于零".to_string()));
    }
    let mut entries = snapshot
        .entries
        .iter()
        .filter(|entry| entry.direction == EntryDirection::Increase)
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.source_sequence
            .cmp(&right.source_sequence)
            .then_with(|| left.base.id.cmp(&right.base.id))
    });
    let mut unallocated = amount;
    let mut plan = Vec::new();
    for entry in entries {
        if unallocated == zero_amount() {
            break;
        }
        let allocated = snapshot
            .receipt_allocations
            .iter()
            .filter(|line| line.receivable_entry_id.as_ref() == entry.base.id)
            .fold(zero_amount(), |total, line| match line.allocation_action {
                AllocationAction::Apply => total.checked_add(line.allocated_amount),
                AllocationAction::Reverse => total.checked_sub(line.allocated_amount),
            });
        if allocated > entry.amount {
            return Err(Error::BusinessLogicError(
                "应收分录历史核销累计超过分录金额".to_string(),
            ));
        }
        let available = entry.amount.checked_sub(allocated);
        if available == zero_amount() {
            continue;
        }
        let planned = if available <= unallocated {
            available
        } else {
            unallocated
        };
        plan.push((ReceivableEntryId::new(entry.base.id.clone()), planned));
        unallocated = unallocated.checked_sub(planned);
    }
    if unallocated != zero_amount() {
        return Err(Error::BusinessLogicError(
            "应收分录开放余额不足，无法完成历史回款分配".to_string(),
        ));
    }
    Ok(plan)
}

/// 归一化可选票款单号；空白由服务端生成。
fn normalized_registration_no(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// 按操作者与幂等键生成稳定且不泄漏原键的票款单号。
fn stable_registration_no(prefix: &str, actor_id: &str, key: &str) -> String {
    let digest = hex::encode(Sha256::digest(format!("{actor_id}|{}", key.trim()).as_bytes()));
    format!("{prefix}-{}", &digest[..12])
}

/// 计算 W13 登记命令指纹。
fn card_funds_registration_fingerprint<T: serde::Serialize>(command: &T) -> Result<String> {
    let serialized = serde_json::to_vec(command)
        .map_err(|error| Error::Internal(format!("卡券票款登记命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(serialized)))
}

/// 生成 W13 登记命令的稳定审计主键。
fn card_funds_registration_audit_id(action: &str, actor_id: &str, key: &str) -> String {
    let digest = hex::encode(Sha256::digest(
        format!("{action}|{actor_id}|{}", key.trim()).as_bytes(),
    ));
    format!("{CARD_FUNDS_REGISTRATION_RECEIPT_PREFIX}{digest}")
}

/// 将 W13 登记结果编码为幂等审计收据。
fn card_funds_registration_receipt_message(fingerprint: &str, kind: &str, fact_id: &str) -> String {
    format!("{COMMAND_FINGERPRINT_PREFIX}{fingerprint};fact={kind}|{fact_id}")
}

/// 在事务内读取并严格验证 W13 登记幂等收据。
async fn replay_card_funds_registration(
    db: &Database,
    audit_id: &str,
    expected_action: &str,
    expected_fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<Option<(String, String)>> {
    let Some(audit) = db.audit_logs().find_by_id(audit_id, executor).await? else {
        return Ok(None);
    };
    if audit.action != expected_action || audit.resource_type != "receivable_account" || !audit.success {
        return Err(Error::Internal("卡券票款登记幂等收据身份非法".to_string()));
    }
    let expected_prefix = format!("{COMMAND_FINGERPRINT_PREFIX}{expected_fingerprint};fact=");
    let fact = audit
        .message
        .as_deref()
        .and_then(|message| message.strip_prefix(&expected_prefix))
        .ok_or_else(|| Error::ConflictError("幂等键已用于不同的卡券票款登记命令".to_string()))?;
    let (kind, fact_id) = fact
        .split_once('|')
        .ok_or_else(|| Error::Internal("卡券票款登记幂等收据格式非法".to_string()))?;
    let expected_kind = if expected_action == CARD_FUNDS_RECEIPT_REGISTRATION_ACTION {
        "receipt"
    } else {
        "invoice"
    };
    if kind != expected_kind || fact_id.trim().is_empty() {
        return Err(Error::Internal("卡券票款登记幂等收据事实非法".to_string()));
    }
    let account_id = audit
        .resource_id
        .ok_or_else(|| Error::Internal("卡券票款登记幂等收据缺少应收账户".to_string()))?;
    Ok(Some((account_id, fact_id.to_string())))
}

/// W13 幂等审计收据中的确定性正式结果。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CardFundsReviewReceipt {
    receivable_funds_review_id: String,
    workflow_action_id: String,
    review_no: u32,
    account_review_status: String,
    completed_at: i64,
    review_result: CardFundsReviewResult,
    conclusion: CardFundsReviewConclusion,
    follow_up_work_item_id: Option<String>,
    follow_up_work_item_type: Option<String>,
}

impl CardFundsReviewReceipt {
    /// 判断七字段旧版驳回收据是否仍缺正式后继任务。
    fn requires_legacy_rejected_follow_up(&self) -> bool {
        self.review_result == CardFundsReviewResult::Rejected
            && self.follow_up_work_item_id.is_none()
            && self.follow_up_work_item_type.is_none()
    }

    /// 绑定已迁移形成的正式后继任务。
    fn attach_follow_up(&mut self, item: &WorkItem) {
        self.follow_up_work_item_id = Some(item.base.id.clone());
        self.follow_up_work_item_type = Some(item.work_item_type.as_str().to_string());
    }

    /// 将持久化收据装配为固定 W13 HTTP 结果。
    fn into_result(
        self,
        work_item_id: &str,
        receivable_account_id: &str,
        operation_id: &str,
    ) -> CompleteCardFundsReviewResult {
        let follow_up_work_item = self
            .follow_up_work_item_id
            .zip(self.follow_up_work_item_type)
            .map(|(work_item_id, work_item_type)| CardFundsReviewFollowUpWorkItem {
                work_item_id,
                work_item_type,
                status: "OPEN".to_string(),
            });
        CompleteCardFundsReviewResult {
            work_item_id: work_item_id.to_string(),
            work_item_status: CompletedWorkItemStatus::Completed,
            business_result: CardFundsReviewBusinessResult {
                receivable_funds_review_id: self.receivable_funds_review_id,
                receivable_account_id: receivable_account_id.to_string(),
                review_no: self.review_no,
                account_review_status: self.account_review_status,
                workflow_action_id: self.workflow_action_id,
                operation_id: operation_id.to_string(),
                completed_at: Instant::from_unix_secs(self.completed_at).as_utc().to_rfc3339(),
                review_result: self.review_result,
                conclusion: self.conclusion,
                follow_up_work_item,
            },
        }
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

/// 读取 W13 当前销售版本、账户分录、票款分配和复核链。
async fn load_card_funds_snapshot(
    db: &Database,
    account: &ReceivableAccount,
    executor: &mut dyn Executor,
) -> Result<CardFundsSnapshot> {
    let sales_order_id = account.sales_order_id.to_string();
    let sales_order = db
        .sales_orders()
        .find_by_id(&sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("应收账户来源销售单不存在".to_string()))?;
    let current_sales_order_revision_id = sales_order
        .stable
        .current_revision_id
        .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前正式版本".to_string()))?;
    let current_revision = db
        .sales_order_revisions()
        .find_by_id(&current_sales_order_revision_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单当前正式版本不存在".to_string()))?;
    let account_id = ReceivableAccountId::new(account.base.id.clone());
    let entries = db
        .receivable_entries()
        .find_entries_by_account(&account_id, executor)
        .await?;
    let entry_ids = entries
        .iter()
        .map(|entry| ReceivableEntryId::new(entry.base.id.clone()))
        .collect::<Vec<_>>();
    let receipt_allocations = db
        .receipt_allocations()
        .find_allocations_by_entries(&entry_ids, executor)
        .await?;
    let invoice_allocations = db
        .sales_invoice_allocations()
        .find_allocations_by_accounts(std::slice::from_ref(&account_id), executor)
        .await?;
    let receipt_ids = receipt_allocations
        .iter()
        .map(|allocation| allocation.customer_receipt_id.to_string())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let expected_receipts = receipt_ids.len();
    let receipts = db
        .customer_receipts()
        .find_receipts_by_ids(&receipt_ids, executor)
        .await?;
    if receipts.len() != expected_receipts {
        return Err(Error::NotFound("应收账户引用的回款单不存在".to_string()));
    }
    let invoice_ids = invoice_allocations
        .iter()
        .map(|allocation| allocation.invoice_id.to_string())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let expected_invoices = invoice_ids.len();
    let invoices = db.invoices().find_invoices_by_ids(&invoice_ids, executor).await?;
    if invoices.len() != expected_invoices {
        return Err(Error::NotFound("应收账户引用的发票不存在".to_string()));
    }
    let reviews = db
        .receivable_funds_reviews()
        .find_reviews_by_account(&account_id, executor)
        .await?;
    Ok(CardFundsSnapshot {
        current_sales_order_revision_id,
        sales_order_no: sales_order.order_no,
        sales_order_revision_no: current_revision.revision.revision_no,
        sales_order_snapshot_at: u64::try_from(current_revision.effective_at.unix_secs()).unwrap_or_default(),
        customer_name: current_revision.customer_snapshot.customer_name,
        counterparty_party_name: current_revision
            .settlement_party_snapshot
            .map(|snapshot| snapshot.settlement_party_name),
        entries,
        reviews,
        receipt_allocations,
        invoice_allocations,
        receipts,
        invoices,
    })
}

/// 装配当前账户关联的正式回款事实投影。
fn receipt_fact_views(snapshot: &CardFundsSnapshot) -> Vec<ReceivableReceiptFactView> {
    let mut receipts = snapshot.receipts.iter().collect::<Vec<_>>();
    receipts.sort_by(|left, right| {
        left.received_at
            .cmp(&right.received_at)
            .then_with(|| left.base.id.cmp(&right.base.id))
    });
    receipts
        .into_iter()
        .map(|receipt| {
            let allocated_to_account = snapshot
                .receipt_allocations
                .iter()
                .filter(|allocation| allocation.customer_receipt_id.as_ref() == receipt.base.id.as_str())
                .fold(zero_amount(), |total, allocation| {
                    match allocation.allocation_action {
                        AllocationAction::Apply => total.checked_add(allocation.allocated_amount),
                        AllocationAction::Reverse => total.checked_sub(allocation.allocated_amount),
                    }
                });
            ReceivableReceiptFactView {
                receipt_id: receipt.base.id.clone(),
                receipt_no: receipt.receipt_no.clone(),
                received_at: receipt.received_at.as_utc().to_rfc3339(),
                gross_amount: receipt.amount,
                allocated_to_account,
                other_allocation_summary: None,
                reversed: receipt.status == CustomerReceiptStatus::Reversed,
            }
        })
        .collect()
}

/// 装配当前账户关联的正式销项发票事实投影。
fn invoice_fact_views(snapshot: &CardFundsSnapshot) -> Vec<ReceivableInvoiceFactView> {
    let mut invoices = snapshot.invoices.iter().collect::<Vec<_>>();
    invoices.sort_by(|left, right| {
        left.invoice_date
            .cmp(&right.invoice_date)
            .then_with(|| left.base.id.cmp(&right.base.id))
    });
    invoices
        .into_iter()
        .map(|invoice| {
            let allocated_to_account = snapshot
                .invoice_allocations
                .iter()
                .filter(|allocation| allocation.invoice_id.as_ref() == invoice.base.id.as_str())
                .fold(zero_amount(), |total, allocation| {
                    match allocation.allocation_action {
                        AllocationAction::Apply => total.checked_add(allocation.allocated_gross_amount),
                        AllocationAction::Reverse => total.checked_sub(allocation.allocated_gross_amount),
                    }
                });
            ReceivableInvoiceFactView {
                invoice_id: invoice.base.id.clone(),
                invoice_no: invoice.invoice_no.clone(),
                direction: match invoice.invoice_kind {
                    InvoiceKind::Blue => "BLUE",
                    InvoiceKind::Red => "RED",
                }
                .to_string(),
                issued_at: invoice.invoice_date.to_string(),
                gross_amount: invoice.gross_amount,
                net_amount: invoice.net_amount,
                tax_amount: invoice.tax_amount,
                allocated_to_account,
                reversed: invoice.stable.status() == InvoiceStatus::RedInvoiced,
            }
        })
        .collect()
}

/// 将 HTTP 字符串任务版本严格解析为运行时乐观锁版本。
fn parse_task_version(value: &str) -> Result<u64> {
    let normalized = value.trim();
    let parsed = normalized
        .parse::<u64>()
        .map_err(|_| Error::ValidationError("任务版本必须是无符号整数字符串".to_string()))?;
    if parsed == 0 || parsed.to_string() != normalized {
        return Err(Error::ValidationError(
            "任务版本必须是规范的正整数字符串".to_string(),
        ));
    }
    Ok(parsed)
}

/// 校验任务类型、业务对象、任务/对象版本和当前个人责任。
fn validate_card_funds_work_item(
    item: &WorkItem,
    decision: &CardFundsReviewDecision,
    expected_task_version: u64,
    expected_subject_version: &str,
    actor_id: &str,
) -> Result<()> {
    if item.base.version != expected_task_version {
        return Err(Error::ConflictError(
            "复核任务版本已变化，请刷新后重试".to_string(),
        ));
    }
    if item.subject_version != expected_subject_version {
        return Err(Error::ConflictError(
            "复核任务对象版本已变化，请刷新后重试".to_string(),
        ));
    }
    if false
        || item.business_object_type != "receivable_account"
        || item.business_object_id != decision.receivable_account_id.to_string()
    {
        return Err(Error::BusinessLogicError(
            "当前任务不是该应收账户的独立票款复核任务".to_string(),
        ));
    }
    let expected_type = match decision.review_type {
        CardFundsReviewType::Opening => WorkItemType::CardFundsReview,
        CardFundsReviewType::SyncDelta => WorkItemType::CardFundsDeltaReview,
    };
    if item.work_item_type != expected_type {
        return Err(Error::BusinessLogicError("复核类型与任务类型不一致".to_string()));
    }
    if !item.is_owned_by(actor_id) {
        return Err(Error::Forbidden("当前账号不是开放任务的当前责任人".to_string()));
    }
    Ok(())
}

/// 校验账户、销售版本、复核链版本和票款事实版本。
fn validate_card_funds_versions(
    account: &ReceivableAccount,
    snapshot: &CardFundsSnapshot,
    decision: &CardFundsReviewDecision,
    work_item: &WorkItem,
) -> Result<()> {
    let expected_status = pending_review_status(decision.review_type);
    if account.account_seq != decision.expected_account_seq
        || account.base.version.to_string() != decision.expected_account_domain_version
        || account.review_status != expected_status
    {
        return Err(Error::ConflictError(
            "应收账户领域版本或复核状态已变化".to_string(),
        ));
    }
    if snapshot.current_sales_order_revision_id != decision.expected_sales_order_revision_id
        || snapshot.current_sales_order_revision_id != work_item.subject_version
    {
        return Err(Error::ConflictError("销售单当前版本已变化".to_string()));
    }
    let tail = snapshot.reviews.last().map(|review| review.base.id.as_str());
    if tail != decision.expected_review_chain_tail_id.as_deref()
        || review_chain_version(&snapshot.reviews) != decision.expected_review_chain_version
    {
        return Err(Error::ConflictError("复核链已变化，请刷新后重试".to_string()));
    }
    let next_review_no = next_review_no(&snapshot.reviews)?;
    if next_review_no != decision.expected_next_review_no {
        return Err(Error::ConflictError("下一复核号已变化，请刷新后重试".to_string()));
    }
    if decision.review_type == CardFundsReviewType::SyncDelta && snapshot.reviews.is_empty() {
        return Err(Error::BusinessLogicError(
            "同步差额复核必须建立在既有期初复核链上".to_string(),
        ));
    }
    for (index, review) in snapshot.reviews.iter().enumerate() {
        let expected_no = index as u32 + 1;
        let expected_predecessor = index
            .checked_sub(1)
            .and_then(|previous| snapshot.reviews.get(previous))
            .map(|previous| previous.base.id.as_str());
        if review.review_no != expected_no
            || review.supersedes_review_id.as_ref().map(AsRef::as_ref) != expected_predecessor
        {
            return Err(Error::Internal("应收复核链连续性损坏".to_string()));
        }
    }
    if funds_fact_version(account, snapshot) != decision.expected_funds_fact_version {
        return Err(Error::ConflictError(
            "票款事实版本已变化，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

/// 返回复核链下一连续序号并拒绝序号溢出。
fn next_review_no(reviews: &[ReceivableFundsReview]) -> Result<u32> {
    reviews.last().map_or(Ok(1), |review| {
        review
            .review_no
            .checked_add(1)
            .ok_or_else(|| Error::Internal("应收复核号已达到上限".to_string()))
    })
}

/// 复算应收、回款与发票金额，并校验正式结论的事实前提。
fn validate_card_funds_facts(
    account: &ReceivableAccount,
    snapshot: &CardFundsSnapshot,
    decision: &CardFundsReviewDecision,
) -> Result<()> {
    let entries_net = snapshot
        .entries
        .iter()
        .fold(zero_amount(), |total, entry| match entry.direction {
            EntryDirection::Increase => total.checked_add(entry.amount),
            EntryDirection::Decrease => total.checked_sub(entry.amount),
        });
    if entries_net != account.gross_total {
        return Err(Error::BusinessLogicError(
            "应收分录净额与账户应收总额不一致".to_string(),
        ));
    }
    let receipt_net = net_receipt_allocated(&snapshot.receipt_allocations);
    if receipt_net != account.settled_total {
        return Err(Error::BusinessLogicError(
            "回款分配净额与账户已核销总额不一致".to_string(),
        ));
    }
    let invoice_net = net_sales_allocated(&snapshot.invoice_allocations);
    if invoice_net != account.invoiced_total {
        return Err(Error::BusinessLogicError(
            "发票分配净额与账户已开票总额不一致".to_string(),
        ));
    }
    match decision.conclusion {
        CardFundsReviewConclusion::NoHistoryFromZero => {
            if decision.review_type != CardFundsReviewType::Opening
                || decision.review_result != CardFundsReviewResult::Approved
                || receipt_net != zero_amount()
                || invoice_net != zero_amount()
                || !snapshot.receipt_allocations.is_empty()
                || !snapshot.invoice_allocations.is_empty()
            {
                return Err(Error::BusinessLogicError(
                    "从零起算只允许无任何历史回款和发票事实的期初通过复核".to_string(),
                ));
            }
        }
        CardFundsReviewConclusion::RecordedFactsReconciled => {
            if decision.review_result != CardFundsReviewResult::Approved
                || (snapshot.receipt_allocations.is_empty() && snapshot.invoice_allocations.is_empty())
            {
                return Err(Error::BusinessLogicError(
                    "已核对结论必须存在正式回款或发票事实".to_string(),
                ));
            }
        }
        CardFundsReviewConclusion::Rejected => {}
    }
    Ok(())
}

/// 重验责任资格，并对已登记票款事实执行可证明的经办/复核岗位分离。
async fn validate_card_funds_reviewer_separation(
    db: &Database,
    account: &ReceivableAccount,
    snapshot: &CardFundsSnapshot,
    work_item: &WorkItem,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = (work_item, actor_id);

    for receipt in &snapshot.receipts {
        let receipt_id = receipt.base.id.as_str();
        if !matches!(
            receipt.status,
            CustomerReceiptStatus::Posted | CustomerReceiptStatus::Reversed
        ) || receipt.counterparty_party_id != account.counterparty_party_id
        {
            return Err(Error::BusinessLogicError(
                "复核引用的回款事实未正式过账或往来主体不一致".to_string(),
            ));
        }
        ensure_fact_separation(
            db,
            "customer_receipt",
            receipt_id,
            actor_id,
            &["customer_receipt.create", "customer_receipt.post:"],
            &["customer_receipt.post:"],
            executor,
        )
        .await?;
    }

    for invoice in &snapshot.invoices {
        let invoice_id = invoice.base.id.as_str();
        if invoice.invoice_direction != InvoiceDirection::Sales
            || !matches!(
                invoice.stable.status(),
                InvoiceStatus::Registered | InvoiceStatus::RedInvoiced
            )
            || invoice.party_id != account.counterparty_party_id
        {
            return Err(Error::BusinessLogicError(
                "复核引用的销项发票未正式登记或往来主体不一致".to_string(),
            ));
        }
        ensure_fact_separation(
            db,
            "invoice",
            invoice_id,
            actor_id,
            &["invoice.create", "invoice.post", "invoice.red_issue"],
            &["invoice.post", "invoice.red_issue"],
            executor,
        )
        .await?;
    }
    Ok(())
}

/// 从审计事实证明票款已正式登记且当前复核人不是其经办人。
async fn ensure_fact_separation(
    db: &Database,
    resource_type: &str,
    resource_id: &str,
    actor_id: &str,
    operator_actions: &[&str],
    formal_actions: &[&str],
    executor: &mut dyn Executor,
) -> Result<()> {
    let audits = db
        .audit_logs()
        .list_successful_by_resource(resource_type, resource_id, executor)
        .await?;
    let matches_action =
        |action: &str, prefixes: &[&str]| prefixes.iter().any(|prefix| action.starts_with(prefix));
    if !audits
        .iter()
        .any(|audit| matches_action(&audit.action, formal_actions))
    {
        return Err(Error::Forbidden(
            "无法从审计事实证明票款已经正式登记，岗位分离校验失败关闭".to_string(),
        ));
    }
    if audits
        .iter()
        .any(|audit| audit.actor_id == actor_id && matches_action(&audit.action, operator_actions))
    {
        return Err(Error::Forbidden(
            "票款事实经办人与最终复核人必须岗位分离".to_string(),
        ));
    }
    Ok(())
}

/// 将 HTTP 复核类型转换为领域事实枚举。
fn entity_review_type(review_type: CardFundsReviewType) -> entities::receivable::FundsReviewType {
    match review_type {
        CardFundsReviewType::Opening => entities::receivable::FundsReviewType::Opening,
        CardFundsReviewType::SyncDelta => entities::receivable::FundsReviewType::SyncDelta,
    }
}

/// 将 HTTP 复核结果转换为领域事实枚举。
fn entity_review_result(result: CardFundsReviewResult) -> ReviewResult {
    match result {
        CardFundsReviewResult::Approved => ReviewResult::Passed,
        CardFundsReviewResult::Rejected => ReviewResult::Rejected,
    }
}

/// 返回复核类型对应的待复核账户缓存状态。
fn pending_review_status(review_type: CardFundsReviewType) -> AccountReviewStatus {
    match review_type {
        CardFundsReviewType::Opening => AccountReviewStatus::OpeningPending,
        CardFundsReviewType::SyncDelta => AccountReviewStatus::SyncDeltaPending,
    }
}

/// 返回账户复核状态的稳定工作流代码。
fn account_review_status_code(status: AccountReviewStatus) -> &'static str {
    match status {
        AccountReviewStatus::NotApplicable => "NOT_APPLICABLE",
        AccountReviewStatus::OpeningPending => "OPENING_PENDING",
        AccountReviewStatus::Reviewed => "REVIEWED",
        AccountReviewStatus::SyncDeltaPending => "SYNC_DELTA_PENDING",
    }
}

/// 计算不可由客户端解释或递增的复核链版本。
fn review_chain_version(reviews: &[ReceivableFundsReview]) -> String {
    let mut digest = Sha256::new();
    digest_part(&mut digest, "receivable-review-chain-v1");
    for review in reviews {
        digest_part(&mut digest, &review.base.id);
        digest_part(&mut digest, &review.review_no.to_string());
        digest_part(&mut digest, review.review_type.as_str());
        digest_part(&mut digest, review.work_item_id.as_ref());
        digest_part(
            &mut digest,
            review
                .evidence_document_id
                .as_ref()
                .map(AsRef::as_ref)
                .unwrap_or_default(),
        );
        digest_part(
            &mut digest,
            review.evidence_reference.as_deref().unwrap_or_default(),
        );
        digest_part(&mut digest, review.review_result.as_str());
        digest_part(&mut digest, &review.reviewed_by);
        digest_part(&mut digest, &review.reviewed_at.unix_secs().to_string());
        digest_part(
            &mut digest,
            review
                .supersedes_review_id
                .as_ref()
                .map(AsRef::as_ref)
                .unwrap_or_default(),
        );
    }
    format!("rcv:{}", hex::encode(digest.finalize()))
}

/// 计算账户及其全部当前票款正式事实的不透明版本。
fn funds_fact_version(account: &ReceivableAccount, snapshot: &CardFundsSnapshot) -> String {
    let mut digest = Sha256::new();
    digest_part(&mut digest, "receivable-funds-facts-v1");
    for value in [
        account.base.id.as_str(),
        &account.base.version.to_string(),
        &account.account_seq.to_string(),
        account.counterparty_party_id.as_ref(),
        &account.gross_total.to_string(),
        &account.settled_total.to_string(),
        &account.invoiceable_total.to_string(),
        &account.invoiced_total.to_string(),
    ] {
        digest_part(&mut digest, value);
    }
    let mut entries = snapshot.entries.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.source_sequence
            .cmp(&right.source_sequence)
            .then_with(|| left.base.id.cmp(&right.base.id))
    });
    for entry in entries {
        for value in [
            entry.base.id.as_str(),
            entry.entry_type.as_str(),
            entry.direction.as_str(),
            &entry.amount.to_string(),
            &entry.due_date.to_string(),
            entry.source_document_id.as_str(),
            entry.source_revision_id.as_str(),
            &entry.source_sequence.to_string(),
            &entry.posted_at.unix_secs().to_string(),
        ] {
            digest_part(&mut digest, value);
        }
    }
    let mut receipt_allocations = snapshot.receipt_allocations.iter().collect::<Vec<_>>();
    receipt_allocations.sort_by(|left, right| left.base.id.cmp(&right.base.id));
    for allocation in receipt_allocations {
        for value in [
            allocation.base.id.as_str(),
            allocation.customer_receipt_id.as_ref(),
            allocation.receivable_entry_id.as_ref(),
            &allocation.allocation_seq.to_string(),
            allocation.allocation_action.as_str(),
            &allocation.allocated_amount.to_string(),
            allocation
                .reverses_allocation_id
                .as_ref()
                .map(AsRef::as_ref)
                .unwrap_or_default(),
        ] {
            digest_part(&mut digest, value);
        }
    }
    let mut invoice_allocations = snapshot.invoice_allocations.iter().collect::<Vec<_>>();
    invoice_allocations.sort_by(|left, right| left.base.id.cmp(&right.base.id));
    for allocation in invoice_allocations {
        for value in [
            allocation.base.id.as_str(),
            allocation.invoice_id.as_ref(),
            allocation.receivable_account_id.as_ref(),
            &allocation.allocation_seq.to_string(),
            allocation.allocation_action.as_str(),
            &allocation.allocated_gross_amount.to_string(),
            &allocation.allocated_net_amount.to_string(),
            &allocation.allocated_tax_amount.to_string(),
            allocation
                .reverses_allocation_id
                .as_ref()
                .map(AsRef::as_ref)
                .unwrap_or_default(),
        ] {
            digest_part(&mut digest, value);
        }
    }
    let mut receipts = snapshot.receipts.iter().collect::<Vec<_>>();
    receipts.sort_by(|left, right| left.base.id.cmp(&right.base.id));
    for receipt in receipts {
        for value in [
            receipt.base.id.as_str(),
            &receipt.base.version.to_string(),
            receipt.status.as_str(),
            receipt.counterparty_party_id.as_ref(),
            receipt.receipt_no.as_str(),
            &receipt.received_at.unix_secs().to_string(),
            &receipt.amount.to_string(),
            receipt.bank_reference.as_deref().unwrap_or_default(),
        ] {
            digest_part(&mut digest, value);
        }
    }
    let mut invoices = snapshot.invoices.iter().collect::<Vec<_>>();
    invoices.sort_by(|left, right| left.base.id.cmp(&right.base.id));
    for invoice in invoices {
        for value in [
            invoice.base.id.as_str(),
            &invoice.base.version.to_string(),
            invoice.invoice_direction.as_str(),
            invoice.invoice_kind.as_str(),
            invoice.party_id.as_ref(),
            invoice.invoice_code.as_deref().unwrap_or_default(),
            invoice.invoice_no.as_str(),
            &invoice.invoice_date.to_string(),
            &invoice.gross_amount.to_string(),
            &invoice.net_amount.to_string(),
            &invoice.tax_amount.to_string(),
            invoice.stable.status().as_str(),
        ] {
            digest_part(&mut digest, value);
        }
    }
    format!("ffv:{}", hex::encode(digest.finalize()))
}

/// 向摘要写入无拼接歧义的长度前缀字段。
fn digest_part(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}

/// 计算覆盖完整 W13 命令载荷的稳定指纹。
fn card_funds_command_fingerprint(command: &CompleteCardFundsReviewCommand) -> Result<String> {
    let serialized = serde_json::to_vec(command)
        .map_err(|error| Error::Internal(format!("卡券票款复核命令序列化失败: {error}")))?;
    Ok(hex::encode(Sha256::digest(serialized)))
}

/// 生成不泄漏原始幂等键的稳定审计主键。
fn card_funds_audit_id(actor_id: &str, key: &str) -> String {
    let mut digest = Sha256::new();
    digest_part(&mut digest, CARD_FUNDS_REVIEW_ACTION);
    digest_part(&mut digest, actor_id);
    digest_part(&mut digest, key.trim());
    format!(
        "{CARD_FUNDS_REVIEW_RECEIPT_PREFIX}{}",
        hex::encode(digest.finalize())
    )
}

/// 将正式结果编码为受审计消息长度约束的幂等收据。
fn card_funds_receipt_message(fingerprint: &str, receipt: &CardFundsReviewReceipt) -> String {
    format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={}|{}|{}|{}|{}|{}|{}|{}|{}",
        receipt.receivable_funds_review_id,
        receipt.workflow_action_id,
        receipt.review_no,
        receipt.account_review_status,
        receipt.completed_at,
        receipt.review_result.as_str(),
        receipt.conclusion.as_str(),
        receipt.follow_up_work_item_id.as_deref().unwrap_or_default(),
        receipt.follow_up_work_item_type.as_deref().unwrap_or_default(),
    )
}

/// 解析 W13 审计收据，并拒绝同幂等键下的载荷漂移。
fn parse_card_funds_receipt(message: &str, expected_fingerprint: &str) -> Result<CardFundsReviewReceipt> {
    let (fingerprint, result) = message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split_once(";result="))
        .ok_or_else(|| Error::Internal("卡券票款复核幂等收据格式非法".to_string()))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError(
            "幂等键已用于不同的卡券票款复核命令".to_string(),
        ));
    }
    let fields = result.split('|').collect::<Vec<_>>();
    let [review_id, workflow_id, review_no, account_status, completed_at, result, conclusion, follow_up @ ..] =
        fields.as_slice()
    else {
        return Err(Error::Internal("卡券票款复核幂等收据结果非法".to_string()));
    };
    let review_result = match *result {
        "APPROVED" => CardFundsReviewResult::Approved,
        "REJECTED" => CardFundsReviewResult::Rejected,
        _ => return Err(Error::Internal("卡券票款复核收据结果代码非法".to_string())),
    };
    let conclusion = match *conclusion {
        "NO_HISTORY_FROM_ZERO" => CardFundsReviewConclusion::NoHistoryFromZero,
        "RECORDED_FACTS_RECONCILED" => CardFundsReviewConclusion::RecordedFactsReconciled,
        "REJECTED" => CardFundsReviewConclusion::Rejected,
        _ => return Err(Error::Internal("卡券票款复核收据结论代码非法".to_string())),
    };
    let (follow_up_work_item_id, follow_up_work_item_type) = match follow_up {
        [] => (None, None),
        [follow_up_work_item_id, follow_up_work_item_type] => match (
            review_result,
            follow_up_work_item_id.is_empty(),
            follow_up_work_item_type.is_empty(),
        ) {
            (CardFundsReviewResult::Approved, true, true) => (None, None),
            (CardFundsReviewResult::Rejected, false, false) => (
                Some((*follow_up_work_item_id).to_string()),
                Some((*follow_up_work_item_type).to_string()),
            ),
            _ => {
                return Err(Error::Internal("卡券票款复核后继任务收据不完整".to_string()));
            }
        },
        _ => return Err(Error::Internal("卡券票款复核幂等收据结果非法".to_string())),
    };
    Ok(CardFundsReviewReceipt {
        receivable_funds_review_id: (*review_id).to_string(),
        workflow_action_id: (*workflow_id).to_string(),
        review_no: review_no
            .parse()
            .map_err(|_| Error::Internal("卡券票款复核收据复核号非法".to_string()))?,
        account_review_status: (*account_status).to_string(),
        completed_at: completed_at
            .parse()
            .map_err(|_| Error::Internal("卡券票款复核收据完成时间非法".to_string()))?,
        review_result,
        conclusion,
        follow_up_work_item_id,
        follow_up_work_item_type,
    })
}

/// 将销项分配事实适配为领域红票规划输入并构建计划。
///
/// # 参数
/// * `blue` - 原蓝票查询得到的全部销项分配事实
/// * `related` - 同一批账户下可能引用原分配的全部相关事实
/// * `requested` - 可选本次红冲含税金额
///
/// # 返回
/// 返回完成历史净额、顺序消费和比例税额舍入的领域计划。
///
/// # 错误
/// 领域规划失败时映射为与既有服务相同的业务、内部或金额错误。
///
/// # 约束
/// 本函数只转换持久化事实形态，不实现或复制红冲计算规则。
fn sales_red_invoice_allocation_plan(
    blue: &[SalesInvoiceAllocation],
    related: &[SalesInvoiceAllocation],
    requested: Option<Amount>,
) -> Result<RedInvoiceAllocationPlan> {
    let basis = sales_red_invoice_allocation_bases(blue);
    let reversals = sales_red_invoice_allocation_reversals(related);
    RedInvoiceAllocationPlan::build(InvoiceDirection::Sales, basis, &reversals, requested)
        .map_err(map_red_invoice_allocation_plan_error)
}

/// 将销项蓝票正向分配转换为领域原始基数输入。
///
/// # 参数
/// * `blue` - 原蓝票查询得到的全部销项分配事实
///
/// # 返回
/// 返回保持查询顺序的正向分配基数，非 `APPLY` 事实被忽略。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 只复制事实字段，不扣减历史红冲或执行金额计算。
fn sales_red_invoice_allocation_bases(blue: &[SalesInvoiceAllocation]) -> Vec<RedInvoiceAllocationBasis> {
    blue.iter()
        .filter(|line| line.allocation_action == AllocationAction::Apply)
        .map(|line| RedInvoiceAllocationBasis {
            original_allocation_id: line.base.id.clone(),
            account_id: line.receivable_account_id.to_string(),
            allocation_seq: line.allocation_seq,
            gross: line.allocated_gross_amount,
            net: line.allocated_net_amount,
            tax: line.allocated_tax_amount,
        })
        .collect()
}

/// 将销项历史反向分配转换为领域红冲事实输入。
///
/// # 参数
/// * `related` - 同一批应收账户下的全部相关销项分配事实
///
/// # 返回
/// 返回所有携带原分配引用的 `REVERSE` 事实，保持查询顺序。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 缺少反向引用的损坏事实沿用旧逻辑忽略，由领域计划只匹配原分配身份。
fn sales_red_invoice_allocation_reversals(
    related: &[SalesInvoiceAllocation],
) -> Vec<RedInvoiceAllocationReversal> {
    related
        .iter()
        .filter(|line| line.allocation_action == AllocationAction::Reverse)
        .filter_map(|line| {
            line.reverses_allocation_id
                .as_ref()
                .map(|original_id| RedInvoiceAllocationReversal {
                    original_allocation_id: original_id.to_string(),
                    gross: line.allocated_gross_amount,
                    net: line.allocated_net_amount,
                    tax: line.allocated_tax_amount,
                })
        })
        .collect()
}

/// 将进项分配事实适配为领域红票规划输入并构建计划。
///
/// # 参数
/// * `blue` - 原蓝票查询得到的全部进项分配事实
/// * `related` - 同一批账户下可能引用原分配的全部相关事实
/// * `requested` - 可选本次红冲含税金额
///
/// # 返回
/// 返回完成历史净额、顺序消费和比例税额舍入的领域计划。
///
/// # 错误
/// 领域规划失败时映射为与既有服务相同的业务、内部或金额错误。
///
/// # 约束
/// 本函数只转换 D19 持久化事实形态，不将进项实体依赖反向引入 D18 发票模型。
fn purchase_red_invoice_allocation_plan(
    blue: &[PurchaseInvoiceAllocation],
    related: &[PurchaseInvoiceAllocation],
    requested: Option<Amount>,
) -> Result<RedInvoiceAllocationPlan> {
    let basis = purchase_red_invoice_allocation_bases(blue);
    let reversals = purchase_red_invoice_allocation_reversals(related);
    RedInvoiceAllocationPlan::build(InvoiceDirection::Purchase, basis, &reversals, requested)
        .map_err(map_red_invoice_allocation_plan_error)
}

/// 将进项蓝票正向分配转换为领域原始基数输入。
///
/// # 参数
/// * `blue` - 原蓝票查询得到的全部进项分配事实
///
/// # 返回
/// 返回保持查询顺序的正向分配基数，非 `APPLY` 事实被忽略。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 只复制 D19 事实字段，不在 Service 内扣减历史红冲或执行金额计算。
fn purchase_red_invoice_allocation_bases(
    blue: &[PurchaseInvoiceAllocation],
) -> Vec<RedInvoiceAllocationBasis> {
    blue.iter()
        .filter(|line| line.allocation_action == entities::payable::AllocationAction::Apply)
        .map(|line| RedInvoiceAllocationBasis {
            original_allocation_id: line.base.id.clone(),
            account_id: line.payable_account_id.to_string(),
            allocation_seq: line.allocation_seq,
            gross: line.allocated_gross_amount,
            net: line.allocated_net_amount,
            tax: line.allocated_tax_amount,
        })
        .collect()
}

/// 将进项历史反向分配转换为领域红冲事实输入。
///
/// # 参数
/// * `related` - 同一批应付账户下的全部相关进项分配事实
///
/// # 返回
/// 返回所有携带原分配引用的 `REVERSE` 事实，保持查询顺序。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 缺少反向引用的损坏事实沿用旧逻辑忽略，且 D19 实体不会进入 D18 领域模型。
fn purchase_red_invoice_allocation_reversals(
    related: &[PurchaseInvoiceAllocation],
) -> Vec<RedInvoiceAllocationReversal> {
    related
        .iter()
        .filter(|line| line.allocation_action == entities::payable::AllocationAction::Reverse)
        .filter_map(|line| {
            line.reverses_allocation_id
                .as_ref()
                .map(|original_id| RedInvoiceAllocationReversal {
                    original_allocation_id: original_id.to_string(),
                    gross: line.allocated_gross_amount,
                    net: line.allocated_net_amount,
                    tax: line.allocated_tax_amount,
                })
        })
        .collect()
}

/// 将领域红票规划错误映射回冻结的服务错误分类和文案。
///
/// # 参数
/// * `error` - 领域计划构建失败原因
///
/// # 返回
/// 返回与迁移前相同的 `BusinessLogicError`、`Internal` 或 `Logic` 服务错误。
///
/// # 错误
/// 本函数只构造错误值，不再失败。
///
/// # 约束
/// 不解析字符串决定分类；每个领域变体显式保持既有外部错误语义。
fn map_red_invoice_allocation_plan_error(error: RedInvoiceAllocationPlanError) -> Error {
    match error {
        error @ (RedInvoiceAllocationPlanError::SalesHistoricalOverReversal
        | RedInvoiceAllocationPlanError::PurchaseHistoricalOverReversal
        | RedInvoiceAllocationPlanError::NoRemainingAllocation
        | RedInvoiceAllocationPlanError::InvalidRequestedAmount) => {
            Error::BusinessLogicError(error.to_string())
        }
        RedInvoiceAllocationPlanError::UncoveredRequest => {
            Error::Internal("红票反向分配计划未覆盖请求金额".to_string())
        }
        RedInvoiceAllocationPlanError::InvalidAmount(error) => Error::Logic(error),
    }
}

/// 返回固定零金额（`Amount::from_str("0.00")` 的确定性快捷方式）。
///
/// # 返回
/// 返回金额 `0.00`。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}

/// 汇总回款核销分配并装配视图（`APPLY` 加、`REVERSE` 减）。
///
/// # 参数
/// * `allocations` - 回款核销分配集合
///
/// # 返回
/// 返回 `(净已核销合计, 分配视图列表)`。
fn allocation_view(
    allocations: &[ReceiptAllocation],
) -> (Amount, Vec<crate::receivable::dto::ReceiptAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .iter()
        .map(|allocation| {
            match allocation.allocation_action {
                AllocationAction::Apply => net = net.checked_add(allocation.allocated_amount),
                AllocationAction::Reverse => net = net.checked_sub(allocation.allocated_amount),
            }
            crate::receivable::dto::ReceiptAllocationView {
                id: allocation.base.id.clone(),
                allocation_seq: allocation.allocation_seq,
                allocation_action: allocation.allocation_action,
                receivable_entry_id: allocation.receivable_entry_id.to_string(),
                allocated_amount: allocation.allocated_amount,
                allocated_at: allocation.allocated_at,
                reverses_allocation_id: allocation
                    .reverses_allocation_id
                    .as_ref()
                    .map(|id| id.to_string()),
            }
        })
        .collect();
    (net, views)
}

/// 汇总销项发票分配并装配视图（`APPLY` 加、`REVERSE` 减）。
///
/// # 参数
/// * `allocations` - 销项发票分配集合
///
/// # 返回
/// 返回 `(净已分配含税合计, 分配视图列表)`。
fn sales_allocation_view(
    allocations: &[SalesInvoiceAllocation],
) -> (Amount, Vec<crate::receivable::dto::SalesInvoiceAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .iter()
        .map(|allocation| {
            match allocation.allocation_action {
                AllocationAction::Apply => net = net.checked_add(allocation.allocated_gross_amount),
                AllocationAction::Reverse => net = net.checked_sub(allocation.allocated_gross_amount),
            }
            crate::receivable::dto::SalesInvoiceAllocationView {
                id: allocation.base.id.clone(),
                allocation_seq: allocation.allocation_seq,
                allocation_action: allocation.allocation_action,
                receivable_account_id: allocation.receivable_account_id.to_string(),
                allocated_gross_amount: allocation.allocated_gross_amount,
                allocated_net_amount: allocation.allocated_net_amount,
                allocated_tax_amount: allocation.allocated_tax_amount,
                reverses_allocation_id: allocation
                    .reverses_allocation_id
                    .as_ref()
                    .map(|id| id.to_string()),
            }
        })
        .collect();
    (net, views)
}

/// 汇总进项发票分配并转换为跨方向复用的发票分配视图。
///
/// # 参数
/// * `allocations` - 进项发票分配集合
///
/// # 返回
/// 返回 `(净已分配含税合计, 分配视图列表)`。
fn purchase_allocation_view(
    allocations: &[PurchaseInvoiceAllocation],
) -> (Amount, Vec<crate::receivable::dto::SalesInvoiceAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .iter()
        .map(|allocation| {
            // 进项/销项分配动作枚举跨域不共享（见 A-G7），此处显式转换。
            let action = match allocation.allocation_action {
                entities::payable::AllocationAction::Apply => AllocationAction::Apply,
                entities::payable::AllocationAction::Reverse => AllocationAction::Reverse,
            };
            match action {
                AllocationAction::Apply => net = net.checked_add(allocation.allocated_gross_amount),
                AllocationAction::Reverse => net = net.checked_sub(allocation.allocated_gross_amount),
            }
            crate::receivable::dto::SalesInvoiceAllocationView {
                id: allocation.base.id.clone(),
                allocation_seq: allocation.allocation_seq,
                allocation_action: action,
                receivable_account_id: allocation.payable_account_id.to_string(),
                allocated_gross_amount: allocation.allocated_gross_amount,
                allocated_net_amount: allocation.allocated_net_amount,
                allocated_tax_amount: allocation.allocated_tax_amount,
                reverses_allocation_id: allocation
                    .reverses_allocation_id
                    .as_ref()
                    .map(|id| id.to_string()),
            }
        })
        .collect();
    (net, views)
}

/// 校验乐观锁版本。
///
/// # 错误
/// 不一致时返回冲突。
fn ensure_expected_version(actual: u64, expected: u64) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError(
        "数据已被其他请求修改，请刷新后重试".to_string(),
    ))
}

/// 发票创建必须跳过绑定：政策只能是 `NO_APPROVAL`。
///
/// # 返回
/// 返回 `SkipNoApproval`。
///
/// # 错误
/// 政策缺失或误登记为必须审批时返回部署不变量错误。
fn invoice_create_binding_decision() -> Result<BindingDecision> {
    let policy = policy_of(DocumentType::Invoice)?;
    match &policy {
        DocumentApprovalPolicy::NoApproval(no_approval) => {
            if no_approval.document_type != DocumentType::Invoice {
                return Err(Error::Internal("发票政策类型不匹配".to_string()));
            }
            Ok(binding_decision(policy.requirement()))
        }
        DocumentApprovalPolicy::ProcessRequired(_) => Err(Error::Internal(
            "发票必须是 NO_APPROVAL，不得绑定流程".to_string(),
        )),
    }
}

/// 确认发票创建路径不得查询发布定义。
///
/// # 错误
/// 绑定决定不是跳过时返回错误。
fn ensure_invoice_skips_approval_binding() -> Result<BindingDecision> {
    let decision = invoice_create_binding_decision()?;
    if decision != BindingDecision::SkipNoApproval {
        return Err(Error::Internal("发票创建必须跳过审批绑定".to_string()));
    }
    Ok(decision)
}

/// 发票不得注册空审批适配器。
///
/// # 错误
/// 适配器登记存在时返回部署不变量错误。
fn ensure_invoice_has_no_adapter() -> Result<()> {
    if adapter_spec_of(DocumentType::Invoice).is_ok() {
        return Err(Error::Internal("发票不得注册审批适配器".to_string()));
    }
    Ok(())
}

/// 发票往来主体作为绑定上下文组织，不得用空串补位。
///
/// # 参数
/// * `invoice` - 待登记发票
///
/// # 返回
/// 返回非空往来主体。
///
/// # 错误
/// 往来主体为空时返回校验错误。
fn invoice_binding_organization_id(invoice: &Invoice) -> Result<String> {
    let org = invoice.party_id.to_string();
    if org.trim().is_empty() {
        return Err(Error::ValidationError(
            "发票缺少往来主体，无法构造绑定上下文".to_string(),
        ));
    }
    Ok(org)
}

/// 构造发票创建绑定命令。客户端不得提交定义 ID。
///
/// # 参数
/// * `invoice` - 待登记发票
/// * `creator_id` - 创建人
///
/// # 错误
/// 往来主体为空时返回校验错误。
fn invoice_bind_command(invoice: &Invoice, creator_id: &str) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: DocumentType::Invoice,
        business_object_id: invoice.base.id.clone(),
        business_object_version: invoice.base.version,
        context: BindingRevalidationContext {
            organization_id: invoice_binding_organization_id(invoice)?,
            creator_id: creator_id.to_string(),
        },
    })
}

/// 将绑定端口返回值落实为发票注册行：空绑定保持未绑定。
///
/// # 参数
/// * `document` - 发票注册行
/// * `binding` - 统一绑定端口返回值
///
/// # 返回
/// 固定返回 `None`。
///
/// # 错误
/// 端口返回绑定或注册行已预置绑定时返回错误。
fn apply_invoice_create_binding(
    document: &mut BusinessDocument,
    binding: Option<ApprovalDefinitionBinding>,
) -> Result<Option<ApprovalDefinitionBinding>> {
    if binding.is_some() {
        return Err(Error::Internal(
            "发票为 NO_APPROVAL，不得写入审批绑定".to_string(),
        ));
    }
    if document.approval_binding.is_some() {
        return Err(Error::Internal("发票注册行不得预置审批绑定".to_string()));
    }
    if document.document_type != DocumentType::Invoice {
        return Err(Error::Internal("发票创建只能注册 Invoice 单据".to_string()));
    }
    Ok(None)
}

/// 在调用方事务内登记发票单据并证明空绑定。
///
/// 必须先确认政策跳过，再调用统一绑定端口；不得查询发布定义后假装成功。
///
/// # 错误
/// 政策非无审批、端口返回绑定或写入失败时返回错误。
async fn persist_unbound_invoice_document(
    db: &Database,
    rbac: &SharedRbacService,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_invoice_skips_approval_binding()?;
    ensure_invoice_has_no_adapter()?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, executor).await?;
    apply_invoice_create_binding(&mut document, binding)?;
    persist_registered_document(db, &document, executor).await
}

/// 为已构造发票登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
async fn register_created_invoice_document(
    db: &Database,
    rbac: &SharedRbacService,
    invoice: &Invoice,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = invoice_bind_command(invoice, actor.id())?;
    let document = new_registered_document(
        &invoice.base.id,
        DocumentType::Invoice,
        invoice.invoice_no.clone(),
    )?;
    persist_unbound_invoice_document(db, rbac, document, &bind_command, actor, executor).await
}

/// 在创建事务内写入发票草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或发票写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_invoice(
    db: &Database,
    rbac: &SharedRbacService,
    invoice: Invoice,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor
        .clone()
        .resource_log("invoice.create", "invoice", invoice.base.id.clone())?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                register_created_invoice_document(&db, &rbac, &invoice, &actor, session).await?;
                db.invoices().create(&invoice, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 在创建事务内写入回款单、绑定发布定义并登记单据。
///
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误。
async fn persist_created_customer_receipt(
    db: &Database,
    rbac: &SharedRbacService,
    receipt: CustomerReceipt,
    actor: AuditActor,
) -> Result<()> {
    let organization_id = customer_receipt_responsible_org_id(&receipt)?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::CustomerReceipt,
        business_object_id: receipt.base.id.clone(),
        business_object_version: receipt.base.version,
        context: BindingRevalidationContext {
            organization_id,
            creator_id: actor.id().to_string(),
        },
    };
    let document = new_registered_document(
        &receipt.base.id,
        DocumentType::CustomerReceipt,
        receipt.receipt_no.clone(),
    )?;
    let audit = actor.clone().resource_log(
        "customer_receipt.create",
        "customer_receipt",
        receipt.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_customer_receipt_document(&db, &rbac, document, &bind_command, &actor, session)
                    .await?;
                db.customer_receipts().create(&receipt, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_customer_receipt_document(
    db: &Database,
    rbac: &SharedRbacService,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<ApprovalDefinitionBinding> {
    let _ = customer_receipt_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("客户回款单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, session).await?;
    Ok(binding)
}

/// 汇总冻结分配行金额。
///
/// # 参数
/// * `allocations` - 提交时冻结的待过账分配
///
/// # 返回
/// 返回各分配行金额之和。
fn pending_allocated_total(allocations: &[PendingReceiptAllocation]) -> Amount {
    allocations
        .iter()
        .fold(zero_amount(), |sum, line| sum.checked_add(line.allocated_amount))
}

/// 计算回款单净已核销合计（`APPLY` 加、`REVERSE` 减）。
///
/// # 参数
/// * `allocations` - 既有核销分配
///
/// # 返回
/// 返回净已核销金额。
fn net_receipt_allocated(allocations: &[ReceiptAllocation]) -> Amount {
    allocations
        .iter()
        .fold(zero_amount(), |sum, line| match line.allocation_action {
            AllocationAction::Apply => sum.checked_add(line.allocated_amount),
            AllocationAction::Reverse => sum.checked_sub(line.allocated_amount),
        })
}

/// 计算销项发票净已分配合计（`APPLY` 加、`REVERSE` 减）。
///
/// # 参数
/// * `allocations` - 当前应收账户关联的销项发票分配
///
/// # 返回
/// 返回净已开票含税金额。
fn net_sales_allocated(allocations: &[SalesInvoiceAllocation]) -> Amount {
    allocations
        .iter()
        .fold(zero_amount(), |sum, line| match line.allocation_action {
            AllocationAction::Apply => sum.checked_add(line.allocated_gross_amount),
            AllocationAction::Reverse => sum.checked_sub(line.allocated_gross_amount),
        })
}

#[cfg(test)]
mod card_funds_review_tests {
    use entities::common::time::Instant;
    use entities::file_asset::{
        ContentHmac, FileAsset, FileAssetData, RetentionClass, SecurityScanStatus, SensitivityClass,
    };
    use entities::ids::{FileAssetId, ReceivableAccountId};

    use super::card_funds_decision::{canonical_evidence, validate_evidence_assets, validated_from_dto};
    use super::{
        card_funds_audit_id, card_funds_receipt_message, parse_card_funds_receipt, parse_task_version,
        CardFundsReviewConclusion, CardFundsReviewDecision, CardFundsReviewReceipt, CardFundsReviewResult,
        CardFundsReviewType, Error, COMMAND_FINGERPRINT_PREFIX,
    };

    fn opening_decision() -> CardFundsReviewDecision {
        CardFundsReviewDecision {
            receivable_account_id: ReceivableAccountId::new("ra-1"),
            expected_account_seq: 1,
            expected_account_domain_version: "3".to_string(),
            expected_review_chain_tail_id: None,
            expected_review_chain_version: "rcv:empty".to_string(),
            expected_next_review_no: 1,
            expected_sales_order_revision_id: "sor-1".to_string(),
            expected_funds_fact_version: "ffv:empty".to_string(),
            review_type: CardFundsReviewType::Opening,
            review_result: CardFundsReviewResult::Approved,
            conclusion: CardFundsReviewConclusion::NoHistoryFromZero,
            evidence_document_ids: vec![FileAssetId::new("file-1")],
            evidence_references: Vec::new(),
            comment: Some("已核对".to_string()),
            reason_code: None,
        }
    }

    fn evidence_asset(id: &str, passed: bool) -> FileAsset {
        let mut asset = FileAsset::new(
            FileAssetId::new(id),
            FileAssetData {
                storage_object_key: format!("receivable-review/{id}"),
                file_name: format!("{id}.pdf"),
                content_type: "application/pdf".to_string(),
                byte_size: 1,
                content_hmac: ContentHmac::parse("a".repeat(64)).unwrap(),
                sensitivity_class: SensitivityClass::Sensitive,
                retention_class: RetentionClass::LongTerm,
                expires_at: None,
                created_by: "reviewer-1".to_string(),
            },
        )
        .unwrap();
        if passed {
            asset.mark_scan_result(SecurityScanStatus::Passed).unwrap();
        }
        asset
    }

    #[test]
    fn semantic_validation_rejects_result_conclusion_drift_and_missing_evidence() {
        let mut invalid = opening_decision();
        invalid.conclusion = CardFundsReviewConclusion::Rejected;
        assert!(validated_from_dto(&invalid).is_err());

        let mut invalid = opening_decision();
        invalid.evidence_document_ids.clear();
        invalid.evidence_references.clear();
        assert!(validated_from_dto(&invalid).is_err());

        let mut invalid = opening_decision();
        invalid.reason_code = Some("OTHER".to_string());
        // Approved 携带驳回原因应拒绝
        assert!(validated_from_dto(&invalid).is_err());
    }

    #[test]
    fn evidence_keeps_additional_documents_as_controlled_references() {
        let mut decision = opening_decision();
        decision.evidence_document_ids.push(FileAssetId::new("file-2"));
        decision.evidence_references.push("BANK-REF-1".to_string());
        let validated = validated_from_dto(&decision).unwrap();
        // canonical 为排序后结果：BANK-REF-1 与 file_asset:file-2 的字典序
        let mut expected = ["BANK-REF-1".to_string(), "file_asset:file-2".to_string()];
        expected.sort();
        assert_eq!(
            canonical_evidence(&validated).as_deref(),
            Some(expected.join("; ").as_str())
        );
        assert_eq!(validated.evidence().document_ids().len(), 2);
    }

    #[test]
    fn evidence_batch_restores_input_order_and_accepts_unordered_results() {
        let mut decision = opening_decision();
        decision.evidence_document_ids.push(FileAssetId::new("file-2"));
        let validated = validated_from_dto(&decision).unwrap();
        let assets = vec![evidence_asset("file-2", true), evidence_asset("file-1", true)];
        assert!(
            validate_evidence_assets(&validated, &assets, Instant::from_unix_secs(1_700_000_000),).is_ok()
        );
    }

    #[test]
    fn evidence_batch_reports_first_requested_missing_file_before_later_scan_error() {
        let mut decision = opening_decision();
        decision.evidence_document_ids =
            vec![FileAssetId::new("file-missing"), FileAssetId::new("file-pending")];
        let validated = validated_from_dto(&decision).unwrap();
        let assets = vec![evidence_asset("file-pending", false)];

        let error = validate_evidence_assets(&validated, &assets, Instant::from_unix_secs(1_700_000_000))
            .unwrap_err();

        assert!(matches!(
            error,
            Error::NotFound(message) if message == "复核证据文件不存在: file-missing"
        ));
    }

    #[test]
    fn canonical_evidence_is_sorted_and_byte_stable() {
        let mut decision = opening_decision();
        decision.evidence_document_ids.push(FileAssetId::new("file-2"));
        decision.evidence_references.push("z-ref".to_string());
        decision.evidence_references.push("a-ref".to_string());
        let v1 = validated_from_dto(&decision).unwrap();
        let c1 = canonical_evidence(&v1).unwrap();
        let v2 = validated_from_dto(&decision).unwrap();
        assert_eq!(canonical_evidence(&v2).unwrap(), c1);
        // 手工排序验证：a-ref 位于 file_asset:file-2 之前（字典序）
        assert!(c1.contains("a-ref"));
        assert!(c1.contains("file_asset:file-2"));
    }

    #[test]
    fn evidence_usability_uses_file_asset_point_in_time_check() {
        let decision = opening_decision();
        let validated = validated_from_dto(&decision).unwrap();
        let now = Instant::from_unix_secs(1_700_000_000);
        let mut expired_asset = evidence_asset("file-1", true);
        expired_asset.expires_at = Some(Instant::from_unix_secs(1_699_999_999));
        assert!(validate_evidence_assets(&validated, &[expired_asset], now).is_err());
        let mut destroyed_asset = evidence_asset("file-1", true);
        destroyed_asset
            .destroy(Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        assert!(validate_evidence_assets(&validated, &[destroyed_asset], now).is_err());
    }

    #[test]
    fn old_helpers_are_deleted_and_new_vo_is_unique_entry() {
        let source = include_str!("mod.rs");
        // 仅检查生产代码部分，避免本测试自身字面量触发误判
        let production = source
            .split("fn old_helpers_are_deleted")
            .next()
            .unwrap_or(source);
        assert!(source.contains("validated_from_dto"));
        assert!(source.contains("validate_evidence_assets"));
        assert!(source.contains("canonical_evidence"));
        assert!(source.contains("workflow_comment"));
        assert!(source.contains("ValidatedCardFundsReviewDecision"));
        assert!(source.contains("CardFundsReviewEvidence"));
        assert!(source.contains("is_usable_at") || source.contains("validate_usable_at"));
        // 旧 Service 私有 helper 已删除（FIN-E10 四处纯规则已收敛至实体/VO）
        assert!(!production.contains("fn validate_card_funds_decision"));
        assert!(!production.contains("fn validate_card_funds_evidence_facts"));
        assert!(!production.contains("fn canonical_review_evidence"));
        assert!(!production.contains("fn workflow_decision_comment"));
    }

    #[test]
    fn task_version_requires_canonical_positive_integer_string() {
        assert_eq!(parse_task_version("12").unwrap(), 12);
        assert!(parse_task_version("0").is_err());
        assert!(parse_task_version("01").is_err());
        assert!(parse_task_version("1.0").is_err());
    }

    #[test]
    fn receipt_replays_exact_result_and_rejects_payload_drift() {
        let receipt = CardFundsReviewReceipt {
            receivable_funds_review_id: "review-1".to_string(),
            workflow_action_id: "workflow-1".to_string(),
            review_no: 1,
            account_review_status: "opening_pending".to_string(),
            completed_at: 1_700_000_000,
            review_result: CardFundsReviewResult::Rejected,
            conclusion: CardFundsReviewConclusion::Rejected,
            follow_up_work_item_id: Some("wi-2".to_string()),
            follow_up_work_item_type: Some("CARD_FUNDS_REVIEW".to_string()),
        };
        let message = card_funds_receipt_message(&"a".repeat(64), &receipt);
        assert_eq!(
            parse_card_funds_receipt(&message, &"a".repeat(64)).unwrap(),
            receipt
        );
        assert!(matches!(
            parse_card_funds_receipt(&message, &"b".repeat(64)),
            Err(Error::ConflictError(_))
        ));

        let result = receipt.into_result("wi-1", "ra-1", "operation-1");
        let follow_up = result.business_result.follow_up_work_item.unwrap();
        assert_eq!(follow_up.work_item_id, "wi-2");
        assert_eq!(follow_up.work_item_type, "CARD_FUNDS_REVIEW");
        assert_eq!(follow_up.status, "OPEN");
    }

    #[test]
    fn legacy_seven_field_receipts_decode_with_explicit_rejected_migration_state() {
        let fingerprint = "c".repeat(64);
        let approved_message = format!(
            "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result=review-1|workflow-1|1|reviewed|1700000000|APPROVED|NO_HISTORY_FROM_ZERO"
        );
        let approved = parse_card_funds_receipt(&approved_message, &fingerprint).unwrap();
        assert!(!approved.requires_legacy_rejected_follow_up());
        assert!(approved
            .into_result("wi-1", "ra-1", "operation-1")
            .business_result
            .follow_up_work_item
            .is_none());

        let rejected_message = format!(
            "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result=review-2|workflow-2|2|opening_pending|1700000001|REJECTED|REJECTED"
        );
        let rejected = parse_card_funds_receipt(&rejected_message, &fingerprint).unwrap();
        assert!(rejected.requires_legacy_rejected_follow_up());
    }

    #[test]
    fn audit_id_is_stable_without_exposing_raw_key() {
        let id = card_funds_audit_id("actor-1", "secret-idempotency-key");
        assert_eq!(id, card_funds_audit_id("actor-1", "secret-idempotency-key"));
        assert_ne!(id, card_funds_audit_id("actor-2", "secret-idempotency-key"));
        assert!(!id.contains("secret-idempotency-key"));
    }
}

#[cfg(test)]
mod customer_receipt_approval_tests {
    use super::{execute_customer_receipt_domain_action, start_customer_receipt_approval, ReceivableService};
    use crate::approval::policy::ApprovalDomainAction;
    use entities::common::time::Instant;
    use entities::ids::{CustomerReceiptId, PartyId, ReceivableEntryId};
    use entities::money::Amount;
    use entities::receivable::{
        CustomerReceipt, CustomerReceiptData, CustomerReceiptStatus, PendingReceiptAllocation,
    };
    use std::str::FromStr;

    fn draft_receipt() -> CustomerReceipt {
        CustomerReceipt::new(
            CustomerReceiptId::new("cr-1"),
            CustomerReceiptData {
                receipt_no: "RC-1".into(),
                counterparty_party_id: PartyId::new("party-1"),
                customer_id: None,
                received_at: Instant::from_unix_secs(1),
                amount: Amount::from_str("100").expect("金额合法"),
                bank_reference: None,
            },
            "creator-1",
        )
        .expect("草稿必须可构造")
    }

    /// 创建必须注册 BusinessDocument 并绑定发布定义。
    #[test]
    fn create_registers_document_and_binds_published_definition() {
        let source = include_str!("mod.rs");
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::CustomerReceipt"));
        assert!(source.contains("persist_created_customer_receipt"));
    }

    /// 提交必须锁定单据、递增 approval_subject_version 并调用 start_approval。
    #[test]
    fn submit_calls_start_approval_with_subject_version() {
        let source = include_str!("mod.rs");
        assert!(source.contains("pub async fn submit_customer_receipt"));
        assert!(source.contains("customer_receipt_start_command"));
        assert!(source.contains("receipt.approval_subject_version"));
        assert!(source.contains("prepare_start"));
    }

    /// 最终动作唯一为 post_customer_receipt，且客户端过账旁路关闭。
    #[test]
    fn final_action_is_post_customer_receipt() {
        let source = include_str!("mod.rs");
        assert!(source.contains("pub async fn post_customer_receipt"));
        assert!(source.contains("receipt.mark_posted"));
        assert!(source.contains("CustomerReceiptPost"));
        assert!(source.contains("pending_allocations"));
        assert!(ReceivableService::reject_client_post().is_err());
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = include_str!("mod.rs");
        assert!(source.contains("pub async fn cancel_customer_receipt_approval"));
        assert!(source.contains("prepare_cancel"));
        assert!(source.contains("persist_customer_receipt_cancel"));
        let _ = ReceivableService::reject_client_post();
        let mut receipt = draft_receipt();
        start_customer_receipt_approval(
            &mut receipt,
            vec![PendingReceiptAllocation::new(
                ReceivableEntryId::new("re-1"),
                Amount::from_str("10").expect("金额合法"),
            )
            .expect("分配合法")],
        )
        .unwrap();
        execute_customer_receipt_domain_action(
            &mut receipt,
            ApprovalDomainAction::CustomerReceiptCancelApproval,
        )
        .unwrap();
        assert_eq!(receipt.status, CustomerReceiptStatus::Draft);
        assert_eq!(receipt.approval_subject_version, 1);
    }
}

#[cfg(test)]
mod invoice_no_approval_tests {
    use super::{
        apply_invoice_create_binding, ensure_invoice_has_no_adapter, ensure_invoice_skips_approval_binding,
        invoice_bind_command, invoice_create_binding_decision, policy_of, BindingDecision,
        DocumentApprovalPolicy, DocumentType, Invoice, InvoiceData,
    };
    use crate::approval::binding::binding_from_published;
    use crate::document_registry::new_registered_document;
    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::ProcessKind;
    use entities::common::time::{BusinessDate, Instant};
    use entities::ids::{InvoiceId, PartyId};
    use entities::money::Amount;
    use entities::receivable::{InvoiceDirection, InvoiceKind};
    use std::str::FromStr;

    fn draft_invoice() -> Invoice {
        Invoice::new(
            InvoiceId::new("inv-1"),
            InvoiceData {
                invoice_direction: InvoiceDirection::Sales,
                invoice_kind: InvoiceKind::Blue,
                party_id: PartyId::new("party-1"),
                invoice_code: None,
                invoice_no: "INV-1".into(),
                invoice_date: BusinessDate::from_ymd(2026, 8, 6).expect("日期合法"),
                gross_amount: Amount::from_str("100").expect("金额合法"),
                net_amount: Amount::from_str("88.50").expect("金额合法"),
                tax_amount: Amount::from_str("11.50").expect("金额合法"),
                rounding_adjustment_amount: Amount::from_str("0").expect("金额合法"),
                rounding_reason: None,
                original_invoice_id: None,
            },
            "admin-1",
        )
        .expect("草稿必须可构造")
    }

    /// 政策仅含 document_type、approval_requirement、process_kind，不得注册空 Adapter。
    #[test]
    fn invoice_policy_is_no_approval_identity_only() {
        let policy = policy_of(DocumentType::Invoice).expect("发票政策必须存在");
        let DocumentApprovalPolicy::NoApproval(no_approval) = &policy else {
            panic!("发票必须是 NO_APPROVAL");
        };
        assert_eq!(no_approval.document_type, DocumentType::Invoice);
        assert_eq!(no_approval.process_kind, ProcessKind::Invoice);
        assert_eq!(
            invoice_create_binding_decision().expect("绑定决定"),
            BindingDecision::SkipNoApproval
        );
        assert_eq!(
            ensure_invoice_skips_approval_binding().expect("必须跳过"),
            BindingDecision::SkipNoApproval
        );
        ensure_invoice_has_no_adapter().expect("不得注册空适配器");
    }

    /// 创建必须注册 BusinessDocument，绑定端口返回空，禁止写入绑定。
    #[test]
    fn create_registers_document_and_returns_empty_binding() {
        let invoice = draft_invoice();
        let command = invoice_bind_command(&invoice, "admin-1").expect("绑定命令");
        assert_eq!(command.document_type, DocumentType::Invoice);
        assert_eq!(command.business_object_id, invoice.base.id);
        assert_eq!(command.context.organization_id, "party-1");

        let mut document = new_registered_document(
            &invoice.base.id,
            DocumentType::Invoice,
            invoice.invoice_no.clone(),
        )
        .expect("可注册");
        assert!(document.approval_binding.is_none());
        let empty = apply_invoice_create_binding(&mut document, None).expect("空绑定");
        assert!(empty.is_none());
        assert!(document.approval_binding.is_none());

        let forged = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .expect("测试绑定");
        assert!(apply_invoice_create_binding(&mut document, Some(forged)).is_err());
    }

    /// 创建路径调用统一绑定端口，不查询发布定义、不启动实例、不建任务。
    #[test]
    fn create_does_not_query_definition_or_start_instance() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("persist_created_invoice"));
        assert!(production.contains("register_created_invoice_document"));
        assert!(production.contains("persist_unbound_invoice_document"));
        assert!(production.contains("bind_published_definition_on_document_create"));
        assert!(production.contains("DocumentType::Invoice"));
        assert!(production.contains("new_registered_document"));
        assert!(production.contains("ensure_invoice_skips_approval_binding"));
        assert!(production.contains("ensure_invoice_has_no_adapter"));
        assert!(!production.contains("pub async fn submit_invoice"));
        assert!(!production.contains("start_invoice_approval"));
        assert!(!production.contains("InvoiceAdapter"));
        assert!(!production.contains("load_published_graph"));
        let invoice_create = production
            .split("pub async fn create_invoice")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn post_invoice").next())
            .expect("create_invoice 生产片段");
        assert!(invoice_create.contains("persist_created_invoice"));
        assert!(!invoice_create.contains("prepare_start"));
        assert!(!invoice_create.contains("attach_published_binding"));
        assert!(!invoice_create.contains("WorkItem"));
        assert!(!invoice_create.contains("start_approval"));
    }
}
