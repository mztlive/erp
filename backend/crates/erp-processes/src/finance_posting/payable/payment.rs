//! 供应商付款单查询、银行回单与过账编排。

use std::collections::HashSet;
use std::sync::Arc;

use application_core::{AuditActor, CommandReceipt};
use erp_audit::{AuditAction, AuditActorLogs, AuditField, AuditFieldKind, BusinessEventContext};
use erp_core::ids::{FileAssetId, PartyBankAccountId, PartyId, SupplierPaymentId, WorkItemId};
use erp_finance::entity::payable::{PendingPaymentAllocation, SupplierPayment, SupplierPaymentData};
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_finance::service::payable::PaymentSettlementFacts;
use erp_identity::SharedRbacService;
use erp_party::PartyExt;
use erp_supplier::{SupplierAccount, SupplierExt};
use erp_support::{
    BankReceiptEvidencePolicy, EmptyPendingAttachments, FileAssetExt, FileAssetView, PendingAttachmentBatch,
};
use erp_workflow::ApprovalObjectReadPort;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::service::approval::binding::BindPublishedDefinitionCommand;
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::document_registry::{new_registered_document, persist_registered_document};
use erp_workflow::service::work_item::expected_task_version;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::dto::{CommitSupplierPaymentRequest, SupplierPaymentView};
use super::mapping::resolve_current_party_payment_recipient;
use super::posting::post_supplier_payment;
use super::{
    PayableService, SupplierPaymentBankReceiptSnapshot, SupplierPaymentWithAssetsResult, payment_task,
};
use crate::adapters::{funds_access_with_rbac, purchase_access};
use crate::audit::persist_log;
use crate::finance_posting::command_recovery::{recovered_resource, recovered_view};
use crate::{Error, Result};

const PAYMENT_COMMIT: AuditAction = AuditAction {
    code: "supplier_payment.commit",
    resource_type: "supplier_payment",
    label: "登记并过账供应商付款",
    version: 1,
    allowed_fields: &[AuditField { code: "amount", label: "付款金额", kind: AuditFieldKind::Amount }],
};

