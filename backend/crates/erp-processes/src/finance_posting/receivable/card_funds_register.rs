//! W13 当前责任任务内的历史回款/销项发票原子登记。

use erp_audit::AuditExt;
use erp_finance::entity::receivable::{
    AccountReviewStatus, CardFundsRegistrationKind, CardFundsRegistrationReceipt, CustomerReceipt,
    CustomerReceiptData, Invoice, InvoiceData, InvoiceDirection, InvoiceKind, ReceivableAccount,
    CARD_FUNDS_INVOICE_REGISTRATION_ACTION, CARD_FUNDS_RECEIPT_REGISTRATION_ACTION,
};
use erp_finance::repository::ReceivableExt;

use erp_core::ids::{CustomerReceiptId, InvoiceId, ReceivableAccountId};

use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::WorkItemExt;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use erp_finance::service::receivable::card_funds_register::{
    card_funds_registration_allocations, map_plan_error, persist_card_funds_receipt_plan,
    PersistCardFundsReceiptPlanInput,
};

use super::adapter::customer_receipt_responsible_org_id;
use super::card_funds_identity::lock_registration_work_item;
use super::card_funds_receipt::{map_registration_receipt_error, replay_card_funds_registration};
use super::customer_receipt::persist_bound_customer_receipt_document;
use super::dto::{
    CardFundsRegistrationResult, RegisterCardFundsInvoiceRequest, RegisterCardFundsReceiptRequest,
};
use super::invoice::register_created_invoice_document;
use super::{invoice_task, ReceivableProcess};
use crate::adapters::workflow::work_item_service;
use crate::{Error, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::SharedRbacService;
use erp_read_models::finance::receivable::snapshot::{
    card_funds_snapshot_of, load_card_funds_snapshot, parse_task_version, zero_amount, CardFundsSnapshot,
};
use erp_workflow::service::approval::binding::BindPublishedDefinitionCommand;
use erp_workflow::service::approval::business_adapter::BindingRevalidationContext;
use erp_workflow::service::document_registry::new_registered_document;

impl ReceivableProcess {
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
        let fingerprint = CardFundsRegistrationReceipt::payload_fingerprint(&req)
            .map_err(map_registration_receipt_error)?;
        let audit_id = CardFundsRegistrationReceipt::audit_id(
            CARD_FUNDS_RECEIPT_REGISTRATION_ACTION,
            actor.id(),
            &req.idempotency_key,
        );
        let receipt_no = CardFundsRegistrationReceipt::normalized_no(req.receipt_no.as_deref())
            .unwrap_or_else(|| {
                CardFundsRegistrationReceipt::stable_no("SK", actor.id(), &req.idempotency_key)
            });
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
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
                        return Ok::<(String, String), crate::Error>(replayed);
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

                    let allocation_plan = card_funds_snapshot_of(&snapshot)?
                        .plan_historical_receipt_allocations(registration_amount)
                        .map_err(map_plan_error)?;
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
                    )
                    .map_err(crate::Error::from)?;
                    persist_bound_customer_receipt_document(
                        &db,
                        &rbac,
                        object_read.as_ref(),
                        document,
                        &bind_command,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    db.customer_receipts().create(&receipt, session).await?;
                    persist_card_funds_receipt_plan(
                        &db,
                        PersistCardFundsReceiptPlanInput {
                            account: &account,
                            snapshot: &snapshot,
                            receipt: &receipt,
                            plan: &allocation_plan,
                            actor_id: &actor_id,
                            insufficient_message: "子账剩余开放余额不足，历史回款登记被拒绝",
                        },
                        session,
                    )
                    .await?;
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
                        Some(
                            CardFundsRegistrationReceipt::new(
                                fingerprint_for_tx.clone(),
                                CardFundsRegistrationKind::Receipt,
                                receipt.base.id.clone(),
                            )
                            .map_err(map_registration_receipt_error)?
                            .encode_message(),
                        ),
                    )?;
                    db.audit_logs().create(&receipt_audit, session).await?;
                    crate::order_to_cash::progress::update_sales_order_money_progress(
                        &db,
                        session,
                        &account.sales_order_id,
                        actor_id.clone(),
                        None,
                    )
                    .await?;
                    Ok::<(String, String), crate::Error>((account.base.id, receipt.base.id))
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
        let fingerprint = CardFundsRegistrationReceipt::payload_fingerprint(&req)
            .map_err(map_registration_receipt_error)?;
        let audit_id = CardFundsRegistrationReceipt::audit_id(
            CARD_FUNDS_INVOICE_REGISTRATION_ACTION,
            actor.id(),
            &req.idempotency_key,
        );
        let invoice_no = CardFundsRegistrationReceipt::normalized_no(req.invoice_no.as_deref())
            .unwrap_or_else(|| {
                CardFundsRegistrationReceipt::stable_no("FP", actor.id(), &req.idempotency_key)
            });
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
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
                        return Ok::<(String, String), crate::Error>(replayed);
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
                    register_created_invoice_document(
                        &db,
                        &rbac,
                        object_read.as_ref(),
                        &invoice,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    erp_finance::service::receivable::card_funds_register::persist_historical_invoice(
                        &db, &account, &invoice, &actor_id, session,
                    )
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
                        Some(
                            CardFundsRegistrationReceipt::new(
                                fingerprint_for_tx.clone(),
                                CardFundsRegistrationKind::Invoice,
                                invoice.base.id.clone(),
                            )
                            .map_err(map_registration_receipt_error)?
                            .encode_message(),
                        ),
                    )?;
                    db.audit_logs().create(&receipt_audit, session).await?;
                    crate::order_to_cash::progress::update_sales_order_money_progress(
                        &db,
                        session,
                        &account.sales_order_id,
                        actor_id.clone(),
                        None,
                    )
                    .await?;
                    Ok::<(String, String), crate::Error>((account.base.id, invoice.base.id))
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
        let view = self.read.receivable_account_view(account_id.to_string()).await?;
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
}

/// 加载 W13 历史票款登记上下文所需的任务版本与责任人事实。
struct CardFundsRegistrationContextInput<'a> {
    rbac: SharedRbacService,
    work_item_id: &'a erp_core::ids::WorkItemId,
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
    lock_registration_work_item(
        &work_item,
        input.actor.id(),
        input.expected_task_version,
        input.expected_subject_version.trim(),
    )?;
    work_item_service(db.clone(), input.rbac)
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
    if card_funds_snapshot_of(&snapshot)?.fact_version(&account) != input.expected_funds_fact_version.trim() {
        return Err(Error::ConflictError("票款事实已变化，请刷新后重试".to_string()));
    }
    Ok((account, snapshot))
}
