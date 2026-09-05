//! 发票列表、详情、草稿创建、销项提交与过账编排。

use database::{AccessControlExt, Executor, NoTransaction, PayableExt, ReceivableExt, Transactional};
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::ids::{InvoiceId, ReceivableAccountId, SalesInvoiceAllocationId};
use entities::money::Amount;
use entities::payable::PurchaseInvoiceAllocation;
use entities::receivable::{
    AllocationAction, Invoice, InvoiceData, InvoiceDirection, InvoiceStatus, ReceivableAccount,
    SalesInvoiceAllocation,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use std::collections::HashMap;

use super::dto::{
    CommitInvoiceRequest, CreateInvoiceRequest, InvoiceListParams, InvoiceView, PageView, PostInvoiceRequest,
    SortDir,
};
use super::invoice_commit::{convert_post_allocations, ensure_sales_invoice, PreparedInvoiceCommit};
use super::mapping::{ensure_expected_version, zero_amount};
use super::{invoice_task, ReceivableService};
use crate::approval::binding::{
    bind_published_definition_on_document_create, binding_decision, BindPublishedDefinitionCommand,
    BindingDecision,
};
use crate::approval::business_adapter::{adapter_spec_of, BindingRevalidationContext};
use crate::approval::policy::{policy_of, DocumentApprovalPolicy};
use crate::audit::{AuditActor, CommandReceipt, CommandReceiptServiceExt as _};
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

impl ReceivableService {
    // -----------------------------------------------------------------------
    // 发票
    // -----------------------------------------------------------------------

    /// 分页查询发票列表（销项/进项共用，`invoice_direction` 筛选）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn invoice_list(&self, params: &InvoiceListParams) -> Result<PageView<InvoiceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let scope_query = database::ScopedInvoiceQuery {
            invoice_direction: query.invoice_direction,
            invoice_kind: query.invoice_kind,
            party_id: query.party_id,
            invoice_no: query.invoice_no,
            status: query.status,
            scope: database::ReceivableListScope {
                sales_order_id: query.sales_order_id,
                receivable_account_id: query.receivable_account_id,
            },
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .receivable()
            .search_invoices_in_account_scope(&scope_query, &mut NoTransaction)
            .await?;
        let invoice_ids = page
            .items
            .iter()
            .map(|row| InvoiceId::new(row.id.clone()))
            .collect::<Vec<_>>();
        let mut sales_allocations_by_invoice = HashMap::<String, Vec<SalesInvoiceAllocation>>::new();
        for allocation in self
            .db
            .sales_invoice_allocations()
            .find_allocations_by_invoices(&invoice_ids, &mut NoTransaction)
            .await?
        {
            sales_allocations_by_invoice
                .entry(allocation.invoice_id.to_string())
                .or_default()
                .push(allocation);
        }
        let mut purchase_allocations_by_invoice = HashMap::<String, Vec<PurchaseInvoiceAllocation>>::new();
        for allocation in self
            .db
            .purchase_invoice_allocations()
            .find_allocations_by_invoices(&invoice_ids, &mut NoTransaction)
            .await?
        {
            purchase_allocations_by_invoice
                .entry(allocation.invoice_id.to_string())
                .or_default()
                .push(allocation);
        }
        for allocations in sales_allocations_by_invoice.values_mut() {
            allocations.sort_unstable_by_key(|allocation| allocation.allocation_seq);
        }
        for allocations in purchase_allocations_by_invoice.values_mut() {
            allocations.sort_unstable_by_key(|allocation| allocation.allocation_seq);
        }
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            let (allocated_total, allocations) = match row.invoice_direction {
                InvoiceDirection::Sales => {
                    sales_allocation_view(&sales_allocations_by_invoice.remove(&row.id).unwrap_or_default())
                }
                InvoiceDirection::Purchase => purchase_allocation_view(
                    &purchase_allocations_by_invoice
                        .remove(&row.id)
                        .unwrap_or_default(),
                ),
            };
            views.push(InvoiceView {
                id: row.id,
                invoice_direction: row.invoice_direction,
                invoice_kind: row.invoice_kind,
                party_id: row.party_id,
                invoice_code: row.invoice_code,
                invoice_no: row.invoice_no,
                invoice_date: row.invoice_date,
                gross_amount: row.gross_amount,
                net_amount: row.net_amount,
                tax_amount: row.tax_amount,
                rounding_adjustment_amount: row.rounding_adjustment_amount,
                rounding_reason: row.rounding_reason,
                original_invoice_id: row.original_invoice_id,
                status: row.stable.status(),
                version: row.version,
                created_at: row.created_at,
                allocated_total,
                unallocated_amount: row.gross_amount.checked_sub(allocated_total),
                allocations,
            });
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: scope_query.page,
            page_size: scope_query.page_size,
        })
    }

    /// 查询发票详情（含分配行）。
    ///
    /// # 参数
    /// * `id` - 发票 ID
    ///
    /// # 返回
    /// 返回发票视图。
    ///
    /// # 错误
    /// * `NotFound` - 发票不存在
    pub async fn invoice_detail(&self, id: &str) -> Result<InvoiceView> {
        self.invoice_view(id.to_string()).await
    }

    /// 登记发票草稿：同一事务注册 `BusinessDocument` 并调用统一绑定端口。
    ///
    /// 发票为 `NO_APPROVAL`：返回空绑定，不查询已发布定义，不启动审批实例，
    /// 不创建审批任务。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建发票视图。
    ///
    /// # 错误
    /// * `ValidationError` - 金额三元组不恒等或字段非法
    pub async fn create_invoice(&self, req: CreateInvoiceRequest, actor: &AuditActor) -> Result<InvoiceView> {
        req.validate()?;
        let invoice = Invoice::new(
            InvoiceId::new(next_id()),
            InvoiceData {
                invoice_direction: req.invoice_direction,
                invoice_kind: req.invoice_kind,
                party_id: req.party_id,
                invoice_code: req.invoice_code,
                invoice_no: req.invoice_no,
                invoice_date: req.invoice_date,
                gross_amount: req.gross_amount,
                net_amount: req.net_amount,
                tax_amount: req.tax_amount,
                rounding_adjustment_amount: req.rounding_adjustment_amount.unwrap_or(zero_amount()),
                rounding_reason: req.rounding_reason,
                original_invoice_id: None,
            },
            actor.id(),
        )?;
        persist_created_invoice(&self.db, &self.rbac, invoice.clone(), actor.clone()).await?;
        self.invoice_detail(&invoice.base.id).await
    }

    /// 原子创建或提交销项发票并完成分配。
    ///
    /// 新发票的 `BusinessDocument` 注册、发票实体、销项分配、应收子账开票进度、
    /// 销售单开票进度和审计全部位于同一 MongoDB 事务。已有草稿则用乐观锁
    /// 校验后在同一事务过账，前端不得再执行“先创建、再过账”。
    ///
    /// # 参数
    /// * `req` - 新发票或已有草稿身份、最终分配与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回已登记发票及其正式分配。
    ///
    /// # 错误
    /// * `ValidationError` - 新建/已有草稿参数组合或金额不合法
    /// * `ConflictError` - 草稿版本、状态或规范化发票号码冲突
    /// * `BusinessLogicError` - 跨主体、分配不守恒或超额开票
    pub async fn commit_invoice(&self, req: CommitInvoiceRequest, actor: &AuditActor) -> Result<InvoiceView> {
        req.validate()?;
        let command_receipt = CommandReceipt::from_payload(
            "sales-invoice-commit-",
            actor.id(),
            "invoice.commit",
            "invoice",
            &req.idempotency_key,
            &req,
        )?;
        if let Some(invoice_id) = command_receipt.committed_resource_id(&self.db).await? {
            return self.invoice_detail(&invoice_id).await;
        }
        let prepared = req.prepare()?;
        let expected_task_version = crate::work_item::expected_task_version(&req.expected_task_version)?;
        let work_item_id = req.work_item_id.clone();
        let policy_revision = self.rbac.current_policy_revision().await?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let command_receipt_for_tx = command_receipt.clone();
        let transaction_result = rbac
            .clone()
            .run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    let (mut invoice, plan_lines) = match prepared {
                        PreparedInvoiceCommit::New { invoice, allocations } => {
                            invoice.validate()?;
                            let new_invoice = Invoice::new(
                                InvoiceId::new(next_id()),
                                InvoiceData {
                                    invoice_direction: invoice.invoice_direction,
                                    invoice_kind: invoice.invoice_kind,
                                    party_id: invoice.party_id,
                                    invoice_code: invoice.invoice_code,
                                    invoice_no: invoice.invoice_no,
                                    invoice_date: invoice.invoice_date,
                                    gross_amount: invoice.gross_amount,
                                    net_amount: invoice.net_amount,
                                    tax_amount: invoice.tax_amount,
                                    rounding_adjustment_amount: invoice
                                        .rounding_adjustment_amount
                                        .unwrap_or(zero_amount()),
                                    rounding_reason: invoice.rounding_reason,
                                    original_invoice_id: None,
                                },
                                actor_id.as_str(),
                            )?;
                            register_created_invoice_document(
                                &db,
                                &rbac,
                                &new_invoice,
                                &actor_owned,
                                session,
                            )
                            .await?;
                            db.invoices().create(&new_invoice, session).await?;
                            (new_invoice, allocations)
                        }
                        PreparedInvoiceCommit::Existing {
                            invoice_id,
                            expected_version,
                            allocations,
                        } => {
                            let invoice = db
                                .invoices()
                                .find_by_id(&invoice_id, session)
                                .await?
                                .ok_or_else(|| Error::NotFound("发票不存在".to_string()))?;
                            ensure_expected_version(invoice.base.version, expected_version)?;
                            ensure_sales_invoice(&invoice)?;
                            (invoice, allocations)
                        }
                    };
                    if invoice.stable.status() != InvoiceStatus::Draft {
                        return Err(Error::ConflictError("发票已登记，请勿重复提交".to_string()));
                    }
                    let duplicate = db
                        .invoices()
                        .find_by_direction_and_normalized_no(
                            invoice.invoice_direction,
                            &invoice.normalized_no,
                            session,
                        )
                        .await?;
                    if duplicate
                        .as_ref()
                        .is_some_and(|other| other.base.id != invoice.base.id)
                    {
                        return Err(Error::ConflictError("发票号码已登记，请勿重复提交".to_string()));
                    }
                    let allocation_account_ids: Vec<ReceivableAccountId> = plan_lines
                        .iter()
                        .map(|line| line.receivable_account_id.clone())
                        .collect();
                    invoice_task::record_invoice_execution(
                        &db,
                        &work_item_id,
                        expected_task_version,
                        &invoice.party_id,
                        &allocation_account_ids,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    let allocation_ids: Vec<SalesInvoiceAllocationId> = (0..plan_lines.len())
                        .map(|_| SalesInvoiceAllocationId::new(next_id()))
                        .collect();
                    let plan = entities::receivable::SalesInvoiceAllocationPlan::new(
                        invoice.base.id.clone().into(),
                        invoice.gross_amount,
                        invoice.net_amount,
                        invoice.tax_amount,
                        &plan_lines,
                        &allocation_ids,
                    )?;
                    let account_id_strs: Vec<String> = plan
                        .account_invoicing_deltas()
                        .iter()
                        .map(|(id, _)| id.to_string())
                        .collect();
                    let accounts = db
                        .receivable_accounts()
                        .find_accounts_by_ids(&account_id_strs, session)
                        .await?;
                    let accounts_by_id: HashMap<&str, &ReceivableAccount> = accounts
                        .iter()
                        .map(|account| (account.base.id.as_str(), account))
                        .collect();
                    for (account_id, _) in plan.account_invoicing_deltas() {
                        let account = accounts_by_id
                            .get(account_id.as_ref())
                            .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
                        if account.counterparty_party_id != invoice.party_id {
                            return Err(Error::BusinessLogicError("禁止跨往来主体开票".to_string()));
                        }
                    }
                    let invoicing = db
                        .receivable_accounts()
                        .apply_invoicings_many(plan.account_invoicing_deltas(), &actor_id, session)
                        .await?;
                    if !invoicing.rejected.is_empty() {
                        return Err(Error::BusinessLogicError(
                            "子账剩余可开票额度不足，开票被拒绝".to_string(),
                        ));
                    }
                    invoice.mark_registered(&actor_id)?;
                    db.invoices().update(&mut invoice, session).await?;
                    db.receivable()
                        .create_sales_invoice_allocations_many(plan.new_allocations(), session)
                        .await?;
                    let audit = actor_owned.clone().resource_log(
                        "invoice.commit",
                        "invoice",
                        invoice.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    let mut receivable_account_ids = account_id_strs.clone();
                    receivable_account_ids.sort();
                    receivable_account_ids.dedup();
                    for account_id in receivable_account_ids {
                        invoice_task::sync_sales_invoice_task(
                            &db,
                            &ReceivableAccountId::new(account_id),
                            invoice_task::SalesInvoiceTaskChange::InvoicePosted,
                            session,
                        )
                        .await?;
                    }
                    let mut sales_order_ids: Vec<String> = accounts
                        .iter()
                        .map(|account| account.sales_order_id.to_string())
                        .collect();
                    sales_order_ids.sort();
                    sales_order_ids.dedup();
                    for sales_order_id in sales_order_ids {
                        crate::sales_order::update_sales_order_money_progress(
                            &db,
                            session,
                            &entities::ids::SalesOrderId::new(sales_order_id),
                            actor_id.clone(),
                            None,
                        )
                        .await?;
                    }
                    let committed_id = invoice.base.id.clone();
                    let command_audit =
                        command_receipt_for_tx.audit(actor_owned.clone(), committed_id.clone())?;
                    db.audit_logs().create(&command_audit, session).await?;
                    Ok::<String, crate::errors::Error>(committed_id)
                })
            })
            .await;

        let detail_id = match transaction_result {
            Ok(invoice_id) => invoice_id,
            Err(error) => match command_receipt.committed_resource_id(&self.db).await? {
                Some(invoice_id) => invoice_id,
                None => return Err(error),
            },
        };

        self.invoice_detail(&detail_id).await
    }

    /// 发票登记过账并分配（§8.3-2 事务不变量）。
    ///
    /// 同一事务内：规范化号码去重（`find_by_direction_and_normalized_no` +
    /// 唯一索引兜底）；校验发票与可开票子账同一往来主体；分配合计等于发票
    /// 金额；写销项发票分配；按条件原子更新子账净已开票进度
    /// （`apply_invoicing` 不超额开票）；发票迁移为已登记。
    /// 任一校验失败整体回滚。规范化发票号码唯一构成重复提交去重。
    ///
    /// # 参数
    /// * `id` - 发票 ID
    /// * `req` - 过账请求（分配行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回登记后发票视图。
    ///
    /// # 错误
    /// * `NotFound` - 发票或子账不存在
    /// * `ConflictError` - 规范化号码已登记或发票已登记
    /// * `BusinessLogicError` - 跨主体开票、分配合计不等或超额开票
    pub async fn post_invoice(
        &self,
        id: &str,
        req: PostInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<InvoiceView> {
        req.validate()?;
        let expected_task_version = crate::work_item::expected_task_version(&req.expected_task_version)?;
        let policy_revision = self.rbac.current_policy_revision().await?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let invoice_id = id.to_string();
        let detail_id = invoice_id.clone();
        let work_item_id = req.work_item_id.clone();
        let plan_lines = convert_post_allocations(&req.allocations);
        rbac.run_authorized_policy_transaction(policy_revision, move |session| {
            Box::pin(async move {
                let mut invoice = db
                    .invoices()
                    .find_by_id(&invoice_id, session)
                    .await?
                    .ok_or_else(|| Error::NotFound("发票不存在".to_string()))?;
                ensure_sales_invoice(&invoice)?;
                if invoice.stable.status() != entities::receivable::InvoiceStatus::Draft {
                    return Err(Error::ConflictError("发票已登记，请勿重复提交".to_string()));
                }
                let duplicate = db
                    .invoices()
                    .find_by_direction_and_normalized_no(
                        invoice.invoice_direction,
                        &invoice.normalized_no,
                        session,
                    )
                    .await?;
                if let Some(other) = duplicate {
                    if other.base.id != invoice.base.id {
                        return Err(Error::ConflictError("发票号码已登记，请勿重复提交".to_string()));
                    }
                }

                let allocation_account_ids: Vec<ReceivableAccountId> = plan_lines
                    .iter()
                    .map(|line| line.receivable_account_id.clone())
                    .collect();
                invoice_task::record_invoice_execution(
                    &db,
                    &work_item_id,
                    expected_task_version,
                    &invoice.party_id,
                    &allocation_account_ids,
                    &actor_owned,
                    session,
                )
                .await?;

                let allocation_ids: Vec<SalesInvoiceAllocationId> = (0..plan_lines.len())
                    .map(|_| SalesInvoiceAllocationId::new(next_id()))
                    .collect();
                let plan = entities::receivable::SalesInvoiceAllocationPlan::new(
                    invoice.base.id.clone().into(),
                    invoice.gross_amount,
                    invoice.net_amount,
                    invoice.tax_amount,
                    &plan_lines,
                    &allocation_ids,
                )?;
                let account_id_strs: Vec<String> = plan
                    .account_invoicing_deltas()
                    .iter()
                    .map(|(id, _)| id.to_string())
                    .collect();
                let accounts = db
                    .receivable_accounts()
                    .find_accounts_by_ids(&account_id_strs, session)
                    .await?;
                let accounts_by_id: HashMap<&str, &ReceivableAccount> = accounts
                    .iter()
                    .map(|account| (account.base.id.as_str(), account))
                    .collect();
                for (account_id, _) in plan.account_invoicing_deltas() {
                    let account = accounts_by_id
                        .get(account_id.as_ref())
                        .ok_or_else(|| Error::NotFound("应收往来子账不存在".to_string()))?;
                    if account.counterparty_party_id != invoice.party_id {
                        return Err(Error::BusinessLogicError("禁止跨往来主体开票".to_string()));
                    }
                }
                let invoicing = db
                    .receivable_accounts()
                    .apply_invoicings_many(plan.account_invoicing_deltas(), &actor_id, session)
                    .await?;
                if !invoicing.rejected.is_empty() {
                    return Err(Error::BusinessLogicError(
                        "子账剩余可开票额度不足，开票被拒绝".to_string(),
                    ));
                }
                invoice.mark_registered(&actor_id)?;
                db.invoices().update(&mut invoice, session).await?;
                db.receivable()
                    .create_sales_invoice_allocations_many(plan.new_allocations(), session)
                    .await?;
                let audit =
                    actor_owned
                        .clone()
                        .resource_log("invoice.post", "invoice", invoice.base.id.clone())?;
                db.audit_logs().create(&audit, session).await?;
                let mut receivable_account_ids = account_id_strs.clone();
                receivable_account_ids.sort();
                receivable_account_ids.dedup();
                for account_id in receivable_account_ids {
                    invoice_task::sync_sales_invoice_task(
                        &db,
                        &ReceivableAccountId::new(account_id),
                        invoice_task::SalesInvoiceTaskChange::InvoicePosted,
                        session,
                    )
                    .await?;
                }
                let mut sales_order_ids: Vec<String> = accounts
                    .iter()
                    .map(|account| account.sales_order_id.to_string())
                    .collect();
                sales_order_ids.sort();
                sales_order_ids.dedup();
                for sales_order_id in sales_order_ids {
                    crate::sales_order::update_sales_order_money_progress(
                        &db,
                        session,
                        &entities::ids::SalesOrderId::new(sales_order_id),
                        actor_id.clone(),
                        None,
                    )
                    .await?;
                }
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await?;

        self.invoice_detail(&detail_id).await
    }

    /// 装配发票视图。
    ///
    /// # 参数
    /// * `id` - 发票 ID
    ///
    /// # 返回
    /// 返回发票视图（含分配行与未分配余额）。
    ///
    /// # 错误
    /// * `NotFound` - 发票不存在
    async fn invoice_view(&self, id: String) -> Result<InvoiceView> {
        let invoice = self
            .db
            .invoices()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("发票不存在".to_string()))?;
        let (allocated_total, views) = match invoice.invoice_direction {
            InvoiceDirection::Purchase => {
                // 进项票分配独立存储（purchase_invoice_allocations）；视图共用销项形状，
                // 应付子账 ID 落 receivable_account_id 字段（前端以
                // payable_account_id ?? receivable_account_id 兜底读取）。
                let rows = self
                    .db
                    .purchase_invoice_allocations()
                    .find_allocations_by_invoices(&[invoice.base.id.clone().into()], &mut NoTransaction)
                    .await?;
                purchase_allocation_view(&rows)
            }
            InvoiceDirection::Sales => {
                let allocations = self
                    .db
                    .sales_invoice_allocations()
                    .find_allocations_by_invoices(&[invoice.base.id.clone().into()], &mut NoTransaction)
                    .await?;
                sales_allocation_view(&allocations)
            }
        };
        Ok(InvoiceView {
            id: invoice.base.id.clone(),
            invoice_direction: invoice.invoice_direction,
            invoice_kind: invoice.invoice_kind,
            party_id: invoice.party_id.to_string(),
            invoice_code: invoice.invoice_code,
            invoice_no: invoice.invoice_no,
            invoice_date: invoice.invoice_date,
            gross_amount: invoice.gross_amount,
            net_amount: invoice.net_amount,
            tax_amount: invoice.tax_amount,
            rounding_adjustment_amount: invoice.rounding_adjustment_amount,
            rounding_reason: invoice.rounding_reason,
            original_invoice_id: invoice.original_invoice_id.map(|id| id.to_string()),
            status: invoice.stable.status(),
            version: invoice.base.version,
            created_at: invoice.base.created_at,
            unallocated_amount: invoice.gross_amount.checked_sub(allocated_total),
            allocated_total,
            allocations: views,
        })
    }
}

/// 汇总销项发票分配并装配视图（`APPLY` 加、`REVERSE` 减）。
///
/// # 参数
/// * `allocations` - 销项发票分配集合
///
/// # 返回
/// 返回 `(净已分配含税合计, 分配视图列表)`。
fn sales_allocation_view(
    allocations: &[SalesInvoiceAllocation],
) -> (Amount, Vec<crate::receivable::dto::SalesInvoiceAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .iter()
        .map(|allocation| {
            match allocation.allocation_action {
                AllocationAction::Apply => net = net.checked_add(allocation.allocated_gross_amount),
                AllocationAction::Reverse => net = net.checked_sub(allocation.allocated_gross_amount),
            }
            crate::receivable::dto::SalesInvoiceAllocationView {
                id: allocation.base.id.clone(),
                allocation_seq: allocation.allocation_seq,
                allocation_action: allocation.allocation_action,
                receivable_account_id: allocation.receivable_account_id.to_string(),
                allocated_gross_amount: allocation.allocated_gross_amount,
                allocated_net_amount: allocation.allocated_net_amount,
                allocated_tax_amount: allocation.allocated_tax_amount,
                reverses_allocation_id: allocation
                    .reverses_allocation_id
                    .as_ref()
                    .map(|id| id.to_string()),
            }
        })
        .collect();
    (net, views)
}

/// 汇总进项发票分配并转换为跨方向复用的发票分配视图。
///
/// # 参数
/// * `allocations` - 进项发票分配集合
///
/// # 返回
/// 返回 `(净已分配含税合计, 分配视图列表)`。
fn purchase_allocation_view(
    allocations: &[PurchaseInvoiceAllocation],
) -> (Amount, Vec<crate::receivable::dto::SalesInvoiceAllocationView>) {
    let mut net = zero_amount();
    let views = allocations
        .iter()
        .map(|allocation| {
            // 进项/销项分配动作枚举跨域不共享（见 A-G7），此处显式转换。
            let action = match allocation.allocation_action {
                entities::payable::AllocationAction::Apply => AllocationAction::Apply,
                entities::payable::AllocationAction::Reverse => AllocationAction::Reverse,
            };
            match action {
                AllocationAction::Apply => net = net.checked_add(allocation.allocated_gross_amount),
                AllocationAction::Reverse => net = net.checked_sub(allocation.allocated_gross_amount),
            }
            crate::receivable::dto::SalesInvoiceAllocationView {
                id: allocation.base.id.clone(),
                allocation_seq: allocation.allocation_seq,
                allocation_action: action,
                receivable_account_id: allocation.payable_account_id.to_string(),
                allocated_gross_amount: allocation.allocated_gross_amount,
                allocated_net_amount: allocation.allocated_net_amount,
                allocated_tax_amount: allocation.allocated_tax_amount,
                reverses_allocation_id: allocation
                    .reverses_allocation_id
                    .as_ref()
                    .map(|id| id.to_string()),
            }
        })
        .collect();
    (net, views)
}

/// 发票创建必须跳过绑定：政策只能是 `NO_APPROVAL`。
///
/// # 返回
/// 返回 `SkipNoApproval`。
///
/// # 错误
/// 政策缺失或误登记为必须审批时返回部署不变量错误。
fn invoice_create_binding_decision() -> Result<BindingDecision> {
    let policy = policy_of(DocumentType::Invoice)?;
    match &policy {
        DocumentApprovalPolicy::NoApproval(no_approval) => {
            if no_approval.document_type != DocumentType::Invoice {
                return Err(Error::Internal("发票政策类型不匹配".to_string()));
            }
            Ok(binding_decision(policy.requirement()))
        }
        DocumentApprovalPolicy::ProcessRequired(_) => Err(Error::Internal(
            "发票必须是 NO_APPROVAL，不得绑定流程".to_string(),
        )),
    }
}

/// 确认发票创建路径不得查询发布定义。
///
/// # 错误
/// 绑定决定不是跳过时返回错误。
fn ensure_invoice_skips_approval_binding() -> Result<BindingDecision> {
    let decision = invoice_create_binding_decision()?;
    if decision != BindingDecision::SkipNoApproval {
        return Err(Error::Internal("发票创建必须跳过审批绑定".to_string()));
    }
    Ok(decision)
}

/// 发票不得注册空审批适配器。
///
/// # 错误
/// 适配器登记存在时返回部署不变量错误。
fn ensure_invoice_has_no_adapter() -> Result<()> {
    if adapter_spec_of(DocumentType::Invoice).is_ok() {
        return Err(Error::Internal("发票不得注册审批适配器".to_string()));
    }
    Ok(())
}

/// 发票往来主体作为绑定上下文组织，不得用空串补位。
///
/// # 参数
/// * `invoice` - 待登记发票
///
/// # 返回
/// 返回非空往来主体。
///
/// # 错误
/// 往来主体为空时返回校验错误。
fn invoice_binding_organization_id(invoice: &Invoice) -> Result<String> {
    let org = invoice.party_id.to_string();
    if org.trim().is_empty() {
        return Err(Error::ValidationError(
            "发票缺少往来主体，无法构造绑定上下文".to_string(),
        ));
    }
    Ok(org)
}

/// 构造发票创建绑定命令。客户端不得提交定义 ID。
///
/// # 参数
/// * `invoice` - 待登记发票
/// * `creator_id` - 创建人
///
/// # 错误
/// 往来主体为空时返回校验错误。
fn invoice_bind_command(invoice: &Invoice, creator_id: &str) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: DocumentType::Invoice,
        business_object_id: invoice.base.id.clone(),
        business_object_version: invoice.base.version,
        context: BindingRevalidationContext {
            organization_id: invoice_binding_organization_id(invoice)?,
            creator_id: creator_id.to_string(),
        },
    })
}

