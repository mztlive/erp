//! 供应商付款单查询、银行回单与过账编排。

use std::collections::{HashMap, HashSet};

use database::{FileAssetExt, PartyExt, PayableExt, SupplierExt};
use entities::file_asset::BankReceiptEvidencePolicy;
use entities::party::PartyBankAccount;
use entities::payable::{
    AllocationAction, PayableAccount, PayableEntry, PaymentAllocation, PaymentAllocationLedger,
    PendingPaymentAllocation, SupplierPayment, SupplierPaymentData, SupplierPaymentStatus,
};
use entities::supplier::SupplierAccount;
use erp_audit::AuditExt;
use erp_core::common::time::Instant;
use erp_core::ids::{
    FileAssetId, PartyBankAccountId, PayableAccountId, PayableEntryId, PaymentAllocationId,
    SupplierAccountId, SupplierPaymentId,
};
use erp_core::money::Amount;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use id_generator::next_id;
use mongodb::{ClientSession, Database};
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::display;
use super::dto::{
    CommitSupplierPaymentRequest, PageView, PaymentRecipientView, SortDir, SupplierPaymentBankReceiptView,
    SupplierPaymentListParams, SupplierPaymentView,
};
use super::mapping::{payment_recipient_view, resolve_current_payment_recipient, zero_amount};
use super::payment_task;
use super::{PayableService, SupplierPaymentFilter, SupplierPaymentWithAssetsResult};
use crate::errors::{Error, Result};
use crate::file_asset::{FileAssetView, PendingFileAssetRequest};
use crate::pending_file_assets::PendingFileAssets;
use application_core::AuditActor;
use application_core::CommandReceipt;
use erp_audit::AuditActorLogs;
use erp_audit::CommandReceiptServiceExt as _;
use erp_identity::SharedRbacService;
use erp_workflow::service::approval::binding::BindPublishedDefinitionCommand;
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::document_registry::{new_registered_document, persist_registered_document};

