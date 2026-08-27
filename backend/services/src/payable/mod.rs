//! 域 D19 `payable` 服务编排（页面：W12 供应商往来）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 供应商付款必须在付款执行事务注册无审批 `BusinessDocument`；
//! - 跨集合资金/票款过账（§8.3 不变量）→
//!   `database::Transactional::with_transaction`。
//! - 资金类入口（付款过账、进项发票登记）以业务唯一键
//!   （付款单号/规范化发票号码）与状态迁移构成去重机制。
//!   采购审批形成付款授权，付款任务由当前责任出纳直接登记并过账。
//!
//! 跨域只经 `DatabaseExt` 调对方域 Repository：
//! - D15 `purchase_orders()` 校验来源采购单存在；
//! - D09 `supplier_accounts()` 校验供应商存在并取 `party_id`（进项发票
//!   与应付子账的往来主体相等键）；
//! - D18 `invoices()` 复用发票仓储（`invoice` 由 D18 拥有实体与仓储，
//!   D19 只拥有 `purchase_invoice_allocation`，禁止复制发票实体）。

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use database::{
    AccessControlExt, Executor, FileAssetExt, NoTransaction, PartyExt, PayableExt, PurchaseOrderExt,
    ReceivableExt, SupplierExt, Transactional,
};
use entities::common::time::{BusinessDate, Instant};
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::file_asset::{RetentionClass, SensitivityClass};
use entities::ids::{
    FileAssetId, InvoiceId, PartyBankAccountId, PartyId, PayableAccountId, PayableEntryId,
    PaymentAllocationId, PurchaseInvoiceAllocationId, SupplierAccountId, SupplierPaymentId,
};
use entities::money::Amount;
use entities::party::PartyBankAccount;
use entities::payable::{
    AllocationAction, EntryDirection, PayableAccount, PayableAccountData, PayableEntry, PayableEntryData,
    PayableEntryType, PaymentAllocation, PaymentAllocationData, PendingPaymentAllocation,
    PurchaseInvoiceAllocation, PurchaseInvoiceAllocationData, SupplierPayment, SupplierPaymentData,
    SupplierPaymentStatus,
};
use entities::receivable::{Invoice, InvoiceData, InvoiceDirection, InvoiceKind};
use id_generator::next_id;
use mongodb::{ClientSession, Database};
use validator::Validate;