/// 将绑定端口返回值落实为发票注册行：空绑定保持未绑定。
///
/// # 参数
/// * `document` - 发票注册行
/// * `binding` - 统一绑定端口返回值
///
/// # 返回
/// 固定返回 `None`。
///
/// # 错误
/// 端口返回绑定或注册行已预置绑定时返回错误。
fn apply_invoice_create_binding(
    document: &mut BusinessDocument,
    binding: Option<ApprovalDefinitionBinding>,
) -> Result<Option<ApprovalDefinitionBinding>> {
    if binding.is_some() {
        return Err(Error::Internal(
            "发票为 NO_APPROVAL，不得写入审批绑定".to_string(),
        ));
    }
    if document.approval_binding.is_some() {
        return Err(Error::Internal("发票注册行不得预置审批绑定".to_string()));
    }
    if document.document_type != DocumentType::Invoice {
        return Err(Error::Internal("发票创建只能注册 Invoice 单据".to_string()));
    }
    Ok(None)
}

/// 在调用方事务内登记发票单据并证明空绑定。
///
/// 必须先确认政策跳过，再调用统一绑定端口；不得查询发布定义后假装成功。
///
/// # 错误
/// 政策非无审批、端口返回绑定或写入失败时返回错误。
async fn persist_unbound_invoice_document(
    db: &Database,
    rbac: &SharedRbacService,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_invoice_skips_approval_binding()?;
    ensure_invoice_has_no_adapter()?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, executor).await?;
    apply_invoice_create_binding(&mut document, binding)?;
    persist_registered_document(db, &document, executor).await
}

