//! 域 D18 `receivable` 服务编排（页面：W11 客户往来、W13 卡券票款复核）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 单集合草稿写入（回款/发票草稿、复核缓存更新）→ `&mut NoTransaction`；
//! - 跨集合资金/票款过账（§8.3 不变量）→
//!   `database::Transactional::with_transaction`，闭包内按稳定顺序锁定两侧，
//!   不执行外部 HTTP/文件 IO。
//! - 资金类入口（回款过账、发票登记、红冲）以业务唯一键
//!   （回款单号/规范化发票号码）与状态迁移构成去重机制，重复提交只产生一条
//!   正式事实。
//!
//! 跨域只经 `DatabaseExt` 调对方域 Repository：D13 `sales_orders()` 校验来源
//! 销售单存在；D18 拥有 `invoice` 实体与仓储，D19 经 `invoices()` 复用。

use database::{AccessControlExt, NoTransaction, ReceivableExt, SalesOrderExt, Transactional};
use entities::common::time::Instant;
use entities::ids::{
    CustomerReceiptId, InvoiceId, ReceiptAllocationId, ReceivableAccountId, ReceivableEntryId,
    ReceivableFundsReviewId, SalesInvoiceAllocationId, SalesOrderRevisionId,
};
use entities::money::Amount;
use entities::receivable::{
    AccountReviewStatus, AllocationAction, CustomerReceipt, CustomerReceiptData, CustomerReceiptStatus,
    EntryDirection, Invoice, InvoiceData, InvoiceKind, ReceiptAllocation, ReceiptAllocationData,
    ReceivableAccount, ReceivableAccountData, ReceivableEntry, ReceivableEntryData, ReceivableEntryType,
    ReceivableFundsReview, ReceivableFundsReviewData, SalesInvoiceAllocation, SalesInvoiceAllocationData,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use std::str::FromStr;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

use self::dto::SortDir;
pub use self::dto::{
    AppendFundsReviewRequest, CreateCustomerReceiptRequest, CreateInvoiceRequest,
    CreateReceivableAccountRequest, CustomerReceiptListParams, CustomerReceiptView, FundsReviewView,
    InvoiceListParams, InvoiceView, IssueRedInvoiceRequest, PageView, PostCustomerReceiptRequest,
    PostInvoiceRequest, ReceiptAllocationView, ReceivableAccountListParams, ReceivableAccountView,
    SalesInvoiceAllocationView, UpdateReceivableAccountReviewRequest,
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
        Self { db }
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

    /// 更新应收往来子账复核缓存（乐观锁语义）。
    ///
    /// 期望版本 `req.version` 与当前版本不一致时返回冲突（409）；
    /// 仓储层 `Repository::update` 同时以 `id + version` CAS 兜底并发竞争。
    ///
    /// # 参数
    /// * `id` - 应收往来子账 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后子账的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 子账不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn update_receivable_account_review(
        &self,
        id: &str,
        req: UpdateReceivableAccountReviewRequest,
        actor: &AuditActor,
    ) -> Result<ReceivableAccountView> {
        req.validate()?;
        let mut account = self
            .db
            .receivable_accounts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
        if account.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        account.update(
            entities::receivable::ReceivableAccountUpdate {
                review_status: Some(req.review_status),
                reviewed_by: Some(req.reviewed_by),
                reviewed_at: Some(req.reviewed_at),
                review_evidence_reference: Some(req.review_evidence_reference),
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "receivable_account.update_review",
            "receivable_account",
            account.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.receivable_accounts().update(&mut account, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.receivable_account_detail(id).await
    }

    /// 追加卡券票款正式复核（W13；复核链尾锁定 + 账户复核缓存同步）。
    ///
    /// 同事务写入 `receivable_funds_reviews`（`append_funds_review` 链尾锁定）
    /// 并刷新账户复核缓存，保证「复核链 + 缓存」原子可见（数据模型 §6.8）。
    ///
    /// # 参数
    /// * `req` - 复核追加请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新增复核记录视图。
    ///
    /// # 错误
    /// * `NotFound` - 往来子账不存在
    /// * `ConflictError` - 复核链尾被并发占用或复核号重复
    pub async fn append_funds_review(
        &self,
        req: AppendFundsReviewRequest,
        actor: &AuditActor,
    ) -> Result<crate::receivable::dto::FundsReviewView> {
        req.validate()?;
        self.db
            .receivable_accounts()
            .find_by_id(&req.receivable_account_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;

        let tail = self
            .db
            .receivable_funds_reviews()
            .find_reviews_by_account(&req.receivable_account_id, &mut NoTransaction)
            .await?;
        let review_no = tail.last().map_or(1, |last| last.review_no + 1);
        let review = ReceivableFundsReview::new(
            ReceivableFundsReviewId::new(next_id()),
            ReceivableFundsReviewData {
                receivable_account_id: req.receivable_account_id.clone(),
                review_no,
                review_type: req.review_type,
                work_item_id: req.work_item_id,
                evidence_document_id: None,
                evidence_reference: req.evidence_reference,
                review_result: req.review_result,
                reviewed_by: req.reviewed_by.clone(),
                reviewed_at: req.reviewed_at,
                supersedes_review_id: tail.last().map(|last| last.base.id.clone().into()),
            },
        )?;
        let cache_status = if review.review_result == entities::receivable::ReviewResult::Passed {
            AccountReviewStatus::Reviewed
        } else {
            AccountReviewStatus::NotApplicable
        };
        let (cache_by, cache_at, cache_evidence) = if cache_status == AccountReviewStatus::Reviewed {
            (
                Some(req.reviewed_by.clone()),
                Some(req.reviewed_at),
                Some(review.evidence_reference.clone().unwrap_or_default()),
            )
        } else {
            (Some(String::new()), None, Some(String::new()))
        };
        let mut account = self
            .db
            .receivable_accounts()
            .find_by_id(&req.receivable_account_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
        account.update(
            entities::receivable::ReceivableAccountUpdate {
                review_status: Some(cache_status),
                reviewed_by: cache_by,
                reviewed_at: cache_at,
                review_evidence_reference: cache_evidence,
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "receivable_funds_review.create",
            "receivable_funds_review",
            review.base.id.clone(),
        )?;
        let review_view = crate::receivable::dto::FundsReviewView {
            id: review.base.id.clone(),
            review_no: review.review_no,
            review_type: review.review_type,
            review_result: review.review_result,
            reviewed_by: review.reviewed_by.clone(),
            reviewed_at: review.reviewed_at,
            evidence_reference: review.evidence_reference.clone(),
        };

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.receivable().append_funds_review(&review, session).await?;
                    db.receivable_accounts().update(&mut account, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(review_view)
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

    /// 登记客户回款草稿（单集合写入，无事务）。
    ///
    /// 回款单号全局唯一（`uk_customer_receipts_no` 唯一索引）构成幂等去重：
    /// 重复登记同一单号落入 409，只产生一条正式事实。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建回款单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 回款单号重复
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
        let audit = actor.clone().resource_log(
            "customer_receipt.create",
            "customer_receipt",
            receipt.base.id.clone(),
        )?;
        self.db
            .customer_receipts()
            .create(&receipt, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;

        self.customer_receipt_detail(&receipt.base.id).await
    }

    /// 客户回款过账并核销（§8.3-1 事务不变量）。
    ///
    /// 同一事务内：校验回款与应收分录同一往来主体、分录开放余额与回款剩余
    /// 余额；写回款核销分配（`APPLY`）；按条件原子更新子账已核销进度
    /// （`apply_settlement` 不超额核销）；草稿回款迁移为已过账。
    /// 任一校验失败整体回滚，不存在只有分配没有进度或只有进度没有分配的
    /// 中间态。回款单号唯一 + 状态迁移构成重复提交去重。
    ///
    /// # 参数
    /// * `id` - 回款单 ID
    /// * `req` - 过账请求（核销分配行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后回款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 回款单或应收分录不存在
    /// * `BusinessLogicError` - 跨主体核销、超额核销或重复过账
    pub async fn post_customer_receipt(
        &self,
        id: &str,
        req: PostCustomerReceiptRequest,
        actor: &AuditActor,
    ) -> Result<CustomerReceiptView> {
        req.validate()?;
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
                    if receipt.status == CustomerReceiptStatus::PendingReview {
                        return Err(Error::BusinessLogicError("待复核回款必须先完成复核".to_string()));
                    }

                    let existing = db
                        .receipt_allocations()
                        .find_allocations_by_receipts(&[receipt.base.id.clone().into()], session)
                        .await?;
                    let net_allocated = net_receipt_allocated(&existing);
                    if net_allocated.checked_add(req_allocated_total(&req)) > receipt.amount {
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
                    let mut new_allocations = Vec::with_capacity(req.allocations.len());
                    for (index, line) in req.allocations.iter().enumerate() {
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
                    if receipt.status == CustomerReceiptStatus::Draft {
                        receipt.transition(CustomerReceiptStatus::Posted)?;
                    }
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

    /// 登记发票草稿（单集合写入，无事务）。
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
        let audit = actor
            .clone()
            .resource_log("invoice.create", "invoice", invoice.base.id.clone())?;
        self.db.invoices().create(&invoice, &mut NoTransaction).await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;

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
                    for (index, line) in req.allocations.iter().enumerate() {
                        let account = db
                            .receivable_accounts()
                            .find_by_id(&line.receivable_account_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
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
                    for (index, line) in req.allocations.iter().enumerate() {
                        let blue = existing
                            .iter()
                            .find(|allocation| allocation.base.id == line.reverses_allocation_id)
                            .ok_or_else(|| Error::NotFound("被红冲的蓝票分配不存在".to_string()))?;
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
        let account_id: ReceivableAccountId = account.base.id.clone().into();
        let entries = self
            .db
            .receivable_entries()
            .find_entries_by_account(&account_id, &mut NoTransaction)
            .await?;
        let offsets = entries
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
        let entry_views = entries
            .into_iter()
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
                    source_document_id: entry.source_document_id,
                    source_sequence: entry.source_sequence,
                    posted_at: entry.posted_at,
                    offset_total,
                }
            })
            .collect();
        let reviews = self
            .db
            .receivable_funds_reviews()
            .find_reviews_by_account(&account_id, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|review| crate::receivable::dto::FundsReviewView {
                id: review.base.id.clone(),
                review_no: review.review_no,
                review_type: review.review_type,
                review_result: review.review_result,
                reviewed_by: review.reviewed_by,
                reviewed_at: review.reviewed_at,
                evidence_reference: review.evidence_reference,
            })
            .collect();

        Ok(ReceivableAccountView {
            id: account.base.id.clone(),
            sales_order_id: account.sales_order_id.to_string(),
            account_seq: account.account_seq,
            customer_id: account.customer_id.to_string(),
            counterparty_party_id: account.counterparty_party_id.to_string(),
            review_status: account.review_status,
            gross_total: account.gross_total,
            settled_total: account.settled_total,
            open_total: account.open_total,
            invoiceable_total: account.invoiceable_total,
            invoiced_total: account.invoiced_total,
            open_invoiceable_total: account.open_invoiceable_total,
            status: account.stable.status(),
            version: account.base.version,
            created_at: account.base.created_at,
            entries: entry_views,
            reviews,
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
        let allocations = self
            .db
            .sales_invoice_allocations()
            .find_allocations_by_invoices(&[invoice.base.id.clone().into()], &mut NoTransaction)
            .await?;
        let (allocated_total, views) = sales_allocation_view(&allocations);
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

/// 汇总请求分配行金额。
///
/// # 参数
/// * `req` - 回款过账请求
///
/// # 返回
/// 返回请求内各分配行金额之和。
fn req_allocated_total(req: &PostCustomerReceiptRequest) -> Amount {
    req.allocations
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
