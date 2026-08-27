//! 域 D19 `payable` 服务编排（页面：W12 供应商往来）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 供应商付款创建必须在同一事务注册 `BusinessDocument` 并绑定发布定义；
//! - 单集合草稿写入（付款单草稿）→ `&mut NoTransaction`；
//! - 跨集合资金/票款过账（§8.3 不变量）→
//!   `database::Transactional::with_transaction`。
//! - 资金类入口（付款过账、进项发票登记）以业务唯一键
//!   （付款单号/规范化发票号码）与状态迁移构成去重机制。
//!   付款过账只能作为审批最终通过动作。
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
    AccessControlExt, DocumentRegistryExt, FileAssetExt, NoTransaction, PartyExt, PayableExt,
    PurchaseOrderExt, ReceivableExt, SupplierExt, Transactional,
};
use entities::common::time::Instant;
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::file_asset::{RetentionClass, SensitivityClass};
use entities::ids::{
    FileAssetId, InvoiceId, PayableAccountId, PayableEntryId, PaymentAllocationId,
    PurchaseInvoiceAllocationId, SupplierAccountId, SupplierPaymentId,
};
use entities::money::Amount;
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
    attach_published_binding, bind_published_definition_on_document_create, BindPublishedDefinitionCommand,
};
use crate::approval::business_adapter::BindingRevalidationContext;
use crate::approval::execution::{prepare_cancel, prepare_start};
use crate::audit::{AuditActor, CommandReceipt};
use crate::document_registry::{find_approval_binding, new_registered_document};
use crate::errors::{Error, Result};
use crate::file_asset::{FileAssetView, PendingFileAssetRequest};
use crate::iam::{self, SharedRbacService};
use crate::pending_file_assets::PendingFileAssets;

mod adapter;
mod cancel_approval;
mod dto;
pub(crate) mod payment_task;
mod start_approval;

