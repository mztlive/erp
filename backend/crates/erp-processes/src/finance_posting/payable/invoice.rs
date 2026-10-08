//! 进项发票登记过账、独立命令回执及结构化业务事件编排。

use std::collections::{HashMap, HashSet};

use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_audit::{
    AuditAction, AuditFact, AuditField, AuditFieldKind, AuditValue, BusinessEventContent,
    BusinessEventContext, BusinessEventResult,
};
use erp_core::ids::{InvoiceId, PartyId, SupplierAccountId};
use erp_finance::entity::payable::{PayableAccount, PurchaseInvoiceAllocationPlan};
use erp_finance::entity::receivable::Invoice;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_finance::service::payable::{self, PayableService as FinancePayableService};
use erp_supplier::repository::prelude::*;
use erp_supplier::{SupplierAccount, SupplierExt};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::PayableService;
use super::dto::{PurchaseInvoiceRegisteredView, RegisterPurchaseInvoiceRequest};
use crate::audit::{AuditedCommand, AuditedWrite, MongoAuditEventSink, execute_audited};
use crate::{Error, Result};

const PURCHASE_INVOICE_REGISTER: AuditAction = AuditAction {
    code: "purchase_invoice_allocation.post",
    resource_type: "purchase_invoice_allocation",
    label: "登记进项发票",
    version: 1,
    allowed_fields: &[
        AuditField { code: "gross_amount", label: "含税金额", kind: AuditFieldKind::Amount },
        AuditField { code: "net_amount", label: "不含税金额", kind: AuditFieldKind::Amount },
        AuditField { code: "tax_amount", label: "税额", kind: AuditFieldKind::Amount },
    ],
};

impl PayableService {
    /// 进项发票登记过账并分配（§8.3-2 事务不变量）。
    ///
    /// 在同一 Executor 内保存发票、分配、收票额度、独立财务回执及一次业务
    /// 事件。首次、事务内和异常恢复均只查独立回执，重放只回读原视图。
    ///
    /// # 参数
    /// * `req` - 进项发票登记请求
    /// * `actor` - 已通过入口鉴权的审计操作人
    ///
    /// # 返回
    /// 返回首次登记发票的当前登记视图及正式分配行。
    ///
    /// # 错误
    /// 请求、供应商、号码、分配及额度校验保持原先顺序；异载荷返回冲突，
    /// 回执损坏返回内部错误；提交结果未知且查证失败时保留原未知错误。
    pub async fn register_purchase_invoice(
        &self,
        req: RegisterPurchaseInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseInvoiceRegisteredView> {
        req.validate()?;
        let receipt = purchase_invoice_receipt(&req, actor)?;
        let port = MongoPurchaseInvoiceCommand { db: self.db.clone() };
        if let Some(invoice_id) = port.receipt_invoice_id(&receipt, &mut NoTransaction).await? {
            return port.registered_view(&invoice_id).await;
        }
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(&req.supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let invoice = payable::prepare_purchase_invoice(&req, supplier.party_id, actor.id())?;
        let context = BusinessEventContext::new(actor.clone(), PURCHASE_INVOICE_REGISTER)?
            .with_command_id(Some(receipt.id().to_string()))?;
        let registration =
            PurchaseInvoiceRegistration { req, invoice, actor: actor.clone(), receipt: receipt.clone() };
        let client = self.db.client().clone();
        let transaction_port = MongoPurchaseInvoiceCommand { db: self.db.clone() };
        let result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let command = RegisterPurchaseInvoiceCommand {
                        port: &transaction_port,
                        registration: &registration,
                        audit_event_id: context.event_id(),
                    };
                    let sink = MongoAuditEventSink::new(&transaction_port.db);
                    execute_audited(&context, &sink, executor, &command).await
                })
            })
            .await;
        finish_registration(&port, result, &receipt).await
    }
}

/// 用登记请求和操作人构造进项发票命令回执。
fn purchase_invoice_receipt(
    req: &RegisterPurchaseInvoiceRequest,
    actor: &AuditActor,
) -> Result<CommandReceipt> {
    Ok(CommandReceipt::from_payload(
        "purchase-invoice-register-",
        actor.id(),
        PURCHASE_INVOICE_REGISTER.code,
        PURCHASE_INVOICE_REGISTER.resource_type,
        &req.idempotency_key,
        req,
    )?)
}

struct PurchaseInvoiceRegistration {
    req: RegisterPurchaseInvoiceRequest,
    invoice: Invoice,
    actor: AuditActor,
    receipt: CommandReceipt,
}

