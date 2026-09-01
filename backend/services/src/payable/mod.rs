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
//! - D15 `purchase_orders()` 校验来源采购单存在，并解析采购单号供展示；
//! - D09 `supplier_accounts()` 校验供应商存在并取 `party_id`（进项发票
//!   与应付子账的往来主体相等键）；
//! - D18 `invoices()` 复用发票仓储（`invoice` 由 D18 拥有实体与仓储，
//!   D19 只拥有 `purchase_invoice_allocation`，禁止复制发票实体）；
//! - D33 `supplier_settlement_statements()` 解析结算单号供展示。

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use database::{
    AccessControlExt, Executor, FileAssetExt, NoTransaction, PartyExt, PayableExt, PurchaseOrderExt,
    ReceivableExt, SupplierExt, SupplierSettlementExt, Transactional,
};
use entities::common::time::{BusinessDate, Instant};
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::file_asset::BankReceiptEvidencePolicy;
use entities::ids::{
    FileAssetId, InvoiceId, PartyBankAccountId, PartyId, PayableAccountId, PayableEntryId,
    PaymentAllocationId, PurchaseInvoiceAllocationId, SupplierAccountId, SupplierPaymentId,
};
use entities::money::Amount;
use entities::party::PartyBankAccount;
use entities::payable::{
    AllocationAction, EntryDirection, PayableAccount, PayableAccountData, PayableEntry, PayableEntryData,
    PayableEntryType, PayableSourceType, PaymentAllocation, PaymentAllocationLedger,
    PendingPaymentAllocation, PurchaseInvoiceAllocation, PurchaseInvoiceAllocationLine,
    PurchaseInvoiceAllocationPlan, SupplierPayment, SupplierPaymentData, SupplierPaymentStatus,
};
use entities::receivable::{Invoice, InvoiceData, InvoiceDirection, InvoiceKind};
use entities::supplier::SupplierAccount;
use id_generator::next_id;
use mongodb::{ClientSession, Database};
use validator::Validate;

