//! 资金与票款单据审批任务的事项简报装载。
//!
//! 覆盖回款、退款、冲正、付款和应收子账。创建人从审计事实回填，往来名称从
//! 主体当前修订读取。正式通过/驳回仍走单据审批命令。

use std::collections::{HashMap, HashSet};

use database::{
    AccessControlExt, Executor, PartyExt, PayableExt, ReceivableExt, ReturnsExt, SalesOrderExt, SupplierExt,
};
use entities::party::Party;
use entities::receivable::{CustomerReceipt, PendingReceiptAllocation, ReceivableAccount, ReceivableEntry};
use mongodb::bson::doc;

use super::brief::{
    format_instant_date, join_list_summary, non_empty, push_section, BriefLine, ObjectBriefSource,
    BRIEF_LINE_LIMIT,
};
use super::presentation::format_yuan;
use super::{object_ids, ObjectFact, ObjectFactMap, ObjectKind, WorkItemService};
use crate::errors::Result;

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
        let audits = self
            .db
            .audit_logs()
            .find_many(
                doc! {
                    "resource_type": resource_type,
                    "resource_id": { "$in": ids.iter().cloned().collect::<Vec<_>>() },
                    "action": format!("{resource_type}.create"),
                    "success": true,
                },
                executor,
            )
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
            .find_many(doc! { "id": { "$in": ids.clone() } }, executor)
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
            .find_many(doc! { "id": { "$in": ids.clone() } }, executor)
            .await?;
        let created_by = self
            .load_created_by_from_audit(
                "customer_refund",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        for refund in refunds {
            let mut fact = ObjectFact::new(
                refund.base.id.clone(),
                format!("客户退款 {}", refund.refund_no),
                created_by.get(&refund.base.id).cloned().unwrap_or_default(),
            );
            fact.impact_summary = Some("不审批则客户退款不能过账".to_string());
            fact.brief_source = Some(amount_reason_brief(
                format_yuan(&refund.amount),
                vec![
                    ("原因", non_empty(&refund.reason_text)),
                    ("发生日", Some(format_instant_date(refund.occurred_at))),
                ],
                format!("{} · {}", format_yuan(&refund.amount), refund.reason_text.trim()),
            ));
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
            .find_many(doc! { "id": { "$in": ids.clone() } }, executor)
            .await?;
        let created_by = self
            .load_created_by_from_audit(
                "receipt_reversal",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        for reversal in reversals {
            let mut fact = ObjectFact::new(
                reversal.base.id.clone(),
                format!("回款冲正 {}", reversal.reversal_no),
                created_by.get(&reversal.base.id).cloned().unwrap_or_default(),
            );
            fact.impact_summary = Some("不审批则回款冲正不能过账".to_string());
            fact.brief_source = Some(amount_reason_brief(
                format_yuan(&reversal.amount),
                vec![
                    ("原因", non_empty(&reversal.reason_text)),
                    ("发生日", Some(format_instant_date(reversal.occurred_at))),
                ],
                format!(
                    "{} · {}",
                    format_yuan(&reversal.amount),
                    reversal.reason_text.trim()
                ),
            ));
            facts.insert((ObjectKind::ReceiptReversal, reversal.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 供应商付款审批任务的对象事实。
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
            .find_many(doc! { "id": { "$in": ids.clone() } }, executor)
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
        for payment in payments {
            let supplier = supplier_names.get(&payment.supplier_id.to_string()).cloned();
            let mut fact = ObjectFact::new(
                payment.base.id.clone(),
                format!("供应商付款 {}", payment.payment_no),
                created_by.get(&payment.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = supplier.clone();
            fact.impact_summary = Some("不审批则付款不能过账、不能核销应付".to_string());
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
            if !payment.pending_allocations.is_empty() {
                push_section(
                    &mut sections,
                    "待核销",
                    Some(format!("{} 笔", payment.pending_allocations.len())).as_deref(),
                    false,
                );
            }
            fact.brief_source = Some(ObjectBriefSource {
                customer: None,
                amount_label: Some(format_yuan(&payment.amount)),
                extra_sections: sections,
                list_summary: join_list_summary([
                    supplier.clone(),
                    Some(format_yuan(&payment.amount)),
                    payment.bank_reference.clone().and_then(|text| non_empty(&text)),
                ]),
                lines: Vec::new(),
                more_count: 0,
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
            .find_many(doc! { "id": { "$in": ids.clone() } }, executor)
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
        for refund in refunds {
            let supplier = supplier_names.get(&refund.supplier_id.to_string()).cloned();
            let mut fact = ObjectFact::new(
                refund.base.id.clone(),
                format!("供应商退款 {}", refund.refund_no),
                created_by.get(&refund.base.id).cloned().unwrap_or_default(),
            );
            fact.counterparty_label = supplier.clone();
            fact.impact_summary = Some("不审批则供应商退款不能过账".to_string());
            fact.brief_source = Some(amount_reason_brief(
                format_yuan(&refund.amount),
                vec![
                    ("供应商", supplier),
                    ("原因", non_empty(&refund.reason_text)),
                    ("发生日", Some(format_instant_date(refund.occurred_at))),
                ],
                format!("{} · {}", format_yuan(&refund.amount), refund.reason_text.trim()),
            ));
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
            .find_many(doc! { "id": { "$in": ids.clone() } }, executor)
            .await?;
        let created_by = self
            .load_created_by_from_audit(
                "payment_reversal",
                &ids.iter().cloned().collect::<HashSet<_>>(),
                executor,
            )
            .await?;
        for reversal in reversals {
            let mut fact = ObjectFact::new(
                reversal.base.id.clone(),
                format!("付款冲正 {}", reversal.reversal_no),
                created_by.get(&reversal.base.id).cloned().unwrap_or_default(),
            );
            fact.impact_summary = Some("不审批则付款冲正不能过账".to_string());
            fact.brief_source = Some(amount_reason_brief(
                format_yuan(&reversal.amount),
                vec![
                    ("原因", non_empty(&reversal.reason_text)),
                    ("发生日", Some(format_instant_date(reversal.occurred_at))),
                ],
                format!(
                    "{} · {}",
                    format_yuan(&reversal.amount),
                    reversal.reason_text.trim()
                ),
            ));
            facts.insert((ObjectKind::PaymentReversal, reversal.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 应收子账票款复核任务的对象事实。
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
            .find_many(doc! { "id": { "$in": ids.clone() } }, executor)
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
        for account in accounts {
            let sales_no = sales_nos.get(&account.sales_order_id.to_string()).cloned();
            let counterparty = party_names
                .get(&account.counterparty_party_id.to_string())
                .cloned();
            let mut fact = ObjectFact::new(
                account.sales_order_id.to_string(),
                format!("卡券应收子账 {}", account.account_seq),
                account.stable.created_by,
            );
            fact.counterparty_label = counterparty.clone();
            fact.impact_summary = Some("不复核则票款与开票事实不能确认".to_string());
            let mut sections = Vec::new();
            push_section(&mut sections, "往来主体", counterparty.as_deref(), false);
            push_section(&mut sections, "销售单", sales_no.as_deref(), false);
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
            fact.brief_source = Some(ObjectBriefSource {
                customer: counterparty.clone(),
                amount_label: Some(format_yuan(&account.open_total)),
                extra_sections: sections,
                list_summary: join_list_summary([
                    counterparty,
                    sales_no.map(|no| format!("销售单 {no}")),
                    Some(format!("开放 {}", format_yuan(&account.open_total))),
                ]),
                lines: Vec::new(),
                more_count: 0,
                submitter_name: None,
            });
            facts.insert((ObjectKind::ReceivableAccount, account.base.id.clone()), fact);
        }
        Ok(())
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
            .find_many(doc! { "id": { "$in": party_ids } }, executor)
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
            .find_many(doc! { "id": { "$in": revision_ids } }, executor)
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
    async fn supplier_display_names(
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
            .find_many(doc! { "id": { "$in": supplier_ids } }, executor)
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
            .find_many(doc! { "id": { "$in": sales_order_ids } }, executor)
            .await?
            .into_iter()
            .map(|order| (order.base.id, order.order_no))
            .collect())
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
            .find_many(doc! { "id": { "$in": entry_ids } }, executor)
            .await?;
        let account_ids = entries
            .iter()
            .map(|entry| entry.receivable_account_id.to_string())
            .collect::<Vec<_>>();
        let accounts = self
            .db
            .receivable_accounts()
            .find_many(doc! { "id": { "$in": account_ids } }, executor)
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
    use entities::money::Amount;

    use super::*;
    use crate::work_item::brief::BriefLine;

    fn amount(value: &str) -> Amount {
        value.parse().expect("测试金额必须合法")
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
}
