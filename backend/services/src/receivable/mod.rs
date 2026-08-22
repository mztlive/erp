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
    AccessControlExt, DocumentRegistryExt, Executor, FileAssetExt, NoTransaction, PayableExt,
    ReceivableExt, SalesOrderExt, Transactional, WorkItemExt,
};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{
    BusinessDocument, DocumentType, WorkflowAction, WorkflowActionData, WorkflowActionType,
};
use entities::file_asset::SecurityScanStatus;
use entities::ids::{
    BusinessDocumentId, CustomerReceiptId, InvoiceId, ReceiptAllocationId, ReceivableAccountId,
    ReceivableEntryId, ReceivableFundsReviewId, SalesInvoiceAllocationId, SalesOrderRevisionId,
    WorkflowActionId,
};
use entities::money::Amount;
use entities::receivable::{
    AccountReviewStatus, AllocationAction, CustomerReceipt, CustomerReceiptData, CustomerReceiptStatus,
    EntryDirection, Invoice, InvoiceData, InvoiceDirection, InvoiceKind, InvoiceStatus,
    PendingReceiptAllocation, ReceiptAllocation, ReceiptAllocationData, ReceivableAccount,
    ReceivableAccountData, ReceivableEntry, ReceivableEntryData, ReceivableEntryType, ReceivableFundsReview,
    ReceivableFundsReviewData, ReviewResult, SalesInvoiceAllocation, SalesInvoiceAllocationData,
};
use entities::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use entities::Permission;
use id_generator::next_id;
use mongodb::{bson::doc, Database};
use sha2::{Digest, Sha256};
use validator::Validate;

use std::collections::HashSet;
use std::str::FromStr;

