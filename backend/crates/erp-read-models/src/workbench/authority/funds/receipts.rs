//! 客户回款、退款与回款冲正的对象事实。

use std::collections::HashSet;

use erp_workflow::ports::{ObjectFactMap, ObjectKind};
use persistence_core::Executor;

use super::super::object_ids;
use super::mapping;
use crate::errors::Result;

impl super::super::WorkItemFactsReader {
    /// 装载客户回款的身份、创建人、往来方与影响；同一入口也装载开票申请。
    ///
    /// # 参数
    /// * `keys` - 本批对象键。有开票申请键时先写入申请事实；没有回款键时就此返回。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 成功时写入读到的开票申请与回款。回款往来方取主体法定名称；回款为空时不继续查名称。
    ///
    /// # 错误
    /// 开票申请、回款或主体名称读取失败时返回错误。
    pub(in crate::workbench) async fn load_customer_receipt_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        use erp_finance::repository::ReceivableExt;
        let request_ids = object_ids(keys, ObjectKind::SalesInvoiceRequest);
        if !request_ids.is_empty() {
            for request in self.db.sales_invoice_requests().list_active_by_ids(&request_ids, executor).await?
            {
                facts.insert(
                    (ObjectKind::SalesInvoiceRequest, request.base.id.clone()),
                    mapping::invoice_request_fact(&request),
                );
            }
        }
        let ids = object_ids(keys, ObjectKind::CustomerReceipt);
        if ids.is_empty() {
            return Ok(());
        }
        let receipts = self.read_customer_receipts(&ids, executor).await?;
        if receipts.is_empty() {
            return Ok(());
        }
        let party_ids =
            receipts.iter().map(|item| item.counterparty_party_id.to_string()).collect::<Vec<_>>();
        let party_names = self.party_legal_names(&party_ids, executor).await?;
        for receipt in receipts {
            let fact = mapping::customer_receipt_fact(
                &receipt,
                party_names.get(&receipt.counterparty_party_id.to_string()).cloned(),
            );
            facts.insert((ObjectKind::CustomerReceipt, receipt.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 装载客户退款的身份、创建人、往来方与影响。
    ///
    /// # 参数
    /// * `keys` - 本批对象键；没有退款键时不读取。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 成功时写入读到的退款。往来方优先客户显示名，否则回退原回款或应收分录上的名称。
    ///
    /// # 错误
    /// 退款、客户名称或来源名称读取失败时返回错误。
    pub(in crate::workbench) async fn load_customer_refund_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::CustomerRefund);
        if ids.is_empty() {
            return Ok(());
        }
        let refunds = self.read_customer_refunds(&ids, executor).await?;
        let customer_ids = refunds.iter().map(|refund| refund.customer_id.to_string()).collect::<Vec<_>>();
        let customer_names = self.customer_display_names(&customer_ids, executor).await?;
        let receipt_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_receipt_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let receipt_origins = self.customer_receipt_origins(&receipt_ids, executor).await?;
        let entry_ids = refunds
            .iter()
            .filter_map(|refund| refund.original_receivable_entry_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        let entry_origins = self.receivable_entry_origins(&entry_ids, executor).await?;
        for refund in refunds {
            let fact = mapping::customer_refund_fact(
                &refund,
                customer_names.get(&refund.customer_id.to_string()).cloned(),
                &receipt_origins,
                &entry_origins,
            );
            facts.insert((ObjectKind::CustomerRefund, refund.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 装载回款冲正的身份、创建人、往来方与影响。
    ///
    /// # 参数
    /// * `keys` - 本批对象键；没有冲正键时不读取。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 成功时写入读到的冲正，往来方只取原回款上已有的名称。
    ///
    /// # 错误
    /// 冲正或原回款名称读取失败时返回错误。
    pub(in crate::workbench) async fn load_receipt_reversal_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::ReceiptReversal);
        if ids.is_empty() {
            return Ok(());
        }
        let reversals = self.read_receipt_reversals(&ids, executor).await?;
        let receipt_ids = reversals
            .iter()
            .map(|reversal| reversal.original_customer_receipt_id.to_string())
            .collect::<Vec<_>>();
        let origins = self.customer_receipt_origins(&receipt_ids, executor).await?;
        for reversal in reversals {
            let fact = mapping::receipt_reversal_fact(&reversal, &origins);
            facts.insert((ObjectKind::ReceiptReversal, reversal.base.id.clone()), fact);
        }
        Ok(())
    }
}
