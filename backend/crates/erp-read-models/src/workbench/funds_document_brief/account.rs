//! 应收与应付子账任务简报。

use std::collections::HashSet;

use persistence_core::Executor;

use super::super::brief::{join_list_summary, push_section, ObjectBriefSource};
use super::super::presentation::format_yuan;
use super::super::WorkbenchReadService;
use super::super::{object_ids, ObjectKind, WorkbenchObjectFactMap as ObjectFactMap};
use super::mapping::{invoice_tax_profile_label, payable_account_fact, receivable_fact_display};
use crate::errors::Result;
use crate::workbench::authority::funds::mapping as authority_mapping;

impl<A: erp_workflow::WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 应收子账销项开票任务的对象事实。
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
    pub(crate) async fn load_receivable_account_facts(
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
            .funds_reader()
            .read_receivable_accounts(&ids, executor)
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
            let mut fact = receivable_fact_display(
                authority_mapping::receivable_account_fact(
                    &account,
                    counterparty.clone(),
                    revision_briefs
                        .command_voucher_revision_ids
                        .contains(&revision_id),
                ),
                voucher.is_some(),
            );
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
            fact.display.brief_source = Some(ObjectBriefSource {
                customer: counterparty.clone(),
                amount_label: None,
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
    pub(crate) async fn load_payable_account_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PayableAccount);
        if ids.is_empty() {
            return Ok(());
        }
        let accounts = self.funds_reader().read_payable_accounts(&ids, executor).await?;
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
}