/// 只抽象本命令的编排边界，不为各财务仓储另建接口层。
#[async_trait]
trait PurchaseInvoiceCommandPort: Send + Sync {
    /// 读取已提交回执中的发票 ID；没有回执时返回 `None`。
    async fn receipt_invoice_id(
        &self,
        receipt: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<InvoiceId>>;
    /// 持久化本次进项发票登记。
    async fn persist_invoice(
        &self,
        registration: &PurchaseInvoiceRegistration,
        executor: &mut dyn Executor,
    ) -> Result<Invoice>;
    /// 保存本命令的发票回执及审计事件关联。
    async fn save_receipt(
        &self,
        registration: &PurchaseInvoiceRegistration,
        invoice_id: InvoiceId,
        audit_event_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    /// 按发票 ID 回读登记视图。
    async fn registered_view(&self, invoice_id: &InvoiceId) -> Result<PurchaseInvoiceRegisteredView>;
}

/// 查证包含原视图回读；未知提交下任何查证失败均保留首次未知错误。
async fn finish_registration(
    port: &impl PurchaseInvoiceCommandPort,
    result: Result<InvoiceId>,
    receipt: &CommandReceipt,
) -> Result<PurchaseInvoiceRegisteredView> {
    let original_error = match result {
        Ok(id) => return port.registered_view(&id).await,
        Err(error) => error,
    };
    let recovered = port.receipt_invoice_id(receipt, &mut NoTransaction).await;
    let invoice_id = match recovered {
        Ok(Some(id)) => id,
        Ok(None) => return Err(original_error),
        Err(_) if matches!(original_error, Error::OutcomeUnknown(_)) => return Err(original_error),
        Err(error) => return Err(error),
    };
    match port.registered_view(&invoice_id).await {
        Ok(view) => Ok(view),
        Err(_) if matches!(original_error, Error::OutcomeUnknown(_)) => Err(original_error),
        Err(error) => Err(error),
    }
}

struct RegisterPurchaseInvoiceCommand<'a, P> {
    port: &'a P,
    registration: &'a PurchaseInvoiceRegistration,
    audit_event_id: &'a str,
}

#[async_trait]
impl<P: PurchaseInvoiceCommandPort> AuditedCommand for RegisterPurchaseInvoiceCommand<'_, P> {
    type Output = InvoiceId;

    /// 已有回执则重放且不再写事件；否则先落发票再保存回执。
    ///
    /// # 参数
    /// * `executor` - 查证与写入共用的执行器。
    ///
    /// # 返回
    /// 回执已指向发票时返回 `AuditedWrite::Replayed`，不再写事件。否则返回 `AuditedWrite::Fresh`，结果为新发票 ID，事件只含含税、不含税和税额。
    ///
    /// # 错误
    /// 回执查询、发票登记或回执保存失败时返回对应错误。
    async fn execute(&self, executor: &mut dyn Executor) -> Result<AuditedWrite<InvoiceId>> {
        let registration = self.registration;
        if let Some(id) = self.port.receipt_invoice_id(&registration.receipt, executor).await? {
            return Ok(AuditedWrite::Replayed(id));
        }
        let invoice = self.port.persist_invoice(registration, executor).await?;
        let invoice_id = InvoiceId::new(invoice.base.id.clone());
        self.port.save_receipt(registration, invoice_id.clone(), self.audit_event_id, executor).await?;
        Ok(AuditedWrite::Fresh { result: invoice_id, content: registered_event_content(&invoice) })
    }
}

/// 登记事件只记录含税、不含税和税额。
fn registered_event_content(invoice: &Invoice) -> BusinessEventContent {
    BusinessEventContent {
        target_id: invoice.base.id.clone(),
        target_number: Some(invoice.invoice_no.clone()),
        result: BusinessEventResult::Succeeded,
        field_changes: vec![],
        facts: vec![
            AuditFact {
                field: "gross_amount".to_string(),
                value: AuditValue::Amount { value: invoice.gross_amount },
            },
            AuditFact {
                field: "net_amount".to_string(),
                value: AuditValue::Amount { value: invoice.net_amount },
            },
            AuditFact {
                field: "tax_amount".to_string(),
                value: AuditValue::Amount { value: invoice.tax_amount },
            },
        ],
    }
}

struct MongoPurchaseInvoiceCommand {
    db: Database,
}

