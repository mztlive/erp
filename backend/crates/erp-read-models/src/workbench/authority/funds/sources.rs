//! 命令与富显示共用的资金原子来源；每次调用原样执行一次仓储读取。
use super::super::WorkItemFactsReader;
use crate::errors::Result;
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_returns::repository::ReturnsExt;
use persistence_core::Executor;
impl WorkItemFactsReader {
    /// 按原输入顺序读取 receivable_accounts；空输入仍由原仓储处理，同一 Executor 透传。
    pub(in crate::workbench) async fn read_receivable_accounts(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_finance::entity::receivable::ReceivableAccount>> {
        self.db
            .receivable_accounts()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(Into::into)
    }
    /// 按原输入顺序读取 payable_accounts；空输入仍由原仓储处理，同一 Executor 透传。
    pub(in crate::workbench) async fn read_payable_accounts(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_finance::entity::payable::PayableAccount>> {
        self.db
            .payable_accounts()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(Into::into)
    }
    /// 按原输入顺序读取 customer_receipts；空输入仍由原仓储处理，同一 Executor 透传。
    pub(in crate::workbench) async fn read_customer_receipts(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_finance::entity::receivable::CustomerReceipt>> {
        self.db
            .customer_receipts()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(Into::into)
    }
    /// 按原输入顺序读取 supplier_payments；空输入仍由原仓储处理，同一 Executor 透传。
    pub(in crate::workbench) async fn read_supplier_payments(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_finance::entity::payable::SupplierPayment>> {
        self.db
            .supplier_payments()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(Into::into)
    }
    /// 按原输入顺序读取 customer_refunds；空输入仍由原仓储处理，同一 Executor 透传。
    pub(in crate::workbench) async fn read_customer_refunds(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_returns::entity::returns::CustomerRefund>> {
        self.db
            .customer_refunds()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(Into::into)
    }
    /// 按原输入顺序读取 supplier_refunds；空输入仍由原仓储处理，同一 Executor 透传。
    pub(in crate::workbench) async fn read_supplier_refunds(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_returns::entity::returns::SupplierRefund>> {
        self.db
            .supplier_refunds()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(Into::into)
    }
    /// 按原输入顺序读取 receipt_reversals；空输入仍由原仓储处理，同一 Executor 透传。
    pub(in crate::workbench) async fn read_receipt_reversals(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_returns::entity::returns::ReceiptReversal>> {
        self.db
            .receipt_reversals()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(Into::into)
    }
    /// 按原输入顺序读取 payment_reversals；空输入仍由原仓储处理，同一 Executor 透传。
    pub(in crate::workbench) async fn read_payment_reversals(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_returns::entity::returns::PaymentReversal>> {
        self.db
            .payment_reversals()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(Into::into)
    }
    /// 按原输入顺序读取 receivable_entries；空输入仍由原仓储处理，同一 Executor 透传。
    pub(in crate::workbench) async fn read_receivable_entries(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_finance::entity::receivable::ReceivableEntry>> {
        self.db
            .receivable_entries()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(Into::into)
    }
    /// 按原输入顺序读取 payable_entries；空输入仍由原仓储处理，同一 Executor 透传。
    pub(in crate::workbench) async fn read_payable_entries(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_finance::entity::payable::PayableEntry>> {
        self.db
            .payable_entries()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(Into::into)
    }
}
