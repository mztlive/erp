//! 进项发票登记过账与分配查询编排。

use std::collections::{HashMap, HashSet};

use erp_finance::entity::payable::PayableAccount;

use erp_audit::AuditExt;
use erp_core::ids::{InvoiceId, PayableAccountId, SupplierAccountId};
use erp_supplier::SupplierAccount;
use erp_supplier::SupplierExt;

use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::dto::{PurchaseInvoiceRegisteredView, RegisterPurchaseInvoiceRequest};

use super::PayableService;
use crate::{Error, Result};
use application_core::AuditActor;
use application_core::CommandReceipt;
use erp_audit::AuditActorLogs;
use erp_audit::CommandReceiptServiceExt as _;

impl PayableService {
    // -----------------------------------------------------------------------
    // 进项发票登记与分配
    // -----------------------------------------------------------------------

    /// 进项发票登记过账并分配（§8.3-2 事务不变量）。
    ///
    /// 发票实体经 D18 `invoices()` 仓储写入（D19 不复制发票实体）；同一事务内：
    /// 规范化号码去重；总额/税额口径、序号与分配实体由
    /// [`erp_finance::entity::payable::PurchaseInvoiceAllocationPlan`] 一次性构造（FIN-E03）；账户与供应商
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
            return Ok(
                erp_finance::service::payable::PayableService::new(self.db.clone())
                    .purchase_invoice_registered_view(&invoice_id)
                    .await?,
            );
        }
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(&req.supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
        let party_id = supplier.party_id.clone();

        let invoice =
            erp_finance::service::payable::prepare_purchase_invoice(&req, party_id.clone(), actor.id())?;
        let invoice_id = InvoiceId::new(invoice.base.id.clone());

        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let invoice_for_tx = invoice.clone();
        let command_receipt_for_tx = command_receipt.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let (plan, accounts) =
                        erp_finance::service::payable::prepare_purchase_invoice_allocations_in_transaction(
                            &db,
                            &req,
                            &invoice_for_tx,
                            session,
                        )
                        .await?;
                    let account_ids: Vec<PayableAccountId> = plan
                        .account_invoicing_deltas()
                        .iter()
                        .map(|(id, _)| id.clone())
                        .collect();
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
                    let invoice_mut = erp_finance::service::payable::persist_purchase_invoice_in_transaction(
                        &db,
                        invoice_for_tx,
                        &plan,
                        &actor_id,
                        session,
                    )
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
                    Ok::<(), crate::Error>(())
                })
            })
            .await;
        if let Err(error) = transaction_result {
            if let Some(invoice_id) = command_receipt.committed_resource_id(&self.db).await? {
                return Ok(
                    erp_finance::service::payable::PayableService::new(self.db.clone())
                        .purchase_invoice_registered_view(&invoice_id)
                        .await?,
                );
            }
            return Err(error);
        }
        Ok(
            erp_finance::service::payable::PayableService::new(self.db.clone())
                .purchase_invoice_registered_view(invoice_id.as_ref())
                .await?,
        )
    }
}