/// 为已构造发票登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
pub(super) async fn register_created_invoice_document(
    db: &Database,
    rbac: &SharedRbacService,
    invoice: &Invoice,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = invoice_bind_command(invoice, actor.id())?;
    let document = new_registered_document(
        &invoice.base.id,
        DocumentType::Invoice,
        invoice.invoice_no.clone(),
    )?;
    persist_unbound_invoice_document(db, rbac, document, &bind_command, actor, executor).await
}

/// 在创建事务内写入发票草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或发票写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_invoice(
    db: &Database,
    rbac: &SharedRbacService,
    invoice: Invoice,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor
        .clone()
        .resource_log("invoice.create", "invoice", invoice.base.id.clone())?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                register_created_invoice_document(&db, &rbac, &invoice, &actor, session).await?;
                db.invoices().create(&invoice, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

#[cfg(test)]
mod invoice_no_approval_tests {
    use super::{
        apply_invoice_create_binding, ensure_invoice_has_no_adapter, ensure_invoice_skips_approval_binding,
        invoice_bind_command, invoice_create_binding_decision, policy_of, BindingDecision,
        DocumentApprovalPolicy, DocumentType, Invoice, InvoiceData,
    };
    use crate::approval::binding::binding_from_published;
    use crate::document_registry::new_registered_document;
    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::ProcessKind;
    use entities::common::time::{BusinessDate, Instant};
    use entities::ids::{InvoiceId, PartyId};
    use entities::money::Amount;
    use entities::receivable::{InvoiceDirection, InvoiceKind};
    use std::str::FromStr;

    fn draft_invoice() -> Invoice {
        Invoice::new(
            InvoiceId::new("inv-1"),
            InvoiceData {
                invoice_direction: InvoiceDirection::Sales,
                invoice_kind: InvoiceKind::Blue,
                party_id: PartyId::new("party-1"),
                invoice_code: None,
                invoice_no: "INV-1".into(),
                invoice_date: BusinessDate::from_ymd(2026, 8, 6).expect("日期合法"),
                gross_amount: Amount::from_str("100").expect("金额合法"),
                net_amount: Amount::from_str("88.50").expect("金额合法"),
                tax_amount: Amount::from_str("11.50").expect("金额合法"),
                rounding_adjustment_amount: Amount::from_str("0").expect("金额合法"),
                rounding_reason: None,
                original_invoice_id: None,
            },
            "admin-1",
        )
        .expect("草稿必须可构造")
    }

    /// 政策仅含 document_type、approval_requirement、process_kind，不得注册空 Adapter。
    #[test]
    fn invoice_policy_is_no_approval_identity_only() {
        let policy = policy_of(DocumentType::Invoice).expect("发票政策必须存在");
        let DocumentApprovalPolicy::NoApproval(no_approval) = &policy else {
            panic!("发票必须是 NO_APPROVAL");
        };
        assert_eq!(no_approval.document_type, DocumentType::Invoice);
        assert_eq!(no_approval.process_kind, ProcessKind::Invoice);
        assert_eq!(
            invoice_create_binding_decision().expect("绑定决定"),
            BindingDecision::SkipNoApproval
        );
        assert_eq!(
            ensure_invoice_skips_approval_binding().expect("必须跳过"),
            BindingDecision::SkipNoApproval
        );
        ensure_invoice_has_no_adapter().expect("不得注册空适配器");
    }

    /// 创建必须注册 BusinessDocument，绑定端口返回空，禁止写入绑定。
    #[test]
    fn create_registers_document_and_returns_empty_binding() {
        let invoice = draft_invoice();
        let command = invoice_bind_command(&invoice, "admin-1").expect("绑定命令");
        assert_eq!(command.document_type, DocumentType::Invoice);
        assert_eq!(command.business_object_id, invoice.base.id);
        assert_eq!(command.context.organization_id, "party-1");

        let mut document = new_registered_document(
            &invoice.base.id,
            DocumentType::Invoice,
            invoice.invoice_no.clone(),
        )
        .expect("可注册");
        assert!(document.approval_binding.is_none());
        let empty = apply_invoice_create_binding(&mut document, None).expect("空绑定");
        assert!(empty.is_none());
        assert!(document.approval_binding.is_none());

        let forged = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .expect("测试绑定");
        assert!(apply_invoice_create_binding(&mut document, Some(forged)).is_err());
    }

    /// 创建路径调用统一绑定端口，不查询发布定义、不启动实例、不建任务。
    #[test]
    fn create_does_not_query_definition_or_start_instance() {
        let production = include_str!("invoice.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("persist_created_invoice"));
        assert!(production.contains("register_created_invoice_document"));
        assert!(production.contains("persist_unbound_invoice_document"));
        assert!(production.contains("bind_published_definition_on_document_create"));
        assert!(production.contains("DocumentType::Invoice"));
        assert!(production.contains("new_registered_document"));
        assert!(production.contains("ensure_invoice_skips_approval_binding"));
        assert!(production.contains("ensure_invoice_has_no_adapter"));
        assert!(!production.contains("pub async fn submit_invoice"));
        assert!(!production.contains("start_invoice_approval"));
        assert!(!production.contains("InvoiceAdapter"));
        assert!(!production.contains("load_published_graph"));
        let invoice_create = production
            .split("pub async fn create_invoice")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn post_invoice").next())
            .expect("create_invoice 生产片段");
        assert!(invoice_create.contains("persist_created_invoice"));
        assert!(!invoice_create.contains("prepare_start"));
        assert!(!invoice_create.contains("attach_published_binding"));
        assert!(!invoice_create.contains("WorkItem"));
        assert!(!invoice_create.contains("start_approval"));
    }
}
