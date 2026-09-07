//! 资金单据往来名称、来源单据与核销行查询。

use std::collections::{HashMap, HashSet};

use erp_core::common::time::BusinessDate;
use erp_core::ids::{PartyId, PayableAccountId, ReceivableAccountId, SalesOrderRevisionLineId};
use erp_finance::entity::payable::PayableAccount;
use erp_finance::entity::payable::SupplierPayment;
use erp_finance::entity::receivable::CustomerReceipt;
use erp_finance::entity::receivable::ReceivableAccount;
use erp_finance::repository::PayableExt;
use erp_finance::repository::ReceivableExt;
use erp_party::PartyExt;
use erp_sales::repository::SalesOrderExt;
use persistence_core::Executor;

use super::super::brief::{format_instant_date, BriefLine, BRIEF_LINE_LIMIT};
use super::super::presentation::format_yuan;
use super::super::WorkbenchReadService;
use super::mapping::{
    payable_origins_from_rows, payment_brief_lines, payment_origins_from_rows, receipt_brief_lines,
    receipt_origins_from_rows, receivable_origins_from_rows, voucher_account_line,
};
use super::{FundsOrigins, InvoiceRequirementBrief, ReceivableRevisionBriefs, VoucherAccountBrief};
use crate::errors::Result;
use crate::workbench::authority::funds::mapping::voucher_revision_ids;

impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 从创建审计回填单据创建人（资金单据实体不落创建人字段）。
    ///
    /// # 参数
    /// * `resource_type` - 审计资源类型（与单据类型一致）
    /// * `ids` - 单据 ID 集合
    /// * `executor` - 事务执行器
    ///
    /// # 返回
    /// 返回单据 ID → 创建人 ID 映射；无审计时返回空映射。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_created_by_from_audit(
        &self,
        resource_type: &str,
        ids: &HashSet<String>,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        self.funds_reader()
            .load_created_by_from_audit(resource_type, ids, executor)
            .await
    }

    /// 批量读取应付供应商展示名。
    pub(super) async fn payable_supplier_names(
        &self,
        accounts: &[PayableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        self.funds_reader()
            .payable_supplier_names(accounts, executor)
            .await
    }

    /// 批量读取应付来源采购单号。
    pub(super) async fn payable_purchase_numbers(
        &self,
        accounts: &[PayableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        self.funds_reader()
            .payable_purchase_numbers(accounts, executor)
            .await
    }

    /// 批量汇总每个应付子账最早分录到期日。
    pub(super) async fn payable_due_dates(
        &self,
        accounts: &[PayableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, erp_core::common::time::BusinessDate>> {
        let ids = accounts
            .iter()
            .map(|account| PayableAccountId::new(account.base.id.clone()))
            .collect::<Vec<_>>();
        self.db
            .payable_entries()
            .minimum_due_dates_by_accounts(&ids, executor)
            .await
            .map_err(Into::into)
    }

    /// 按主体 ID 批量读取当前修订法定名称。
    ///
    /// # 参数
    /// * `party_ids` - 主体 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回主体 ID 到法定名称；没有当前修订时该主体不上表。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn party_legal_names(
        &self,
        party_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        self.funds_reader().party_legal_names(party_ids, executor).await
    }

    /// 按客户账号批量解析展示名（当前主体法定名称，缺失时回退客户编号）。
    ///
    /// # 参数
    /// * `customer_ids` - 客户账号 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回客户账号 ID 到展示名。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn customer_display_names(
        &self,
        customer_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        self.funds_reader()
            .customer_display_names(customer_ids, executor)
            .await
    }

    /// 按供应商账号批量解析展示名（当前主体法定名称，缺失时回退供应商编号）。
    ///
    /// # 参数
    /// * `supplier_ids` - 供应商账号 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回供应商账号 ID 到展示名。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(crate) async fn supplier_display_names(
        &self,
        supplier_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        self.funds_reader()
            .supplier_display_names(supplier_ids, executor)
            .await
    }

    /// 按销售单 ID 批量读取单号。
    ///
    /// # 参数
    /// * `sales_order_ids` - 销售单 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回销售单 ID 到单号。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn sales_order_numbers(
        &self,
        sales_order_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if sales_order_ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(self
            .funds_reader()
            .read_sales_orders(sales_order_ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.order_no))
            .collect())
    }

    /// 批量读取应收子账来源销售版本中的开票要求、卡券面值、张数与有效期。
    ///
    /// # 参数
    /// * `accounts` - 本批应收子账
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部销售版本的开票要求，以及卡券销售版本 ID 到专属卡券简报。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn receivable_account_revision_briefs(
        &self,
        accounts: &[ReceivableAccount],
        executor: &mut dyn Executor,
    ) -> Result<ReceivableRevisionBriefs> {
        let revision_ids = accounts
            .iter()
            .map(|account| account.source_sales_order_revision_id.clone())
            .collect::<Vec<_>>();
        let revisions = self
            .funds_reader()
            .read_sales_revisions(
                &revision_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let revision_lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revisions(&revision_ids, executor)
            .await?;
        let revision_line_ids = revision_lines
            .iter()
            .map(|line| SalesOrderRevisionLineId::new(line.base.id.clone()))
            .collect::<Vec<_>>();
        let voucher_lines = self
            .db
            .sales_order_voucher_line_revisions()
            .list_by_revision_line_ids(&revision_line_ids, executor)
            .await?;
        let revision_line_by_id = revision_lines
            .iter()
            .map(|line| (line.base.id.clone(), line))
            .collect::<HashMap<_, _>>();
        let invoice_requirements = revisions
            .iter()
            .map(|revision| {
                (
                    revision.base.id.clone(),
                    InvoiceRequirementBrief {
                        invoice_type: revision.invoice_requirement_snapshot.invoice_type.clone(),
                        tax_point: revision.invoice_requirement_snapshot.tax_point.clone(),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let command_voucher_revision_ids = voucher_revision_ids(&revisions);
        let mut briefs = revisions
            .iter()
            .filter(|revision| command_voucher_revision_ids.contains(&revision.base.id))
            .map(|revision| {
                (
                    revision.base.id.clone(),
                    VoucherAccountBrief {
                        expiry_label: revision.voucher_expiry_at.map(format_instant_date),
                        ..VoucherAccountBrief::default()
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let mut raw_lines: HashMap<String, Vec<(u32, BriefLine)>> = HashMap::new();
        let mut face_values: HashMap<String, HashSet<String>> = HashMap::new();
        for voucher in voucher_lines {
            let Some(revision_line) = revision_line_by_id.get(&voucher.revision_line_id.to_string()) else {
                continue;
            };
            let revision_id = revision_line.sales_order_revision_id.to_string();
            let brief = briefs.entry(revision_id.clone()).or_default();
            brief.total_count = brief.total_count.saturating_add(u64::from(voucher.card_count));
            face_values
                .entry(revision_id.clone())
                .or_default()
                .insert(format_yuan(&voucher.face_value));
            raw_lines.entry(revision_id).or_default().push((
                revision_line.line_no,
                voucher_account_line(revision_line, &voucher),
            ));
        }
        for (revision_id, brief) in &mut briefs {
            let mut lines = raw_lines.remove(revision_id).unwrap_or_default();
            lines.sort_by_key(|(line_no, _)| *line_no);
            brief.more_count = lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
            brief.lines = lines
                .into_iter()
                .map(|(_, line)| line)
                .take(BRIEF_LINE_LIMIT)
                .collect();
            if let Some(values) = face_values.get(revision_id) {
                let mut values = values.iter().cloned().collect::<Vec<_>>();
                values.sort();
                brief.face_summary = Some(values.join(" / "));
            }
        }
        Ok(ReceivableRevisionBriefs {
            command_voucher_revision_ids,
            invoice_requirements,
            vouchers: briefs,
        })
    }

    /// 批量读取主体在当前业务日生效的默认优先税号。
    ///
    /// # 参数
    /// * `party_ids` - 本批往来主体 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回主体 ID 到税号；同一主体存在多条有效记录时采用仓储默认优先顺序的首条。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn current_tax_profile_nos(
        &self,
        party_ids: &[PartyId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let mut unique_ids = party_ids.to_vec();
        unique_ids.sort_by_key(ToString::to_string);
        unique_ids.dedup();
        let profiles = self
            .db
            .party_tax_profiles()
            .list_current_for_parties_on(&unique_ids, BusinessDate::today(), executor)
            .await?;
        let mut tax_nos = HashMap::new();
        for profile in profiles {
            tax_nos
                .entry(profile.party_id.to_string())
                .or_insert(profile.tax_no);
        }
        Ok(tax_nos)
    }

    /// 批量计算应收子账正向分录的最早到期日。
    ///
    /// # 参数
    /// * `accounts` - 本批应收子账
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回子账 ID 到最早正向应收到期日；冲减分录不参与计算。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn receivable_account_due_dates(
        &self,
        accounts: &[ReceivableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, BusinessDate>> {
        let account_ids = accounts
            .iter()
            .map(|account| ReceivableAccountId::new(account.base.id.clone()))
            .collect::<Vec<_>>();
        self.db
            .receivable_entries()
            .minimum_increase_due_dates_by_accounts(&account_ids, executor)
            .await
            .map_err(Into::into)
    }

    /// 把回款待过账核销转成按回款单分组的简报行。
    ///
    /// # 参数
    /// * `receipts` - 本批回款单
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回回款单 ID 到核销销售单行。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn receipt_allocation_lines(
        &self,
        receipts: &[CustomerReceipt],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<BriefLine>>> {
        let entry_ids = receipts
            .iter()
            .flat_map(|receipt| {
                receipt
                    .pending_allocations
                    .iter()
                    .map(|item| item.receivable_entry_id.to_string())
            })
            .collect::<Vec<_>>();
        if entry_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let entries = self
            .funds_reader()
            .read_receivable_entries(&entry_ids, executor)
            .await?;
        let account_ids = entries
            .iter()
            .map(|entry| entry.receivable_account_id.to_string())
            .collect::<Vec<_>>();
        let accounts = self
            .funds_reader()
            .read_receivable_accounts(&account_ids, executor)
            .await?;
        let sales_order_ids = accounts
            .iter()
            .map(|account| account.sales_order_id.to_string())
            .collect::<Vec<_>>();
        let sales_nos = self.sales_order_numbers(&sales_order_ids, executor).await?;
        let account_by_id = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account))
            .collect::<HashMap<_, _>>();
        let entry_by_id = entries
            .into_iter()
            .map(|entry| (entry.base.id.clone(), entry))
            .collect::<HashMap<_, _>>();
        Ok(receipts
            .iter()
            .map(|receipt| {
                (
                    receipt.base.id.clone(),
                    receipt_brief_lines(
                        &receipt.pending_allocations,
                        &entry_by_id,
                        &account_by_id,
                        &sales_nos,
                    ),
                )
            })
            .collect())
    }

    /// 批量读取原回款单及其核销对象，供退款和冲正简报复用。
    ///
    /// # 参数
    /// * `receipt_ids` - 原回款单 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回原回款 ID 到业务单号、主体、金额、银行流水和核销行。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn customer_receipt_origins(
        &self,
        receipt_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<FundsOrigins> {
        let receipts = self
            .funds_reader()
            .read_customer_receipts(receipt_ids, executor)
            .await?;
        let party_ids = receipts
            .iter()
            .map(|receipt| receipt.counterparty_party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        let lines = self.receipt_allocation_lines(&receipts, executor).await?;
        Ok(receipt_origins_from_rows(receipts, party_names, lines))
    }

    /// 批量读取原应收分录及其销售单、往来主体。
    ///
    /// # 参数
    /// * `entry_ids` - 原应收分录 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回分录 ID 到可读来源和核销影响上下文。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn receivable_entry_origins(
        &self,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<FundsOrigins> {
        let entries = self
            .funds_reader()
            .read_receivable_entries(entry_ids, executor)
            .await?;
        let account_ids = entries
            .iter()
            .map(|entry| entry.receivable_account_id.to_string())
            .collect::<Vec<_>>();
        let accounts = self
            .funds_reader()
            .read_receivable_accounts(&account_ids, executor)
            .await?;
        let sales_nos = self
            .sales_order_numbers(
                &accounts
                    .iter()
                    .map(|account| account.sales_order_id.to_string())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let party_names = self
            .party_legal_names(
                &accounts
                    .iter()
                    .map(|account| account.counterparty_party_id.to_string())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let accounts = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account))
            .collect::<HashMap<_, _>>();
        Ok(receivable_origins_from_rows(
            entries,
            accounts,
            party_names,
            sales_nos,
        ))
    }

    /// 批量读取原付款单及其核销对象，供退款和冲正简报复用。
    ///
    /// # 参数
    /// * `payment_ids` - 原付款单 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回原付款 ID 到业务单号、供应商、金额、凭证和核销行。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn supplier_payment_origins(
        &self,
        payment_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<FundsOrigins> {
        let payments = self
            .funds_reader()
            .read_supplier_payments(payment_ids, executor)
            .await?;
        let supplier_names = self
            .supplier_display_names(
                &payments
                    .iter()
                    .map(|payment| payment.supplier_id.to_string())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let lines = self.payment_allocation_lines(&payments, executor).await?;
        Ok(payment_origins_from_rows(payments, supplier_names, lines))
    }

    /// 批量读取原应付分录及其采购单、供应商。
    ///
    /// # 参数
    /// * `entry_ids` - 原应付分录 ID
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回分录 ID 到可读来源和核销影响上下文。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn payable_entry_origins(
        &self,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<FundsOrigins> {
        let entries = self
            .funds_reader()
            .read_payable_entries(entry_ids, executor)
            .await?;
        let account_ids = entries
            .iter()
            .map(|entry| entry.payable_account_id.to_string())
            .collect::<Vec<_>>();
        let accounts = self
            .funds_reader()
            .read_payable_accounts(&account_ids, executor)
            .await?;
        let purchase_nos = self.payable_purchase_numbers(&accounts, executor).await?;
        let supplier_names = self.payable_supplier_names(&accounts, executor).await?;
        let accounts = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account))
            .collect::<HashMap<_, _>>();
        Ok(payable_origins_from_rows(
            entries,
            accounts,
            supplier_names,
            purchase_nos,
        ))
    }

    /// 把已过账付款核销事实转成按付款单分组的采购单简报行。
    ///
    /// # 参数
    /// * `payments` - 本批付款单
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回付款单 ID 到采购单核销行。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn payment_allocation_lines(
        &self,
        payments: &[SupplierPayment],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, Vec<BriefLine>>> {
        if payments.is_empty() {
            return Ok(HashMap::new());
        }
        let payment_ids = payments
            .iter()
            .map(|payment| payment.base.id.clone().into())
            .collect::<Vec<_>>();
        let allocations = self
            .db
            .payment_allocations()
            .find_allocations_by_payments(&payment_ids, executor)
            .await?;
        let entry_ids = allocations
            .iter()
            .map(|allocation| allocation.payable_entry_id.to_string())
            .collect::<Vec<_>>();
        let entries = self
            .funds_reader()
            .read_payable_entries(&entry_ids, executor)
            .await?;
        let accounts = self
            .funds_reader()
            .read_payable_accounts(
                &entries
                    .iter()
                    .map(|entry| entry.payable_account_id.to_string())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let purchase_nos = self.payable_purchase_numbers(&accounts, executor).await?;
        let account_by_id = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account))
            .collect::<HashMap<_, _>>();
        let entry_by_id = entries
            .into_iter()
            .map(|entry| (entry.base.id.clone(), entry))
            .collect::<HashMap<_, _>>();
        Ok(payments
            .iter()
            .map(|payment| {
                let payment_allocations = allocations
                    .iter()
                    .filter(|allocation| allocation.supplier_payment_id.as_ref() == payment.base.id)
                    .cloned()
                    .collect::<Vec<_>>();
                (
                    payment.base.id.clone(),
                    payment_brief_lines(&payment_allocations, &entry_by_id, &account_by_id, &purchase_nos),
                )
            })
            .collect())
    }
}
