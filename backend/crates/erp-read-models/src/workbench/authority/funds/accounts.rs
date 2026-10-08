//! 应收与应付账户的对象事实。

use std::collections::HashSet;

use erp_workflow::ports::{ObjectFactMap, ObjectKind};
use persistence_core::Executor;

use super::super::object_ids;
use super::mapping;
use crate::errors::Result;

impl super::super::WorkItemFactsReader {
    /// 装载应收子账的身份、往来方与影响。
    ///
    /// # 参数
    /// * `keys` - 本批对象键；没有应收子账键时不读取。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 成功时写入读到的账户。往来方取主体法定名称，影响按来源修订是否卡券决定。账户为空时不继续查名称。
    ///
    /// # 错误
    /// 账户、主体名称或来源修订读取失败时返回错误。
    pub(in crate::workbench) async fn load_receivable_account_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReceivableAccount);
        if ids.is_empty() {
            return Ok(());
        }
        let accounts = self.read_receivable_accounts(&ids, executor).await?;
        if accounts.is_empty() {
            return Ok(());
        }
        let party_ids =
            accounts.iter().map(|item| item.counterparty_party_id.to_string()).collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        let voucher_revisions = self.receivable_voucher_revision_ids(&accounts, executor).await?;
        for account in accounts {
            let fact = mapping::receivable_account_fact(
                &account,
                party_names.get(&account.counterparty_party_id.to_string()).cloned(),
                voucher_revisions.contains(&account.source_sales_order_revision_id.to_string()),
            );
            facts.insert((ObjectKind::ReceivableAccount, account.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 装载应付账户的身份、往来方与未付影响。
    ///
    /// # 参数
    /// * `keys` - 本批对象键；没有应付账户键时不读取。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 成功时写入读到的账户。标题优先用来源采购单号，往来方取供应商显示名，影响为未付金额。
    ///
    /// # 错误
    /// 账户、供应商名称或采购单号读取失败时返回错误。
    pub(in crate::workbench) async fn load_payable_account_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PayableAccount);
        if ids.is_empty() {
            return Ok(());
        }
        let accounts = self.read_payable_accounts(&ids, executor).await?;
        let supplier_names = self.payable_supplier_names(&accounts, executor).await?;
        let purchase_nos = self.payable_purchase_numbers(&accounts, executor).await?;
        for account in accounts {
            let fact = mapping::payable_account_fact(
                &account,
                supplier_names.get(&account.supplier_id.to_string()).cloned(),
                purchase_nos.get(&account.source_document_id).cloned(),
            );
            facts.insert((ObjectKind::PayableAccount, account.base.id.clone()), fact);
        }
        Ok(())
    }
}