#[async_trait]
impl PurchaseInvoiceCommandPort for MongoPurchaseInvoiceCommand {
    async fn receipt_invoice_id(
        &self,
        receipt: &CommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<Option<InvoiceId>> {
        Ok(FinanceCommandReceiptService::new(self.db.clone())
            .committed_purchase_invoice_id(receipt, executor)
            .await?)
    }

    /// 先按分配顺序查证供应商，再写入进项发票。
    ///
    /// # 参数
    /// * `registration` - 进项发票登记请求、发票草案与操作人。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回已持久化的进项发票。
    ///
    /// # 错误
    /// 分配准备或发票写入失败时返回对应错误。供应商不存在时返回 `NotFound`；发票主体与供应商不一致时返回 `BusinessLogicError`。
    async fn persist_invoice(
        &self,
        registration: &PurchaseInvoiceRegistration,
        executor: &mut dyn Executor,
    ) -> Result<Invoice> {
        let (plan, accounts) = payable::prepare_purchase_invoice_allocations(
            &self.db,
            &registration.req,
            &registration.invoice,
            executor,
        )
        .await?;
        validate_supplier_parties(&self.db, &accounts, &plan, &registration.invoice.party_id, executor)
            .await?;
        Ok(payable::persist_purchase_invoice(
            &self.db,
            registration.invoice.clone(),
            &plan,
            registration.actor.id(),
            executor,
        )
        .await?)
    }

    async fn save_receipt(
        &self,
        registration: &PurchaseInvoiceRegistration,
        invoice_id: InvoiceId,
        audit_event_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(FinanceCommandReceiptService::new(self.db.clone())
            .save_purchase_invoice(&registration.receipt, invoice_id, audit_event_id.to_string(), executor)
            .await?)
    }

    async fn registered_view(&self, invoice_id: &InvoiceId) -> Result<PurchaseInvoiceRegisteredView> {
        Ok(FinancePayableService::new(self.db.clone())
            .purchase_invoice_registered_view(invoice_id.as_ref())
            .await?)
    }
}

/// 按分配账户的首次出现顺序查证供应商，保持缺账户先于缺供应商的原首错语义。
async fn validate_supplier_parties(
    db: &Database,
    accounts: &[PayableAccount],
    plan: &PurchaseInvoiceAllocationPlan,
    party_id: &PartyId,
    executor: &mut dyn Executor,
) -> Result<()> {
    let ordered_accounts = ordered_allocation_accounts(accounts, plan)?;
    let mut seen = HashSet::new();
    let supplier_ids: Vec<SupplierAccountId> = ordered_accounts
        .iter()
        .filter(|account| seen.insert(account.supplier_id.to_string()))
        .map(|account| account.supplier_id.clone())
        .collect();
    let suppliers = db.supplier_accounts().find_accounts_by_ids(&supplier_ids, executor).await?;
    let suppliers_by_id: HashMap<&str, &SupplierAccount> =
        suppliers.iter().map(|supplier| (supplier.base.id.as_str(), supplier)).collect();
    for account in ordered_accounts {
        let supplier = suppliers_by_id
            .get(account.supplier_id.as_ref())
            .ok_or_else(|| Error::NotFound("应付子账供应商不存在".to_string()))?;
        if &supplier.party_id != party_id {
            return Err(Error::BusinessLogicError("禁止跨供应商收票".to_string()));
        }
    }
    Ok(())
}

/// 按分配计划中的账户顺序取子账；计划引用的子账不在已装载集合中时返回 `NotFound`。
fn ordered_allocation_accounts<'a>(
    accounts: &'a [PayableAccount],
    plan: &PurchaseInvoiceAllocationPlan,
) -> Result<Vec<&'a PayableAccount>> {
    let accounts_by_id: HashMap<&str, &PayableAccount> =
        accounts.iter().map(|account| (account.base.id.as_str(), account)).collect();
    plan.account_invoicing_deltas()
        .iter()
        .map(|(id, _)| {
            accounts_by_id
                .get(id.as_ref())
                .copied()
                .ok_or_else(|| Error::NotFound("应付往来子账不存在".to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Mutex;

    use erp_audit::AuditLog;
    use erp_core::AccountKind;
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::PayableAccountId;
    use erp_core::money::Amount;
    use erp_finance::FinanceCommandReceipt;
    use mongodb::error::Error as MongoError;
    use persistence_core::Error as PersistenceError;

    use super::super::dto::PurchaseInvoiceAllocationLineRequest;
    use super::*;
    use crate::audit::AuditEventSink;

    struct RecordingPort {
        receipt: Mutex<Option<FinanceCommandReceipt>>,
        fail: Option<&'static str>,
        executor_id: Option<usize>,
        calls: Mutex<Vec<&'static str>>,
        saved_receipts: Mutex<Vec<FinanceCommandReceipt>>,
        events: Mutex<Vec<AuditLog>>,
    }

    impl RecordingPort {
        fn new(receipt: Option<FinanceCommandReceipt>) -> Self {
            Self {
                receipt: Mutex::new(receipt),
                fail: None,
                executor_id: None,
                calls: Mutex::new(vec![]),
                saved_receipts: Mutex::new(vec![]),
                events: Mutex::new(vec![]),
            }
        }

        fn visit(&self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
            if let Some(expected) = self.executor_id {
                assert_eq!(executor as *mut dyn Executor as *mut () as usize, expected);
            }
            self.calls.lock().unwrap().push(step);
            if self.fail == Some(step) {
                return Err(Error::ConflictError(format!("{step} 原错误")));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl PurchaseInvoiceCommandPort for RecordingPort {
        async fn receipt_invoice_id(
            &self,
            command: &CommandReceipt,
            executor: &mut dyn Executor,
        ) -> Result<Option<InvoiceId>> {
            self.visit("lookup", executor)?;
            self.receipt
                .lock()
                .unwrap()
                .as_ref()
                .map(|receipt| receipt.purchase_invoice_id(command).map_err(Error::from))
                .transpose()
        }

        async fn persist_invoice(
            &self,
            registration: &PurchaseInvoiceRegistration,
            executor: &mut dyn Executor,
        ) -> Result<Invoice> {
            self.visit("business", executor)?;
            let mut invoice = registration.invoice.clone();
            invoice.mark_registered(registration.actor.id())?;
            Ok(invoice)
        }

        async fn save_receipt(
            &self,
            registration: &PurchaseInvoiceRegistration,
            invoice_id: InvoiceId,
            audit_event_id: &str,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.visit("receipt", executor)?;
            let receipt = FinanceCommandReceipt::purchase_invoice(
                &registration.receipt,
                invoice_id,
                audit_event_id.to_string(),
            )?;
            self.saved_receipts.lock().unwrap().push(receipt.clone());
            *self.receipt.lock().unwrap() = Some(receipt);
            Ok(())
        }

        async fn registered_view(&self, invoice_id: &InvoiceId) -> Result<PurchaseInvoiceRegisteredView> {
            self.calls.lock().unwrap().push("view");
            if self.fail == Some("view") {
                return Err(Error::Internal("原结果读取失败".to_string()));
            }
            Ok(PurchaseInvoiceRegisteredView {
                invoice_id: invoice_id.to_string(),
                invoice_no: "INV-001".to_string(),
                gross_amount: Amount::from_str("100.00").unwrap(),
                allocations: vec![],
            })
        }
    }

    #[async_trait]
    impl AuditEventSink for RecordingPort {
        async fn persist(&self, log: &AuditLog, executor: &mut dyn Executor) -> Result<()> {
            self.visit("event", executor)?;
            self.events.lock().unwrap().push(log.clone());
            Ok(())
        }
    }

    fn registration() -> PurchaseInvoiceRegistration {
        let actor = AuditActor::new("actor-1".to_string(), "kaipiao".to_string(), AccountKind::Admin);
        let req = RegisterPurchaseInvoiceRequest {
            idempotency_key: "operation-1".to_string(),
            invoice_code: None,
            invoice_no: "INV-001".to_string(),
            invoice_date: BusinessDate::from_str("2026-10-04").unwrap(),
            gross_amount: Amount::from_str("100.00").unwrap(),
            net_amount: Amount::from_str("90.00").unwrap(),
            tax_amount: Amount::from_str("10.00").unwrap(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            allocations: vec![PurchaseInvoiceAllocationLineRequest {
                payable_account_id: PayableAccountId::new("account-1"),
                allocated_gross_amount: Amount::from_str("100.00").unwrap(),
                allocated_net_amount: Amount::from_str("90.00").unwrap(),
                allocated_tax_amount: Amount::from_str("10.00").unwrap(),
            }],
        };
        let receipt = purchase_invoice_receipt(&req, &actor).unwrap();
        let mut invoice =
            payable::prepare_purchase_invoice(&req, PartyId::new("party-1"), actor.id()).unwrap();
        invoice.base.id = "invoice-original".to_string();
        PurchaseInvoiceRegistration { req, invoice, actor, receipt }
    }

    fn context(registration: &PurchaseInvoiceRegistration) -> BusinessEventContext {
        BusinessEventContext::new(registration.actor.clone(), PURCHASE_INVOICE_REGISTER)
            .unwrap()
            .with_command_id(Some(registration.receipt.id().to_string()))
            .unwrap()
    }

    fn receipt(registration: &PurchaseInvoiceRegistration) -> FinanceCommandReceipt {
        FinanceCommandReceipt::purchase_invoice(
            &registration.receipt,
            InvoiceId::new("invoice-original"),
            "event-original".to_string(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn fresh_registration_records_receipt_and_one_chinese_event_with_same_executor() {
        let registration = registration();
        let context = context(&registration);
        let mut executor = NoTransaction;
        let mut port = RecordingPort::new(None);
        port.executor_id = Some(&mut executor as *mut NoTransaction as usize);
        let command = RegisterPurchaseInvoiceCommand {
            port: &port,
            registration: &registration,
            audit_event_id: context.event_id(),
        };
        let invoice_id = execute_audited(&context, &port, &mut executor, &command).await.unwrap();
        assert_eq!(invoice_id.as_ref(), "invoice-original");
        assert_eq!(*port.calls.lock().unwrap(), ["lookup", "business", "receipt", "event"]);
        let receipts = port.saved_receipts.lock().unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].purchase_invoice_id(&registration.receipt).unwrap(), invoice_id);
        assert_eq!(receipts[0].audit_event_id, context.event_id());
        let events = port.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].base.id, context.event_id());
        assert_eq!(events[0].resource_id.as_deref(), Some("invoice-original"));
        let event = events[0].structured_event.as_ref().unwrap();
        assert_eq!(event.command_id.as_deref(), Some(registration.receipt.id()));
        assert_eq!(event.resource_number_snapshot.as_deref(), Some("INV-001"));
        assert_eq!(event.facts[0].value, AuditValue::Amount { value: registration.invoice.gross_amount });
        assert!(events[0].message.as_deref().unwrap().contains("登记进项发票"));
        assert!(events[0].message.as_deref().unwrap().contains("含税金额：100.00元"));
        assert!(!events[0].message.as_deref().unwrap().contains("operation-1"));
    }

    #[tokio::test]
    async fn replay_returns_original_invoice_without_business_receipt_or_event_writes() {
        let registration = registration();
        let context = context(&registration);
        let port = RecordingPort::new(Some(receipt(&registration)));
        let command = RegisterPurchaseInvoiceCommand {
            port: &port,
            registration: &registration,
            audit_event_id: context.event_id(),
        };
        let id = execute_audited(&context, &port, &mut NoTransaction, &command).await.unwrap();
        assert_eq!(id.as_ref(), "invoice-original");
        assert_eq!(*port.calls.lock().unwrap(), ["lookup"]);
        assert!(port.saved_receipts.lock().unwrap().is_empty());
        assert!(port.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn retry_after_fresh_success_reuses_actual_saved_receipt_and_never_appends_a_second_event() {
        let registration = registration();
        let context = context(&registration);
        let port = RecordingPort::new(None);
        let command = RegisterPurchaseInvoiceCommand {
            port: &port,
            registration: &registration,
            audit_event_id: context.event_id(),
        };
        let first = execute_audited(&context, &port, &mut NoTransaction, &command).await.unwrap();
        port.calls.lock().unwrap().clear();
        let replayed = execute_audited(&context, &port, &mut NoTransaction, &command).await.unwrap();
        assert_eq!(replayed, first);
        assert_eq!(*port.calls.lock().unwrap(), ["lookup"]);
        assert_eq!(port.saved_receipts.lock().unwrap().len(), 1);
        assert_eq!(port.events.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn different_payload_or_damaged_receipt_stops_before_any_write() {
        for damaged in [false, true] {
            let mut registration = registration();
            let mut saved = receipt(&registration);
            if damaged {
                saved.result_schema_version = 2;
            } else {
                registration.req.invoice_no = "OTHER-INVOICE".to_string();
                registration.receipt =
                    purchase_invoice_receipt(&registration.req, &registration.actor).unwrap();
            }
            let context = context(&registration);
            let port = RecordingPort::new(Some(saved));
            let command = RegisterPurchaseInvoiceCommand {
                port: &port,
                registration: &registration,
                audit_event_id: context.event_id(),
            };
            let error = execute_audited(&context, &port, &mut NoTransaction, &command).await.unwrap_err();
            if damaged {
                assert!(matches!(error, Error::Internal(message) if message == "业务命令收据格式无效"));
            } else {
                assert!(
                    matches!(error, Error::ConflictError(message) if message == "同一操作号已用于不同提交，请重新发起操作")
                );
            }
            assert_eq!(*port.calls.lock().unwrap(), ["lookup"]);
            assert!(port.saved_receipts.lock().unwrap().is_empty());
            assert!(port.events.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn each_write_failure_stops_later_steps_and_preserves_first_error() {
        for (failure, expected_calls) in [
            ("business", vec!["lookup", "business"]),
            ("receipt", vec!["lookup", "business", "receipt"]),
            ("event", vec!["lookup", "business", "receipt", "event"]),
        ] {
            let registration = registration();
            let context = context(&registration);
            let mut port = RecordingPort::new(None);
            port.fail = Some(failure);
            let command = RegisterPurchaseInvoiceCommand {
                port: &port,
                registration: &registration,
                audit_event_id: context.event_id(),
            };
            let error = execute_audited(&context, &port, &mut NoTransaction, &command).await.unwrap_err();
            assert!(matches!(error, Error::ConflictError(message) if message == format!("{failure} 原错误")));
            assert_eq!(*port.calls.lock().unwrap(), expected_calls);
            assert!(port.events.lock().unwrap().is_empty());
        }
    }

    fn unknown_commit() -> Error {
        Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(MongoError::custom(
            "original unknown commit",
        )))
    }

    #[tokio::test]
    async fn unknown_commit_checks_receipt_and_recovers_original_view_without_writes() {
        let registration = registration();
        let port = RecordingPort::new(Some(receipt(&registration)));
        let view = finish_registration(&port, Err(unknown_commit()), &registration.receipt).await.unwrap();
        assert_eq!(view.invoice_id, "invoice-original");
        assert_eq!(*port.calls.lock().unwrap(), ["lookup", "view"]);
        assert!(port.saved_receipts.lock().unwrap().is_empty());
        assert!(port.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_commit_preserves_original_error_when_receipt_or_view_cannot_be_confirmed() {
        for (with_receipt, fail, expected_calls) in [
            (false, None, vec!["lookup"]),
            (false, Some("lookup"), vec!["lookup"]),
            (true, Some("view"), vec!["lookup", "view"]),
        ] {
            let registration = registration();
            let mut port = RecordingPort::new(with_receipt.then(|| receipt(&registration)));
            port.fail = fail;
            let error =
                finish_registration(&port, Err(unknown_commit()), &registration.receipt).await.unwrap_err();
            let Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(source)) = error else {
                panic!("查证失败必须保留首次提交未知分类");
            };
            assert_eq!(source.get_custom::<&'static str>(), Some(&"original unknown commit"));
            assert_eq!(*port.calls.lock().unwrap(), expected_calls);
            assert!(port.saved_receipts.lock().unwrap().is_empty());
            assert!(port.events.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn unknown_commit_preserves_original_error_for_damaged_or_conflicting_receipt() {
        for damaged in [false, true] {
            let mut registration = registration();
            let mut saved = receipt(&registration);
            if damaged {
                saved.audit_event_id.clear();
            } else {
                registration.req.invoice_no = "OTHER-INVOICE".to_string();
                registration.receipt =
                    purchase_invoice_receipt(&registration.req, &registration.actor).unwrap();
            }
            let port = RecordingPort::new(Some(saved));
            let error =
                finish_registration(&port, Err(unknown_commit()), &registration.receipt).await.unwrap_err();
            assert!(matches!(error, Error::OutcomeUnknown(_)));
            assert_eq!(*port.calls.lock().unwrap(), ["lookup"]);
            assert!(port.saved_receipts.lock().unwrap().is_empty());
            assert!(port.events.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn known_transaction_error_is_preserved_when_no_receipt_was_saved() {
        let registration = registration();
        let port = RecordingPort::new(None);
        let error = finish_registration(
            &port,
            Err(Error::BusinessLogicError("原额度不足".to_string())),
            &registration.receipt,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::BusinessLogicError(message) if message == "原额度不足"));
        assert_eq!(*port.calls.lock().unwrap(), ["lookup"]);
        assert!(port.saved_receipts.lock().unwrap().is_empty());
        assert!(port.events.lock().unwrap().is_empty());
    }
}