use crate::approval::binding::{
    bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::audit::{AuditActor, CommandReceipt};
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use crate::file_asset::{FileAssetView, PendingFileAssetRequest};
use crate::iam::{self, SharedRbacService};
use crate::pending_file_assets::PendingFileAssets;

mod dto;
pub(crate) mod payment_task;
use self::dto::SortDir;
pub use self::dto::{
    CommitSupplierPaymentRequest, CreatePayableAccountRequest, CreateSupplierPaymentRequest, PageView,
    PayableAccountListParams, PayableAccountView, PaymentAllocationLineRequest, PaymentAllocationView,
    PaymentRecipientRevealView, PaymentRecipientView, PurchaseInvoiceAllocationListParams,
    PurchaseInvoiceAllocationView, PurchaseInvoiceRegisteredView, RegisterPurchaseInvoiceRequest,
    RevealPaymentRecipientRequest, SupplierPaymentBankReceiptView, SupplierPaymentListParams,
    SupplierPaymentView,
};
use crate::party::SensitiveDataCodec;

/// 应付往来子账列表筛选条件类型（经 `PayableExt` 关联类型跨 crate 可达）。
type PayableAccountFilter = <mongodb::Database as PayableExt>::PayableAccountFilter;
/// 供应商付款单列表筛选条件类型。
type SupplierPaymentFilter = <mongodb::Database as PayableExt>::SupplierPaymentFilter;

/// 供应商往来服务。
///
/// 提供应付台账、付款单与进项发票登记编排。
pub struct PayableService {
    db: Database,
    rbac: SharedRbacService,
}

/// 携带银行回单文件资产的付款提交结果。
pub struct SupplierPaymentWithAssetsResult {
    /// 稳定付款单结果。
    pub view: SupplierPaymentView,
    /// 本次上传对象是否已随业务事务登记；幂等重放时为 `false`。
    pub assets_committed: bool,
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
        let rbac = iam::shared_rbac_service(db.clone());
        Self { db, rbac }
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
            views.push(self.payable_account_view(row.id, false).await?);
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
        self.payable_account_view(id.to_string(), true).await
    }

    /// 在付款任务责任校验后揭示当前默认收款账号。
    ///
    /// 查看不修改任务版本；页面可在核对账号后继续使用同一任务版本提交付款。
    /// 每次成功揭示均写入敏感信息审计。
    ///
    /// # 参数
    /// * `id` - 当前付款任务绑定的应付往来子账 ID
    /// * `req` - 任务身份、任务版本与页面所见收款账户
    /// * `actor` - 当前操作人
    /// * `sensitive_data` - 应用启动期共享的敏感数据编解码器
    ///
    /// # 返回
    /// 返回完整收款账号和对应账户事实行主键。
    ///
    /// # 错误
    /// 任务责任、版本、账户身份或敏感密文不合法时失败关闭。
    pub async fn reveal_payment_recipient(
        &self,
        id: &str,
        req: RevealPaymentRecipientRequest,
        actor: &AuditActor,
        sensitive_data: &SensitiveDataCodec,
    ) -> Result<PaymentRecipientRevealView> {
        req.validate()?;
        let expected_task_version = crate::work_item::expected_task_version(&req.expected_task_version)?;
        let account_id = PayableAccountId::new(id);
        let (_, account) = payment_task::authorize_payment_execution(
            &self.db,
            &req.work_item_id,
            expected_task_version,
            Some(&account_id),
            actor,
            &mut NoTransaction,
        )
        .await?;
        let recipient =
            resolve_current_payment_recipient(&self.db, &account.supplier_id, &mut NoTransaction).await?;
        ensure_expected_payment_recipient(
            &recipient.base.id,
            recipient.base.version,
            &PartyBankAccountId::new(req.expected_bank_account_id.trim()),
            req.expected_bank_account_version,
        )?;
        let account_number = sensitive_data.decrypt(&recipient.account_number_ciphertext)?;
        let audit = actor.clone().resource_log(
            "party_bank_account.reveal_for_payment",
            "party_bank_account",
            recipient.base.id.clone(),
        )?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(PaymentRecipientRevealView {
            bank_account_id: recipient.base.id,
            account_number,
        })
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
            views.push(self.supplier_payment_view(row.id, false).await?);
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
        self.supplier_payment_view(id.to_string(), true).await
    }

    /// 读取付款单归属的银行回单元数据，并记录受控预览审计。
    ///
    /// 实际对象字节由 HTTP 层在事务外读取；本方法只允许读取付款实体直接引用的
    /// 回单资产，禁止用付款详情权限预览任意文件资产。
    ///
    /// # 错误
    /// 付款单、回单引用或文件资产不存在，以及审计写入失败时返回错误。
    pub async fn supplier_payment_bank_receipt(&self, id: &str, actor: &AuditActor) -> Result<FileAssetView> {
        let payment = self.load_supplier_payment(id).await?;
        let asset_id = payment.require_bank_receipt()?;
        let asset = self
            .db
            .file_assets()
            .find_by_id(asset_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("银行回单不存在".to_string()))?;
        let audit = actor.clone().resource_log(
            "supplier_payment.bank_receipt.preview",
            "supplier_payment",
            id.to_string(),
        )?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(asset.into())
    }

    /// 原子登记并过账供应商付款。
    ///
    /// 不携带新上传对象的内部兼容入口；HTTP 付款工作台使用
    /// [`Self::commit_supplier_payment_with_assets`]。
    ///
    /// # 错误
    /// 参数组合、银行回单、任务责任、收款账户或事务提交不合法时返回错误。
    pub async fn commit_supplier_payment(
        &self,
        req: CommitSupplierPaymentRequest,
        actor: &AuditActor,
    ) -> Result<SupplierPaymentView> {
        Ok(self
            .commit_supplier_payment_with_assets(req, Vec::new(), actor)
            .await?
            .view)
    }

    /// 原子登记并过账供应商付款，同时登记银行回单。
    ///
    /// 任务责任、当前默认收款账户、付款实体、核销分配、应付余额、回单资产、
    /// 付款任务和审计全部位于同一事务。采购单审批是付款授权来源，本命令不得
    /// 创建付款审批实例或审批任务。
    ///
    /// # 参数
    /// * `req` - 本次付款事实、冻结分配与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回已过账付款单视图。
    ///
    /// # 错误
    /// * `ValidationError` - 参数组合或分配不合法
    /// * `ConflictError` - 任务版本、付款单号或收款账户漂移
    /// * `NotFound` - 任务、应付、供应商或银行回单不存在
    pub async fn commit_supplier_payment_with_assets(
        &self,
        mut req: CommitSupplierPaymentRequest,
        asset_requests: Vec<PendingFileAssetRequest>,
        actor: &AuditActor,
    ) -> Result<SupplierPaymentWithAssetsResult> {
        req.validate()?;
        let command_receipt = CommandReceipt::new(
            "supplier-payment-commit-",
            actor,
            "supplier_payment.commit",
            "supplier_payment",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(payment_id) = command_receipt.committed_resource_id(&self.db).await? {
            return Ok(SupplierPaymentWithAssetsResult {
                view: self.supplier_payment_detail(&payment_id).await?,
                assets_committed: false,
            });
        }
        validate_bank_receipt_pending_requests(&asset_requests)?;
        let has_pending_assets = !asset_requests.is_empty();
        let pending_assets = PendingFileAssets::prepare(asset_requests, actor)?;
        let used_assets = resolve_payment_receipt_references(&mut req, &pending_assets)?;
        pending_assets.ensure_all_used(&used_assets)?;
        let expected_task_version = crate::work_item::expected_task_version(&req.expected_task_version)?;
        let work_item_id = req.work_item_id.clone();
        let expected_payee_bank_account_id =
            PartyBankAccountId::new(req.expected_payee_bank_account_id.trim());
        let expected_payee_bank_account_version = req.expected_payee_bank_account_version;
        req.payment.validate()?;
        let payment = SupplierPayment::new(
            SupplierPaymentId::new(next_id()),
            SupplierPaymentData {
                payment_no: req.payment.payment_no,
                supplier_id: req.payment.supplier_id,
                payee_bank_account_id: expected_payee_bank_account_id.clone(),
                paid_at: req.payment.paid_at,
                amount: req.payment.amount,
                bank_reference: req.payment.bank_reference,
                bank_receipt_asset_id: req.payment.bank_receipt_asset_id,
            },
        )?;
        let allocations = pending_allocations_from_request(&req.allocations)?;
        let policy_revision = self.rbac.current_policy_revision().await?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor_owned = actor.clone();
        let command_receipt_for_tx = command_receipt.clone();
        let transaction_result = rbac
            .clone()
            .run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    let mut payment = payment;
                    if db
                        .supplier_payments()
                        .find_by_payment_no(&payment.payment_no, session)
                        .await?
                        .is_some()
                    {
                        return Err(Error::ConflictError("付款单号已存在，请刷新后重试".to_string()));
                    }
                    let supplier = db
                        .supplier_accounts()
                        .find_by_id(payment.supplier_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
                    let bind_command = BindPublishedDefinitionCommand {
                        document_type: DocumentType::SupplierPayment,
                        business_object_id: payment.base.id.clone(),
                        business_object_version: payment.base.version,
                        context: BindingRevalidationContext {
                            organization_id: supplier.party_id.to_string(),
                            creator_id: actor_owned.id().to_string(),
                        },
                    };
                    let document = new_registered_document(
                        &payment.base.id,
                        DocumentType::SupplierPayment,
                        payment.payment_no.clone(),
                    )?;
                    persist_unbound_supplier_payment_document(
                        &db,
                        &rbac,
                        document,
                        &bind_command,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    ensure_bank_receipt_asset_in_transaction(
                        &db,
                        payment.require_bank_receipt()?,
                        &pending_assets,
                        session,
                    )
                    .await?;
                    pending_assets.persist(&db, session).await?;
                    db.supplier_payments().create(&payment, session).await?;
                    let audit = actor_owned.clone().resource_log(
                        "supplier_payment.create",
                        "supplier_payment",
                        payment.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    lock_expected_payment_recipient(
                        &db,
                        &payment.supplier_id,
                        &expected_payee_bank_account_id,
                        expected_payee_bank_account_version,
                        session,
                    )
                    .await?;
                    payment_task::record_payment_execution(
                        &db,
                        &work_item_id,
                        expected_task_version,
                        &payment.supplier_id,
                        &allocations,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    let id = payment.base.id.clone();
                    post_supplier_payment_in_transaction(
                        &db,
                        &mut payment,
                        &allocations,
                        PaymentPostSource::ExecutionTask,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    let command_audit = command_receipt_for_tx.audit(actor_owned.clone(), id)?;
                    db.audit_logs().create(&command_audit, session).await?;
                    Ok::<SupplierPayment, crate::errors::Error>(payment)
                })
            })
            .await;

        let committed = match transaction_result {
            Ok(committed) => committed,
            Err(error) => {
                let assets_may_be_committed = matches!(&error, Error::OutcomeUnknown(_));
                match command_receipt.committed_resource_id(&self.db).await? {
                    Some(payment_id) => {
                        return Ok(SupplierPaymentWithAssetsResult {
                            view: self.supplier_payment_detail(&payment_id).await?,
                            assets_committed: has_pending_assets && assets_may_be_committed,
                        });
                    }
                    None => return Err(error),
                }
            }
        };

        Ok(SupplierPaymentWithAssetsResult {
            view: self.supplier_payment_detail(&committed.base.id).await?,
            assets_committed: has_pending_assets,
        })
    }

    /// 按主键读取供应商付款单。
    ///
    /// # 错误
    /// 不存在时返回 `NotFound`。
    async fn load_supplier_payment(&self, id: &str) -> Result<SupplierPayment> {
        self.db
            .supplier_payments()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商付款单不存在".to_string()))
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

    /// 装配应付往来子账视图。
    ///
    /// # 参数
    /// * `id` - 子账 ID
    /// * `include_payment_recipient` - 是否加载任务/详情所需的当前收款账户
    ///
    /// # 返回
    /// 返回完整应付台账视图。
    ///
    /// # 错误
    /// * `NotFound` - 子账不存在
    async fn payable_account_view(
        &self,
        id: String,
        include_payment_recipient: bool,
    ) -> Result<PayableAccountView> {
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
        let (supplier_no, supplier_name) =
            resolve_supplier_display(&self.db, account.supplier_id.as_ref()).await?;
        let payment_recipient = if include_payment_recipient {
            resolve_optional_payment_recipient_for_read(&self.db, &account.supplier_id, &mut NoTransaction)
                .await?
                .as_ref()
                .map(payment_recipient_view)
        } else {
            None
        };
        let source_document_no = resolve_source_document_no(&self.db, &account).await?;
        Ok(PayableAccountView {
            id: account.base.id.clone(),
            source_document_id: account.source_document_id,
            source_document_no,
            supplier_id: account.supplier_id.to_string(),
            supplier_no,
            supplier_name,
            payment_recipient,
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
    /// * `include_payment_recipient` - 是否加载付款详情所需的冻结收款账户
    ///
    /// # 返回
    /// 返回付款单视图（含分配行与未分配余额）。
    ///
    /// # 错误
    /// * `NotFound` - 付款单不存在
    async fn supplier_payment_view(
        &self,
        id: String,
        include_payment_recipient: bool,
    ) -> Result<SupplierPaymentView> {
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
        let payment_recipient = match (include_payment_recipient, payment.payee_bank_account_id.as_ref()) {
            (true, Some(account_id)) => self
                .db
                .party_bank_accounts()
                .find_by_id(account_id.as_ref(), &mut NoTransaction)
                .await?
                .as_ref()
                .map(payment_recipient_view),
            _ => None,
        };
        let bank_receipt =
            payment_bank_receipt_view(&self.db, payment.bank_receipt_asset_id.as_ref()).await?;
        Ok(SupplierPaymentView {
            id: payment.base.id.clone(),
            payment_no: payment.payment_no,
            status: payment.status,
            supplier_id: payment.supplier_id.to_string(),
            payment_recipient,
            paid_at: payment.paid_at,
            amount: payment.amount,
            bank_reference: payment.bank_reference,
            bank_receipt,
            version: payment.base.version,
            created_at: payment.base.created_at,
            unallocated_amount: payment.amount.checked_sub(allocated_total),
            allocated_total,
            allocations: views,
        })
    }
}

/// 付款过账授权来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaymentPostSource {
    /// 当前开放付款执行任务。
    ExecutionTask,
}

/// 在调用方事务内写入付款核销、应付余额、任务进度与审计。
///
/// # 错误
/// 付款状态、供应商、应付开放余额、分配金额或仓储写入不合法时返回错误。
async fn post_supplier_payment_in_transaction(
    db: &Database,
    payment: &mut SupplierPayment,
    pending: &[PendingPaymentAllocation],
    source: PaymentPostSource,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<()> {
    if payment.status == SupplierPaymentStatus::Reversed {
        return Err(Error::BusinessLogicError("已冲正付款不能再核销".to_string()));
    }
    let existing = db
        .payment_allocations()
        .find_allocations_by_payments(&[payment.base.id.clone().into()], session)
        .await?;
    let net_allocated = net_payment_allocated(&existing);
    if net_allocated.checked_add(pending_allocated_total(pending)) > payment.amount {
        return Err(Error::BusinessLogicError("核销合计超过付款金额".to_string()));
    }

    let mut entry_balances: HashMap<String, Amount> = HashMap::new();
    for allocation in &existing {
        let balance = entry_balances
            .entry(allocation.payable_entry_id.to_string())
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
    let mut new_allocations = Vec::with_capacity(pending.len());
    for (index, line) in pending.iter().enumerate() {
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

    let mut affected_accounts = HashSet::new();
    for allocation in &new_allocations {
        let entry = db
            .payable_entries()
            .find_by_id(&allocation.payable_entry_id, session)
            .await?
            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
        let applied = db
            .payable_accounts()
            .apply_settlement(
                &entry.payable_account_id,
                &allocation.allocated_amount,
                actor.id(),
                session,
            )
            .await?;
        if !applied {
            return Err(Error::BusinessLogicError(
                "子账剩余开放余额不足，核销被拒绝".to_string(),
            ));
        }
        affected_accounts.insert(entry.payable_account_id);
    }
    for account_id in affected_accounts {
        payment_task::sync_purchase_payment_task(db, &account_id, session).await?;
    }
    match source {
        PaymentPostSource::ExecutionTask => payment.post_from_execution(pending)?,
    }
    db.supplier_payments().update(payment, session).await?;
    for allocation in &new_allocations {
        db.payment_allocations().create(allocation, session).await?;
    }
    let audit = actor.clone().resource_log(
        "supplier_payment.post",
        "supplier_payment",
        payment.base.id.clone(),
    )?;
    db.audit_logs().create(&audit, session).await?;
    Ok(())
}

/// 把付款核销请求行转换为强类型分配行。
///
/// # 错误
/// 任一金额非正时返回校验错误。
fn pending_allocations_from_request(
    lines: &[PaymentAllocationLineRequest],
) -> Result<Vec<PendingPaymentAllocation>> {
    lines
        .iter()
        .map(|line| {
            PendingPaymentAllocation::new(line.payable_entry_id.clone(), line.allocated_amount)
                .map_err(Into::into)
        })
        .collect()
}

/// 解析供应商在当前业务日唯一生效的默认收款账户。
///
/// # 错误
/// 供应商不存在、未配置默认账户或出现多个默认账户时失败关闭。
async fn resolve_current_payment_recipient(
    db: &Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<PartyBankAccount> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(supplier_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    resolve_optional_party_payment_recipient(db, &supplier.party_id, executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("供应商未配置当前默认收款账户，无法付款".to_string()))
}

/// 为只读任务/详情投影解析供应商当前默认收款账户。
///
/// 供应商已软删除或缺失时返回空，保证历史应付仍可读取；付款执行必须调用
/// [`resolve_current_payment_recipient`] 并严格校验活跃供应商。
///
/// # 错误
/// 出现多个默认账户或仓储读取失败时返回错误。
async fn resolve_optional_payment_recipient_for_read(
    db: &Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<Option<PartyBankAccount>> {
    let Some(supplier) = db
        .supplier_accounts()
        .find_by_id(supplier_id.as_ref(), executor)
        .await?
    else {
        return Ok(None);
    };
    resolve_optional_party_payment_recipient(db, &supplier.party_id, executor).await
}

/// 解析主体当前唯一默认收款账户；未配置时返回空。
///
/// # 错误
/// 出现多个默认账户或仓储读取失败时返回错误。
async fn resolve_optional_party_payment_recipient(
    db: &Database,
    party_id: &PartyId,
    executor: &mut dyn Executor,
) -> Result<Option<PartyBankAccount>> {
    let accounts = db
        .party_bank_accounts()
        .list_current_on(party_id, BusinessDate::today(), executor)
        .await?;
    let mut defaults = accounts.into_iter().filter(|account| account.is_default);
    let account = defaults.next();
    if defaults.next().is_some() {
        return Err(Error::BusinessLogicError(
            "供应商存在多个当前默认收款账户，请先修复主数据".to_string(),
        ));
    }
    Ok(account)
}

/// 在付款事务内校验并占用页面所见收款账户版本。
///
/// 该 CAS 写入递增账户版本，使付款事务与默认账户切换、停用、删除或内容修改
/// 在同一事实行上串行化；任一并发写入成功后，另一方必须刷新重试。
///
/// # 错误
/// 账户身份/版本漂移或 CAS 写入冲突时返回业务冲突。
async fn lock_expected_payment_recipient(
    db: &Database,
    supplier_id: &SupplierAccountId,
    expected_id: &PartyBankAccountId,
    expected_version: u64,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut recipient = resolve_current_payment_recipient(db, supplier_id, executor).await?;
    ensure_expected_payment_recipient(
        &recipient.base.id,
        recipient.base.version,
        expected_id,
        expected_version,
    )?;
    db.party_bank_accounts()
        .update(&mut recipient, executor)
        .await
        .map_err(payment_recipient_lock_error)
}

/// 把收款账户占用冲突映射为可操作的刷新提示。
fn payment_recipient_lock_error(error: database::Error) -> Error {
    match error {
        database::Error::OptimisticLockingError | database::Error::TransientTransactionConflict(_) => {
            Error::ConflictError("供应商收款账户已变化，请刷新付款任务并重新核对".to_string())
        }
        other => other.into(),
    }
}

/// 校验页面所见收款账户身份与版本均未变化。
///
/// # 错误
/// 账户事实行发生变化时返回冲突，要求用户重新核对。
fn ensure_expected_payment_recipient(
    actual_id: &str,
    actual_version: u64,
    expected_id: &PartyBankAccountId,
    expected_version: u64,
) -> Result<()> {
    if actual_id == expected_id.as_ref() && actual_version == expected_version {
        return Ok(());
    }
    Err(Error::ConflictError(
        "供应商收款账户已变化，请刷新付款任务并重新核对".to_string(),
    ))
}

/// 构造不含敏感明文的收款账户摘要。
fn payment_recipient_view(account: &PartyBankAccount) -> PaymentRecipientView {
    PaymentRecipientView {
        bank_account_id: account.base.id.clone(),
        version: account.base.version,
        account_name: account.account_name.clone(),
        bank_name: account.bank_name.clone(),
        bank_branch_name: account.bank_branch_name.clone(),
        account_number_masked: masked_bank_account_number(&account.account_number_last4),
    }
}

/// 使用账号末四位构造不可恢复的工作台掩码。
fn masked_bank_account_number(last4: &str) -> String {
    let last4 = last4.trim();
    if last4.is_empty() {
        "********".to_string()
    } else {
        format!("********{last4}")
    }
}

/// 校验付款命令携带的待登记银行回单元数据。
fn validate_bank_receipt_pending_requests(requests: &[PendingFileAssetRequest]) -> Result<()> {
    for request in requests {
        validate_bank_receipt_metadata(
            &request.registration.content_type,
            request.registration.sensitivity_class,
            request.registration.retention_class,
            false,
        )?;
    }
    Ok(())
}

/// 解析本次付款字段中的临时回单引用。
fn resolve_payment_receipt_references(
    req: &mut CommitSupplierPaymentRequest,
    pending_assets: &PendingFileAssets,
) -> Result<HashSet<String>> {
    let mut used = HashSet::new();
    pending_assets.resolve_id(&mut req.payment.bank_receipt_asset_id, &mut used)?;
    Ok(used)
}

/// 在付款事务内校验正式或本批次待登记的银行回单资产。
async fn ensure_bank_receipt_asset_in_transaction(
    db: &Database,
    asset_id: &FileAssetId,
    pending_assets: &PendingFileAssets,
    session: &mut ClientSession,
) -> Result<()> {
    if pending_assets.contains_id(asset_id) {
        let sensitivity = pending_assets
            .sensitivity(asset_id)
            .ok_or_else(|| Error::ValidationError("银行回单临时引用无效".to_string()))?;
        if sensitivity == SensitivityClass::General {
            return Err(Error::ValidationError("银行回单必须按敏感文件保存".to_string()));
        }
        return Ok(());
    }
    let asset = db
        .file_assets()
        .find_by_id(asset_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("银行回单不存在".to_string()))?;
    validate_bank_receipt_metadata(
        &asset.content_type,
        asset.sensitivity_class,
        asset.retention_class,
        asset.destroyed_at.is_some(),
    )
}

/// 校验银行回单的图片类型、敏感级别、保留策略与销毁状态。
fn validate_bank_receipt_metadata(
    content_type: &str,
    sensitivity: SensitivityClass,
    retention: RetentionClass,
    destroyed: bool,
) -> Result<()> {
    if !matches!(content_type, "image/jpeg" | "image/png" | "image/webp") {
        return Err(Error::ValidationError(
            "银行回单仅支持 JPG、PNG 或 WebP 图片".to_string(),
        ));
    }
    if sensitivity == SensitivityClass::General {
        return Err(Error::ValidationError("银行回单必须按敏感文件保存".to_string()));
    }
    if retention != RetentionClass::LongTerm {
        return Err(Error::ValidationError("银行回单必须长期保留".to_string()));
    }
    if destroyed {
        return Err(Error::ValidationError("银行回单已销毁".to_string()));
    }
    Ok(())
}

/// 装配付款详情中的银行回单安全元数据。
async fn payment_bank_receipt_view(
    db: &Database,
    asset_id: Option<&FileAssetId>,
) -> Result<Option<SupplierPaymentBankReceiptView>> {
    let Some(asset_id) = asset_id else {
        return Ok(None);
    };
    let asset = db
        .file_assets()
        .find_by_id(asset_id.as_ref(), &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("银行回单不存在".to_string()))?;
    Ok(Some(SupplierPaymentBankReceiptView {
        asset_id: asset.base.id,
        file_name: asset.file_name,
        content_type: asset.content_type,
        byte_size: asset.byte_size,
    }))
}

/// 解析供应商展示信息（编号 + 名称）。
///
/// 供应商编号取自 `supplier_accounts`；名称取共用主体当前修订的法定名称
/// （与 supplier_offering 的解析路径一致）。主数据缺失时返回 `None`，不阻断列表。
///
/// # 参数
/// * `db` - 数据库实例
/// * `supplier_id` - 供应商账号 ID
///
/// # 返回
/// 返回 `(供应商编号, 供应商名称)`。
async fn resolve_supplier_display(
    db: &mongodb::Database,
    supplier_id: &str,
) -> Result<(Option<String>, Option<String>)> {
    let account = db
        .supplier_accounts()
        .find_by_id(supplier_id, &mut NoTransaction)
        .await?;
    let Some(account) = account else {
        return Ok((None, None));
    };
    let supplier_no = Some(account.supplier_no.clone());
    let party = db
        .parties()
        .find_by_id(account.party_id.as_ref(), &mut NoTransaction)
        .await?;
    let Some(party) = party else {
        return Ok((supplier_no, None));
    };
    let Some(revision_id) = party.stable.current_revision_id.clone() else {
        return Ok((supplier_no, None));
    };
    let revision = db
        .party_revisions()
        .find_by_id(&revision_id, &mut NoTransaction)
        .await?;
    Ok((supplier_no, revision.map(|value| value.legal_name)))
}

/// 解析应付子账来源单据的业务单号（采购单来源取采购单号）。
///
/// # 参数
/// * `db` - 数据库实例
/// * `account` - 应付子账
///
/// # 返回
/// 返回业务单号；未知来源或单据缺失时返回 `None`。
async fn resolve_source_document_no(
    db: &mongodb::Database,
    account: &PayableAccount,
) -> Result<Option<String>> {
    if account.source_type != entities::payable::PayableSourceType::PurchaseOrder {
        return Ok(None);
    }
    let order = db
        .purchase_orders()
        .find_by_id(&account.source_document_id, &mut NoTransaction)
        .await?;
    Ok(order.map(|value| value.purchase_no.clone()))
}

/// 返回固定零金额。
///
/// # 返回
/// 返回金额 `0.00`。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}

/// 汇总冻结分配行金额。
///
/// # 参数
/// * `allocations` - 提交时冻结的待过账分配
///
/// # 返回
/// 返回各分配行金额之和。
fn pending_allocated_total(allocations: &[PendingPaymentAllocation]) -> Amount {
    allocations
        .iter()
        .fold(zero_amount(), |sum, line| sum.checked_add(line.allocated_amount))
}

/// 证明供应商付款为无审批单据并持久化未绑定注册行。
///
/// # 错误
/// 政策不是 `NO_APPROVAL`、绑定端口意外返回定义或注册写入失败时返回错误。
async fn persist_unbound_supplier_payment_document(
    db: &Database,
    rbac: &SharedRbacService,
    document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    if binding.is_some() || document.approval_binding.is_some() {
        return Err(Error::Internal(
            "供应商付款为 NO_APPROVAL，不得写入审批绑定".to_string(),
        ));
    }
    persist_registered_document(db, &document, session).await
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

#[cfg(test)]
mod supplier_payment_execution_tests {
    use super::{
        ensure_expected_payment_recipient, masked_bank_account_number, validate_bank_receipt_metadata,
    };
    use entities::file_asset::{RetentionClass, SensitivityClass};
    use entities::ids::PartyBankAccountId;

    #[test]
    fn bank_receipt_requires_sensitive_long_term_image() {
        assert!(validate_bank_receipt_metadata(
            "image/png",
            SensitivityClass::Sensitive,
            RetentionClass::LongTerm,
            false,
        )
        .is_ok());
        assert!(validate_bank_receipt_metadata(
            "application/pdf",
            SensitivityClass::Sensitive,
            RetentionClass::LongTerm,
            false,
        )
        .is_err());
        assert!(validate_bank_receipt_metadata(
            "image/png",
            SensitivityClass::General,
            RetentionClass::LongTerm,
            false,
        )
        .is_err());
        assert!(validate_bank_receipt_metadata(
            "image/png",
            SensitivityClass::Sensitive,
            RetentionClass::SevenDays,
            false,
        )
        .is_err());
    }

    /// 付款登记必须注册无审批 BusinessDocument。
    #[test]
    fn commit_registers_unbound_document() {
        let source = include_str!("mod.rs");
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::SupplierPayment"));
        assert!(source.contains("persist_unbound_supplier_payment_document"));
        assert!(source.contains("供应商付款为 NO_APPROVAL"));
    }

    /// 付款必须绑定执行任务并在同一事务直接过账。
    #[test]
    fn commit_records_task_and_posts_without_approval() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("record_payment_execution"));
        assert!(production.contains("post_supplier_payment_in_transaction"));
        assert!(production.contains("PaymentPostSource::ExecutionTask"));
        assert!(!production.contains("pub async fn submit_supplier_payment"));
        assert!(!production.contains("prepare_start"));
    }

    /// 收款账号掩码不得泄露除末四位外的内容。
    #[test]
    fn recipient_mask_only_contains_last_four() {
        assert_eq!(masked_bank_account_number("1234"), "********1234");
        assert_eq!(masked_bank_account_number(""), "********");
    }

    /// 提交时默认收款账户身份或版本发生漂移必须失败关闭。
    #[test]
    fn recipient_identity_or_version_drift_is_rejected() {
        assert!(ensure_expected_payment_recipient(
            "bank-account-1",
            7,
            &PartyBankAccountId::new("bank-account-1"),
            7,
        )
        .is_ok());
        assert!(ensure_expected_payment_recipient(
            "bank-account-2",
            7,
            &PartyBankAccountId::new("bank-account-1"),
            7,
        )
        .is_err());
        assert!(ensure_expected_payment_recipient(
            "bank-account-1",
            8,
            &PartyBankAccountId::new("bank-account-1"),
            7,
        )
        .is_err());
    }

    /// 列表投影不得逐行解析收款账户；收款账户只允许详情/任务路径加载。
    #[test]
    fn list_views_omit_payment_recipient_lookups() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("payable_account_view(row.id, false)"));
        assert!(production.contains("supplier_payment_view(row.id, false)"));
        assert!(production.contains("payable_account_view(id.to_string(), true)"));
        assert!(production.contains("supplier_payment_view(id.to_string(), true)"));
    }

    /// 历史应付读取不得因供应商已软删除而返回 NotFound。
    #[test]
    fn historical_payable_recipient_projection_is_nonblocking() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let read_projection = production
            .split("async fn resolve_optional_payment_recipient_for_read")
            .nth(1)
            .expect("只读收款账户解析")
            .split("async fn resolve_optional_party_payment_recipient")
            .next()
            .expect("只读解析函数体");
        assert!(read_projection.contains("return Ok(None)"));
        assert!(!read_projection.contains("Error::NotFound"));
    }

    /// 事务内收款账户校验必须以页面版本执行真实 CAS 写入。
    #[test]
    fn payment_recipient_lock_performs_cas_write() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let lock = production
            .split("async fn lock_expected_payment_recipient")
            .nth(1)
            .expect("收款账户事务锁")
            .split("fn payment_recipient_lock_error")
            .next()
            .expect("收款账户事务锁函数体");
        assert!(lock.contains("expected_version"));
        assert!(lock.contains(".update(&mut recipient, executor)"));
    }

    /// 正常付款生产代码不得创建付款审批实例或暴露审批命令。
    #[test]
    fn production_has_no_supplier_payment_approval_commands() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("SupplierPaymentStatus::PendingReview"));
        assert!(!production.contains("submit_supplier_payment"));
        assert!(!production.contains("cancel_supplier_payment_approval"));
        assert!(!production.contains("start_supplier_payment_approval"));
        assert!(!production.contains("pub async fn post_supplier_payment"));
        assert!(!production.contains("SupplierPaymentStatus::InApproval"));
    }
}
