//! 资金与票款单据审批任务的事项简报装载。
//!
//! 覆盖回款、退款、冲正、付款和应收子账。创建人从审计事实回填，往来名称从
//! 主体当前修订读取。正式通过/驳回仍走单据审批命令。

use std::collections::{HashMap, HashSet};

use database::{
    AccessControlExt, CustomerExt, Executor, PartyExt, PayableExt, PurchaseOrderExt, ReceivableExt,
    ReturnsExt, SalesOrderExt, SupplierExt,
};
use entities::common::time::BusinessDate;
use entities::ids::{PartyId, PayableAccountId, ReceivableAccountId, SalesOrderRevisionLineId};
use entities::party::Party;
use entities::payable::{PayableAccount, PayableEntry, PaymentAllocation, SupplierPayment};
use entities::receivable::{
    CustomerReceipt, EntryDirection as ReceivableEntryDirection, PendingReceiptAllocation, ReceivableAccount,
    ReceivableEntry,
};
use entities::sales_order::{SalesOrderRevisionLine, SalesOrderVoucherLineRevision};

use super::brief::{
    format_instant_date, join_list_summary, line_title, non_empty, push_section, BriefLine,
    ObjectBriefSource, BRIEF_LINE_LIMIT,
};
use super::presentation::format_yuan;
use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, WorkItemService};
use crate::errors::Result;

#[derive(Debug, Clone, Default)]
struct FundsOriginBrief {
    counterparty: Option<String>,
    original_document: Option<String>,
    original_amount: Option<String>,
    bank_reference: Option<String>,
    allocation_summary: Option<String>,
    lines: Vec<BriefLine>,
}

#[derive(Debug, Clone, Default)]
struct VoucherAccountBrief {
    expiry_label: Option<String>,
    face_summary: Option<String>,
    total_count: u64,
    lines: Vec<BriefLine>,
    more_count: u32,
}

#[derive(Debug, Clone, Default)]
struct InvoiceRequirementBrief {
    invoice_type: String,
    tax_point: String,
}

#[derive(Debug, Clone, Default)]
struct ReceivableRevisionBriefs {
    invoice_requirements: HashMap<String, InvoiceRequirementBrief>,
    vouchers: HashMap<String, VoucherAccountBrief>,
}

