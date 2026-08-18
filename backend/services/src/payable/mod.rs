//! 域 D19 `payable` 服务编排（页面：W12 供应商往来）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 单集合草稿写入（付款单草稿）→ `&mut NoTransaction`；
//! - 跨集合资金/票款过账（§8.3 不变量）→
//!   `database::Transactional::with_transaction`。
//! - 资金类入口（付款过账、进项发票登记）以业务唯一键
//!   （付款单号/规范化发票号码）与状态迁移构成去重机制。
//!
//! 跨域只经 `DatabaseExt` 调对方域 Repository：
//! - D15 `purchase_orders()` 校验来源采购单存在；
//! - D09 `supplier_accounts()` 校验供应商存在并取 `party_id`（进项发票
//!   与应付子账的往来主体相等键）；
//! - D18 `invoices()` 复用发票仓储（`invoice` 由 D18 拥有实体与仓储，
//!   D19 只拥有 `purchase_invoice_allocation`，禁止复制发票实体）。

use std::collections::HashMap;
use std::str::FromStr;

use database::{
    AccessControlExt, NoTransaction, PayableExt, PurchaseOrderExt, ReceivableExt, SupplierExt, Transactional,
};
use entities::common::time::Instant;
use entities::ids::{
    InvoiceId, PayableAccountId, PayableEntryId, PaymentAllocationId, PurchaseInvoiceAllocationId,
    SupplierPaymentId,
};
use entities::money::Amount;
use entities::payable::{
    AllocationAction, EntryDirection, PayableAccount, PayableAccountData, PayableEntry, PayableEntryData,
    PayableEntryType, PaymentAllocation, PaymentAllocationData, PurchaseInvoiceAllocation,
    PurchaseInvoiceAllocationData, SupplierPayment, SupplierPaymentData, SupplierPaymentStatus,
};
use entities::receivable::{Invoice, InvoiceData, InvoiceDirection, InvoiceKind};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use entities::document_registry::DocumentType;

mod dto;

use self::dto::SortDir;
pub use self::dto::{
    CreatePayableAccountRequest, CreateSupplierPaymentRequest, PageView, PayableAccountListParams,
    PayableAccountView, PaymentAllocationLineRequest, PaymentAllocationView, PostSupplierPaymentRequest,
    PurchaseInvoiceAllocationListParams, PurchaseInvoiceAllocationView, PurchaseInvoiceRegisteredView,
    RegisterPurchaseInvoiceRequest, SupplierPaymentListParams, SupplierPaymentView,
};

/// 应付往来子账列表筛选条件类型（经 `PayableExt` 关联类型跨 crate 可达）。
type PayableAccountFilter = <mongodb::Database as PayableExt>::PayableAccountFilter;
/// 供应商付款单列表筛选条件类型。
type SupplierPaymentFilter = <mongodb::Database as PayableExt>::SupplierPaymentFilter;

/// 供应商往来服务。
///
/// 提供应付台账、付款单与进项发票登记编排。
pub struct PayableService {
    db: Database,
}

impl PayableService {
    /// 创建供应商往来服务实例。
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
    // 应付往来子账
    // -----------------------------------------------------------------------

