//! 供应商付款单查询、银行回单与过账编排。

use std::collections::HashSet;
use std::sync::Arc;

use erp_audit::AuditExt;
use erp_finance::entity::payable::{SupplierPayment, SupplierPaymentData};
use erp_finance::repository::PayableExt;

use erp_core::ids::{FileAssetId, PartyBankAccountId, SupplierAccountId, SupplierPaymentId, WorkItemId};

use erp_party::PartyExt;
use erp_supplier::SupplierExt;

use erp_support::{
    BankReceiptEvidencePolicy, EmptyPendingAttachments, FileAssetExt, FileAssetView, PendingAttachmentBatch,
};
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use id_generator::next_id;
use mongodb::{ClientSession, Database};
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::dto::{CommitSupplierPaymentRequest, SupplierPaymentView};
use super::mapping::resolve_current_payment_recipient;
use super::payment_task;
use super::posting::{post_supplier_payment_in_transaction, PaymentPostSource};
use super::{PayableService, SupplierPaymentWithAssetsResult};
use crate::{Error, Result};
use application_core::AuditActor;
use application_core::CommandReceipt;
use erp_audit::AuditActorLogs;
use erp_audit::CommandReceiptServiceExt as _;
use erp_identity::SharedRbacService;
use erp_workflow::service::approval::binding::BindPublishedDefinitionCommand;
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::document_registry::{new_registered_document, persist_registered_document};

impl PayableService {
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
        mut req: CommitSupplierPaymentRequest,
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
        if let Some(payment_id) = command_receipt.committed_resource_id(&self.db).await? {
            return Ok(SupplierPaymentWithAssetsResult {
                view: self.read().supplier_payment_detail(&payment_id).await?,
                assets_committed: false,
            });
        }
        let has_pending_assets = !pending_assets.is_empty();
        let used_assets = resolve_payment_receipt_references(&mut req, pending_assets.as_ref())?;
        pending_assets.ensure_all_used(&used_assets)?;
        let expected_task_version =
            erp_workflow::service::work_item::expected_task_version(&req.expected_task_version)?;
        let work_item_id = req.work_item_id.clone();
        let additional_tasks = lock_additional_payment_tasks(&req.additional_work_items)?;
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
                            order_source: None,
                            customer_id: None,
                            business_org_unit_id: None,
                            scope_owner_user_id: None,
                            organization_id: supplier.party_id.to_string(),
                            creator_id: actor_owned.id().to_string(),
                        },
                    };
                    let document = new_registered_document(
                        &payment.base.id,
                        DocumentType::SupplierPayment,
                        payment.payment_no.clone(),
                    )
                    .map_err(crate::Error::from)?;
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
                        payment_task::PaymentExecutionCommand {
                            work_item_id: &work_item_id,
                            expected_task_version,
                            additional_tasks: &additional_tasks,
                            supplier_id: &payment.supplier_id,
                            allocations: &allocations,
                        },
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
                    Ok::<SupplierPayment, crate::Error>(payment)
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
                            view: self.read().supplier_payment_detail(&payment_id).await?,
                            assets_committed: has_pending_assets && assets_may_be_committed,
                        });
                    }
                    None => return Err(error),
                }
            }
        };

        Ok(SupplierPaymentWithAssetsResult {
            view: self.read().supplier_payment_detail(&committed.base.id).await?,
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
        locks.push((
            item.work_item_id.clone(),
            erp_workflow::service::work_item::expected_task_version(&item.expected_task_version)?,
        ));
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
async fn ensure_bank_receipt_asset_in_transaction(
    db: &Database,
    asset_id: &FileAssetId,
    pending_assets: &dyn PendingAttachmentBatch,
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
    let binding = crate::adapters::workflow::bind_published_definition_on_document_create(
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
        .map_err(crate::Error::from)
}