use crate::approval::binding::{
    bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::audit::{AuditActor, CommandReceipt, CommandReceiptServiceExt as _};
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use crate::file_asset::{FileAssetView, PendingFileAssetRequest};
use crate::iam::{self, SharedRbacService};
use crate::pending_file_assets::PendingFileAssets;

mod display;
mod dto;
pub(crate) mod payment_task;
use self::dto::SortDir;
pub use self::dto::{
    CommitSupplierPaymentRequest, CreatePayableAccountRequest, CreateSupplierPaymentRequest, PageView,
    PayableAccountListParams, PayableAccountSummaryView, PayableAccountView, PaymentAllocationLineRequest,
    PaymentAllocationView, PaymentRecipientRevealView, PaymentRecipientView,
    PurchaseInvoiceAllocationLineRequest, PurchaseInvoiceAllocationListParams, PurchaseInvoiceAllocationView,
    PurchaseInvoiceRegisteredView, RegisterPurchaseInvoiceRequest, RevealPaymentRecipientRequest,
    SupplierPaymentBankReceiptView, SupplierPaymentListParams, SupplierPaymentReversalView,
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
    ) -> Result<PageView<PayableAccountSummaryView>> {
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
        let account_ids = page
            .items
            .iter()
            .map(|row| PayableAccountId::new(row.id.clone()))
            .collect::<Vec<_>>();
        let mut entries_by_account = HashMap::<String, Vec<PayableEntry>>::new();
        for entry in self
            .db
            .payable_entries()
            .find_entries_by_accounts(&account_ids, &mut NoTransaction)
            .await?
        {
            entries_by_account
                .entry(entry.payable_account_id.to_string())
                .or_default()
                .push(entry);
        }
        for entries in entries_by_account.values_mut() {
            entries.sort_unstable_by_key(|entry| entry.source_sequence);
        }

        let supplier_ids = page
            .items
            .iter()
            .map(|row| SupplierAccountId::new(row.supplier_id.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let suppliers = self
            .db
            .supplier_accounts()
            .find_accounts_by_ids(&supplier_ids, &mut NoTransaction)
            .await?;
        let party_ids = suppliers
            .iter()
            .map(|supplier| supplier.party_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let parties = self
            .db
            .parties()
            .find_parties_by_ids(&party_ids, &mut NoTransaction)
            .await?;
        let revision_ids = parties
            .iter()
            .filter_map(|party| party.stable.current_revision_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let revisions = self
            .db
            .party_revisions()
            .find_revisions_by_ids(&revision_ids, &mut NoTransaction)
            .await?;
        let supplier_by_id = suppliers
            .into_iter()
            .map(|supplier| (supplier.base.id.clone(), supplier))
            .collect::<HashMap<_, _>>();
        let party_by_id = parties
            .into_iter()
            .map(|party| (party.base.id.clone(), party))
            .collect::<HashMap<_, _>>();
        let revision_by_id = revisions
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<HashMap<_, _>>();

        let purchase_order_ids = page
            .items
            .iter()
            .filter(|row| row.source_type == PayableSourceType::PurchaseOrder)
            .map(|row| row.source_document_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let settlement_ids = page
            .items
            .iter()
            .filter(|row| row.source_type == PayableSourceType::SupplierSettlement)
            .map(|row| row.source_document_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let purchase_order_nos = self
            .db
            .purchase_order()
            .find_orders_by_ids(&purchase_order_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|order| (order.base.id.clone(), order.purchase_no))
            .collect::<HashMap<_, _>>();
        let settlement_nos = self
            .db
            .supplier_settlement_statements()
            .find_statements_by_ids(&settlement_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|statement| (statement.base.id.clone(), statement.statement_no))
            .collect::<HashMap<_, _>>();

        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            let source_document_no = match row.source_type {
                PayableSourceType::PurchaseOrder => purchase_order_nos.get(&row.source_document_id),
                PayableSourceType::SupplierSettlement => settlement_nos.get(&row.source_document_id),
            }
            .filter(|value| !value.trim().is_empty())
            .cloned();
            let supplier = supplier_by_id.get(&row.supplier_id);
            let supplier_no = supplier.map(|value| value.supplier_no.clone());
            let supplier_name = supplier
                .and_then(|value| party_by_id.get(value.party_id.as_ref()))
                .and_then(|party| party.stable.current_revision_id.as_ref())
                .and_then(|revision_id| revision_by_id.get(revision_id))
                .map(|revision| revision.legal_name.clone());
            let entries = entries_by_account
                .remove(&row.id)
                .unwrap_or_default()
                .into_iter()
                .map(|entry| crate::payable::dto::PayableEntryView {
                    id: entry.base.id,
                    entry_type: entry.entry_type,
                    direction: entry.direction,
                    amount: entry.amount,
                    due_date: entry.due_date,
                    source_document_no: (entry.source_document_id == row.source_document_id)
                        .then(|| source_document_no.clone())
                        .flatten(),
                    source_document_id: entry.source_document_id,
                    source_sequence: entry.source_sequence,
                    posted_at: entry.posted_at,
                })
                .collect();
            views.push(PayableAccountSummaryView {
                id: row.id,
                source_document_id: row.source_document_id,
                source_document_no,
                supplier_id: row.supplier_id,
                supplier_no,
                supplier_name,
                source_type: row.source_type,
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
        if !recipient.matches_expected(
            &PartyBankAccountId::new(req.expected_bank_account_id.trim()),
            req.expected_bank_account_version,
        ) {
            return Err(Error::ConflictError(
                "供应商收款账户已变化，请刷新付款任务并重新核对".to_string(),
            ));
        }
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
        self.attach_supplier_payment_reversals(&mut views).await?;
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
        let mut views = vec![self.supplier_payment_view(id.to_string(), true).await?];
        self.attach_supplier_payment_reversals(&mut views).await?;
        views
            .pop()
            .ok_or_else(|| Error::Internal("供应商付款详情装配失败".to_string()))
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
        let command_receipt = CommandReceipt::from_payload(
            "supplier-payment-commit-",
            actor.id(),
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
        for request in &asset_requests {
            BankReceiptEvidencePolicy::validate(
                &request.registration.content_type,
                request.registration.sensitivity_class,
                request.registration.retention_class,
                false,
            )
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        }
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
    /// 规范化号码去重；总额/税额口径、序号与分配实体由
    /// [`PurchaseInvoiceAllocationPlan`] 一次性构造（FIN-E03）；账户与供应商
    /// 事实按去重集合批量装载并逐账户校验跨供应商主体一致；收票进度按账户
    /// 聚合后批量条件更新（`apply_invoicings_many` 不超额收票），分配行
    /// 批量插入；发票迁移为已登记。业务命令收据负责同键同载荷回放；规范化
    /// 发票号码唯一键负责业务去重。
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
        let command_receipt = CommandReceipt::from_payload(
            "purchase-invoice-register-",
            actor.id(),
            "purchase_invoice_allocation.post",
            "purchase_invoice_allocation",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(invoice_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self.purchase_invoice_registered_view(&invoice_id).await;
        }
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
        let command_receipt_for_tx = command_receipt.clone();
        let transaction_result = client
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

                    let lines: Vec<PurchaseInvoiceAllocationLine> = req
                        .allocations
                        .iter()
                        .map(|line| PurchaseInvoiceAllocationLine {
                            payable_account_id: line.payable_account_id.clone(),
                            allocated_gross_amount: line.allocated_gross_amount,
                            allocated_net_amount: line.allocated_net_amount,
                            allocated_tax_amount: line.allocated_tax_amount,
                        })
                        .collect();
                    let allocation_ids: Vec<PurchaseInvoiceAllocationId> = (0..lines.len())
                        .map(|_| PurchaseInvoiceAllocationId::new(next_id()))
                        .collect();
                    let plan = PurchaseInvoiceAllocationPlan::new(
                        invoice_for_tx.base.id.clone().into(),
                        invoice_for_tx.gross_amount,
                        invoice_for_tx.net_amount,
                        invoice_for_tx.tax_amount,
                        &lines,
                        &allocation_ids,
                    )?;

                    // 账户/供应商事实一次批量装载；按行首次出现顺序逐账户校验，
                    // 保持原逐行首错语义（存在性 → 供应商 → 跨供应商主体一致）。
                    let account_ids: Vec<PayableAccountId> = plan
                        .account_invoicing_deltas()
                        .iter()
                        .map(|(account_id, _)| account_id.clone())
                        .collect();
                    let accounts = db
                        .payable_accounts()
                        .find_accounts_by_ids(&account_ids, session)
                        .await?;
                    let accounts_by_id: HashMap<&str, &PayableAccount> = accounts
                        .iter()
                        .map(|account| (account.base.id.as_str(), account))
                        .collect();
                    let mut supplier_ids: Vec<SupplierAccountId> = Vec::new();
                    let mut seen_suppliers: HashSet<String> = HashSet::new();
                    for account_id in &account_ids {
                        let account = accounts_by_id
                            .get(account_id.as_ref())
                            .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
                        if seen_suppliers.insert(account.supplier_id.to_string()) {
                            supplier_ids.push(account.supplier_id.clone());
                        }
                    }
                    let suppliers = db
                        .supplier_accounts()
                        .find_accounts_by_ids(&supplier_ids, session)
                        .await?;
                    let suppliers_by_id: HashMap<&str, &SupplierAccount> = suppliers
                        .iter()
                        .map(|supplier| (supplier.base.id.as_str(), supplier))
                        .collect();
                    for account_id in &account_ids {
                        let account = accounts_by_id
                            .get(account_id.as_ref())
                            .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
                        let account_supplier = suppliers_by_id
                            .get(account.supplier_id.as_ref())
                            .ok_or_else(|| Error::NotFound("应付子账供应商不存在".to_string()))?;
                        if account_supplier.party_id != party_id {
                            return Err(Error::BusinessLogicError("禁止跨供应商收票".to_string()));
                        }
                    }
                    let invoicing = db
                        .payable_accounts()
                        .apply_invoicings_many(plan.account_invoicing_deltas(), &actor_id, session)
                        .await?;
                    if !invoicing.rejected.is_empty() {
                        return Err(Error::BusinessLogicError(
                            "子账剩余可收票额度不足，收票被拒绝".to_string(),
                        ));
                    }
                    let mut invoice_mut = invoice_for_tx;
                    invoice_mut.mark_registered(&actor_id)?;
                    db.invoices().create(&invoice_mut, session).await?;
                    db.payable()
                        .create_purchase_invoice_allocations_many(plan.new_allocations(), session)
                        .await?;
                    let audit = actor_owned.clone().resource_log(
                        "purchase_invoice_allocation.post",
                        "purchase_invoice_allocation",
                        invoice_mut.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    let receipt_audit =
                        command_receipt_for_tx.audit(actor_owned.clone(), invoice_mut.base.id.clone())?;
                    db.audit_logs().create(&receipt_audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await;
        if let Err(error) = transaction_result {
            if let Some(invoice_id) = command_receipt.committed_resource_id(&self.db).await? {
                return self.purchase_invoice_registered_view(&invoice_id).await;
            }
            return Err(error);
        }
        self.purchase_invoice_registered_view(invoice_id.as_ref()).await
    }

    /// 按已提交发票主键回读稳定登记结果。
    ///
    /// # 参数
    /// * `invoice_id` - 首次命令收据记录的进项发票主键
    ///
    /// # 返回
    /// 返回发票号码、金额和正式分配行。
    ///
    /// # 错误
    /// 收据引用损坏、发票或分配查询失败时返回错误。
    async fn purchase_invoice_registered_view(
        &self,
        invoice_id: &str,
    ) -> Result<PurchaseInvoiceRegisteredView> {
        let invoice_id = InvoiceId::new(invoice_id);
        let invoice = self
            .db
            .invoices()
            .find_by_id(&invoice_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("进项发票命令收据引用不存在".to_string()))?;
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
    /// 账户必填政策与请求校验保留在 Service；仓储复用既有批量读取入口及其
    /// 未删除过滤，Service 按已验证方向执行稳定排序和分页。
    ///
    /// # 参数
    /// * `params` - 应付子账、分页与排序校验参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图，页码与单页条数沿用归一化请求值。
    ///
    /// # 错误
    /// 参数非法、应付子账缺失或仓储查询失败时返回既有服务错误。
    ///
    /// # 约束
    /// 排序固定使用 `(created_at, id)` 同方向并列键，不引入未建索引的新查询形状。
    pub async fn purchase_invoice_allocation_list(
        &self,
        params: &PurchaseInvoiceAllocationListParams,
    ) -> Result<PageView<PurchaseInvoiceAllocationView>> {
        params.validate()?;
        let query = params.normalized()?;
        let payable_account_id = query
            .payable_account_id
            .ok_or_else(|| Error::ValidationError("按应付子账筛选进项发票分配为必填条件".to_string()))?;
        let allocations = self
            .db
            .purchase_invoice_allocations()
            .find_allocations_by_accounts(std::slice::from_ref(&payable_account_id), &mut NoTransaction)
            .await?;
        Ok(purchase_invoice_allocation_page(allocations, query.paging))
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
        let source_document_no = resolve_source_document_no(&self.db, &account).await?;
        let account_source_id = account.source_document_id.clone();
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
                source_document_id: entry.source_document_id.clone(),
                source_document_no: (entry.source_document_id == account_source_id)
                    .then(|| source_document_no.clone())
                    .flatten(),
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
        self.enrich_supplier_payment_view(SupplierPaymentView {
            id: payment.base.id.clone(),
            payment_no: payment.payment_no,
            status: payment.status,
            supplier_id: payment.supplier_id.to_string(),
            supplier_no: None,
            supplier_name: None,
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
            related_reversals: Vec::new(),
        })
        .await
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
/// 数据面职责归位（FIN-E02/FIN-R05）：分录/子账事实一次批量装载并去重，
/// 核销净额、逐分录开放余额、连续序号与分配实体构造由
/// [`PaymentAllocationLedger`] 完成；子账进度按账户聚合后批量条件更新，
/// 分配行批量插入。供应商一致性、事务、任务同步与审计仍在本方法编排。
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
    let mut ledger =
        PaymentAllocationLedger::new(payment.base.id.clone().into(), payment.amount, &existing, pending)?;

    let mut entry_ids: Vec<PayableEntryId> =
        pending.iter().map(|line| line.payable_entry_id.clone()).collect();
    entry_ids.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    entry_ids.dedup();
    let entries = db
        .payable_entries()
        .find_entries_by_ids(&entry_ids, session)
        .await?;
    let entries_by_id: HashMap<&str, &PayableEntry> = entries
        .iter()
        .map(|entry| (entry.base.id.as_str(), entry))
        .collect();
    let mut account_ids: Vec<PayableAccountId> = entries
        .iter()
        .map(|entry| entry.payable_account_id.clone())
        .collect();
    account_ids.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    account_ids.dedup();
    let accounts = db
        .payable_accounts()
        .find_accounts_by_ids(&account_ids, session)
        .await?;
    let accounts_by_id: HashMap<&str, &PayableAccount> = accounts
        .iter()
        .map(|account| (account.base.id.as_str(), account))
        .collect();
    let mut checked_accounts: HashSet<PayableAccountId> = HashSet::new();
    let allocation_ids: Vec<PaymentAllocationId> = (0..pending.len())
        .map(|_| PaymentAllocationId::new(next_id()))
        .collect();
    for (index, line) in pending.iter().enumerate() {
        let entry = entries_by_id
            .get(line.payable_entry_id.as_ref())
            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
        if checked_accounts.insert(entry.payable_account_id.clone()) {
            let account = accounts_by_id
                .get(entry.payable_account_id.as_ref())
                .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))?;
            if account.supplier_id != payment.supplier_id {
                return Err(Error::BusinessLogicError("禁止跨供应商核销".to_string()));
            }
        }
        ledger.apply(line, entry, allocation_ids[index].clone(), Instant::now())?;
    }

    let settlement = db
        .payable_accounts()
        .apply_settlements_many(ledger.account_settlement_deltas(), actor.id(), session)
        .await?;
    if !settlement.rejected.is_empty() {
        return Err(Error::BusinessLogicError(
            "子账剩余开放余额不足，核销被拒绝".to_string(),
        ));
    }
    for account_id in &settlement.applied {
        payment_task::sync_purchase_payment_task(db, account_id, session).await?;
    }
    match source {
        PaymentPostSource::ExecutionTask => payment.post_from_execution(pending)?,
    }
    db.supplier_payments().update(payment, session).await?;
    db.payable()
        .create_payment_allocations_many(ledger.new_allocations(), session)
        .await?;
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
/// 当前有效账户集合由 Repository 按业务日期过滤（有效期窗口与启停状态），
/// 唯一默认值解析与主数据损坏判定由 [`PartyBankAccount::resolve_current_default`]
/// 完成，本函数只负责日期读取、错误到 API 的映射与所有权转换。
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
    PartyBankAccount::resolve_current_default(&accounts)
        .map(|account| account.cloned())
        .map_err(|_| Error::BusinessLogicError("供应商存在多个当前默认收款账户，请先修复主数据".to_string()))
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
    if !recipient.matches_expected(expected_id, expected_version) {
        return Err(Error::ConflictError(
            "供应商收款账户已变化，请刷新付款任务并重新核对".to_string(),
        ));
    }
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
///
/// 待登记资产已在事务前经 [`BankReceiptEvidencePolicy::validate`] 同一入口
/// 完成全量规则校验（与 stored 元数据共用规则），事务内只确认临时引用属于
/// 本批次；正式资产按已落库元数据再次执行同一策略。
async fn ensure_bank_receipt_asset_in_transaction(
    db: &Database,
    asset_id: &FileAssetId,
    pending_assets: &PendingFileAssets,
    session: &mut ClientSession,
) -> Result<()> {
    if pending_assets.contains_id(asset_id) {
        return Ok(());
    }
    let asset = db
        .file_assets()
        .find_by_id(asset_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("银行回单不存在".to_string()))?;
    BankReceiptEvidencePolicy::validate(
        &asset.content_type,
        asset.sensitivity_class,
        asset.retention_class,
        asset.destroyed_at.is_some(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
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

/// 解析应付子账来源单据的业务单号。
///
/// 采购单来源取采购单号；供应商结算来源取结算单号。空单号视为缺失。
///
/// # 参数
/// * `db` - 数据库实例
/// * `account` - 应付子账
///
/// # 返回
/// 返回业务单号；未知来源或单据缺失时返回 `None`，不得回退内部 ID。
///
/// # 错误
/// 仓储读取失败时返回错误。
async fn resolve_source_document_no(
    db: &mongodb::Database,
    account: &PayableAccount,
) -> Result<Option<String>> {
    let document_no = match account.source_type {
        entities::payable::PayableSourceType::PurchaseOrder => db
            .purchase_orders()
            .find_by_id(&account.source_document_id, &mut NoTransaction)
            .await?
            .map(|value| value.purchase_no),
        entities::payable::PayableSourceType::SupplierSettlement => db
            .supplier_settlement_statements()
            .find_by_id(&account.source_document_id, &mut NoTransaction)
            .await?
            .map(|value| value.statement_no),
    };
    Ok(document_no.filter(|value| !value.trim().is_empty()))
}

/// 返回固定零金额。
///
/// # 返回
/// 返回金额 `0.00`。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
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

/// 对进项发票分配执行稳定排序、分页和视图映射。
///
/// # 参数
/// * `allocations` - 仓储按单个应付子账返回的未删除分配事实
/// * `paging` - 已完成字段白名单和方向校验的分页参数
///
/// # 返回
/// 返回带总数、原页码和原分页大小的契约分页视图。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// `created_at` 与 `id` 始终使用同一方向，确保同秒事实跨页顺序确定。
fn purchase_invoice_allocation_page(
    mut allocations: Vec<PurchaseInvoiceAllocation>,
    paging: dto::PageParams,
) -> PageView<PurchaseInvoiceAllocationView> {
    allocations.sort_by(|left, right| {
        left.base
            .created_at
            .cmp(&right.base.created_at)
            .then_with(|| left.base.id.cmp(&right.base.id))
    });
    if matches!(paging.sort_dir, SortDir::Desc) {
        allocations.reverse();
    }
    let total = allocations.len() as i64;
    let start = (paging.page.saturating_sub(1) * u64::from(paging.page_size)) as usize;
    let items = allocations
        .iter()
        .skip(start)
        .take(paging.page_size as usize)
        .map(purchase_invoice_allocation_view)
        .collect();
    PageView {
        items,
        total,
        page: paging.page,
        page_size: paging.page_size,
    }
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
mod purchase_invoice_allocation_list_tests {
    use std::str::FromStr;

    use entities::ids::{InvoiceId, PayableAccountId, PurchaseInvoiceAllocationId};
    use entities::money::Amount;
    use entities::payable::{AllocationAction, PurchaseInvoiceAllocation, PurchaseInvoiceAllocationData};

    use super::dto::PageParams;
    use super::{purchase_invoice_allocation_page, SortDir};

    /// 构造具有指定稳定 ID 与秒级创建时间的最小进项发票分配事实。
    ///
    /// 参数提供排序键，返回通过实体校验的正式分配；测试金额固定为 `1.00`，
    /// 构造失败时直接 panic，且不访问数据库。
    fn allocation(id: &str, created_at: u64) -> PurchaseInvoiceAllocation {
        let mut allocation = PurchaseInvoiceAllocation::new(
            PurchaseInvoiceAllocationId::new(id),
            PurchaseInvoiceAllocationData {
                invoice_id: InvoiceId::new("invoice-1"),
                payable_account_id: PayableAccountId::new("account-1"),
                allocation_seq: 1,
                allocation_action: AllocationAction::Apply,
                allocated_gross_amount: Amount::from_str("1.00").unwrap(),
                allocated_net_amount: Amount::from_str("1.00").unwrap(),
                allocated_tax_amount: Amount::from_str("0.00").unwrap(),
                reverses_allocation_id: None,
            },
        )
        .unwrap();
        allocation.base.created_at = created_at;
        allocation
    }

    /// 验证升序分页按创建时间再按稳定 ID 返回。
    ///
    /// 测试使用乱序事实，不访问数据库；首升序页的 ID 顺序变化时失败。
    #[test]
    fn allocation_page_sorts_ascending() {
        let page = purchase_invoice_allocation_page(
            vec![
                allocation("a-3", 30),
                allocation("a-1", 10),
                allocation("a-2", 20),
            ],
            PageParams {
                page: 1,
                page_size: 3,
                sort_by: "created_at",
                sort_dir: SortDir::Asc,
            },
        );

        assert_eq!(
            page.items.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["a-1", "a-2", "a-3"]
        );
    }

    /// 验证降序分页对创建时间与稳定 ID 使用同一方向。
    ///
    /// 测试使用乱序事实，不访问数据库；业务时间或并列键方向变化时失败。
    #[test]
    fn allocation_page_sorts_descending() {
        let page = purchase_invoice_allocation_page(
            vec![
                allocation("a-1", 10),
                allocation("a-3", 30),
                allocation("a-2", 20),
            ],
            PageParams {
                page: 1,
                page_size: 3,
                sort_by: "created_at",
                sort_dir: SortDir::Desc,
            },
        );

        assert_eq!(
            page.items.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["a-3", "a-2", "a-1"]
        );
    }

    /// 验证同秒事实使用 ID 并列键后跨页边界保持确定。
    ///
    /// 测试同时断言升降序第二页和总数，避免同秒记录在页间漂移或重复。
    #[test]
    fn allocation_page_paginates_equal_timestamps_deterministically() {
        let allocations = vec![
            allocation("a-2", 10),
            allocation("a-4", 10),
            allocation("a-1", 10),
            allocation("a-3", 10),
        ];
        let ascending = purchase_invoice_allocation_page(
            allocations.clone(),
            PageParams {
                page: 2,
                page_size: 2,
                sort_by: "created_at",
                sort_dir: SortDir::Asc,
            },
        );
        let descending = purchase_invoice_allocation_page(
            allocations,
            PageParams {
                page: 2,
                page_size: 2,
                sort_by: "created_at",
                sort_dir: SortDir::Desc,
            },
        );

        assert_eq!(
            ascending
                .items
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            ["a-3", "a-4"]
        );
        assert_eq!(
            descending
                .items
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            ["a-2", "a-1"]
        );
        assert_eq!(ascending.total, 4);
        assert_eq!(descending.total, 4);
    }
}

#[cfg(test)]
mod supplier_payment_execution_tests {
    use super::masked_bank_account_number;

    /// 银行回单可用性规则必须由 `BankReceiptEvidencePolicy` 单一入口承担：
    /// pending 待登记与 stored 已落库两条路径都调用该 VO，旧 Service 校验
    /// helper 已删除。
    #[test]
    fn bank_receipt_evidence_uses_single_policy_entry() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let pending_path = production
            .split("BankReceiptEvidencePolicy::validate(")
            .nth(1)
            .expect("待登记校验入口");
        assert!(pending_path.contains("request.registration.content_type"));
        assert!(pending_path.contains("request.registration.sensitivity_class"));
        assert!(pending_path.contains("request.registration.retention_class"));

        let stored_path = production
            .split("BankReceiptEvidencePolicy::validate(")
            .nth(2)
            .expect("已落库校验入口");
        assert!(stored_path.contains("asset.content_type"));
        assert!(stored_path.contains("asset.sensitivity_class"));
        assert!(stored_path.contains("asset.retention_class"));
        assert!(stored_path.contains("asset.destroyed_at.is_some()"));
        assert!(!production.contains("fn validate_bank_receipt_metadata"));
        assert!(!production.contains("fn validate_bank_receipt_pending_requests"));
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

    /// 收款账户唯一默认解析与 expected 身份匹配必须由
    /// `PartyBankAccount` 领域方法承担，旧 Service 手工判断 helper 已删除。
    #[test]
    fn recipient_rules_live_in_party_bank_account_domain() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("PartyBankAccount::resolve_current_default(&accounts)"));
        assert!(production.contains("recipient.matches_expected("));
        assert!(!production.contains("fn ensure_expected_payment_recipient"));
    }

    /// 列表投影不得逐行解析收款账户；收款账户只允许详情/任务路径加载。
    #[test]
    fn list_views_omit_payment_recipient_lookups() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        let account_list = production
            .split("pub async fn payable_account_list")
            .nth(1)
            .expect("应付列表")
            .split("pub async fn payable_account_detail")
            .next()
            .expect("应付列表函数体");
        assert!(account_list.contains("PayableAccountSummaryView"));
        assert!(!account_list.contains("payable_account_view("));
        assert!(!account_list.contains("resolve_optional_payment_recipient_for_read"));
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