impl PayableService {
    /// 读取付款单归属的银行回单元数据，并记录受控预览审计。
    ///
    /// 完整对象资格、付款引用和文件资产在同一事务 Executor 读取，资格通过后
    /// 记录敏感预览审计；实际对象字节由 HTTP 层在事务外读取。
    ///
    /// # 错误
    /// 付款单、回单引用或文件资产不存在，以及审计写入失败时返回错误。
    pub async fn supplier_payment_bank_receipt(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SupplierPaymentBankReceiptSnapshot> {
        let snapshot = self.bank_receipt_snapshot(id, actor).await?;
        let audit = actor.clone().resource_log(
            "supplier_payment.bank_receipt.preview",
            "supplier_payment",
            id.to_string(),
        )?;
        persist_log(&self.db, &audit, &mut NoTransaction).await?;
        Ok(snapshot)
    }

    /// 在对象字节读取后重验付款引用与银行回单治理事实。
    ///
    /// 本方法在新的事务 Executor 内读取当前完整资格、付款和文件元数据，
    /// 不复用授权或治理缓存，不重复审计；对象缓存命中使用同一重验入口。
    ///
    /// # 参数
    /// * `id` - 当前请求的付款单身份
    /// * `actor` - 当前已认证操作人，重新读取其当前完整对象资格
    /// * `expected` - 已通过初始完整对象读取与审计的回单快照
    ///
    /// # 返回
    /// 引用、版本、对象键、指纹、扫描和销毁状态均保持一致时返回成功。
    ///
    /// # 错误
    /// 引用缺失、元数据读取失败或治理事实发生变化时拒绝返回对象字节。
    pub async fn revalidate_supplier_payment_bank_receipt(
        &self,
        id: &str,
        actor: &AuditActor,
        expected: &SupplierPaymentBankReceiptSnapshot,
    ) -> Result<()> {
        let current = self.bank_receipt_snapshot(id, actor).await?;
        if current.qualification != expected.qualification {
            return Err(Error::ConflictError("银行回单读取资格已变化，请刷新后重试".to_string()));
        }
        if current.asset != expected.asset {
            return Err(Error::ConflictError("银行回单已变化，请刷新后重试".to_string()));
        }
        Ok(())
    }

    /// 同一事务中取得完整来源读取资格、付款直接引用和当前文件治理元数据。
    async fn bank_receipt_snapshot(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SupplierPaymentBankReceiptSnapshot> {
        let db = self.db.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        let purchase = purchase_access(self.db.clone(), self.authorization_rbac.clone());
        let funds = funds_access_with_rbac(self.db.clone(), self.authorization_rbac.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let qualification = funds
                        .supplier_payment_full_read_qualification(&id, &actor, &purchase, executor)
                        .await?;
                    let payment = db
                        .supplier_payments()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商付款单不存在".to_string()))?;
                    let asset_id = payment.require_bank_receipt()?;
                    let asset = db
                        .file_assets()
                        .find_by_id(asset_id.as_ref(), executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("银行回单不存在".to_string()))?;
                    Ok(SupplierPaymentBankReceiptSnapshot {
                        asset: FileAssetView::from(asset),
                        qualification,
                    })
                })
            })
            .await
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
            .commit_supplier_payment_with_assets(req, Arc::new(EmptyPendingAttachments), actor)
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
        req: CommitSupplierPaymentRequest,
        pending_assets: Arc<dyn PendingAttachmentBatch>,
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
        if let Some(payment_id) = FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(&command_receipt, &mut NoTransaction)
            .await?
        {
            return Ok(SupplierPaymentWithAssetsResult {
                view: self.read().supplier_payment_detail(&payment_id).await?,
                assets_committed: false,
            });
        }
        let has_pending_assets = !pending_assets.is_empty();
        let prepared = PreparedSupplierPayment::prepare(req, pending_assets.as_ref(), actor.id())?;
        let policy_revision = self.authorization_rbac.current_policy_revision().await?;
        let transaction = SupplierPaymentTransaction {
            db: self.db.clone(),
            rbac: self.authorization_rbac.clone(),
            object_read: Arc::clone(&self.object_read),
            prepared,
            pending_assets,
            actor: actor.clone(),
            command_receipt: command_receipt.clone(),
        };
        let result = self
            .transaction_rbac
            .run_authorized_policy_transaction(policy_revision, move |executor| {
                Box::pin(transaction.execute(executor))
            })
            .await;
        self.committed_payment_view(result, &command_receipt, has_pending_assets).await
    }

    /// 按原命令收据恢复与未知提交规则回读付款结果。
    async fn committed_payment_view(
        &self,
        transaction_result: Result<(SupplierPayment, bool)>,
        command_receipt: &CommandReceipt,
        has_pending_assets: bool,
    ) -> Result<SupplierPaymentWithAssetsResult> {
        let (payment, fresh) = match transaction_result {
            Ok(payment) => payment,
            Err(error) => {
                let assets_may_be_committed = matches!(&error, Error::OutcomeUnknown(_));
                let recovered = FinanceCommandReceiptService::new(self.db.clone())
                    .committed_resource_id(command_receipt, &mut NoTransaction)
                    .await;
                let recovered = recovered_resource(error, recovered.map_err(Error::from))?;
                let view = self.read().supplier_payment_detail(&recovered.id).await.map_err(Error::from);
                return Ok(SupplierPaymentWithAssetsResult {
                    view: recovered_view(view, recovered.original_unknown)?,
                    assets_committed: has_pending_assets && assets_may_be_committed,
                });
            },
        };
        Ok(SupplierPaymentWithAssetsResult {
            view: self.read().supplier_payment_detail(&payment.base.id).await?,
            assets_committed: has_pending_assets && fresh,
        })
    }
}

/// 收据已完成重放检查后，本次付款的强类型事务输入。
struct PreparedSupplierPayment {
    payment: SupplierPayment,
    allocations: Vec<PendingPaymentAllocation>,
    work_item_id: WorkItemId,
    expected_task_version: u64,
    additional_tasks: Vec<(WorkItemId, u64)>,
    expected_payee_bank_account_id: PartyBankAccountId,
    expected_payee_bank_account_version: u64,
}