impl WorkItemService {
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
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let resource_ids = ids.iter().cloned().collect::<Vec<_>>();
        let audits = self
            .db
            .audit_logs()
            .list_work_item_creation_audits(resource_type, &resource_ids, executor)
            .await?;
        let mut created_by = HashMap::new();
        for audit in audits {
            if let Some(resource_id) = audit.resource_id.as_deref() {
                created_by
                    .entry(resource_id.to_string())
                    .or_insert_with(|| audit.actor_id.clone());
            }
        }
        Ok(created_by)
    }

    /// 回款审批任务的对象事实：任务对象是回款单本身。
    ///
    /// 回款单实体不记录创建人，创建操作人从 `customer_receipt.create` 审计事实取，
    /// 缺失时为空参与权（创建人无管理权限时不得仅凭创建事实看到任务）。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入回款单号、往来主体、金额和待核销销售单。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_customer_receipt_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::CustomerReceipt);
        if ids.is_empty() {
            return Ok(());
        }
        let receipts = self
            .db
            .customer_receipts()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        if receipts.is_empty() {
            return Ok(());
        }
        let created_by = self
            .load_created_by_from_audit(
                "customer_receipt",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let party_ids = receipts
            .iter()
            .map(|item| item.counterparty_party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        let allocation_lines = self.receipt_allocation_lines(&receipts, executor).await?;
        for receipt in receipts {
            let counterparty = party_names
                .get(&receipt.counterparty_party_id.to_string())
                .cloned();
            let lines = allocation_lines
                .get(&receipt.base.id)
                .cloned()
                .unwrap_or_default();
            let mut fact = ObjectFact::new(
                receipt.base.id.clone(),
                format!("回款单 {}", receipt.receipt_no),
                created_by.get(&receipt.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = counterparty.clone();
            fact.impact_summary = Some("不审批则回款不能过账、不能核销应收".to_string());
            fact.brief_source = Some(receipt_brief_source(&receipt, counterparty.as_deref(), lines));
            facts.insert((ObjectKind::CustomerReceipt, receipt.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 客户退款审批任务的对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入退款单号、金额和原因。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_customer_refund_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::CustomerRefund);
        if ids.is_empty() {
            return Ok(());
        }
        let refunds = self
            .db
            .customer_refunds()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        let created_by = self
            .load_created_by_from_audit(
                "customer_refund",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let customer_ids = refunds
            .iter()
            .map(|refund| refund.customer_id.to_string())
            .collect::<Vec<_>>();
        let customer_names = self.customer_display_names(&customer_ids, executor).await?;
        let receipt_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_receipt_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let receipt_origins = self.customer_receipt_origins(&receipt_ids, executor).await?;
        let entry_ids = refunds
            .iter()
            .filter_map(|refund| {
                refund
                    .original_receivable_entry_id
                    .as_ref()
                    .map(ToString::to_string)
            })
            .collect::<Vec<_>>();
        let entry_origins = self.receivable_entry_origins(&entry_ids, executor).await?;
        for refund in refunds {
            let customer = customer_names.get(&refund.customer_id.to_string()).cloned();
            let origin = refund
                .original_receipt_id
                .as_ref()
                .and_then(|id| receipt_origins.get(&id.to_string()))
                .or_else(|| {
                    refund
                        .original_receivable_entry_id
                        .as_ref()
                        .and_then(|id| entry_origins.get(&id.to_string()))
                });
            let mut fact = ObjectFact::new(
                refund.base.id.clone(),
                format!("客户退款 {}", refund.refund_no),
                created_by.get(&refund.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = customer
                .clone()
                .or_else(|| origin.and_then(|item| item.counterparty.clone()));
            fact.impact_summary = Some("不审批则客户退款不能过账".to_string());
            let mut brief = amount_reason_brief(
                format_yuan(&refund.amount),
                vec![
                    ("客户", customer.clone()),
                    ("原因", non_empty(&refund.reason_text)),
                    ("发生日", Some(format_instant_date(refund.occurred_at))),
                ],
                join_list_summary([
                    customer,
                    Some(format_yuan(&refund.amount)),
                    non_empty(&refund.reason_text),
                ]),
            );
            append_funds_origin(
                &mut brief,
                origin,
                refund.evidence_attachment_id.is_some(),
                "通过后追加应收冲减与反向核销，原回款或应收事实保留",
            );
            fact.brief_source = Some(brief);
            facts.insert((ObjectKind::CustomerRefund, refund.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 回款冲正审批任务的对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入冲正单号、金额和原因。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_receipt_reversal_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReceiptReversal);
        if ids.is_empty() {
            return Ok(());
        }
        let reversals = self
            .db
            .receipt_reversals()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        let created_by = self
            .load_created_by_from_audit(
                "receipt_reversal",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let receipt_ids = reversals
            .iter()
            .map(|reversal| reversal.original_customer_receipt_id.to_string())
            .collect::<Vec<_>>();
        let origins = self.customer_receipt_origins(&receipt_ids, executor).await?;
        for reversal in reversals {
            let origin = origins.get(&reversal.original_customer_receipt_id.to_string());
            let mut fact = ObjectFact::new(
                reversal.base.id.clone(),
                format!("回款冲正 {}", reversal.reversal_no),
                created_by.get(&reversal.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = origin.and_then(|item| item.counterparty.clone());
            fact.impact_summary = Some("不审批则回款冲正不能过账".to_string());
            let mut brief = amount_reason_brief(
                format_yuan(&reversal.amount),
                vec![
                    ("往来主体", origin.and_then(|item| item.counterparty.clone())),
                    ("原因", non_empty(&reversal.reason_text)),
                    ("发生日", Some(format_instant_date(reversal.occurred_at))),
                ],
                join_list_summary([
                    origin.and_then(|item| item.counterparty.clone()),
                    Some(format_yuan(&reversal.amount)),
                    non_empty(&reversal.reason_text),
                ]),
            );
            append_funds_origin(
                &mut brief,
                origin,
                reversal.evidence_attachment_id.is_some(),
                "通过后追加反向回款与反向核销，原回款事实保留并标记已冲正",
            );
            fact.brief_source = Some(brief);
            facts.insert((ObjectKind::ReceiptReversal, reversal.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 供应商付款事实简报。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入付款单号、供应商、金额和凭证。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_supplier_payment_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierPayment);
        if ids.is_empty() {
            return Ok(());
        }
        let payments = self
            .db
            .supplier_payments()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        let created_by = self
            .load_created_by_from_audit(
                "supplier_payment",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let supplier_ids = payments
            .iter()
            .map(|item| item.supplier_id.to_string())
            .collect::<Vec<_>>();
        let supplier_names = self.supplier_display_names(&supplier_ids, executor).await?;
        let allocation_lines = self.payment_allocation_lines(&payments, executor).await?;
        for payment in payments {
            let supplier = supplier_names.get(&payment.supplier_id.to_string()).cloned();
            let lines = allocation_lines
                .get(&payment.base.id)
                .cloned()
                .unwrap_or_default();
            let mut fact = ObjectFact::new(
                payment.base.id.clone(),
                format!("供应商付款 {}", payment.payment_no),
                created_by.get(&payment.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = supplier.clone();
            fact.impact_summary = Some("付款已登记并过账；纠错须走付款冲正或供应商退款".to_string());
            let mut sections = Vec::new();
            push_section(&mut sections, "供应商", supplier.as_deref(), false);
            push_section(
                &mut sections,
                "含税金额",
                Some(format_yuan(&payment.amount)).as_deref(),
                true,
            );
            push_section(
                &mut sections,
                "付款日",
                Some(format_instant_date(payment.paid_at)).as_deref(),
                false,
            );
            push_section(&mut sections, "凭证", payment.bank_reference.as_deref(), false);
            if !lines.is_empty() {
                push_section(
                    &mut sections,
                    "核销事实",
                    Some(format!("{} 笔", lines.len())).as_deref(),
                    false,
                );
            }
            let more_count = lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
            let lines = lines.into_iter().take(BRIEF_LINE_LIMIT).collect();
            fact.brief_source = Some(ObjectBriefSource {
                customer: None,
                amount_label: Some(format_yuan(&payment.amount)),
                extra_sections: sections,
                list_summary: join_list_summary([
                    supplier.clone(),
                    Some(format_yuan(&payment.amount)),
                    payment.bank_reference.clone().and_then(|text| non_empty(&text)),
                ]),
                lines,
                more_count,
                submitter_name: None,
            });
            facts.insert((ObjectKind::SupplierPayment, payment.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 供应商退款审批任务的对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入退款单号、金额和原因。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_supplier_refund_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierRefund);
        if ids.is_empty() {
            return Ok(());
        }
        let refunds = self
            .db
            .supplier_refunds()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        let created_by = self
            .load_created_by_from_audit(
                "supplier_refund",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let supplier_ids = refunds
            .iter()
            .map(|item| item.supplier_id.to_string())
            .collect::<Vec<_>>();
        let supplier_names = self.supplier_display_names(&supplier_ids, executor).await?;
        let payment_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_payment_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let payment_origins = self.supplier_payment_origins(&payment_ids, executor).await?;
        let entry_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_payable_entry_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let entry_origins = self.payable_entry_origins(&entry_ids, executor).await?;
        for refund in refunds {
            let supplier = supplier_names.get(&refund.supplier_id.to_string()).cloned();
            let origin = refund
                .original_payment_id
                .as_ref()
                .and_then(|id| payment_origins.get(&id.to_string()))
                .or_else(|| {
                    refund
                        .original_payable_entry_id
                        .as_ref()
                        .and_then(|id| entry_origins.get(&id.to_string()))
                });
            let mut fact = ObjectFact::new(
                refund.base.id.clone(),
                format!("供应商退款 {}", refund.refund_no),
                created_by.get(&refund.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = supplier.clone();
            fact.impact_summary = Some("不审批则供应商退款不能过账".to_string());
            let mut brief = amount_reason_brief(
                format_yuan(&refund.amount),
                vec![
                    ("供应商", supplier.clone()),
                    ("原因", non_empty(&refund.reason_text)),
                    ("发生日", Some(format_instant_date(refund.occurred_at))),
                ],
                join_list_summary([
                    supplier,
                    Some(format_yuan(&refund.amount)),
                    non_empty(&refund.reason_text),
                ]),
            );
            append_funds_origin(
                &mut brief,
                origin,
                refund.evidence_attachment_id.is_some(),
                "通过后追加应付冲减；已付款部分追加反向付款分配，原付款或应付事实保留",
            );
            fact.brief_source = Some(brief);
            facts.insert((ObjectKind::SupplierRefund, refund.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 付款冲正审批任务的对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入冲正单号、金额和原因。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_payment_reversal_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PaymentReversal);
        if ids.is_empty() {
            return Ok(());
        }
        let reversals = self
            .db
            .payment_reversals()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        let created_by = self
            .load_created_by_from_audit(
                "payment_reversal",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        let payment_ids = reversals
            .iter()
            .map(|reversal| reversal.original_supplier_payment_id.to_string())
            .collect::<Vec<_>>();
        let origins = self.supplier_payment_origins(&payment_ids, executor).await?;
        for reversal in reversals {
            let origin = origins.get(&reversal.original_supplier_payment_id.to_string());
            let mut fact = ObjectFact::new(
                reversal.base.id.clone(),
                format!("付款冲正 {}", reversal.reversal_no),
                created_by.get(&reversal.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = origin.and_then(|item| item.counterparty.clone());
            fact.impact_summary = Some("不审批则付款冲正不能过账".to_string());
            let mut brief = amount_reason_brief(
                format_yuan(&reversal.amount),
                vec![
                    ("供应商", origin.and_then(|item| item.counterparty.clone())),
                    ("原因", non_empty(&reversal.reason_text)),
                    ("发生日", Some(format_instant_date(reversal.occurred_at))),
                ],
                join_list_summary([
                    origin.and_then(|item| item.counterparty.clone()),
                    Some(format_yuan(&reversal.amount)),
                    non_empty(&reversal.reason_text),
                ]),
            );
            append_funds_origin(
                &mut brief,
                origin,
                reversal.evidence_attachment_id.is_some(),
                "通过后追加反向付款与反向核销，原付款事实保留并标记已冲正",
            );
            fact.brief_source = Some(brief);
            facts.insert((ObjectKind::PaymentReversal, reversal.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 应收子账票款复核与销项开票任务共用的对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入子账号、销售单号和开放余额。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_receivable_account_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReceivableAccount);
        if ids.is_empty() {
            return Ok(());
        }
        let accounts = self
            .db
            .receivable_accounts()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        if accounts.is_empty() {
            return Ok(());
        }
        let sales_order_ids = accounts
            .iter()
            .map(|item| item.sales_order_id.to_string())
            .collect::<Vec<_>>();
        let sales_nos = self.sales_order_numbers(&sales_order_ids, executor).await?;
        let party_ids = accounts
            .iter()
            .map(|item| item.counterparty_party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        let revision_briefs = self
            .receivable_account_revision_briefs(&accounts, executor)
            .await?;
        let tax_profile_nos = self
            .current_tax_profile_nos(
                &accounts
                    .iter()
                    .map(|account| account.counterparty_party_id.clone())
                    .collect::<Vec<_>>(),
                executor,
            )
            .await?;
        let due_dates = self.receivable_account_due_dates(&accounts, executor).await?;
        for account in accounts {
            let sales_no = sales_nos.get(&account.sales_order_id.to_string()).cloned();
            let counterparty = party_names
                .get(&account.counterparty_party_id.to_string())
                .cloned();
            let revision_id = account.source_sales_order_revision_id.to_string();
            let invoice_requirement = revision_briefs.invoice_requirements.get(&revision_id);
            let voucher = revision_briefs.vouchers.get(&revision_id);
            let tax_profile_label = invoice_tax_profile_label(
                tax_profile_nos
                    .get(&account.counterparty_party_id.to_string())
                    .map(String::as_str),
            );
            let due_date = due_dates.get(&account.base.id).map(ToString::to_string);
            let mut fact = ObjectFact::new(
                account.sales_order_id.to_string(),
                format!("应收子账 {}", account.account_seq),
                account.stable.created_by,
            );
            fact.counterparty_label = counterparty.clone();
            fact.impact_summary = Some(if voucher.is_some() {
                "不复核则卡券票款、开票与兑付前置事实不能确认".to_string()
            } else {
                "不复核则票款与开票事实不能确认".to_string()
            });
            let mut sections = Vec::new();
            push_section(&mut sections, "往来主体", counterparty.as_deref(), false);
            push_section(&mut sections, "销售单", sales_no.as_deref(), false);
            push_section(
                &mut sections,
                "开票类型",
                invoice_requirement.map(|item| item.invoice_type.as_str()),
                false,
            );
            push_section(
                &mut sections,
                "税点",
                invoice_requirement.map(|item| item.tax_point.as_str()),
                false,
            );
            push_section(
                &mut sections,
                "开票抬头资料",
                Some(tax_profile_label.as_str()),
                false,
            );
            push_section(&mut sections, "应收最早到期日", due_date.as_deref(), false);
            if let Some(voucher) = voucher {
                push_section(
                    &mut sections,
                    "卡券有效期",
                    voucher.expiry_label.as_deref(),
                    false,
                );
                push_section(&mut sections, "面值结构", voucher.face_summary.as_deref(), true);
                let total_count = (voucher.total_count > 0).then(|| format!("{} 张", voucher.total_count));
                push_section(&mut sections, "卡券张数", total_count.as_deref(), true);
                push_section(
                    &mut sections,
                    "票款金额",
                    Some(format_yuan(&account.gross_total)).as_deref(),
                    true,
                );
                push_section(
                    &mut sections,
                    "已到账",
                    Some(format_yuan(&account.settled_total)).as_deref(),
                    true,
                );
                push_section(
                    &mut sections,
                    "待到账/核销",
                    Some(format_yuan(&account.open_total)).as_deref(),
                    true,
                );
                push_section(
                    &mut sections,
                    "已开票",
                    Some(format_yuan(&account.invoiced_total)).as_deref(),
                    true,
                );
                push_section(
                    &mut sections,
                    "复核状态",
                    Some(account.review_status.label()),
                    false,
                );
                push_section(
                    &mut sections,
                    "复核证据",
                    account.review_evidence_reference.as_deref(),
                    false,
                );
            } else {
                push_section(
                    &mut sections,
                    "开放余额",
                    Some(format_yuan(&account.open_total)).as_deref(),
                    true,
                );
                push_section(
                    &mut sections,
                    "含税总额",
                    Some(format_yuan(&account.gross_total)).as_deref(),
                    true,
                );
            }
            push_section(
                &mut sections,
                "待开票金额",
                Some(format_yuan(&account.open_invoiceable_total)).as_deref(),
                true,
            );
            let card_summary = voucher
                .filter(|item| item.total_count > 0)
                .map(|item| format!("卡券 {} 张", item.total_count));
            fact.brief_source = Some(ObjectBriefSource {
                customer: counterparty.clone(),
                amount_label: Some(format_yuan(&account.open_total)),
                extra_sections: sections,
                list_summary: join_list_summary([
                    counterparty,
                    sales_no.map(|no| format!("销售单 {no}")),
                    invoice_requirement.map(|item| format!("开票 {}", item.invoice_type)),
                    due_date.map(|date| format!("应收到期 {date}")),
                    card_summary,
                    Some(format!("开放 {}", format_yuan(&account.open_total))),
                    Some(format!("待开票 {}", format_yuan(&account.open_invoiceable_total))),
                ]),
                lines: voucher.map(|item| item.lines.clone()).unwrap_or_default(),
                more_count: voucher.map(|item| item.more_count).unwrap_or_default(),
                submitter_name: None,
            });
            facts.insert((ObjectKind::ReceivableAccount, account.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 付款执行任务的应付子账对象事实。
    ///
    /// # 参数
    /// * `keys` - 本批任务引用的对象键
    /// * `facts` - 输出的对象事实表
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 成功时写入供应商、采购单、计划付款日与开放金额摘要。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    pub(super) async fn load_payable_account_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PayableAccount);
        if ids.is_empty() {
            return Ok(());
        }
        let accounts = self
            .db
            .payable_accounts()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?;
        let supplier_names = self.payable_supplier_names(&accounts, executor).await?;
        let purchase_nos = self.payable_purchase_numbers(&accounts, executor).await?;
        let due_dates = self.payable_due_dates(&accounts, executor).await?;
        for account in accounts {
            let supplier = supplier_names.get(&account.supplier_id.to_string()).cloned();
            let purchase_no = purchase_nos.get(&account.source_document_id).cloned();
            let due_date = due_dates.get(&account.base.id).map(ToString::to_string);
            let id = account.base.id.clone();
            facts.insert(
                (ObjectKind::PayableAccount, id),
                payable_account_fact(account, supplier, purchase_no, due_date),
            );
        }
        Ok(())
    }

    /// 批量读取应付供应商展示名。
    async fn payable_supplier_names(
        &self,
        accounts: &[PayableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let ids = accounts
            .iter()
            .map(|account| account.supplier_id.to_string())
            .collect::<Vec<_>>();
        self.supplier_display_names(&ids, executor).await
    }

    /// 批量读取应付来源采购单号。
    async fn payable_purchase_numbers(
        &self,
        accounts: &[PayableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let ids = accounts
            .iter()
            .map(|account| account.source_document_id.clone())
            .collect::<Vec<_>>();
        Ok(self
            .db
            .purchase_orders()
            .list_work_item_brief_entities_by_ids(&ids, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.purchase_no))
            .collect())
    }

    /// 批量汇总每个应付子账最早分录到期日。
    async fn payable_due_dates(
        &self,
        accounts: &[PayableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, entities::common::time::BusinessDate>> {
        let ids = accounts
            .iter()
            .map(|account| PayableAccountId::new(account.base.id.clone()))
            .collect::<Vec<_>>();
        let mut due_dates = HashMap::new();
        for entry in self
            .db
            .payable_entries()
            .find_entries_by_accounts(&ids, executor)
            .await?
        {
            due_dates
                .entry(entry.payable_account_id.to_string())
                .and_modify(|due: &mut entities::common::time::BusinessDate| {
                    *due = (*due).min(entry.due_date)
                })
                .or_insert(entry.due_date);
        }
        Ok(due_dates)
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
    async fn party_legal_names(
        &self,
        party_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if party_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let parties = self
            .db
            .parties()
            .list_work_item_brief_entities_by_ids(party_ids, executor)
            .await?;
        self.legal_names_for_parties(&parties, executor).await
    }

    /// 读取本批主体当前修订的法定名称。
    ///
    /// # 参数
    /// * `parties` - 本批主体
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回主体 ID 到法定名称。
    ///
    /// # 错误
    /// 仓储查询失败时返回错误。
    async fn legal_names_for_parties(
        &self,
        parties: &[Party],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        let revision_ids = parties
            .iter()
            .filter_map(|party| party.stable.current_revision_id.clone())
            .collect::<Vec<_>>();
        if revision_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let names_by_revision = self
            .db
            .party_revisions()
            .list_work_item_brief_entities_by_ids(&revision_ids, executor)
            .await?
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision.legal_name))
            .collect::<HashMap<_, _>>();
        Ok(parties
            .iter()
            .filter_map(|party| {
                let revision_id = party.stable.current_revision_id.as_ref()?;
                let name = names_by_revision.get(revision_id).cloned()?;
                non_empty(&name).map(|name| (party.base.id.clone(), name))
            })
            .collect())
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
    async fn customer_display_names(
        &self,
        customer_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if customer_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let customers = self
            .db
            .customer_accounts()
            .list_work_item_brief_entities_by_ids(customer_ids, executor)
            .await?;
        let party_ids = customers
            .iter()
            .map(|item| item.party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        Ok(customers
            .into_iter()
            .map(|customer| {
                let name = party_names
                    .get(&customer.party_id.to_string())
                    .cloned()
                    .unwrap_or(customer.customer_no);
                (customer.base.id, name)
            })
            .collect())
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
    pub(super) async fn supplier_display_names(
        &self,
        supplier_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if supplier_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let suppliers = self
            .db
            .supplier_accounts()
            .list_work_item_brief_entities_by_ids(supplier_ids, executor)
            .await?;
        let party_ids = suppliers
            .iter()
            .map(|item| item.party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        Ok(suppliers
            .into_iter()
            .map(|supplier| {
                let name = party_names
                    .get(&supplier.party_id.to_string())
                    .cloned()
                    .unwrap_or(supplier.supplier_no);
                (supplier.base.id, name)
            })
            .collect())
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
    async fn sales_order_numbers(
        &self,
        sales_order_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if sales_order_ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(self
            .db
            .sales_orders()
            .list_work_item_brief_entities_by_ids(sales_order_ids, executor)
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
    async fn receivable_account_revision_briefs(
        &self,
        accounts: &[ReceivableAccount],
        executor: &mut dyn Executor,
    ) -> Result<ReceivableRevisionBriefs> {
        let revision_ids = accounts
            .iter()
            .map(|account| account.source_sales_order_revision_id.clone())
            .collect::<Vec<_>>();
        let revisions = self
            .db
            .sales_order_revisions()
            .list_work_item_brief_entities_by_ids(
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
        let mut briefs = revisions
            .iter()
            .filter(|revision| {
                revision.voucher_category_sku_id.is_some() || revision.voucher_expiry_at.is_some()
            })
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
    async fn current_tax_profile_nos(
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
    async fn receivable_account_due_dates(
        &self,
        accounts: &[ReceivableAccount],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, BusinessDate>> {
        let account_ids = accounts
            .iter()
            .map(|account| ReceivableAccountId::new(account.base.id.clone()))
            .collect::<Vec<_>>();
        let entries = self
            .db
            .receivable_entries()
            .find_entries_by_accounts(&account_ids, executor)
            .await?;
        Ok(receivable_due_dates(&entries))
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
    async fn receipt_allocation_lines(
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
            .db
            .receivable_entries()
            .list_work_item_brief_entities_by_ids(&entry_ids, executor)
            .await?;
        let account_ids = entries
            .iter()
            .map(|entry| entry.receivable_account_id.to_string())
            .collect::<Vec<_>>();
        let accounts = self
            .db
            .receivable_accounts()
            .list_work_item_brief_entities_by_ids(&account_ids, executor)
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
    async fn customer_receipt_origins(
        &self,
        receipt_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, FundsOriginBrief>> {
        let receipts = self
            .db
            .customer_receipts()
            .list_work_item_brief_entities_by_ids(receipt_ids, executor)
            .await?;
        let party_ids = receipts
            .iter()
            .map(|receipt| receipt.counterparty_party_id.to_string())
            .collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        let lines = self.receipt_allocation_lines(&receipts, executor).await?;
        Ok(receipts
            .into_iter()
            .map(|receipt| {
                let id = receipt.base.id.clone();
                let allocation_summary = if receipt.pending_allocations.is_empty() {
                    "未记录核销分配".to_string()
                } else {
                    format!("已关联 {} 笔核销", receipt.pending_allocations.len())
                };
                (
                    id.clone(),
                    FundsOriginBrief {
                        counterparty: party_names
                            .get(&receipt.counterparty_party_id.to_string())
                            .cloned(),
                        original_document: Some(format!("回款单 {}", receipt.receipt_no)),
                        original_amount: Some(format_yuan(&receipt.amount)),
                        bank_reference: receipt.bank_reference,
                        allocation_summary: Some(allocation_summary),
                        lines: lines.get(&id).cloned().unwrap_or_default(),
                    },
                )
            })
            .collect())
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
    async fn receivable_entry_origins(
        &self,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, FundsOriginBrief>> {
        let entries = self
            .db
            .receivable_entries()
            .list_work_item_brief_entities_by_ids(entry_ids, executor)
            .await?;
        let account_ids = entries
            .iter()
            .map(|entry| entry.receivable_account_id.to_string())
            .collect::<Vec<_>>();
        let accounts = self
            .db
            .receivable_accounts()
            .list_work_item_brief_entities_by_ids(&account_ids, executor)
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
        Ok(entries
            .into_iter()
            .map(|entry| {
                let account = accounts.get(&entry.receivable_account_id.to_string());
                let sales_no = account.and_then(|account| sales_nos.get(&account.sales_order_id.to_string()));
                let counterparty = account.and_then(|account| {
                    party_names
                        .get(&account.counterparty_party_id.to_string())
                        .cloned()
                });
                let title = sales_no
                    .map(|no| format!("销售单 {no}"))
                    .unwrap_or_else(|| "销售单号待补全".to_string());
                let id = entry.base.id.clone();
                (
                    id,
                    FundsOriginBrief {
                        counterparty,
                        original_document: Some(format!("应收分录 · {title}")),
                        original_amount: Some(format_yuan(&entry.amount)),
                        bank_reference: None,
                        allocation_summary: Some(format!("原应收到期 {}", entry.due_date)),
                        lines: vec![BriefLine {
                            title,
                            quantity: Some(format_yuan(&entry.amount)),
                            due_label: Some(format!("{} 到期", entry.due_date)),
                        }],
                    },
                )
            })
            .collect())
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
    async fn supplier_payment_origins(
        &self,
        payment_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, FundsOriginBrief>> {
        let payments = self
            .db
            .supplier_payments()
            .list_work_item_brief_entities_by_ids(payment_ids, executor)
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
        Ok(payments
            .into_iter()
            .map(|payment| {
                let id = payment.base.id.clone();
                let allocation_count = lines.get(&id).map_or(0, Vec::len);
                let allocation_summary = if allocation_count == 0 {
                    "未记录核销分配".to_string()
                } else {
                    format!("已关联 {allocation_count} 笔核销事实")
                };
                (
                    id.clone(),
                    FundsOriginBrief {
                        counterparty: supplier_names.get(&payment.supplier_id.to_string()).cloned(),
                        original_document: Some(format!("付款单 {}", payment.payment_no)),
                        original_amount: Some(format_yuan(&payment.amount)),
                        bank_reference: payment.bank_reference,
                        allocation_summary: Some(allocation_summary),
                        lines: lines.get(&id).cloned().unwrap_or_default(),
                    },
                )
            })
            .collect())
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
    async fn payable_entry_origins(
        &self,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, FundsOriginBrief>> {
        let entries = self
            .db
            .payable_entries()
            .list_work_item_brief_entities_by_ids(entry_ids, executor)
            .await?;
        let account_ids = entries
            .iter()
            .map(|entry| entry.payable_account_id.to_string())
            .collect::<Vec<_>>();
        let accounts = self
            .db
            .payable_accounts()
            .list_work_item_brief_entities_by_ids(&account_ids, executor)
            .await?;
        let purchase_nos = self.payable_purchase_numbers(&accounts, executor).await?;
        let supplier_names = self.payable_supplier_names(&accounts, executor).await?;
        let accounts = accounts
            .into_iter()
            .map(|account| (account.base.id.clone(), account))
            .collect::<HashMap<_, _>>();
        Ok(entries
            .into_iter()
            .map(|entry| {
                let account = accounts.get(&entry.payable_account_id.to_string());
                let purchase_no = account.and_then(|account| purchase_nos.get(&account.source_document_id));
                let counterparty =
                    account.and_then(|account| supplier_names.get(&account.supplier_id.to_string()).cloned());
                let title = purchase_no
                    .map(|no| format!("采购单 {no}"))
                    .unwrap_or_else(|| "采购单号待补全".to_string());
                let id = entry.base.id.clone();
                (
                    id,
                    FundsOriginBrief {
                        counterparty,
                        original_document: Some(format!("应付分录 · {title}")),
                        original_amount: Some(format_yuan(&entry.amount)),
                        bank_reference: None,
                        allocation_summary: Some(format!("原应付到期 {}", entry.due_date)),
                        lines: vec![BriefLine {
                            title,
                            quantity: Some(format_yuan(&entry.amount)),
                            due_label: Some(format!("{} 到期", entry.due_date)),
                        }],
                    },
                )
            })
            .collect())
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
    async fn payment_allocation_lines(
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
            .db
            .payable_entries()
            .list_work_item_brief_entities_by_ids(&entry_ids, executor)
            .await?;
        let accounts = self
            .db
            .payable_accounts()
            .list_work_item_brief_entities_by_ids(
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

/// 组装付款执行任务的应付对象事实。
fn payable_account_fact(
    account: PayableAccount,
    supplier: Option<String>,
    purchase_no: Option<String>,
    due_date: Option<String>,
) -> ObjectFact {
    let brief_source = payable_brief_source(&account, supplier.clone(), purchase_no.clone(), due_date);
    let label = purchase_no
        .as_ref()
        .map(|no| format!("采购应付 {no}"))
        .unwrap_or_else(|| "采购应付".to_string());
    let mut fact = ObjectFact::new(account.source_document_id, label, account.stable.created_by);
    fact.counterparty_label = supplier;
    fact.impact_summary = Some(format!("未付金额 {}", format_yuan(&account.open_total)));
    fact.brief_source = Some(brief_source);
    fact
}

/// 组装付款执行任务的结构化应付简报。
fn payable_brief_source(
    account: &PayableAccount,
    supplier: Option<String>,
    purchase_no: Option<String>,
    due_date: Option<String>,
) -> ObjectBriefSource {
    let mut sections = Vec::new();
    push_section(&mut sections, "供应商", supplier.as_deref(), false);
    push_section(&mut sections, "采购单", purchase_no.as_deref(), false);
    push_section(
        &mut sections,
        "未付金额",
        Some(format_yuan(&account.open_total)).as_deref(),
        true,
    );
    push_section(
        &mut sections,
        "已付金额",
        Some(format_yuan(&account.settled_total)).as_deref(),
        true,
    );
    push_section(&mut sections, "计划付款日", due_date.as_deref(), false);
    ObjectBriefSource {
        customer: supplier.clone(),
        amount_label: Some(format_yuan(&account.open_total)),
        extra_sections: sections,
        list_summary: join_list_summary([
            supplier,
            purchase_no.map(|no| format!("采购单 {no}")),
            due_date.map(|date| format!("计划付款 {date}")),
            Some(format!("未付 {}", format_yuan(&account.open_total))),
        ]),
        lines: Vec::new(),
        more_count: 0,
        submitter_name: None,
    }
}

/// 组装回款单简报。
///
/// # 参数
/// * `receipt` - 回款单
/// * `counterparty` - 往来主体名称
/// * `lines` - 待核销销售单行
///
/// # 返回
/// 返回可上屏的对象简报源。
///
/// # 错误
/// 无。
fn receipt_brief_source(
    receipt: &CustomerReceipt,
    counterparty: Option<&str>,
    lines: Vec<BriefLine>,
) -> ObjectBriefSource {
    let more_count = lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
    let mut visible = lines;
    visible.truncate(BRIEF_LINE_LIMIT);
    let mut sections = Vec::new();
    push_section(&mut sections, "往来主体", counterparty, false);
    push_section(
        &mut sections,
        "含税金额",
        Some(format_yuan(&receipt.amount)).as_deref(),
        true,
    );
    push_section(
        &mut sections,
        "到账日",
        Some(format_instant_date(receipt.received_at)).as_deref(),
        false,
    );
    push_section(
        &mut sections,
        "银行流水",
        receipt.bank_reference.as_deref(),
        false,
    );
    if !visible.is_empty() {
        push_section(
            &mut sections,
            "待核销",
            Some(format!("{} 笔", visible.len() + more_count as usize)).as_deref(),
            false,
        );
    }
    let first_line = visible.first().map(|line| line.title.clone());
    ObjectBriefSource {
        customer: None,
        amount_label: Some(format_yuan(&receipt.amount)),
        extra_sections: sections,
        list_summary: join_list_summary([
            counterparty.map(str::to_string),
            Some(format_yuan(&receipt.amount)),
            receipt.bank_reference.clone().and_then(|text| non_empty(&text)),
            first_line,
        ]),
        lines: visible,
        more_count,
        submitter_name: None,
    }
}

/// 把冻结卡券版本行转成卡券票款专属简报行。
///
/// # 参数
/// * `line` - 销售版本公共行
/// * `voucher` - 与公共行一一对应的卡券版本行
///
/// # 返回
/// 返回品名、面值、张数、成交金额和配赠金额。
///
/// # 错误
/// 无。
fn voucher_account_line(line: &SalesOrderRevisionLine, voucher: &SalesOrderVoucherLineRevision) -> BriefLine {
    BriefLine {
        title: line_title(&line.item_name_snapshot, line.spec_snapshot.as_deref()),
        quantity: Some(format!(
            "面值 {} × {} 张",
            format_yuan(&voucher.face_value),
            voucher.card_count
        )),
        due_label: Some(format!(
            "成交 {} · 配赠 {}",
            format_yuan(&voucher.transaction_amount),
            format_yuan(&voucher.gift_amount)
        )),
    }
}

/// 格式化当前开票抬头税务资料状态。
///
/// # 参数
/// * `tax_no` - 当前业务日生效的税号
///
/// # 返回
/// 有有效税号时返回可核对文案；缺失时返回明确阻断提示。
///
/// # 错误
/// 无。
fn invoice_tax_profile_label(tax_no: Option<&str>) -> String {
    match tax_no.and_then(non_empty) {
        Some(tax_no) => format!("税务资料有效 · 税号 {tax_no}"),
        None => "未找到当前有效税务资料；登记开票前必须补齐".to_string(),
    }
}

/// 按应收子账汇总正向分录的最早到期日。
///
/// # 参数
/// * `entries` - 本批应收分录
///
/// # 返回
/// 返回子账 ID 到最早正向应收到期日；冲减分录不参与计算。
///
/// # 错误
/// 无。
fn receivable_due_dates(entries: &[ReceivableEntry]) -> HashMap<String, BusinessDate> {
    let mut due_dates = HashMap::new();
    for entry in entries
        .iter()
        .filter(|entry| entry.direction == ReceivableEntryDirection::Increase)
    {
        due_dates
            .entry(entry.receivable_account_id.to_string())
            .and_modify(|due: &mut BusinessDate| *due = (*due).min(entry.due_date))
            .or_insert(entry.due_date);
    }
    due_dates
}

/// 把待过账核销转成简报行。
///
/// # 参数
/// * `allocations` - 待过账核销
/// * `entries` - 分录 ID 到分录
/// * `accounts` - 子账 ID 到子账
/// * `sales_nos` - 销售单 ID 到单号
///
/// # 返回
/// 返回销售单号和核销金额。
///
/// # 错误
/// 无。
fn receipt_brief_lines(
    allocations: &[PendingReceiptAllocation],
    entries: &HashMap<String, ReceivableEntry>,
    accounts: &HashMap<String, ReceivableAccount>,
    sales_nos: &HashMap<String, String>,
) -> Vec<BriefLine> {
    allocations
        .iter()
        .map(|allocation| {
            let sales_no = entries
                .get(&allocation.receivable_entry_id.to_string())
                .and_then(|entry| accounts.get(&entry.receivable_account_id.to_string()))
                .and_then(|account| sales_nos.get(&account.sales_order_id.to_string()))
                .cloned();
            BriefLine {
                title: sales_no
                    .map(|no| format!("销售单 {no}"))
                    .unwrap_or_else(|| "应收分录".to_string()),
                quantity: Some(format_yuan(&allocation.allocated_amount)),
                due_label: None,
            }
        })
        .collect()
}

/// 把已过账付款核销事实转成采购单号、动作与金额。
///
/// # 参数
/// * `allocations` - 已过账付款核销事实
/// * `entries` - 应付分录 ID 到分录
/// * `accounts` - 应付子账 ID 到子账
/// * `purchase_nos` - 采购单 ID 到单号
///
/// # 返回
/// 返回采购单号和核销金额；业务单号缺失时使用明确占位。
///
/// # 错误
/// 无。
fn payment_brief_lines(
    allocations: &[PaymentAllocation],
    entries: &HashMap<String, PayableEntry>,
    accounts: &HashMap<String, PayableAccount>,
    purchase_nos: &HashMap<String, String>,
) -> Vec<BriefLine> {
    allocations
        .iter()
        .map(|allocation| {
            let purchase_no = entries
                .get(&allocation.payable_entry_id.to_string())
                .and_then(|entry| accounts.get(&entry.payable_account_id.to_string()))
                .and_then(|account| purchase_nos.get(&account.source_document_id))
                .cloned();
            BriefLine {
                title: purchase_no
                    .map(|no| format!("{}采购单 {no}", allocation.allocation_action.label()))
                    .unwrap_or_else(|| format!("{}采购单号待补全", allocation.allocation_action.label())),
                quantity: Some(format_yuan(&allocation.allocated_amount)),
                due_label: None,
            }
        })
        .collect()
}

/// 把原资金事实与核销影响追加到退款或冲正简报。
///
/// # 参数
/// * `brief` - 待补充的金额原因简报
/// * `origin` - 原回款、付款、应收或应付上下文
/// * `has_evidence` - 当前退款或冲正是否已上传凭证
/// * `impact` - 通过后的正式核销影响
///
/// # 返回
/// 无。原事实读取失败时显示业务单号待补全，不回退内部 ID。
///
/// # 错误
/// 无。
fn append_funds_origin(
    brief: &mut ObjectBriefSource,
    origin: Option<&FundsOriginBrief>,
    has_evidence: bool,
    impact: &str,
) {
    let original_document = origin
        .and_then(|item| item.original_document.as_deref())
        .unwrap_or("原始资金单据业务号待补全");
    push_section(
        &mut brief.extra_sections,
        "原始资金单据",
        Some(original_document),
        false,
    );
    push_section(
        &mut brief.extra_sections,
        "原单金额",
        origin.and_then(|item| item.original_amount.as_deref()),
        true,
    );
    push_section(
        &mut brief.extra_sections,
        "原银行流水/凭证",
        origin.and_then(|item| item.bank_reference.as_deref()),
        false,
    );
    push_section(
        &mut brief.extra_sections,
        "原核销事实",
        origin.and_then(|item| item.allocation_summary.as_deref()),
        false,
    );
    push_section(&mut brief.extra_sections, "核销影响", Some(impact), false);
    push_section(
        &mut brief.extra_sections,
        "本单凭证附件",
        Some(if has_evidence { "已上传" } else { "未上传" }),
        false,
    );
    let lines = origin.map(|item| item.lines.clone()).unwrap_or_default();
    brief.more_count = lines.len().saturating_sub(BRIEF_LINE_LIMIT) as u32;
    brief.lines = lines.into_iter().take(BRIEF_LINE_LIMIT).collect();
    brief.list_summary = join_list_summary([
        Some(brief.list_summary.clone()),
        Some(original_document.to_string()),
    ]);
}

/// 金额加原因类资金单据的共用简报。
///
/// # 参数
/// * `amount_label` - 含税金额展示
/// * `fields` - 额外键值
/// * `list_summary` - 列表一行摘要
///
/// # 返回
/// 返回可上屏的对象简报源。
///
/// # 错误
/// 无。
fn amount_reason_brief(
    amount_label: String,
    fields: Vec<(&str, Option<String>)>,
    list_summary: String,
) -> ObjectBriefSource {
    let mut sections = Vec::new();
    push_section(&mut sections, "含税金额", Some(amount_label.as_str()), true);
    for (label, value) in fields {
        push_section(&mut sections, label, value.as_deref(), false);
    }
    ObjectBriefSource {
        customer: None,
        amount_label: Some(amount_label),
        extra_sections: sections,
        list_summary,
        lines: Vec::new(),
        more_count: 0,
        submitter_name: None,
    }
}

#[cfg(test)]
mod tests {
    use entities::common::time::{BusinessDate, Instant};
    use entities::ids::{ReceivableAccountId, ReceivableEntryId};
    use entities::money::Amount;
    use entities::receivable::{EntryDirection, ReceivableEntryData, ReceivableEntryType};

    use super::*;
    use crate::work_item::brief::BriefLine;

    fn amount(value: &str) -> Amount {
        value.parse().expect("测试金额必须合法")
    }

    fn receivable_entry(
        id: &str,
        account_id: &str,
        direction: EntryDirection,
        due_date: BusinessDate,
    ) -> ReceivableEntry {
        let entry_type = match direction {
            EntryDirection::Increase => ReceivableEntryType::Original,
            EntryDirection::Decrease => ReceivableEntryType::Refund,
        };
        ReceivableEntry::new(
            ReceivableEntryId::new(id),
            ReceivableEntryData {
                receivable_account_id: ReceivableAccountId::new(account_id),
                entry_type,
                direction,
                amount: amount("100"),
                due_date,
                source_fact_type: "sales_order".to_string(),
                source_document_id: "SO-1".to_string(),
                source_revision_id: "SOR-1".to_string(),
                source_sequence: 1,
                posted_at: Instant::from_unix_secs(1),
            },
        )
        .expect("测试分录必须合法")
    }

    #[test]
    fn receipt_list_summary_joins_counterparty_amount_and_first_line() {
        let lines = [BriefLine {
            title: "销售单 SO-1".to_string(),
            quantity: Some("¥8,000".to_string()),
            due_label: None,
        }];
        let summary = join_list_summary([
            Some("华东纸业".into()),
            Some(format_yuan(&amount("8000"))),
            Some("流水-9".into()),
            lines.first().map(|line| line.title.clone()),
        ]);
        assert!(summary.contains("华东纸业"));
        assert!(summary.contains("¥8,000"));
        assert!(summary.contains("销售单 SO-1"));
    }

    #[test]
    fn amount_reason_brief_keeps_amount_and_reason() {
        let brief = amount_reason_brief(
            "¥500".to_string(),
            vec![("原因", Some("重复到账".into()))],
            "¥500 · 重复到账".to_string(),
        );
        assert_eq!(brief.amount_label.as_deref(), Some("¥500"));
        assert!(brief.extra_sections.iter().any(|section| section.label == "原因"));
        assert_eq!(brief.list_summary, "¥500 · 重复到账");
    }

    #[test]
    fn funds_origin_adds_original_document_allocation_impact_and_evidence() {
        let mut brief = amount_reason_brief(
            "¥500".to_string(),
            vec![("原因", Some("重复到账".into()))],
            "¥500 · 重复到账".to_string(),
        );
        let origin = FundsOriginBrief {
            original_document: Some("回款单 CR-1".to_string()),
            original_amount: Some("¥800".to_string()),
            bank_reference: Some("BANK-9".to_string()),
            allocation_summary: Some("已关联 1 笔核销".to_string()),
            lines: vec![BriefLine {
                title: "销售单 SO-1".to_string(),
                quantity: Some("¥800".to_string()),
                due_label: None,
            }],
            ..FundsOriginBrief::default()
        };

        append_funds_origin(&mut brief, Some(&origin), true, "通过后追加反向核销，原事实保留");

        assert!(brief
            .extra_sections
            .iter()
            .any(|section| { section.label == "原始资金单据" && section.value == "回款单 CR-1" }));
        assert!(brief
            .extra_sections
            .iter()
            .any(|section| section.label == "核销影响"));
        assert!(brief
            .extra_sections
            .iter()
            .any(|section| section.label == "本单凭证附件" && section.value == "已上传"));
        assert_eq!(brief.lines[0].title, "销售单 SO-1");
    }

    #[test]
    fn invoice_tax_profile_label_fails_closed_when_current_profile_is_missing() {
        assert_eq!(
            invoice_tax_profile_label(Some("91310000ABC")),
            "税务资料有效 · 税号 91310000ABC"
        );
        assert_eq!(
            invoice_tax_profile_label(None),
            "未找到当前有效税务资料；登记开票前必须补齐"
        );
    }

    #[test]
    fn receivable_due_dates_uses_earliest_increase_and_ignores_decrease() {
        let later = BusinessDate::from_ymd(2026, 9, 20).expect("测试日期必须合法");
        let earlier = BusinessDate::from_ymd(2026, 9, 10).expect("测试日期必须合法");
        let decrease = BusinessDate::from_ymd(2026, 8, 1).expect("测试日期必须合法");
        let entries = vec![
            receivable_entry("RE-1", "RA-1", EntryDirection::Increase, later),
            receivable_entry("RE-2", "RA-1", EntryDirection::Increase, earlier),
            receivable_entry("RE-3", "RA-1", EntryDirection::Decrease, decrease),
        ];

        let result = receivable_due_dates(&entries);

        assert_eq!(result.get("RA-1"), Some(&earlier));
    }
}
