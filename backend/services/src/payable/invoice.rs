//! 进项发票登记过账与分配查询编排。

use std::collections::{HashMap, HashSet};

use database::{AccessControlExt, PayableExt, ReceivableExt, SupplierExt};
use entities::payable::{
    PayableAccount, PurchaseInvoiceAllocation, PurchaseInvoiceAllocationLine, PurchaseInvoiceAllocationPlan,
};
use entities::receivable::{Invoice, InvoiceData, InvoiceDirection, InvoiceKind};
use entities::supplier::SupplierAccount;
use erp_core::ids::{InvoiceId, PayableAccountId, PurchaseInvoiceAllocationId, SupplierAccountId};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::dto::{
    PageView, PurchaseInvoiceAllocationListParams, PurchaseInvoiceAllocationView,
    PurchaseInvoiceRegisteredView, RegisterPurchaseInvoiceRequest, SortDir,
};
use super::mapping::zero_amount;
use super::{PayableService, PurchaseInvoiceAllocationFilter};
use crate::audit::AuditActorLogs;
use crate::audit::{CommandReceipt, CommandReceiptServiceExt as _};
use crate::errors::{Error, Result};
use application_core::AuditActor;

impl PayableService {
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

    /// 分页查询进项发票分配列表（按应付子账筛选，FIN-R06 服务端分页）。
    ///
    /// 账户必填政策与请求校验保留在 Service；过滤、稳定排序
    /// (`(created_at, id)` 同方向）、`skip/limit` 与总数均由 Repository 在
    /// 数据库完成，只装载当前页。
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
        let filter = PurchaseInvoiceAllocationFilter {
            payable_account_id: Some(payable_account_id),
            invoice_id: None,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .purchase_invoice_allocations()
            .search_purchase_invoice_allocations(&filter, &mut NoTransaction)
            .await?;
        Ok(PageView {
            items: page.items.iter().map(purchase_invoice_allocation_view).collect(),
            total: page.total,
            page: query.paging.page,
            page_size: query.paging.page_size,
        })
    }
}

/// 装配进项发票分配视图。
///
/// # 参数
/// * `allocation` - 进项发票分配实体
///
/// # 返回
/// 返回响应视图。
pub(super) fn purchase_invoice_allocation_view(
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