impl PreparedSupplierPayment {
    /// 按既有临时引用、任务版本、付款与分配次序构造事务输入。
    fn prepare(
        mut req: CommitSupplierPaymentRequest,
        pending_assets: &dyn PendingAttachmentBatch,
        created_by: &str,
    ) -> Result<Self> {
        let used_assets = resolve_payment_receipt_references(&mut req, pending_assets)?;
        pending_assets.ensure_all_used(&used_assets)?;
        let expected_task_version = expected_task_version(&req.expected_task_version)?;
        let additional_tasks = lock_additional_payment_tasks(&req.additional_work_items)?;
        let expected_payee_bank_account_id =
            PartyBankAccountId::new(req.expected_payee_bank_account_id.trim());
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
            created_by,
        )?;
        Ok(Self {
            payment,
            allocations,
            work_item_id: req.work_item_id,
            expected_task_version,
            additional_tasks,
            expected_payee_bank_account_id,
            expected_payee_bank_account_version: req.expected_payee_bank_account_version,
        })
    }
}

/// 所有权事务内付款命令、授权读取与附件登记的固定装配。
struct SupplierPaymentTransaction {
    db: Database,
    rbac: SharedRbacService,
    object_read: Arc<dyn ApprovalObjectReadPort>,
    prepared: PreparedSupplierPayment,
    pending_assets: Arc<dyn PendingAttachmentBatch>,
    actor: AuditActor,
    command_receipt: CommandReceipt,
}

impl SupplierPaymentTransaction {
    /// 在同一 Executor 按原首错顺序完成注册、资产、账户占用、任务和过账。
    async fn execute(mut self, executor: &mut dyn Executor) -> Result<(SupplierPayment, bool)> {
        let receipts = FinanceCommandReceiptService::new(self.db.clone());
        if let Some(id) = receipts.committed_resource_id(&self.command_receipt, executor).await? {
            let payment = self
                .db
                .supplier_payments()
                .find_by_id(&id, executor)
                .await?
                .ok_or_else(|| Error::Internal("付款命令回执引用不存在".to_string()))?;
            return Ok((payment, false));
        }
        let context = BusinessEventContext::new(self.actor.clone(), PAYMENT_COMMIT)?
            .with_command_id(Some(self.command_receipt.id().to_string()))?;
        let supplier = self.load_supplier(executor).await?;
        self.persist_document(&supplier, executor).await?;
        self.persist_payment(executor).await?;
        lock_expected_payment_recipient(
            &self.db,
            &supplier.party_id,
            &self.prepared.expected_payee_bank_account_id,
            self.prepared.expected_payee_bank_account_version,
            executor,
        )
        .await?;
        self.finish_payment(&context, executor).await?;
        receipts
            .save_resource(
                &self.command_receipt,
                self.prepared.payment.base.id.clone(),
                context.event_id().to_string(),
                executor,
            )
            .await?;
        Ok((self.prepared.payment, true))
    }

    /// 保持付款任务记录、财务过账及单次业务事件的原执行顺序。
    async fn finish_payment(
        &mut self,
        context: &BusinessEventContext,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let facts = payment_task::record_payment_execution(
            &self.db,
            &self.rbac,
            payment_task::PaymentExecutionCommand {
                work_item_id: &self.prepared.work_item_id,
                expected_task_version: self.prepared.expected_task_version,
                additional_tasks: &self.prepared.additional_tasks,
                supplier_id: &self.prepared.payment.supplier_id,
                allocations: &self.prepared.allocations,
            },
            &self.actor,
            executor,
        )
        .await?;
        post_supplier_payment(
            &self.db,
            &mut self.prepared.payment,
            &self.prepared.allocations,
            PaymentSettlementFacts::new(&facts.entries, &facts.accounts),
            context,
            &self.actor,
            executor,
        )
        .await?;
        Ok(())
    }

