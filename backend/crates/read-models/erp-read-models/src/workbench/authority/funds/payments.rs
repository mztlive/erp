//! 供应商付款、退款与付款冲正的对象事实。

use std::collections::HashSet;

use erp_workflow::ports::{ObjectFactMap, ObjectKind};
use persistence_core::Executor;

use super::super::object_ids;
use super::mapping;
use crate::errors::Result;

impl super::super::WorkItemFactsReader {
    /// 装载供应商付款的身份、创建人、往来方与影响。
    ///
    /// # 参数
    /// * `keys` - 本批对象键；没有付款键时不读取。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 成功时写入读到的付款，往来方取供应商显示名。
    ///
    /// # 错误
    /// 付款或供应商名称读取失败时返回错误。
    pub(in crate::workbench) async fn load_supplier_payment_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierPayment);
        if ids.is_empty() {
            return Ok(());
        }
        let payments = self.read_supplier_payments(&ids, executor).await?;
        let supplier_ids = payments.iter().map(|item| item.supplier_id.to_string()).collect::<Vec<_>>();
        let supplier_names = self.supplier_display_names(&supplier_ids, executor).await?;
        for payment in payments {
            let fact = mapping::supplier_payment_fact(
                &payment,
                supplier_names.get(&payment.supplier_id.to_string()).cloned(),
            );
            facts.insert((ObjectKind::SupplierPayment, payment.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 装载供应商退款的身份、创建人、往来方与影响。
    ///
    /// # 参数
    /// * `keys` - 本批对象键；没有退款键时不读取。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 成功时写入读到的退款。往来方优先供应商显示名，否则回退原付款或应付分录上的名称。
    ///
    /// # 错误
    /// 退款、供应商名称或来源名称读取失败时返回错误。
    pub(in crate::workbench) async fn load_supplier_refund_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::SupplierRefund);
        if ids.is_empty() {
            return Ok(());
        }
        let refunds = self.read_supplier_refunds(&ids, executor).await?;
        let supplier_ids = refunds.iter().map(|item| item.supplier_id.to_string()).collect::<Vec<_>>();
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
            let fact = mapping::supplier_refund_fact(
                &refund,
                supplier_names.get(&refund.supplier_id.to_string()).cloned(),
                &payment_origins,
                &entry_origins,
            );
            facts.insert((ObjectKind::SupplierRefund, refund.base.id.clone()), fact);
        }
        Ok(())
    }

    /// 装载付款冲正的身份、创建人、往来方与影响。
    ///
    /// # 参数
    /// * `keys` - 本批对象键；没有冲正键时不读取。
    /// * `facts` - 输出事实表。
    /// * `executor` - 数据访问执行器。
    ///
    /// # 返回
    /// 成功时写入读到的冲正，往来方只取原付款上已有的供应商名称。
    ///
    /// # 错误
    /// 冲正或原付款名称读取失败时返回错误。
    pub(in crate::workbench) async fn load_payment_reversal_facts(
        &self,
        keys: &HashSet<(ObjectKind, String)>,
        facts: &mut ObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let ids = object_ids(keys, ObjectKind::PaymentReversal);
        if ids.is_empty() {
            return Ok(());
        }
        let reversals = self.read_payment_reversals(&ids, executor).await?;
        let payment_ids = reversals
            .iter()
            .map(|reversal| reversal.original_supplier_payment_id.to_string())
            .collect::<Vec<_>>();
        let origins = self.supplier_payment_origins(&payment_ids, executor).await?;
        for reversal in reversals {
            let fact = mapping::payment_reversal_fact(&reversal, &origins);
            facts.insert((ObjectKind::PaymentReversal, reversal.base.id.clone()), fact);
        }
        Ok(())
    }
}