impl PayableService {
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
        let payment_ids: Vec<SupplierPaymentId> = page
            .items
            .iter()
            .map(|row| SupplierPaymentId::new(row.id.clone()))
            .collect();
        let mut views = self.assemble_supplier_payment_views(&payment_ids, false).await?;
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
        let expected_task_version =
            erp_workflow::service::work_item::expected_task_version(&req.expected_task_version)?;
        let work_item_id = req.work_item_id.clone();
        let expected_payee_bank_account_id =
            PartyBankAccountId::new(req.expected_payee_bank_account_id.trim());
        let expected_payee_bank_account_version = req.expected_payee_bank_account_version;
        req.payment.validate()?;
        let allocations = req.pending_allocations()?;
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
        let policy_revision = self.rbac.current_policy_revision().await?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
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
                    )
                    .map_err(crate::errors::Error::from)?;
                    persist_unbound_supplier_payment_document(
                        &db,
                        &rbac,
                        object_read.as_ref(),
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

    /// 装配供应商付款单视图（FIN-R02 单笔入口，经批量装载保持与列表一致）。
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
        let mut views = self
            .assemble_supplier_payment_views(
                std::slice::from_ref(&SupplierPaymentId::new(id)),
                include_payment_recipient,
            )
            .await?;
        views
            .pop()
            .ok_or_else(|| Error::NotFound("供应商付款单不存在".to_string()))
    }

    /// 按付款 ID 集合批量装载并集中映射付款视图（FIN-R02）。
    ///
    /// 数据库往返固定：付款批量 1 次、核销分配 1 次、分配来源（分录／子账／
    /// 两类来源单号）共 4 次、供应商／主体／修订 3 次、银行回单 1 次、
    /// 冻结收款账户（仅详情）1 次，不随页长增长。响应映射、掩码与可见字段
    /// 选择集中在本函数；Repository 只返回实体事实，不返回 View、不执行
    /// 脱敏策略。缺失付款返回 `NotFound`；缺失分录／子账／主数据的行保持
    /// 对应展示字段为空；引用的银行回单缺失返回 `NotFound`，与原单笔语义一致。
    ///
    /// # 参数
    /// * `payment_ids` - 付款单 ID 集合（保持输入顺序装配）
    /// * `include_payment_recipient` - 是否加载冻结收款账户（仅详情）
    ///
    /// # 返回
    /// 返回与输入顺序对齐的付款单视图；空输入不访问数据库。
    ///
    /// # 错误
    /// 付款或其引用的银行回单缺失、仓储读取失败时返回错误。
    async fn assemble_supplier_payment_views(
        &self,
        payment_ids: &[SupplierPaymentId],
        include_payment_recipient: bool,
    ) -> Result<Vec<SupplierPaymentView>> {
        if payment_ids.is_empty() {
            return Ok(Vec::new());
        }
        let payments = self
            .db
            .supplier_payments()
            .find_supplier_payments_by_ids(payment_ids, &mut NoTransaction)
            .await?;
        let payments_by_id: HashMap<&str, &SupplierPayment> = payments
            .iter()
            .map(|payment| (payment.base.id.as_str(), payment))
            .collect();
        let mut ordered = Vec::with_capacity(payment_ids.len());
        for id in payment_ids {
            let payment = payments_by_id
                .get(id.as_ref())
                .ok_or_else(|| Error::NotFound("供应商付款单不存在".to_string()))?;
            ordered.push(*payment);
        }
        let allocations = self
            .db
            .payment_allocations()
            .find_allocations_by_payments(payment_ids, &mut NoTransaction)
            .await?;
        let mut allocations_by_payment: HashMap<String, Vec<&PaymentAllocation>> = HashMap::new();
        for allocation in &allocations {
            allocations_by_payment
                .entry(allocation.supplier_payment_id.to_string())
                .or_default()
                .push(allocation);
        }
        for group in allocations_by_payment.values_mut() {
            group.sort_by(|left, right| {
                left.allocation_seq
                    .cmp(&right.allocation_seq)
                    .then_with(|| left.base.id.cmp(&right.base.id))
            });
        }
        let mut grouped_views = Vec::with_capacity(ordered.len());
        let mut allocated_totals = Vec::with_capacity(ordered.len());
        for payment in &ordered {
            let group = allocations_by_payment
                .get(payment.base.id.as_str())
                .cloned()
                .unwrap_or_default();
            let owned: Vec<PaymentAllocation> = group.into_iter().map(|item| (*item).clone()).collect();
            let (allocated_total, views) = payment_allocation_view(&owned);
            grouped_views.push(views);
            allocated_totals.push(allocated_total);
        }
        let enriched_groups =
            display::enrich_payment_allocation_views_batched(&self.db, grouped_views).await?;
        let supplier_displays = self.supplier_displays_by_ids(&ordered).await?;
        let receipt_views = self.bank_receipt_views_by_ids(&ordered).await?;
        let recipient_views = if include_payment_recipient {
            self.recipient_views_by_ids(&ordered).await?
        } else {
            HashMap::new()
        };
        let mut views = Vec::with_capacity(ordered.len());
        for ((payment, allocated_total), enriched) in
            ordered.into_iter().zip(allocated_totals).zip(enriched_groups)
        {
            let (supplier_no, supplier_name) = supplier_displays
                .get(payment.base.id.as_str())
                .cloned()
                .unwrap_or((None, None));
            views.push(SupplierPaymentView {
                id: payment.base.id.clone(),
                payment_no: payment.payment_no.clone(),
                status: payment.status,
                supplier_id: payment.supplier_id.to_string(),
                supplier_no,
                supplier_name,
                payment_recipient: recipient_views.get(payment.base.id.as_str()).cloned(),
                paid_at: payment.paid_at,
                amount: payment.amount,
                bank_reference: payment.bank_reference.clone(),
                bank_receipt: receipt_views.get(payment.base.id.as_str()).cloned(),
                version: payment.base.version,
                created_at: payment.base.created_at,
                unallocated_amount: payment.amount.checked_sub(allocated_total),
                allocated_total,
                allocations: enriched,
                related_reversals: Vec::new(),
            });
        }
        Ok(views)
    }

    /// 按付款集合一次批量解析供应商展示名（FIN-R02）。
    ///
    /// 供应商、主体、修订各一次 `$in` 查询；主数据或来源修订缺失时对应字段
    /// 为空，不阻断列表。
    async fn supplier_displays_by_ids(
        &self,
        payments: &[&SupplierPayment],
    ) -> Result<HashMap<String, (Option<String>, Option<String>)>> {
        let mut seen = HashSet::new();
        let mut supplier_ids = Vec::new();
        for payment in payments {
            if seen.insert(payment.supplier_id.to_string()) {
                supplier_ids.push(payment.supplier_id.clone());
            }
        }
        let suppliers = self
            .db
            .supplier_accounts()
            .find_accounts_by_ids(&supplier_ids, &mut NoTransaction)
            .await?;
        let mut seen_parties = HashSet::new();
        let mut party_ids = Vec::new();
        for supplier in &suppliers {
            if seen_parties.insert(supplier.party_id.to_string()) {
                party_ids.push(supplier.party_id.clone());
            }
        }
        let parties = self
            .db
            .parties()
            .find_parties_by_ids(&party_ids, &mut NoTransaction)
            .await?;
        let revision_ids: Vec<String> = parties
            .iter()
            .filter_map(|party| party.stable.current_revision_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let revisions = self
            .db
            .party_revisions()
            .find_revisions_by_ids(&revision_ids, &mut NoTransaction)
            .await?;
        let suppliers_by_id: HashMap<&str, &SupplierAccount> = suppliers
            .iter()
            .map(|supplier| (supplier.base.id.as_str(), supplier))
            .collect();
        let parties_by_id: HashMap<String, Option<String>> = parties
            .iter()
            .map(|party| (party.base.id.clone(), party.stable.current_revision_id.clone()))
            .collect();
        let names_by_revision: HashMap<&str, &str> = revisions
            .iter()
            .map(|revision| (revision.base.id.as_str(), revision.legal_name.as_str()))
            .collect();
        let mut displays = HashMap::with_capacity(payments.len());
        for payment in payments {
            let display = suppliers_by_id
                .get(payment.supplier_id.as_ref())
                .map(|supplier| {
                    let supplier_no = Some(supplier.supplier_no.clone());
                    let supplier_name = parties_by_id
                        .get(supplier.party_id.as_ref())
                        .and_then(|revision| revision.as_deref())
                        .and_then(|revision_id| {
                            names_by_revision.get(revision_id).map(|name| name.to_string())
                        });
                    (supplier_no, supplier_name)
                })
                .unwrap_or((None, None));
            displays.insert(payment.base.id.clone(), display);
        }
        Ok(displays)
    }

    /// 按付款集合一次批量装载银行回单安全元数据（FIN-R02）。
    ///
    /// 只返回安全展示字段；付款引用的回单缺失时返回 `NotFound`，
    /// 与原单笔语义一致。
    async fn bank_receipt_views_by_ids(
        &self,
        payments: &[&SupplierPayment],
    ) -> Result<HashMap<String, SupplierPaymentBankReceiptView>> {
        let mut seen = HashSet::new();
        let mut asset_ids = Vec::new();
        for payment in payments {
            if let Some(asset_id) = payment.bank_receipt_asset_id.as_ref() {
                if seen.insert(asset_id.to_string()) {
                    asset_ids.push(FileAssetId::new(asset_id.to_string()));
                }
            }
        }
        let assets = self
            .db
            .file_assets()
            .find_by_ids(&asset_ids, &mut NoTransaction)
            .await?;
        let assets_by_id: HashMap<&str, &entities::file_asset::FileAsset> = assets
            .iter()
            .map(|asset| (asset.base.id.as_str(), asset))
            .collect();
        let mut views = HashMap::with_capacity(payments.len());
        for payment in payments {
            if let Some(asset_id) = payment.bank_receipt_asset_id.as_ref() {
                let asset = assets_by_id
                    .get(asset_id.as_ref())
                    .ok_or_else(|| Error::NotFound("银行回单不存在".to_string()))?;
                views.insert(
                    payment.base.id.clone(),
                    SupplierPaymentBankReceiptView {
                        asset_id: asset.base.id.clone(),
                        file_name: asset.file_name.clone(),
                        content_type: asset.content_type.clone(),
                        byte_size: asset.byte_size,
                    },
                );
            }
        }
        Ok(views)
    }

    /// 按付款集合一次批量装载冻结收款账户掩码视图（FIN-R02，仅详情）。
    ///
    /// 账户缺失时对应付款的收款账户为空；掩码只含末四位，
    /// 不泄漏敏感明文。
    async fn recipient_views_by_ids(
        &self,
        payments: &[&SupplierPayment],
    ) -> Result<HashMap<String, PaymentRecipientView>> {
        let mut seen = HashSet::new();
        let mut account_ids = Vec::new();
        for payment in payments {
            if let Some(account_id) = payment.payee_bank_account_id.as_ref() {
                if seen.insert(account_id.to_string()) {
                    account_ids.push(PartyBankAccountId::new(account_id.to_string()));
                }
            }
        }
        let accounts = self
            .db
            .party_bank_accounts()
            .find_bank_accounts_by_ids(&account_ids, &mut NoTransaction)
            .await?;
        let accounts_by_id: HashMap<&str, &PartyBankAccount> = accounts
            .iter()
            .map(|account| (account.base.id.as_str(), account))
            .collect();
        let mut views = HashMap::new();
        for payment in payments {
            if let Some(account_id) = payment.payee_bank_account_id.as_ref() {
                if let Some(account) = accounts_by_id.get(account_id.as_ref()) {
                    views.insert(payment.base.id.clone(), payment_recipient_view(account));
                }
            }
        }
        Ok(views)
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
fn payment_recipient_lock_error(error: persistence_core::Error) -> Error {
    match error {
        persistence_core::Error::OptimisticLockingError
        | persistence_core::Error::TransientTransactionConflict(_) => {
            Error::ConflictError("供应商收款账户已变化，请刷新付款任务并重新核对".to_string())
        }
        other => other.into(),
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

/// 证明供应商付款为无审批单据并持久化未绑定注册行。
///
/// # 错误
/// 政策不是 `NO_APPROVAL`、绑定端口意外返回定义或注册写入失败时返回错误。
async fn persist_unbound_supplier_payment_document(
    db: &Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let binding = crate::workflow_compose::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        session,
    )
    .await?;
    if binding.is_some() || document.approval_binding.is_some() {
        return Err(Error::Internal(
            "供应商付款为 NO_APPROVAL，不得写入审批绑定".to_string(),
        ));
    }
    persist_registered_document(db, &document, session)
        .await
        .map_err(crate::errors::Error::from)
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