pub use self::adapter::supplier_payment_object_readable;
use self::adapter::{
    build_supplier_payment_snapshot, document_approval_view, ensure_final_approve_posting,
    execute_supplier_payment_domain_action, pending_allocations_from_request, require_frozen_binding,
    start_supplier_payment_approval, supplier_payment_adapter, supplier_payment_responsible_org_id,
    supplier_payment_subject_ref,
};
use self::cancel_approval::{
    build_supplier_payment_cancel_input, load_cancel_runtime, persist_supplier_payment_cancel,
    SupplierPaymentCancelPersistInput,
};
use self::dto::SortDir;
pub use self::dto::{
    CancelSupplierPaymentApprovalRequest, CommitSupplierPaymentRequest, CreatePayableAccountRequest,
    CreateSupplierPaymentRequest, DocumentApprovalView, PageView, PayableAccountListParams,
    PayableAccountView, PaymentAllocationLineRequest, PaymentAllocationView, PostSupplierPaymentRequest,
    PurchaseInvoiceAllocationListParams, PurchaseInvoiceAllocationView, PurchaseInvoiceRegisteredView,
    RegisterPurchaseInvoiceRequest, SubmitSupplierPaymentRequest, SupplierPaymentBankReceiptView,
    SupplierPaymentListParams, SupplierPaymentView,
};
use self::start_approval::{
    build_supplier_payment_start_input, load_bound_definition_graph_with_executor,
    load_start_receipt_with_executor, persist_supplier_payment_start_in_transaction,
    SupplierPaymentStartInput, SupplierPaymentStartPersistInput,
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

    /// 登记供应商付款草稿，并在同一事务绑定已发布审批定义。
    ///
    /// 付款单号全局唯一（唯一索引）构成幂等去重。绑定失败必须回滚业务实体，
    /// 不得把绑定推迟到提交。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建付款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 供应商不存在
    /// * `ConflictError` - 付款单号重复或流程未配置
    pub async fn create_supplier_payment(
        &self,
        req: CreateSupplierPaymentRequest,
        actor: &AuditActor,
    ) -> Result<SupplierPaymentView> {
        req.validate()?;
        self.ensure_existing_bank_receipt(&req.bank_receipt_asset_id)
            .await?;
        let payment = SupplierPayment::new(
            SupplierPaymentId::new(next_id()),
            SupplierPaymentData {
                payment_no: req.payment_no,
                supplier_id: req.supplier_id,
                paid_at: req.paid_at,
                amount: req.amount,
                bank_reference: req.bank_reference,
                bank_receipt_asset_id: req.bank_receipt_asset_id,
            },
        )?;
        persist_created_supplier_payment(&self.db, &self.rbac, payment.clone(), actor.clone()).await?;
        self.supplier_payment_detail(&payment.base.id).await
    }

    /// 原子创建或提交供应商付款并启动审批。
    ///
    /// 不携带新上传对象的内部兼容入口；HTTP 付款工作台使用
    /// [`Self::commit_supplier_payment_with_assets`]。
    ///
    /// # 错误
    /// 参数组合、银行回单、任务责任、审批绑定或事务提交不合法时返回错误。
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

    /// 原子创建或提交供应商付款、登记银行回单并启动审批。
    ///
    /// 新付款的单据注册与定义绑定、付款实体、冻结核销分配、审批运行事实、
    /// 不可变快照、入口任务和审计全部位于同一事务。已有草稿用乐观锁校验后
    /// 走同一启动事务，前端不得再执行“先创建草稿、再提交”。
    ///
    /// # 参数
    /// * `req` - 新付款或已有草稿身份、冻结分配与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回进入审批后的付款单视图。
    ///
    /// # 错误
    /// * `ValidationError` - 参数组合或分配不合法
    /// * `ConflictError` - 草稿版本、状态、绑定或审批定义冲突
    /// * `NotFound` - 已有草稿或供应商不存在
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
        let receipt_override = req.bank_receipt_asset_id;
        let new_payment = match (&req.payment_id, req.expected_version, req.payment) {
            (None, None, Some(create)) if receipt_override.is_none() => {
                create.validate()?;
                Some(SupplierPayment::new(
                    SupplierPaymentId::new(next_id()),
                    SupplierPaymentData {
                        payment_no: create.payment_no,
                        supplier_id: create.supplier_id,
                        paid_at: create.paid_at,
                        amount: create.amount,
                        bank_reference: create.bank_reference,
                        bank_receipt_asset_id: create.bank_receipt_asset_id,
                    },
                )?)
            }
            (Some(_), Some(version), None) if version > 0 => None,
            _ => {
                return Err(Error::ValidationError(
                    "新付款必须提交 payment；已有草稿必须提交 payment_id 与 expected_version".to_string(),
                ));
            }
        };
        let requested_id = req.payment_id.clone();
        let expected_version = req.expected_version;
        let allocations = pending_allocations_from_request(&req.allocations)?;
        let idempotency_key = req.idempotency_key;
        let adapter = supplier_payment_adapter()?;
        let policy_revision = self.rbac.current_policy_revision().await?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor_owned = actor.clone();
        let command_receipt_for_tx = command_receipt.clone();
        let transaction_result = rbac
            .clone()
            .run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    let (mut payment, binding, organization_id) = match new_payment {
                        Some(candidate) => {
                            if db
                                .supplier_payments()
                                .find_by_payment_no(&candidate.payment_no, session)
                                .await?
                                .is_some()
                            {
                                return Err(Error::ConflictError("付款单号已存在，请刷新后重试".to_string()));
                            }
                            let supplier = db
                                .supplier_accounts()
                                .find_by_id(candidate.supplier_id.as_ref(), session)
                                .await?
                                .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
                            let organization_id =
                                supplier_payment_responsible_org_id(supplier.party_id.as_ref())?;
                            let bind_command = BindPublishedDefinitionCommand {
                                document_type: DocumentType::SupplierPayment,
                                business_object_id: candidate.base.id.clone(),
                                business_object_version: candidate.base.version,
                                context: BindingRevalidationContext {
                                    organization_id: organization_id.clone(),
                                    creator_id: actor_owned.id().to_string(),
                                },
                            };
                            let document = new_registered_document(
                                &candidate.base.id,
                                DocumentType::SupplierPayment,
                                candidate.payment_no.clone(),
                            )?;
                            let binding = persist_bound_supplier_payment_document(
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
                                candidate.require_bank_receipt()?,
                                &pending_assets,
                                session,
                            )
                            .await?;
                            pending_assets.persist(&db, session).await?;
                            db.supplier_payments().create(&candidate, session).await?;
                            let audit = actor_owned.clone().resource_log(
                                "supplier_payment.create",
                                "supplier_payment",
                                candidate.base.id.clone(),
                            )?;
                            db.audit_logs().create(&audit, session).await?;
                            (candidate, binding, organization_id)
                        }
                        None => {
                            let payment_id = requested_id
                                .as_deref()
                                .ok_or_else(|| Error::ValidationError("已有付款缺少主键".to_string()))?;
                            let mut payment =
                                db.supplier_payments()
                                    .find_by_id(payment_id, session)
                                    .await?
                                    .ok_or_else(|| Error::NotFound("供应商付款单不存在".to_string()))?;
                            ensure_expected_version(
                                payment.base.version,
                                expected_version.ok_or_else(|| {
                                    Error::ValidationError("已有付款缺少期望版本".to_string())
                                })?,
                            )?;
                            if let Some(asset_id) = receipt_override {
                                payment.replace_bank_receipt(asset_id)?;
                            }
                            ensure_bank_receipt_asset_in_transaction(
                                &db,
                                payment.require_bank_receipt()?,
                                &pending_assets,
                                session,
                            )
                            .await?;
                            pending_assets.persist(&db, session).await?;
                            let binding = find_approval_binding(&db, payment_id, session)
                                .await?
                                .ok_or_else(|| {
                                    Error::ConflictError("供应商付款单缺少审批绑定".to_string())
                                })?;
                            let supplier = db
                                .supplier_accounts()
                                .find_by_id(payment.supplier_id.as_ref(), session)
                                .await?
                                .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
                            let organization_id =
                                supplier_payment_responsible_org_id(supplier.party_id.as_ref())?;
                            (payment, binding, organization_id)
                        }
                    };
                    let binding = require_frozen_binding(Some(&binding))?.clone();
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
                    start_supplier_payment_approval(&mut payment, allocations)?;
                    let id = payment.base.id.clone();
                    let subject = supplier_payment_subject_ref(&id)?;
                    let now = Instant::now();
                    let snapshot =
                        build_supplier_payment_snapshot(&payment, &organization_id, actor_owned.id(), now)?;
                    let _ = supplier_payment_object_readable(&organization_id, actor_owned.id())?;
                    let graph = load_bound_definition_graph_with_executor(&db, &binding, session).await?;
                    let existing_start_receipt = load_start_receipt_with_executor(
                        &db,
                        &subject,
                        payment.approval_subject_version,
                        &idempotency_key,
                        session,
                    )
                    .await?;
                    let start_input = build_supplier_payment_start_input(SupplierPaymentStartInput {
                        graph,
                        binding: &binding,
                        subject,
                        subject_version: payment.approval_subject_version,
                        actor_id: actor_owned.id(),
                        organization_id: &organization_id,
                        idempotency_key: &idempotency_key,
                        receipt: existing_start_receipt,
                        now,
                    })?;
                    let prepared = prepare_start(start_input)?;
                    let committed = persist_supplier_payment_start_in_transaction(
                        &db,
                        SupplierPaymentStartPersistInput {
                            payment,
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
                    Ok::<SupplierPayment, crate::errors::Error>(committed)
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

    /// 提交供应商付款并调用统一 `start_approval`。
    ///
    /// 按合同 §4.4.1 冻结 `approval_subject_version` 与 `subject_snapshot`，
    /// 单据进入 `IN_APPROVAL`。定义与审批人取自已绑定事实，不接受客户端选择。
    ///
    /// # 参数
    /// * `id` - 付款单主键
    /// * `req` - 提交请求（版本、幂等键与冻结分配）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交后的付款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 付款单或供应商不存在
    /// * `ConflictError` - 非草稿、无绑定或并发冲突
    pub async fn submit_supplier_payment(
        &self,
        id: &str,
        req: SubmitSupplierPaymentRequest,
        actor: &AuditActor,
    ) -> Result<SupplierPaymentView> {
        self.commit_supplier_payment(
            CommitSupplierPaymentRequest {
                work_item_id: req.work_item_id,
                expected_task_version: req.expected_task_version,
                payment_id: Some(id.to_string()),
                expected_version: Some(req.expected_version),
                payment: None,
                bank_receipt_asset_id: None,
                allocations: req.allocations,
                idempotency_key: req.idempotency_key,
            },
            actor,
        )
        .await
    }

    /// 撤回供应商付款审批，成功后回到草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// # 参数
    /// * `id` - 付款单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回撤回后的付款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 付款单不存在
    /// * `ConflictError` - 非审批中、已最终通过或并发冲突
    pub async fn cancel_supplier_payment_approval(
        &self,
        id: &str,
        req: CancelSupplierPaymentApprovalRequest,
        actor: &AuditActor,
    ) -> Result<SupplierPaymentView> {
        req.validate()?;
        let mut payment = self.load_supplier_payment(id).await?;
        ensure_expected_version(payment.base.version, req.expected_version)?;
        self.persist_cancelled_supplier_payment(id, &mut payment, &req, actor)
            .await?;
        self.supplier_payment_detail(id).await
    }

    /// 客户端直接过账失败关闭。最终动作只能由审批运行时调用。
    ///
    /// # 返回
    /// 恒返回冲突。
    ///
    /// # 错误
    /// 恒返回 `ConflictError`。
    pub fn reject_client_post() -> Result<SupplierPaymentView> {
        Err(Error::ConflictError(
            "供应商付款过账只能由审批最终通过动作执行，客户端不得直接过账".to_string(),
        ))
    }

    /// 加载撤回运行事实并写回草稿。
    ///
    /// # 错误
    /// 无绑定、实例终态或写入失败时返回错误。
    async fn persist_cancelled_supplier_payment(
        &self,
        id: &str,
        payment: &mut SupplierPayment,
        req: &CancelSupplierPaymentApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let adapter = supplier_payment_adapter()?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let subject = supplier_payment_subject_ref(id)?;
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, payment.approval_subject_version).await?;
        let now = Instant::now();
        let input = build_supplier_payment_cancel_input(
            &runtime,
            &req.reason,
            actor.id(),
            &req.idempotency_key,
            None,
            now,
        )?;
        let prepared = prepare_cancel(input)?;
        execute_supplier_payment_domain_action(payment, adapter.cancel_action)?;
        let audit = actor.clone().resource_log(
            "supplier_payment.cancel_approval",
            "supplier_payment",
            id.to_string(),
        )?;
        persist_supplier_payment_cancel(
            &self.db,
            SupplierPaymentCancelPersistInput {
                payment: payment.clone(),
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

    /// 校验 JSON 创建入口引用的既有银行回单资产。
    async fn ensure_existing_bank_receipt(&self, asset_id: &FileAssetId) -> Result<()> {
        let asset = self
            .db
            .file_assets()
            .find_by_id(asset_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("银行回单不存在".to_string()))?;
        validate_bank_receipt_metadata(
            &asset.content_type,
            asset.sensitivity_class,
            asset.retention_class,
            asset.destroyed_at.is_some(),
        )
    }

    /// 最终通过过账并核销（§8.3-1 事务不变量）。
    ///
    /// 作为合同 `on_final_approve`，仅 `IN_APPROVAL` 可进入过账。同一事务内：
    /// 校验付款与应付分录同一供应商、分录开放余额与付款剩余余额；写提交时
    /// 冻结的核销分配（`APPLY`）；按条件原子更新子账已核销进度。
    ///
    /// # 参数
    /// * `id` - 付款单 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后付款单视图。
    ///
    /// # 错误
    /// * `NotFound` - 付款单或应付分录不存在
    /// * `ConflictError` - 非审批中
    /// * `BusinessLogicError` - 跨供应商核销、超额核销或重复过账
    pub async fn post_supplier_payment(&self, id: &str, actor: &AuditActor) -> Result<SupplierPaymentView> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let payment_id = id.to_string();
        let detail_id = payment_id.clone();
        let audit_action = format!("supplier_payment.post:{id}");
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
                    ensure_final_approve_posting(&payment)?;
                    execute_supplier_payment_domain_action(
                        &mut payment,
                        crate::approval::policy::ApprovalDomainAction::SupplierPaymentPost,
                    )?;

                    let existing = db
                        .payment_allocations()
                        .find_allocations_by_payments(&[payment.base.id.clone().into()], session)
                        .await?;
                    let net_allocated = net_payment_allocated(&existing);
                    if net_allocated.checked_add(pending_allocated_total(&payment.pending_allocations))
                        > payment.amount
                    {
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
                    let pending = payment.pending_allocations.clone();
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
                    for (line_index, line) in new_allocations.iter().enumerate() {
                        let entry = db
                            .payable_entries()
                            .find_by_id(&line.payable_entry_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("应付分录不存在".to_string()))?;
                        let applied = db
                            .payable_accounts()
                            .apply_settlement(
                                &entry.payable_account_id,
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
                        affected_accounts.insert(entry.payable_account_id);
                    }
                    for account_id in affected_accounts {
                        payment_task::sync_purchase_payment_task(&db, &account_id, session).await?;
                    }
                    payment.mark_posted()?;
                    db.supplier_payments().update(&mut payment, session).await?;
                    for allocation in &new_allocations {
                        db.payment_allocations().create(allocation, session).await?;
                    }
                    let audit = actor_owned.clone().resource_log(
                        &audit_action,
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
        let (supplier_no, supplier_name) =
            resolve_supplier_display(&self.db, account.supplier_id.as_ref()).await?;
        let source_document_no = resolve_source_document_no(&self.db, &account).await?;
        Ok(PayableAccountView {
            id: account.base.id.clone(),
            source_document_id: account.source_document_id,
            source_document_no,
            supplier_id: account.supplier_id.to_string(),
            supplier_no,
            supplier_name,
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
        let binding = match find_approval_binding(&self.db, &id, &mut NoTransaction).await {
            Ok(binding) => binding,
            Err(Error::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let bank_receipt =
            payment_bank_receipt_view(&self.db, payment.bank_receipt_asset_id.as_ref()).await?;
        Ok(SupplierPaymentView {
            id: payment.base.id.clone(),
            payment_no: payment.payment_no,
            status: payment.status,
            supplier_id: payment.supplier_id.to_string(),
            paid_at: payment.paid_at,
            amount: payment.amount,
            bank_reference: payment.bank_reference,
            bank_receipt,
            version: payment.base.version,
            created_at: payment.base.created_at,
            unallocated_amount: payment.amount.checked_sub(allocated_total),
            allocated_total,
            allocations: views,
            approval: document_approval_view(binding.as_ref(), None, payment.status),
        })
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

/// 解析新付款或既有草稿覆盖字段中的临时回单引用。
fn resolve_payment_receipt_references(
    req: &mut CommitSupplierPaymentRequest,
    pending_assets: &PendingFileAssets,
) -> Result<HashSet<String>> {
    let mut used = HashSet::new();
    if let Some(payment) = req.payment.as_mut() {
        pending_assets.resolve_id(&mut payment.bank_receipt_asset_id, &mut used)?;
    }
    if let Some(asset_id) = req.bank_receipt_asset_id.as_mut() {
        pending_assets.resolve_id(asset_id, &mut used)?;
    }
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

/// 在创建事务内写入付款单、绑定发布定义并登记单据。
///
/// 绑定失败必须回滚业务实体，不得留下以后补流程的单据。
///
/// # 错误
/// 无发布定义、人员重验失败或写入失败时返回错误。
async fn persist_created_supplier_payment(
    db: &Database,
    rbac: &SharedRbacService,
    payment: SupplierPayment,
    actor: AuditActor,
) -> Result<()> {
    let organization_id = load_supplier_responsible_org_id(db, &payment.supplier_id).await?;
    let bind_command = BindPublishedDefinitionCommand {
        document_type: DocumentType::SupplierPayment,
        business_object_id: payment.base.id.clone(),
        business_object_version: payment.base.version,
        context: BindingRevalidationContext {
            organization_id,
            creator_id: actor.id().to_string(),
        },
    };
    let document = new_registered_document(
        &payment.base.id,
        DocumentType::SupplierPayment,
        payment.payment_no.clone(),
    )?;
    let audit = actor.clone().resource_log(
        "supplier_payment.create",
        "supplier_payment",
        payment.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                persist_bound_supplier_payment_document(&db, &rbac, document, &bind_command, &actor, session)
                    .await?;
                db.supplier_payments().create(&payment, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 查询供应商往来主体作为责任组织。
///
/// # 错误
/// 供应商不存在或往来主体为空时返回错误。
async fn load_supplier_responsible_org_id(db: &Database, supplier_id: &SupplierAccountId) -> Result<String> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(supplier_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    supplier_payment_responsible_org_id(supplier.party_id.as_ref())
}

/// 查询发布定义、写入绑定并持久化注册行。
///
/// # 错误
/// 无发布定义或绑定失败时返回错误。
async fn persist_bound_supplier_payment_document(
    db: &Database,
    rbac: &SharedRbacService,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<entities::document_registry::business_document::ApprovalDefinitionBinding> {
    let _ = supplier_payment_object_readable(
        &bind_command.context.organization_id,
        &bind_command.context.creator_id,
    )?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, session).await?;
    let binding = binding.ok_or_else(|| Error::Internal("供应商付款单必须绑定已发布定义".to_string()))?;
    attach_published_binding(&mut document, binding.clone())?;
    db.business_documents().create(&document, session).await?;
    Ok(binding)
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
mod supplier_payment_approval_tests {
    use super::{
        execute_supplier_payment_domain_action, start_supplier_payment_approval,
        validate_bank_receipt_metadata, PayableService,
    };
    use crate::approval::policy::ApprovalDomainAction;
    use entities::common::time::Instant;
    use entities::file_asset::{RetentionClass, SensitivityClass};
    use entities::ids::{FileAssetId, PayableEntryId, SupplierAccountId, SupplierPaymentId};
    use entities::money::Amount;
    use entities::payable::{
        PendingPaymentAllocation, SupplierPayment, SupplierPaymentData, SupplierPaymentStatus,
    };
    use std::str::FromStr;

    fn draft_payment() -> SupplierPayment {
        SupplierPayment::new(
            SupplierPaymentId::new("sp-1"),
            SupplierPaymentData {
                payment_no: "SP-1".into(),
                supplier_id: SupplierAccountId::new("sup-1"),
                paid_at: Instant::from_unix_secs(1),
                amount: Amount::from_str("100").expect("金额合法"),
                bank_reference: None,
                bank_receipt_asset_id: FileAssetId::new("asset-receipt-1"),
            },
        )
        .expect("草稿必须可构造")
    }

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

    /// 创建必须注册 BusinessDocument 并绑定发布定义。
    #[test]
    fn create_registers_document_and_binds_published_definition() {
        let source = include_str!("mod.rs");
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::SupplierPayment"));
        assert!(source.contains("persist_created_supplier_payment"));
    }

    /// 对象读取权必须已接线；组织覆盖时绑定闸门不得报未接线。
    #[test]
    fn object_read_is_wired_for_binding() {
        use crate::approval::business_adapter::{
            adapter_object_read_decision, adapter_spec_of, revalidate_assignee_binding_access,
            BindingRevalidationContext,
        };
        use entities::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
        use entities::document_registry::DocumentType;
        use entities::ids::DataScopeId;

        let spec = adapter_spec_of(DocumentType::SupplierPayment).expect("供应商付款必须有适配器");
        let context = BindingRevalidationContext {
            organization_id: "org-1".to_string(),
            creator_id: "creator-1".to_string(),
        };
        assert_eq!(
            adapter_object_read_decision(&spec, &context, "u1").expect("读取权必须已接线"),
            Some(true)
        );

        let role_company = DataScope::new(
            DataScopeId::new("ds-role"),
            DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: "role-1".to_string(),
                scope_type: DataScopeType::Company,
                scope_targets: Vec::new(),
            },
        )
        .expect("角色范围夹具");
        let user_org = DataScope::new(
            DataScopeId::new("ds-user"),
            DataScopeData {
                subject_type: DataScopeSubjectType::User,
                subject_id: "u1".to_string(),
                scope_type: DataScopeType::Organization,
                scope_targets: vec!["org-1".to_string()],
            },
        )
        .expect("用户范围夹具");
        revalidate_assignee_binding_access(
            &spec,
            std::slice::from_ref(&user_org),
            std::slice::from_ref(&role_company),
            &context,
            "u1",
        )
        .expect("组织覆盖时不得报对象读取权未接线");
    }

    /// 提交必须绑定付款执行任务并在同一事务启动审批。
    #[test]
    fn submit_calls_start_approval_with_subject_version() {
        let source = include_str!("mod.rs");
        assert!(source.contains("pub async fn submit_supplier_payment"));
        assert!(source.contains("record_payment_execution"));
        assert!(source.contains("payment.approval_subject_version"));
        assert!(source.contains("prepare_start"));
    }

    /// 最终动作唯一为 post_supplier_payment，且客户端过账旁路关闭。
    #[test]
    fn final_action_is_post_supplier_payment() {
        let source = include_str!("mod.rs");
        assert!(source.contains("pub async fn post_supplier_payment"));
        assert!(source.contains("payment.mark_posted"));
        assert!(source.contains("SupplierPaymentPost"));
        assert!(source.contains("pending_allocations"));
        assert!(PayableService::reject_client_post().is_err());
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = include_str!("mod.rs");
        assert!(source.contains("pub async fn cancel_supplier_payment_approval"));
        assert!(source.contains("prepare_cancel"));
        assert!(source.contains("persist_supplier_payment_cancel"));
        let _ = PayableService::reject_client_post();
        let mut payment = draft_payment();
        start_supplier_payment_approval(
            &mut payment,
            vec![PendingPaymentAllocation::new(
                PayableEntryId::new("pe-1"),
                Amount::from_str("10").expect("金额合法"),
            )
            .expect("分配合法")],
        )
        .unwrap();
        execute_supplier_payment_domain_action(
            &mut payment,
            ApprovalDomainAction::SupplierPaymentCancelApproval,
        )
        .unwrap();
        assert_eq!(payment.status, SupplierPaymentStatus::Draft);
        assert_eq!(payment.approval_subject_version, 1);
    }

    /// 生产代码不得保留草稿直接过账或待复核旁路。
    #[test]
    fn production_closes_draft_post_and_pending_review() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("SupplierPaymentStatus::PendingReview"));
        assert!(!production.contains("Draft =>"));
        assert!(!production.contains("pending_review"));
    }
}
