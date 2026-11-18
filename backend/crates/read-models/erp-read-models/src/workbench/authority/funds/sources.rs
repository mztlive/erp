//! 命令与富显示共用的资金原子来源；每次调用原样执行一次仓储读取。
use erp_finance::repository::{PayableExt, ReceivableExt};
use erp_returns::repository::ReturnsExt;
use persistence_core::Executor;

use super::super::WorkItemFactsReader;
use crate::errors::Result;
impl WorkItemFactsReader {
    // 工作项授权和简报需要应收子账时，保持传入 ID 的原顺序并沿用原执行器交给领域仓储；空批次仍按原仓储处理。
    //
    // 调用参数：
    // * `ids` - 应收子账 ID。
    //
    // 返回仓储读到的有效应收子账。
    pub(in crate::workbench) async fn read_receivable_accounts(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_finance::entity::receivable::ReceivableAccount>> {
        self.db.receivable_accounts().list_active_by_ids(ids, executor).await.map_err(Into::into)
    }
    // 工作项授权和简报需要应付账户时，保持传入 ID 的原顺序并沿用原执行器交给领域仓储；空批次仍按原仓储处理。
    //
    // 调用参数：
    // * `ids` - 应付账户 ID。
    //
    // 返回仓储读到的有效应付账户。
    pub(in crate::workbench) async fn read_payable_accounts(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_finance::entity::payable::PayableAccount>> {
        self.db.payable_accounts().list_active_by_ids(ids, executor).await.map_err(Into::into)
    }
    // 工作项授权和简报需要客户回款时，保持传入 ID 的原顺序并沿用原执行器交给领域仓储；空批次仍按原仓储处理。
    //
    // 调用参数：
    // * `ids` - 客户回款单 ID。
    //
    // 返回仓储读到的有效客户回款单。
    pub(in crate::workbench) async fn read_customer_receipts(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_finance::entity::receivable::CustomerReceipt>> {
        self.db.customer_receipts().list_active_by_ids(ids, executor).await.map_err(Into::into)
    }
    // 工作项授权和简报需要供应商付款时，保持传入 ID 的原顺序并沿用原执行器交给领域仓储；空批次仍按原仓储处理。
    //
    // 调用参数：
    // * `ids` - 供应商付款 ID。
    //
    // 返回仓储读到的有效供应商付款。
    pub(in crate::workbench) async fn read_supplier_payments(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_finance::entity::payable::SupplierPayment>> {
        self.db.supplier_payments().list_active_by_ids(ids, executor).await.map_err(Into::into)
    }
    // 工作项授权和简报需要客户退款时，保持传入 ID 的原顺序并沿用原执行器交给领域仓储；空批次仍按原仓储处理。
    //
    // 调用参数：
    // * `ids` - 客户退款 ID。
    //
    // 返回仓储读到的有效客户退款。
    pub(in crate::workbench) async fn read_customer_refunds(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_returns::entity::returns::CustomerRefund>> {
        self.db.customer_refunds().list_active_by_ids(ids, executor).await.map_err(Into::into)
    }
    // 工作项授权和简报需要供应商退款时，保持传入 ID 的原顺序并沿用原执行器交给领域仓储；空批次仍按原仓储处理。
    //
    // 调用参数：
    // * `ids` - 供应商退款 ID。
    //
    // 返回仓储读到的有效供应商退款。
    pub(in crate::workbench) async fn read_supplier_refunds(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_returns::entity::returns::SupplierRefund>> {
        self.db.supplier_refunds().list_active_by_ids(ids, executor).await.map_err(Into::into)
    }
    // 工作项授权和简报需要回款冲正时，保持传入 ID 的原顺序并沿用原执行器交给领域仓储；空批次仍按原仓储处理。
    //
    // 调用参数：
    // * `ids` - 回款冲正 ID。
    //
    // 返回仓储读到的有效回款冲正。
    pub(in crate::workbench) async fn read_receipt_reversals(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_returns::entity::returns::ReceiptReversal>> {
        self.db.receipt_reversals().list_active_by_ids(ids, executor).await.map_err(Into::into)
    }
    // 工作项授权和简报需要付款冲正时，保持传入 ID 的原顺序并沿用原执行器交给领域仓储；空批次仍按原仓储处理。
    //
    // 调用参数：
    // * `ids` - 付款冲正 ID。
    //
    // 返回仓储读到的有效付款冲正。
    pub(in crate::workbench) async fn read_payment_reversals(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_returns::entity::returns::PaymentReversal>> {
        self.db.payment_reversals().list_active_by_ids(ids, executor).await.map_err(Into::into)
    }
    // 工作项命令和简报装配需要应收分录时，按原输入顺序向领域仓储传入 ID，空输入仍由原仓储处理，复用调用方执行器。
    //
    // 调用参数：
    // * `ids` - 应收分录 ID。
    //
    // 返回仓储读到的有效应收分录。
    pub(in crate::workbench) async fn read_receivable_entries(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_finance::entity::receivable::ReceivableEntry>> {
        self.db.receivable_entries().list_active_by_ids(ids, executor).await.map_err(Into::into)
    }
    // 工作项授权和简报需要应付分录时，保持传入 ID 的原顺序并沿用原执行器交给领域仓储；空批次仍按原仓储处理。
    //
    // 调用参数：
    // * `ids` - 应付分录 ID。
    //
    // 返回仓储读到的有效应付分录。
    pub(in crate::workbench) async fn read_payable_entries(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_finance::entity::payable::PayableEntry>> {
        self.db.payable_entries().list_active_by_ids(ids, executor).await.map_err(Into::into)
    }
}