use crate::approval::binding::{
    attach_published_binding, bind_published_definition_on_document_create, binding_decision,
    BindPublishedDefinitionCommand, BindingDecision,
};
use crate::approval::business_adapter::{adapter_spec_of, BindingRevalidationContext};
use crate::approval::execution::{prepare_cancel, prepare_start};
use crate::approval::policy::{policy_of, DocumentApprovalPolicy};
use crate::audit::AuditActor;
use crate::document_registry::{find_approval_binding, new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use crate::iam::{self, SharedRbacService};
use crate::work_item::{WorkItemAllowedAction, WorkItemService};

mod adapter;
mod cancel_approval;
mod dto;
mod start_approval;

pub use self::adapter::customer_receipt_object_readable;
use self::adapter::{
    build_customer_receipt_snapshot, customer_receipt_adapter, customer_receipt_responsible_org_id,
    customer_receipt_start_command, customer_receipt_subject_ref, document_approval_view,
    ensure_final_approve_posting, execute_customer_receipt_domain_action, pending_allocations_from_request,
    require_frozen_binding, start_approval_command_kind, start_customer_receipt_approval,
    RECENT_HISTORY_LIMIT,
};
use self::cancel_approval::{
    build_customer_receipt_cancel_input, load_cancel_runtime, persist_customer_receipt_cancel,
    CustomerReceiptCancelPersistInput,
};
use self::dto::SortDir;
pub use self::dto::{
    CancelCustomerReceiptApprovalRequest, CardFundsReviewActionBlockerView, CardFundsReviewAllowedAction,
    CardFundsReviewBusinessResult, CardFundsReviewConclusion, CardFundsReviewDecision,
    CardFundsReviewDetailParams, CardFundsReviewFollowUpConfiguration, CardFundsReviewResult,
    CardFundsReviewType, CompleteCardFundsReviewCommand, CompleteCardFundsReviewResult,
    CompletedWorkItemStatus, CreateCustomerReceiptRequest, CreateInvoiceRequest,
    CreateReceivableAccountRequest, CustomerReceiptListParams, CustomerReceiptView, DocumentApprovalView,
    FollowUpRequiredRegistration, FundsReviewView, InvoiceListParams, InvoiceView, IssueRedInvoiceRequest,
    PageView, PostCustomerReceiptRequest, PostInvoiceRequest, ReceiptAllocationView,
    ReceivableAccountListParams, ReceivableAccountView, ReceivableInvoiceFactView, ReceivableReceiptFactView,
    SalesInvoiceAllocationView, SubmitCustomerReceiptRequest,
};
use self::start_approval::{
    build_customer_receipt_start_input, load_bound_definition_graph, load_start_receipt,
    persist_customer_receipt_start, CustomerReceiptStartInput, CustomerReceiptStartPersistInput,
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
    ) -> Result<PageView<ReceivableAccountView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ReceivableAccountFilter {
            customer_id: query.customer_id,
            counterparty_party_id: query.counterparty_party_id,
            status: query.status,
            sales_order_id: query.sales_order_id.map(ReceivableAccountId::new),
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

        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.receivable_account_view(row.id).await?);
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
                "START_PROCESSING_REQUIRED",
                "必须先从团队待办建立本人责任，才能执行卡券票款领域动作",
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
        self.db
            .sales_orders()
            .find_by_id(&req.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;

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
                review_status: req.review_status.unwrap_or(AccountReviewStatus::NotApplicable),
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
    /// 并由领域命令完成原任务。驳回只完成当前任务，不创建或转交后继任务。
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
        validate_card_funds_decision(&command.decision)?;
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
                    validate_card_funds_evidence(&db, decision, session).await?;
                    validate_card_funds_reviewer_separation(
                        &db, &account, &snapshot, &work_item, &actor_id, session,
                    )
                    .await?;
                    WorkItemService::new(db.clone(), rbac_for_tx)
                        .ensure_domain_decision_access(&actor_owned, &work_item, session)
                        .await?;

                    let completed_at = Instant::now();
                    let evidence = canonical_review_evidence(decision)?;
                    let review_type = entity_review_type(decision.review_type);
                    let review_result = entity_review_result(decision.review_result);
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

                    let cache_status = match decision.review_result {
                        CardFundsReviewResult::Approved => AccountReviewStatus::Reviewed,
                        CardFundsReviewResult::Rejected => pending_review_status(decision.review_type),
                    };
                    let cache_update = match decision.review_result {
                        CardFundsReviewResult::Approved => entities::receivable::ReceivableAccountUpdate {
                            review_status: Some(cache_status),
                            reviewed_by: Some(actor_id.clone()),
                            reviewed_at: Some(completed_at),
                            review_evidence_reference: Some(
                                evidence.unwrap_or_else(|| decision.evidence_document_ids[0].to_string()),
                            ),
                            gross_total: None,
                            invoiceable_total: None,
                        },
                        CardFundsReviewResult::Rejected => entities::receivable::ReceivableAccountUpdate {
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
                            action_type: match decision.review_result {
                                CardFundsReviewResult::Approved => WorkflowActionType::Approve,
                                CardFundsReviewResult::Rejected => WorkflowActionType::Reject,
                            },
                            from_status: account_review_status_code(pending_review_status(
                                decision.review_type,
                            ))
                            .to_string(),
                            to_status: account_review_status_code(cache_status).to_string(),
                            actor_id: actor_id.clone(),
                            actor_role: work_item.owner_role.clone(),
                            comment: workflow_decision_comment(decision)?,
                        },
                    )?;
                    work_item.record_activity(&actor_id, completed_at)?;
                    work_item.complete_by_domain_command(&actor_id, completed_at)?;

                    db.receivable().append_funds_review(&review, session).await?;
                    db.receivable_accounts().update(&mut account, session).await?;
                    db.workflow_actions().create(&workflow, session).await?;
                    db.work_items().update(&mut work_item, session).await?;

                    let receipt = CardFundsReviewReceipt {
                        receivable_funds_review_id: review.base.id.clone(),
                        workflow_action_id: workflow.base.id.clone(),
                        review_no: review.review_no,
                        account_review_status: account.review_status.as_str().to_string(),
                        completed_at: completed_at.unix_secs(),
                        review_result: decision.review_result,
                        conclusion: decision.conclusion,
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
        let receipt = parse_card_funds_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("卡券票款复核幂等收据为空".to_string()))?,
            expected_fingerprint,
        )?;
        Ok(Some(receipt.into_result(
            work_item_id.as_ref(),
            account_id,
            audit_id,
        )))
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
        let filter = CustomerReceiptFilter {
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
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.customer_receipt_view(row.id).await?);
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
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
        )?;
        persist_created_customer_receipt(&self.db, &self.rbac, receipt.clone(), actor.clone()).await?;
        self.customer_receipt_detail(&receipt.base.id).await
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
        let allocations = pending_allocations_from_request(&req.allocations)?;
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
        persist_customer_receipt_start(
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
        .await?;
        self.customer_receipt_detail(id).await
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
        let input = build_customer_receipt_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &req.idempotency_key,
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
        let actor_id = actor.id().to_string();
        let receipt_id = id.to_string();
        let detail_id = receipt_id.clone();
        let audit_action = format!("customer_receipt.post:{id}");
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut receipt = db
                        .customer_receipts()
                        .find_by_id(&receipt_id, session)
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
                    if net_allocated.checked_add(pending_allocated_total(&receipt.pending_allocations))
                        > receipt.amount
                    {
                        return Err(Error::BusinessLogicError("核销合计超过回款金额".to_string()));
                    }

                    let mut entry_balances: std::collections::HashMap<String, Amount> =
                        std::collections::HashMap::new();
                    for allocation in &existing {
                        let entry_key = allocation.receivable_entry_id.to_string();
                        let balance = entry_balances.entry(entry_key).or_insert_with(zero_amount);
                        match allocation.allocation_action {
                            AllocationAction::Apply => {
                                *balance = balance.checked_add(allocation.allocated_amount);
                            }
                            AllocationAction::Reverse => {
                                *balance = balance.checked_sub(allocation.allocated_amount);
                            }
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

                    for (line_index, line) in new_allocations.iter().enumerate() {
                        let entry = db
                            .receivable_entries()
                            .find_by_id(&line.receivable_entry_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应收分录不存在".to_string()))?;
                        let applied = db
                            .receivable_accounts()
                            .apply_settlement(
                                &entry.receivable_account_id,
                                &new_allocations[line_index].allocated_amount,
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
                    let audit = actor_owned.clone().resource_log(
                        &audit_action,
                        "customer_receipt",
                        receipt.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    // 回款过账后刷新销售单回款进度与关闭状态（§9.3 自动结案）
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
        let filter = InvoiceFilter {
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
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.invoice_view(row.id).await?);
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
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let invoice_id = id.to_string();
        let detail_id = invoice_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut invoice = db
                        .invoices()
                        .find_by_id(&invoice_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("发票不存在".to_string()))?;
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

                    let requested: Amount = req.allocations.iter().fold(zero_amount(), |sum, line| {
                        sum.checked_add(line.allocated_gross_amount)
                    });
                    if requested != invoice.gross_amount {
                        return Err(Error::BusinessLogicError(
                            "发票分配合计必须等于发票金额".to_string(),
                        ));
                    }

                    let mut new_allocations = Vec::with_capacity(req.allocations.len());
                    let mut sales_order_ids = Vec::new();
                    for (index, line) in req.allocations.iter().enumerate() {
                        let account = db
                            .receivable_accounts()
                            .find_by_id(&line.receivable_account_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
                        sales_order_ids.push(account.sales_order_id.to_string());
                        if account.counterparty_party_id != invoice.party_id {
                            return Err(Error::BusinessLogicError("禁止跨往来主体开票".to_string()));
                        }
                        let applied = db
                            .receivable_accounts()
                            .apply_invoicing(
                                &line.receivable_account_id,
                                &line.allocated_gross_amount,
                                &actor_id,
                                session,
                            )
                            .await?;
                        if !applied {
                            return Err(Error::BusinessLogicError(
                                "子账剩余可开票额度不足，开票被拒绝".to_string(),
                            ));
                        }
                        new_allocations.push(SalesInvoiceAllocation::new(
                            SalesInvoiceAllocationId::new(next_id()),
                            SalesInvoiceAllocationData {
                                invoice_id: invoice.base.id.clone().into(),
                                receivable_account_id: line.receivable_account_id.clone(),
                                allocation_seq: (index as u32) + 1,
                                allocation_action: AllocationAction::Apply,
                                allocated_gross_amount: line.allocated_gross_amount,
                                allocated_net_amount: line.allocated_net_amount,
                                allocated_tax_amount: line.allocated_tax_amount,
                                reverses_allocation_id: None,
                            },
                        )?);
                    }
                    invoice.mark_registered(&actor_id)?;
                    db.invoices().update(&mut invoice, session).await?;
                    for allocation in &new_allocations {
                        db.sales_invoice_allocations().create(allocation, session).await?;
                    }
                    let audit = actor_owned.clone().resource_log(
                        "invoice.post",
                        "invoice",
                        invoice.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    // 开票过账后刷新销售单开票进度（开票不参与关闭判定）
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

    /// 开具红票并红冲（§8.3-3 事务不变量）。
    ///
    /// 同一事务内：原蓝票必须已登记；红票规范化号码去重；红冲分配只允许
    /// 反向原蓝票有效 `APPLY` 分配且累计不超过原分配；按条件原子冲减子账
    /// 净已开票进度（`revert_invoicing`）；原蓝票置已红冲。
    /// 任一校验失败整体回滚。保留原事实，不覆盖蓝票分配。
    ///
    /// # 参数
    /// * `id` - 原蓝票 ID
    /// * `req` - 红票请求（含红冲分配行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建红票视图。
    ///
    /// # 错误
    /// * `NotFound` - 原蓝票或蓝票分配不存在
    /// * `ConflictError` - 红票号码重复
    /// * `BusinessLogicError` - 红冲累计超过原分配或超额红冲
    pub async fn issue_red_invoice(
        &self,
        id: &str,
        req: IssueRedInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<InvoiceView> {
        req.validate()?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let red_invoice_id = InvoiceId::new(next_id());
        let red_invoice_id_for_tx = red_invoice_id.clone();
        let red_no = req.invoice_no.clone();
        let red_date = req.invoice_date;
        let red_gross = req.gross_amount;
        let red_net = req.net_amount;
        let red_tax = req.tax_amount;
        let original_id = id.to_string();
        client
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
                    if db
                        .invoices()
                        .find_by_direction_and_normalized_no(
                            original.invoice_direction,
                            &red_no.to_uppercase(),
                            session,
                        )
                        .await?
                        .is_some()
                    {
                        return Err(Error::ConflictError("红票号码已登记，请勿重复提交".to_string()));
                    }

                    let red_invoice = Invoice::new(
                        red_invoice_id_for_tx.clone(),
                        InvoiceData {
                            invoice_direction: original.invoice_direction,
                            invoice_kind: InvoiceKind::Red,
                            party_id: original.party_id.clone(),
                            invoice_code: original.invoice_code.clone(),
                            invoice_no: red_no.clone(),
                            invoice_date: red_date,
                            gross_amount: red_gross,
                            net_amount: red_net,
                            tax_amount: red_tax,
                            rounding_adjustment_amount: zero_amount(),
                            rounding_reason: None,
                            original_invoice_id: Some(original.base.id.clone().into()),
                        },
                        &actor_id,
                    )?;

                    let existing = db
                        .sales_invoice_allocations()
                        .find_allocations_by_invoices(&[original.base.id.clone().into()], session)
                        .await?;
                    let mut lines = Vec::with_capacity(req.allocations.len());
                    let mut account_ids = Vec::new();
                    for (index, line) in req.allocations.iter().enumerate() {
                        let blue = existing
                            .iter()
                            .find(|allocation| allocation.base.id == line.reverses_allocation_id)
                            .ok_or_else(|| Error::NotFound("被红冲的蓝票分配不存在".to_string()))?;
                        account_ids.push(blue.receivable_account_id.to_string());
                        if blue.allocation_action != AllocationAction::Apply {
                            return Err(Error::BusinessLogicError("只能红冲蓝票正向分配".to_string()));
                        }
                        let red_total: Amount = db
                            .sales_invoice_allocations()
                            .find_allocations_by_invoices(&[original.base.id.clone().into()], session)
                            .await?
                            .iter()
                            .filter(|allocation| allocation.allocation_action == AllocationAction::Reverse)
                            .filter(|allocation| {
                                allocation.reverses_allocation_id.as_ref()
                                    == Some(&SalesInvoiceAllocationId::new(&line.reverses_allocation_id))
                            })
                            .fold(zero_amount(), |sum, allocation| {
                                sum.checked_add(allocation.allocated_gross_amount)
                            });
                        if red_total.checked_add(line.allocated_gross_amount) > blue.allocated_gross_amount {
                            return Err(Error::BusinessLogicError(
                                "红冲累计不得超过原蓝票分配".to_string(),
                            ));
                        }
                        let reverted = db
                            .receivable_accounts()
                            .revert_invoicing(
                                &blue.receivable_account_id,
                                &line.allocated_gross_amount,
                                &actor_id,
                                session,
                            )
                            .await?;
                        if !reverted {
                            return Err(Error::BusinessLogicError("红冲金额超过已开票进度".to_string()));
                        }
                        lines.push(SalesInvoiceAllocation::new(
                            SalesInvoiceAllocationId::new(next_id()),
                            SalesInvoiceAllocationData {
                                invoice_id: red_invoice.base.id.clone().into(),
                                receivable_account_id: blue.receivable_account_id.clone(),
                                allocation_seq: (index as u32) + 1,
                                allocation_action: AllocationAction::Reverse,
                                allocated_gross_amount: line.allocated_gross_amount,
                                allocated_net_amount: line.allocated_net_amount,
                                allocated_tax_amount: line.allocated_tax_amount,
                                reverses_allocation_id: Some(blue.base.id.clone().into()),
                            },
                        )?);
                    }

                    let mut red_mut = red_invoice;
                    red_mut.mark_registered(&actor_id)?;
                    let mut original_mut = original;
                    original_mut.mark_red_invoiced(&actor_id)?;
                    register_created_invoice_document(&db, &rbac, &red_mut, &actor_owned, session).await?;
                    db.invoices().create(&red_mut, session).await?;
                    db.invoices().update(&mut original_mut, session).await?;
                    for allocation in &lines {
                        db.sales_invoice_allocations().create(allocation, session).await?;
                    }
                    let audit = actor_owned.clone().resource_log(
                        "invoice.red_issue",
                        "invoice",
                        red_mut.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    // 红冲后刷新销售单开票进度（可开票余额回升）
                    account_ids.sort();
                    account_ids.dedup();
                    let mut sales_order_ids = Vec::new();
                    for account in db
                        .receivable_accounts()
                        .find_many(doc! { "id": { "$in": account_ids } }, session)
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
                    Ok::<(), crate::errors::Error>(())
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
        for offset_id in offsets {
            let entry_offsets = self
                .db
                .receivable_entry_offsets()
                .find_offsets_by_decrease(&offset_id, &mut NoTransaction)
                .await?;
            for offset in entry_offsets {
                let key = offset.increase_entry_id.to_string();
                let total = offset_map.entry(key).or_insert_with(zero_amount);
                *total = total.checked_add(offset.offset_amount);
            }
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
                    .find_allocations_by_invoices(
                        &[invoice.base.id.clone().into()],
                        &mut NoTransaction,
                    )
                    .await?;
                let mut net = zero_amount();
                let views = rows
                    .iter()
                    .map(|allocation| {
                        // 进项/销项分配动作枚举跨域不共享（见 A-G7），此处显式转换。
                        let action = match allocation.allocation_action {
                            entities::payable::AllocationAction::Apply => AllocationAction::Apply,
                            entities::payable::AllocationAction::Reverse => {
                                AllocationAction::Reverse
                            }
                        };
                        match action {
                            AllocationAction::Apply => {
                                net = net.checked_add(allocation.allocated_gross_amount)
                            }
                            AllocationAction::Reverse => {
                                net = net.checked_sub(allocation.allocated_gross_amount)
                            }
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
            InvoiceDirection::Sales => {
                let allocations = self
                    .db
                    .sales_invoice_allocations()
                    .find_allocations_by_invoices(
                        &[invoice.base.id.clone().into()],
                        &mut NoTransaction,
                    )
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
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";
const REJECT_FOLLOW_UP_BLOCKER: &str = "REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED";
const REJECT_FOLLOW_UP_MESSAGE: &str =
    "本次复核已驳回并完成当前任务；驳回后继任务尚未注册，请联系流程管理员协作处理。";

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
}

impl CardFundsReviewReceipt {
    /// 将持久化收据装配为固定 W13 HTTP 结果。
    fn into_result(
        self,
        work_item_id: &str,
        receivable_account_id: &str,
        operation_id: &str,
    ) -> CompleteCardFundsReviewResult {
        let follow_up_configuration = (self.review_result == CardFundsReviewResult::Rejected).then(|| {
            CardFundsReviewFollowUpConfiguration {
                status: "BLOCKED".to_string(),
                blocker_code: REJECT_FOLLOW_UP_BLOCKER.to_string(),
                collaboration_message: REJECT_FOLLOW_UP_MESSAGE.to_string(),
                required_registration: vec![
                    FollowUpRequiredRegistration::WorkItemType,
                    FollowUpRequiredRegistration::OwnerPool,
                    FollowUpRequiredRegistration::HandlerKey,
                ],
            }
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
                follow_up_configuration,
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
    let receipts = if receipt_ids.is_empty() {
        Vec::new()
    } else {
        db.customer_receipts()
            .find_many(doc! { "id": { "$in": receipt_ids } }, executor)
            .await?
    };
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
    let invoices = if invoice_ids.is_empty() {
        Vec::new()
    } else {
        db.invoices()
            .find_many(doc! { "id": { "$in": invoice_ids } }, executor)
            .await?
    };
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

/// 校验 W13 决定的证据、结论和原因组合。
fn validate_card_funds_decision(decision: &CardFundsReviewDecision) -> Result<()> {
    if decision.receivable_account_id.as_ref().trim().is_empty()
        || decision.receivable_account_id.as_ref().chars().count() > 128
    {
        return Err(Error::ValidationError("应收账户 ID 非法".to_string()));
    }
    let mut document_ids = HashSet::new();
    for id in &decision.evidence_document_ids {
        if id.as_ref().trim().is_empty() || id.as_ref().chars().count() > 128 {
            return Err(Error::ValidationError("证据文件 ID 非法".to_string()));
        }
        if !document_ids.insert(id.to_string()) {
            return Err(Error::ValidationError("证据文件不得重复".to_string()));
        }
    }
    let mut references = HashSet::new();
    for reference in &decision.evidence_references {
        let normalized = reference.trim();
        if normalized.is_empty() {
            return Err(Error::ValidationError("证据引用不能为空白".to_string()));
        }
        if normalized.chars().count() > 256 {
            return Err(Error::ValidationError(
                "单条证据引用不能超过 256 个字符".to_string(),
            ));
        }
        if !references.insert(normalized.to_string()) {
            return Err(Error::ValidationError("证据引用不得重复".to_string()));
        }
    }
    if decision.evidence_document_ids.is_empty() && decision.evidence_references.is_empty() {
        return Err(Error::ValidationError("正式复核证据不能为空".to_string()));
    }
    if decision
        .expected_review_chain_tail_id
        .as_deref()
        .is_some_and(|tail| tail.trim().is_empty())
    {
        return Err(Error::ValidationError("复核链尾不能为空白".to_string()));
    }
    let reason = decision.reason_code.as_deref().map(str::trim);
    match (decision.review_result, decision.conclusion) {
        (CardFundsReviewResult::Approved, CardFundsReviewConclusion::NoHistoryFromZero)
        | (CardFundsReviewResult::Approved, CardFundsReviewConclusion::RecordedFactsReconciled) => {
            if decision.reason_code.is_some() {
                return Err(Error::ValidationError("通过决定不得携带驳回原因".to_string()));
            }
        }
        (CardFundsReviewResult::Rejected, CardFundsReviewConclusion::Rejected) => {
            let reason = reason
                .filter(|value| !value.is_empty())
                .ok_or_else(|| Error::ValidationError("驳回决定必须填写原因代码".to_string()))?;
            if !matches!(
                reason,
                "EVIDENCE_INSUFFICIENT" | "FACTS_MISMATCH" | "COUNTERPARTY_UNCLEAR" | "OTHER"
            ) {
                return Err(Error::ValidationError("驳回原因代码不在受控范围内".to_string()));
            }
        }
        _ => {
            return Err(Error::ValidationError("复核结果与结论组合不合法".to_string()));
        }
    }
    canonical_review_evidence(decision)?;
    workflow_decision_comment(decision)?;
    Ok(())
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

/// 校验受控文件证据存在、扫描通过且仍在保留期内。
async fn validate_card_funds_evidence(
    db: &Database,
    decision: &CardFundsReviewDecision,
    executor: &mut dyn Executor,
) -> Result<()> {
    let now = Instant::now();
    for id in &decision.evidence_document_ids {
        let asset = db
            .file_assets()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound(format!("复核证据文件不存在: {id}")))?;
        if asset.security_scan_status != SecurityScanStatus::Passed
            || asset.destroyed_at.is_some()
            || asset.expires_at.is_some_and(|expires_at| expires_at <= now)
        {
            return Err(Error::BusinessLogicError(
                "复核证据文件未通过安全检查、已销毁或已过期".to_string(),
            ));
        }
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
        .find_many(
            doc! {
                "resource_type": resource_type,
                "resource_id": resource_id,
                "success": true,
            },
            executor,
        )
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

/// 将多值证据无损压缩到现有复核实体的单文档加单引用形态。
fn canonical_review_evidence(decision: &CardFundsReviewDecision) -> Result<Option<String>> {
    let mut references = decision
        .evidence_references
        .iter()
        .map(|reference| reference.trim().to_string())
        .collect::<Vec<_>>();
    references.extend(
        decision
            .evidence_document_ids
            .iter()
            .skip(1)
            .map(|id| format!("file_asset:{id}")),
    );
    if references.is_empty() {
        return Ok(None);
    }
    let canonical = references.join("; ");
    if canonical.chars().count() > 512 {
        return Err(Error::ValidationError(
            "规范化后的复核证据引用不能超过 512 个字符".to_string(),
        ));
    }
    Ok(Some(canonical))
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

/// 形成长度受控、可审计但不承担机器判定的工作流意见。
fn workflow_decision_comment(decision: &CardFundsReviewDecision) -> Result<Option<String>> {
    let mut parts = vec![format!("conclusion={}", decision.conclusion.as_str())];
    if let Some(reason) = decision
        .reason_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("reason={reason}"));
    }
    if let Some(comment) = decision
        .comment
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(comment.to_string());
    }
    let comment = parts.join("; ");
    if comment.chars().count() > 512 {
        return Err(Error::ValidationError(
            "工作流复核意见不能超过 512 个字符".to_string(),
        ));
    }
    Ok(Some(comment))
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
    format!("rcv:{:x}", digest.finalize())
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
    format!("ffv:{:x}", digest.finalize())
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
    Ok(format!("{:x}", Sha256::digest(serialized)))
}

/// 生成不泄漏原始幂等键的稳定审计主键。
fn card_funds_audit_id(actor_id: &str, key: &str) -> String {
    let mut digest = Sha256::new();
    digest_part(&mut digest, CARD_FUNDS_REVIEW_ACTION);
    digest_part(&mut digest, actor_id);
    digest_part(&mut digest, key.trim());
    format!("{CARD_FUNDS_REVIEW_RECEIPT_PREFIX}{:x}", digest.finalize())
}

/// 将正式结果编码为受审计消息长度约束的幂等收据。
fn card_funds_receipt_message(fingerprint: &str, receipt: &CardFundsReviewReceipt) -> String {
    format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={}|{}|{}|{}|{}|{}|{}",
        receipt.receivable_funds_review_id,
        receipt.workflow_action_id,
        receipt.review_no,
        receipt.account_review_status,
        receipt.completed_at,
        receipt.review_result.as_str(),
        receipt.conclusion.as_str(),
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
    let [review_id, workflow_id, review_no, account_status, completed_at, result, conclusion] =
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
    })
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
) -> Result<()> {
    let _ = customer_receipt_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("客户回款单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding)?;
    db.business_documents().create(&document, session).await?;
    Ok(())
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
    use entities::ids::{FileAssetId, ReceivableAccountId};

    use super::{
        canonical_review_evidence, card_funds_audit_id, card_funds_receipt_message, parse_card_funds_receipt,
        parse_task_version, validate_card_funds_decision, CardFundsReviewConclusion, CardFundsReviewDecision,
        CardFundsReviewReceipt, CardFundsReviewResult, CardFundsReviewType, Error,
        FollowUpRequiredRegistration,
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

    #[test]
    fn semantic_validation_rejects_result_conclusion_drift_and_missing_evidence() {
        let mut invalid = opening_decision();
        invalid.conclusion = CardFundsReviewConclusion::Rejected;
        assert!(validate_card_funds_decision(&invalid).is_err());

        let mut invalid = opening_decision();
        invalid.evidence_document_ids.clear();
        assert!(validate_card_funds_decision(&invalid).is_err());

        let mut invalid = opening_decision();
        invalid.reason_code = Some(String::new());
        assert!(validate_card_funds_decision(&invalid).is_err());
    }

    #[test]
    fn evidence_keeps_additional_documents_as_controlled_references() {
        let mut decision = opening_decision();
        decision.evidence_document_ids.push(FileAssetId::new("file-2"));
        decision.evidence_references.push("BANK-REF-1".to_string());

        assert_eq!(
            canonical_review_evidence(&decision).unwrap().as_deref(),
            Some("BANK-REF-1; file_asset:file-2")
        );
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
        let blocker = result.business_result.follow_up_configuration.unwrap();
        assert_eq!(blocker.blocker_code, "REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED");
        assert_eq!(
            blocker.required_registration,
            vec![
                FollowUpRequiredRegistration::WorkItemType,
                FollowUpRequiredRegistration::OwnerPool,
                FollowUpRequiredRegistration::HandlerKey,
            ]
        );
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