    /// 保持付款单号冲突先于供应商缺失的读取错误次序。
    async fn load_supplier(&self, executor: &mut dyn Executor) -> Result<SupplierAccount> {
        if self
            .db
            .supplier_payments()
            .find_by_payment_no(&self.prepared.payment.payment_no, executor)
            .await?
            .is_some()
        {
            return Err(Error::ConflictError("付款单号已存在，请刷新后重试".to_string()));
        }
        self.db
            .supplier_accounts()
            .find_by_id(self.prepared.payment.supplier_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))
    }

    /// 在同一阶段已读取的供应商主体上校验 NO_APPROVAL 并登记付款单据。
    async fn persist_document(&self, supplier: &SupplierAccount, executor: &mut dyn Executor) -> Result<()> {
        let payment = &self.prepared.payment;
        let bind_command = BindPublishedDefinitionCommand {
            document_type: DocumentType::SupplierPayment,
            business_object_id: payment.base.id.clone(),
            business_object_version: payment.base.version,
            context: BindingRevalidationContext::new(
                supplier.party_id.to_string(),
                self.actor.id().to_string(),
            ),
        };
        let document = new_registered_document(
            &payment.base.id,
            DocumentType::SupplierPayment,
            payment.payment_no.clone(),
        )
        .map_err(crate::Error::from)?;
        persist_unbound_supplier_payment_document(
            &self.db,
            &self.rbac,
            self.object_read.as_ref(),
            document,
            &bind_command,
            &self.actor,
            executor,
        )
        .await
    }

    /// 保持回单验证、资产登记及付款创建次序，完整命令统一记录一次事件。
    async fn persist_payment(&self, executor: &mut dyn Executor) -> Result<()> {
        let payment = &self.prepared.payment;
        ensure_bank_receipt_asset(&self.db, payment.require_bank_receipt()?, &self.pending_assets, executor)
            .await?;
        self.pending_assets.persist(&self.db, executor).await?;
        self.db.supplier_payments().create(payment, executor).await?;
        Ok(())
    }
}

/// 把附加付款任务的字符串版本解析为乐观锁。
///
/// # 参数
/// * `items` - 提交命令中的附加任务身份
///
/// # 返回
/// 返回与输入顺序一致的任务 ID 与正整数版本。
///
/// # 错误
/// 任一版本不是正整数字符串时返回校验错误。
fn lock_additional_payment_tasks(
    items: &[super::dto::PaymentExecutionTaskRef],
) -> Result<Vec<(WorkItemId, u64)>> {
    let mut locks = Vec::with_capacity(items.len());
    for item in items {
        locks.push((item.work_item_id.clone(), expected_task_version(&item.expected_task_version)?));
    }
    Ok(locks)
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
    party_id: &PartyId,
    expected_id: &PartyBankAccountId,
    expected_version: u64,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut recipient = resolve_current_party_payment_recipient(db, party_id, executor).await?;
    if !recipient.matches_expected(expected_id, expected_version) {
        return Err(Error::ConflictError("供应商收款账户已变化，请刷新付款任务并重新核对".to_string()));
    }
    db.party_bank_accounts().update(&mut recipient, executor).await.map_err(payment_recipient_lock_error)
}

/// 把收款账户占用冲突映射为可操作的刷新提示。
fn payment_recipient_lock_error(error: persistence_core::Error) -> Error {
    match error {
        persistence_core::Error::OptimisticLockingError
        | persistence_core::Error::TransientTransactionConflict(_) => {
            Error::ConflictError("供应商收款账户已变化，请刷新付款任务并重新核对".to_string())
        },
        other => other.into(),
    }
}

/// 解析本次付款字段中的临时回单引用。
fn resolve_payment_receipt_references(
    req: &mut CommitSupplierPaymentRequest,
    pending_assets: &dyn PendingAttachmentBatch,
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
async fn ensure_bank_receipt_asset(
    db: &Database,
    asset_id: &FileAssetId,
    pending_assets: &dyn PendingAttachmentBatch,
    executor: &mut dyn Executor,
) -> Result<()> {
    if pending_assets.contains_id(asset_id) {
        return Ok(());
    }
    let asset = db
        .file_assets()
        .find_by_id(asset_id.as_ref(), executor)
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
    object_read: &dyn ApprovalObjectReadPort,
    document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
        db,
        rbac,
        object_read,
        bind_command,
        actor,
        executor,
    )
    .await?;
    if binding.is_some() || document.approval_binding.is_some() {
        return Err(Error::Internal("供应商付款为 NO_APPROVAL，不得写入审批绑定".to_string()));
    }
    persist_registered_document(db, &document, executor).await.map_err(crate::Error::from)
}