    /// 分页查询应付往来子账列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`supplier_id`/`source_type`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn payable_account_list(
        &self,
        params: &PayableAccountListParams,
    ) -> Result<PageView<PayableAccountView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = PayableAccountFilter {
            supplier_id: query.supplier_id,
            source_type: query.source_type,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .payable_accounts()
            .search_payable_accounts(&filter, &mut NoTransaction)
            .await?;
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.payable_account_view(row.id).await?);
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询应付往来子账详情（子账 + 分录）。
    ///
    /// # 参数
    /// * `id` - 应付往来子账 ID
    ///
    /// # 返回
    /// 返回完整应付台账视图。
    ///
    /// # 错误
    /// * `NotFound` - 子账不存在
    pub async fn payable_account_detail(&self, id: &str) -> Result<PayableAccountView> {
        self.payable_account_view(id.to_string()).await
    }

    /// 建立应付往来子账与原始应付分录（跨集合事务写入）。
    ///
    /// 校验来源单据存在（D15 `purchase_orders()`）；同事务写入子账与分录，
    /// 保证「子账 + 原始应付」原子可见（数据模型 §6.9）。业务幂等唯一
    /// `(payable_account_id, source_fact_type, source_document_id,
    /// source_revision_id, entry_type, source_sequence)` 由唯一索引保证。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建子账的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源采购单不存在
    /// * `ConflictError` - 业务唯一键重复
    pub async fn create_payable_account(
        &self,
        req: CreatePayableAccountRequest,
        actor: &AuditActor,
    ) -> Result<PayableAccountView> {
        req.validate()?;
        self.db
            .purchase_orders()
            .find_by_id(&req.source_document_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;

        let account_id = PayableAccountId::new(next_id());
        let entry_id = PayableEntryId::new(next_id());
        let account = PayableAccount::new(
            account_id.clone(),
            PayableAccountData {
                source_document_id: req.source_document_id.clone(),
                supplier_id: req.supplier_id.clone(),
                source_type: req.source_type,
                gross_total: req.gross_total,
                settled_total: zero_amount(),
                invoiceable_total: req.invoiceable_total.unwrap_or(req.gross_total),
                invoiced_total: zero_amount(),
            },
            actor.id(),
        )?;
        let entry = PayableEntry::new(
            entry_id,
            PayableEntryData {
                payable_account_id: account_id.clone(),
                entry_type: PayableEntryType::Original,
                direction: EntryDirection::Increase,
                amount: account.gross_total,
                due_date: req.due_date,
                source_fact_type: "purchase_order".to_string(),
                source_document_id: req.source_document_id,
                source_revision_id: req.source_revision_id,
                source_sequence: req.source_sequence,
                posted_at: Instant::now(),
            },
        )?;
        let audit = actor.clone().resource_log(
            "payable_account.create",
            "payable_account",
            account_id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.payable()
                        .create_payable_with_entry(&account, &entry, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.payable_account_detail(&account_id).await
    }

    // -----------------------------------------------------------------------
    // 供应商付款单
    // -----------------------------------------------------------------------

    /// 分页查询供应商付款单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`payment_no`/`supplier_id`/`status`）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn supplier_payment_list(
        &self,
        params: &SupplierPaymentListParams,
    ) -> Result<PageView<SupplierPaymentView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SupplierPaymentFilter {
            payment_no: query.payment_no,
            supplier_id: query.supplier_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_payments()
            .search_supplier_payments(&filter, &mut NoTransaction)
            .await?;
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.supplier_payment_view(row.id).await?);
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询供应商付款单详情（含核销分配行）。
    ///
    /// # 参数
    /// * `id` - 付款单 ID
    ///
    /// # 返回
    /// 返回付款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 付款单不存在
    pub async fn supplier_payment_detail(&self, id: &str) -> Result<SupplierPaymentView> {
        self.supplier_payment_view(id.to_string()).await
    }

    /// 登记供应商付款草稿（单集合写入，无事务）。
    ///
    /// 付款单号全局唯一（唯一索引）构成幂等去重：重复登记同一单号落入 409。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建付款单视图。
    ///
    /// # 错误
    /// * `ConflictError` - 付款单号重复
    pub async fn create_supplier_payment(
        &self,
        req: CreateSupplierPaymentRequest,
        actor: &AuditActor,
    ) -> Result<SupplierPaymentView> {
        req.validate()?;
        let payment = SupplierPayment::new(
            SupplierPaymentId::new(next_id()),
            SupplierPaymentData {
                payment_no: req.payment_no,
                supplier_id: req.supplier_id,
                paid_at: req.paid_at,
                amount: req.amount,
                bank_reference: req.bank_reference,
            },
        )?;
        let audit = actor.clone().resource_log(
            "supplier_payment.create",
            "supplier_payment",
            payment.base.id.clone(),
        )?;
        let document = new_registered_document(
            &payment.base.id,
            DocumentType::SupplierPayment,
            payment.payment_no.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let payment_for_tx = payment.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_payments().create(&payment_for_tx, session).await?;
                    persist_registered_document(&db, &document, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.supplier_payment_detail(&payment.base.id).await
    }

    /// 供应商付款过账并核销（§8.3-1 事务不变量）。
    ///
    /// 同一事务内：校验付款与应付分录同一供应商、分录开放余额与付款剩余
    /// 余额；写付款核销分配（`APPLY`）；按条件原子更新子账已核销进度
    /// （`apply_settlement` 不超额核销）；草稿付款迁移为已过账。
    /// 任一校验失败整体回滚。付款单号唯一 + 状态迁移构成重复提交去重。
    ///
    /// # 参数
    /// * `id` - 付款单 ID
    /// * `req` - 过账请求（核销分配行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后付款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 付款单或应付分录不存在
    /// * `BusinessLogicError` - 跨供应商核销、超额核销或重复过账
    pub async fn post_supplier_payment(
        &self,
        id: &str,
        req: PostSupplierPaymentRequest,
        actor: &AuditActor,
    ) -> Result<SupplierPaymentView> {
        req.validate()?;
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let payment_id = id.to_string();
        let detail_id = payment_id.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut payment = db
                        .supplier_payments()
                        .find_by_id(&payment_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商付款单不存在".to_string()))?;
                    if payment.status == SupplierPaymentStatus::Reversed {
                        return Err(Error::BusinessLogicError("已冲正付款不能再核销".to_string()));
                    }
                    if payment.status == SupplierPaymentStatus::PendingReview {
                        return Err(Error::BusinessLogicError("待复核付款必须先完成复核".to_string()));
                    }

                    let existing = db
                        .payment_allocations()
                        .find_allocations_by_payments(&[payment.base.id.clone().into()], session)
                        .await?;
                    let net_allocated = net_payment_allocated(&existing);
                    if net_allocated.checked_add(req_allocated_total(&req)) > payment.amount {
                        return Err(Error::BusinessLogicError("核销合计超过付款金额".to_string()));
                    }

                    let mut entry_balances: HashMap<String, Amount> = HashMap::new();
                    for allocation in &existing {
                        let entry_key = allocation.payable_entry_id.to_string();
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
                            .payable_entries()
                            .find_by_id(&line.payable_entry_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
                        let account = db
                            .payable_accounts()
                            .find_by_id(&entry.payable_account_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
                        if account.supplier_id != payment.supplier_id {
                            return Err(Error::BusinessLogicError("禁止跨供应商核销".to_string()));
                        }
                        let allocated = entry_balances
                            .entry(entry.base.id.clone())
                            .or_insert_with(zero_amount);
                        if allocated.checked_add(line.allocated_amount) > entry.amount {
                            return Err(Error::BusinessLogicError(
                                "核销金额超过应付分录开放余额".to_string(),
                            ));
                        }
                        *allocated = allocated.checked_add(line.allocated_amount);

                        new_allocations.push(PaymentAllocation::new(
                            PaymentAllocationId::new(next_id()),
                            PaymentAllocationData {
                                supplier_payment_id: payment.base.id.clone().into(),
                                payable_entry_id: line.payable_entry_id.clone(),
                                allocation_seq: next_seq + index as u32,
                                allocation_action: AllocationAction::Apply,
                                allocated_amount: line.allocated_amount,
                                allocated_at: Instant::now(),
                                reverses_allocation_id: None,
                            },
                        )?);
                    }

                    for line in &new_allocations {
                        let entry = db
                            .payable_entries()
                            .find_by_id(&line.payable_entry_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
                        let applied = db
                            .payable_accounts()
                            .apply_settlement(
                                &entry.payable_account_id,
                                &line.allocated_amount,
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
                    if payment.status == SupplierPaymentStatus::Draft {
                        payment.transition(SupplierPaymentStatus::Posted)?;
                    }
                    db.supplier_payments().update(&mut payment, session).await?;
                    for allocation in &new_allocations {
                        db.payment_allocations().create(allocation, session).await?;
                    }
                    let audit = actor_owned.clone().resource_log(
                        "supplier_payment.post",
                        "supplier_payment",
                        payment.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        self.supplier_payment_detail(&detail_id).await
    }

    // -----------------------------------------------------------------------
    // 进项发票登记与分配
    // -----------------------------------------------------------------------

    /// 进项发票登记过账并分配（§8.3-2 事务不变量）。
    ///
    /// 发票实体经 D18 `invoices()` 仓储写入（D19 不复制发票实体）；同一事务内：
    /// 规范化号码去重；校验发票往来主体（供应商 `party_id`）与应付子账
    /// 供应商一致；分配合计等于发票金额；写进项发票分配；按条件原子更新
    /// 应付子账净已收票进度（`apply_invoicing` 不超额收票）；发票迁移为已登记。
    /// 规范化发票号码唯一构成重复提交去重。
    ///
    /// # 参数
    /// * `req` - 进项发票登记请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回登记后发票与分配行视图。
    ///
    /// # 错误
    /// * `NotFound` - 供应商或应付子账不存在
    /// * `ConflictError` - 规范化号码已登记
    /// * `BusinessLogicError` - 跨主体收票、分配合计不等或超额收票
    pub async fn register_purchase_invoice(
        &self,
        req: RegisterPurchaseInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseInvoiceRegisteredView> {
        req.validate()?;
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(&req.supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let party_id = supplier.party_id.clone();

        let invoice_id = InvoiceId::new(next_id());
        let invoice = Invoice::new(
            invoice_id.clone(),
            InvoiceData {
                invoice_direction: InvoiceDirection::Purchase,
                invoice_kind: InvoiceKind::Blue,
                party_id: party_id.clone(),
                invoice_code: req.invoice_code.clone(),
                invoice_no: req.invoice_no.clone(),
                invoice_date: req.invoice_date,
                gross_amount: req.gross_amount,
                net_amount: req.net_amount,
                tax_amount: req.tax_amount,
                rounding_adjustment_amount: zero_amount(),
                rounding_reason: None,
                original_invoice_id: None,
            },
            actor.id(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let invoice_for_tx = invoice.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if db
                        .invoices()
                        .find_by_direction_and_normalized_no(
                            InvoiceDirection::Purchase,
                            &invoice_for_tx.normalized_no,
                            session,
                        )
                        .await?
                        .is_some()
                    {
                        return Err(Error::ConflictError("发票号码已登记，请勿重复提交".to_string()));
                    }

                    let requested: Amount = req.allocations.iter().fold(zero_amount(), |sum, line| {
                        sum.checked_add(line.allocated_gross_amount)
                    });
                    if requested != invoice_for_tx.gross_amount {
                        return Err(Error::BusinessLogicError(
                            "发票分配合计必须等于发票金额".to_string(),
                        ));
                    }

                    let mut new_allocations = Vec::with_capacity(req.allocations.len());
                    for (index, line) in req.allocations.iter().enumerate() {
                        let account = db
                            .payable_accounts()
                            .find_by_id(&line.payable_account_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
                        let account_supplier = db
                            .supplier_accounts()
                            .find_by_id(&account.supplier_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应付子账供应商不存在".to_string()))?;
                        if account_supplier.party_id != party_id {
                            return Err(Error::BusinessLogicError("禁止跨供应商收票".to_string()));
                        }
                        let applied = db
                            .payable_accounts()
                            .apply_invoicing(
                                &line.payable_account_id,
                                &line.allocated_gross_amount,
                                &actor_id,
                                session,
                            )
                            .await?;
                        if !applied {
                            return Err(Error::BusinessLogicError(
                                "子账剩余可收票额度不足，收票被拒绝".to_string(),
                            ));
                        }
                        new_allocations.push(PurchaseInvoiceAllocation::new(
                            PurchaseInvoiceAllocationId::new(next_id()),
                            PurchaseInvoiceAllocationData {
                                invoice_id: invoice_for_tx.base.id.clone().into(),
                                payable_account_id: line.payable_account_id.clone(),
                                allocation_seq: (index as u32) + 1,
                                allocation_action: AllocationAction::Apply,
                                allocated_gross_amount: line.allocated_gross_amount,
                                allocated_net_amount: line.allocated_net_amount,
                                allocated_tax_amount: line.allocated_tax_amount,
                                reverses_allocation_id: None,
                            },
                        )?);
                    }
                    let mut invoice_mut = invoice_for_tx;
                    invoice_mut.mark_registered(&actor_id)?;
                    db.invoices().create(&invoice_mut, session).await?;
                    for allocation in &new_allocations {
                        db.purchase_invoice_allocations()
                            .create(allocation, session)
                            .await?;
                    }
                    let audit = actor_owned.clone().resource_log(
                        "purchase_invoice_allocation.post",
                        "purchase_invoice_allocation",
                        invoice_mut.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        let allocations = self
            .db
            .purchase_invoice_allocations()
            .find_allocations_by_invoices(std::slice::from_ref(&invoice_id), &mut NoTransaction)
            .await?;
        let views = allocations.iter().map(purchase_invoice_allocation_view).collect();
        Ok(PurchaseInvoiceRegisteredView {
            invoice_id: invoice_id.to_string(),
            invoice_no: invoice.invoice_no,
            gross_amount: invoice.gross_amount,
            allocations: views,
        })
    }

    /// 分页查询进项发票分配列表（按应付子账筛选）。
    ///
    /// 仓储冻结集未提供该组合的投影分页查询（`repository/payable.rs` 只提供
    /// `find_allocations_by_accounts`），此处按既有取回结果做内存分页，排序固定
    /// `created_at` 降序（分配行过账后不可更新，顺序稳定）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn purchase_invoice_allocation_list(
        &self,
        params: &PurchaseInvoiceAllocationListParams,
    ) -> Result<PageView<PurchaseInvoiceAllocationView>> {
        params.validate()?;
        let query = params.normalized()?;
        let mut allocations = match &query.payable_account_id {
            Some(account_id) => {
                self.db
                    .purchase_invoice_allocations()
                    .find_allocations_by_accounts(std::slice::from_ref(account_id), &mut NoTransaction)
                    .await?
            }
            None => {
                return Err(Error::ValidationError(
                    "按应付子账筛选进项发票分配为必填条件".to_string(),
                ))
            }
        };
        allocations.sort_by_key(|allocation| std::cmp::Reverse(allocation.base.created_at));
        let total = allocations.len() as i64;
        let start = (query.paging.page.saturating_sub(1)) as usize * query.paging.page_size as usize;
        let items = allocations
            .into_iter()
            .skip(start)
            .take(query.paging.page_size as usize)
            .map(|allocation| purchase_invoice_allocation_view(&allocation))
            .collect();
        Ok(PageView {
            items,
            total,
            page: query.paging.page,
            page_size: query.paging.page_size,
        })
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配应付往来子账详情视图。
    ///
    /// # 参数
    /// * `id` - 子账 ID
    ///
    /// # 返回
    /// 返回完整应付台账视图。
    ///
    /// # 错误
    /// * `NotFound` - 子账不存在
    async fn payable_account_view(&self, id: String) -> Result<PayableAccountView> {
        let account = self
            .db
            .payable_accounts()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
        let entries = self
            .db
            .payable_entries()
            .find_entries_by_account(&account.base.id.clone().into(), &mut NoTransaction)
            .await?
            .into_iter()
            .map(|entry| crate::payable::dto::PayableEntryView {
                id: entry.base.id.clone(),
                entry_type: entry.entry_type,
                direction: entry.direction,
                amount: entry.amount,
                due_date: entry.due_date,
                source_document_id: entry.source_document_id,
                source_sequence: entry.source_sequence,
                posted_at: entry.posted_at,
            })
            .collect();
        Ok(PayableAccountView {
            id: account.base.id.clone(),
            source_document_id: account.source_document_id,
            supplier_id: account.supplier_id.to_string(),
            source_type: account.source_type,
            gross_total: account.gross_total,
            settled_total: account.settled_total,
            open_total: account.open_total,
            invoiceable_total: account.invoiceable_total,
            invoiced_total: account.invoiced_total,
            open_invoiceable_total: account.open_invoiceable_total,
            status: account.stable.status(),
            version: account.base.version,
            created_at: account.base.created_at,
            entries,
        })
    }

    /// 装配供应商付款单视图。
    ///
    /// # 参数
    /// * `id` - 付款单 ID
    ///
    /// # 返回
    /// 返回付款单视图（含分配行与未分配余额）。
    ///
    /// # 错误
    /// * `NotFound` - 付款单不存在
    async fn supplier_payment_view(&self, id: String) -> Result<SupplierPaymentView> {
        let payment = self
            .db
            .supplier_payments()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商付款单不存在".to_string()))?;
        let allocations = self
            .db
            .payment_allocations()
            .find_allocations_by_payments(&[payment.base.id.clone().into()], &mut NoTransaction)
            .await?;
        let (allocated_total, views) = payment_allocation_view(&allocations);
        Ok(SupplierPaymentView {
            id: payment.base.id.clone(),
            payment_no: payment.payment_no,
            status: payment.status,
            supplier_id: payment.supplier_id.to_string(),
            paid_at: payment.paid_at,
            amount: payment.amount,
            bank_reference: payment.bank_reference,
            version: payment.base.version,
            created_at: payment.base.created_at,
            unallocated_amount: payment.amount.checked_sub(allocated_total),
            allocated_total,
            allocations: views,
        })
    }
}

/// 返回固定零金额。
///
/// # 返回
/// 返回金额 `0.00`。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}

/// 汇总请求分配行金额。
///
/// # 参数
/// * `req` - 付款过账请求
///
/// # 返回
/// 返回请求内各分配行金额之和。
fn req_allocated_total(req: &PostSupplierPaymentRequest) -> Amount {
    req.allocations
        .iter()
        .fold(zero_amount(), |sum, line| sum.checked_add(line.allocated_amount))
}

/// 计算付款单净已核销合计（`APPLY` 加、`REVERSE` 减）。
///
/// # 参数
/// * `allocations` - 既有核销分配
///
/// # 返回
/// 返回净已核销金额。
fn net_payment_allocated(allocations: &[PaymentAllocation]) -> Amount {
    allocations
        .iter()
        .fold(zero_amount(), |sum, line| match line.allocation_action {
            AllocationAction::Apply => sum.checked_add(line.allocated_amount),
            AllocationAction::Reverse => sum.checked_sub(line.allocated_amount),
        })
}

/// 汇总付款核销分配并装配视图。
///
/// # 参数
/// * `allocations` - 付款核销分配集合
///
/// # 返回
/// 返回 `(净已核销合计, 分配视图列表)`。
fn payment_allocation_view(
    allocations: &[PaymentAllocation],
) -> (Amount, Vec<crate::payable::dto::PaymentAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .iter()
        .map(|allocation| {
            match allocation.allocation_action {
                AllocationAction::Apply => net = net.checked_add(allocation.allocated_amount),
                AllocationAction::Reverse => net = net.checked_sub(allocation.allocated_amount),
            }
            allocation.into()
        })
        .collect();
    (net, views)
}

/// 装配进项发票分配视图。
///
/// # 参数
/// * `allocation` - 进项发票分配实体
///
/// # 返回
/// 返回响应视图。
fn purchase_invoice_allocation_view(
    allocation: &PurchaseInvoiceAllocation,
) -> crate::payable::dto::PurchaseInvoiceAllocationView {
    crate::payable::dto::PurchaseInvoiceAllocationView {
        id: allocation.base.id.clone(),
        invoice_id: allocation.invoice_id.to_string(),
        allocation_seq: allocation.allocation_seq,
        allocation_action: allocation.allocation_action,
        payable_account_id: allocation.payable_account_id.to_string(),
        allocated_gross_amount: allocation.allocated_gross_amount,
        allocated_net_amount: allocation.allocated_net_amount,
        allocated_tax_amount: allocation.allocated_tax_amount,
        reverses_allocation_id: allocation
            .reverses_allocation_id
            .as_ref()
            .map(|id| id.to_string()),
    }
}
