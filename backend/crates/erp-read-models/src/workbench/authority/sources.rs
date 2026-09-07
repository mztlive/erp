//! 命令和显示复用的原子来源读取；调用者决定空输入护栏与执行顺序。
use super::WorkItemFactsReader;
use crate::errors::Result;
use erp_integration::repository::IntegrationOpsExt;
use erp_inventory::InventoryExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use erp_supply::repository::SupplierSettlementExt;
use persistence_core::Executor;
impl WorkItemFactsReader {
    /// 按原仓储合同读取 sales_orders；不增加空输入护栏或事务。
    pub(in crate::workbench) async fn read_sales_orders(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_sales::entity::sales_order::SalesOrder>> {
        Ok(self.db.sales_orders().list_active_by_ids(ids, executor).await?)
    }
    /// 按原仓储合同读取 purchase_orders；不增加空输入护栏或事务。
    pub(in crate::workbench) async fn read_purchase_orders(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_procurement::entity::purchase_order::PurchaseOrder>> {
        Ok(self
            .db
            .purchase_orders()
            .list_active_by_ids(ids, executor)
            .await?)
    }
    /// 按原仓储合同读取 sales_revisions；不增加空输入护栏或事务。
    pub(in crate::workbench) async fn read_sales_revisions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_sales::entity::sales_order::SalesOrderRevision>> {
        Ok(self
            .db
            .sales_order_revisions()
            .list_active_by_ids(ids, executor)
            .await?)
    }
    /// 按原仓储合同读取 purchase_revisions；不增加空输入护栏或事务。
    pub(in crate::workbench) async fn read_purchase_revisions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_procurement::entity::purchase_order::PurchaseOrderRevision>> {
        Ok(self
            .db
            .purchase_order_revisions()
            .list_active_by_ids(ids, executor)
            .await?)
    }
    /// 按原仓储合同读取 sales_changes；不增加空输入护栏或事务。
    pub(in crate::workbench) async fn read_sales_changes(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_sales::entity::sales_review::SalesChangeOrder>> {
        Ok(self
            .db
            .sales_change_orders()
            .list_active_by_ids(ids, executor)
            .await?)
    }
    /// 按原仓储合同读取 purchase_changes；不增加空输入护栏或事务。
    pub(in crate::workbench) async fn read_purchase_changes(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_procurement::entity::purchase_order::PurchaseChangeOrder>> {
        Ok(self
            .db
            .purchase_change_orders()
            .list_active_by_ids(ids, executor)
            .await?)
    }
    /// 按原仓储合同读取 sales_change_submissions；不增加空输入护栏或事务。
    pub(in crate::workbench) async fn read_sales_change_submissions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_sales::entity::sales_review::SalesChangeSubmission>> {
        Ok(self
            .db
            .sales_change_submissions()
            .list_active_by_ids(ids, executor)
            .await?)
    }
    /// 按原仓储合同读取 purchase_change_submissions；不增加空输入护栏或事务。
    pub(in crate::workbench) async fn read_purchase_change_submissions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_procurement::entity::purchase_order::PurchaseChangeSubmission>> {
        Ok(self
            .db
            .purchase_change_submissions()
            .list_active_by_ids(ids, executor)
            .await?)
    }
    /// 复用原 integration_errors 仓储读取，空输入由原仓储处理。
    pub(in crate::workbench) async fn read_integration_errors(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_integration::entity::integration_ops::IntegrationErrorTask>> {
        Ok(self
            .db
            .integration_error_tasks()
            .list_active_by_ids(ids, executor)
            .await?)
    }
    /// 复用原 reconciliation_differences 仓储读取，空输入由原仓储处理。
    pub(in crate::workbench) async fn read_reconciliation_differences(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_integration::entity::integration_ops::ReconciliationDifference>> {
        Ok(self
            .db
            .reconciliation_differences()
            .list_active_by_ids(ids, executor)
            .await?)
    }
    /// 复用原 stock_adjustments 仓储读取，空输入由原仓储处理。
    pub(in crate::workbench) async fn read_stock_adjustments(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_inventory::StockAdjustment>> {
        Ok(self
            .db
            .stock_adjustments()
            .list_active_by_ids(ids, executor)
            .await?)
    }
    /// 复用原 settlement_statements 仓储读取，空输入由原仓储处理。
    pub(in crate::workbench) async fn read_settlement_statements(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_supply::entity::supplier_settlement::SupplierSettlementStatement>> {
        Ok(self
            .db
            .supplier_settlement_statements()
            .list_active_by_ids(ids, executor)
            .await?)
    }
    /// 读取结算明细，保留调用者安排的差异与名称查询插槽。
    pub(in crate::workbench) async fn read_settlement_items(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_supply::entity::supplier_settlement::SupplierSettlementItem>> {
        Ok(self
            .db
            .supplier_settlement_items()
            .list_by_statement_ids(ids, executor)
            .await?)
    }
    /// 读取结算差异，不增加过滤、排序或空集合护栏。
    pub(in crate::workbench) async fn read_settlement_differences(
        &self,
        ids: &[erp_core::ids::SupplierSettlementItemId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_supply::entity::supplier_settlement::SupplierSettlementDifference>> {
        Ok(self
            .db
            .supplier_settlement_differences()
            .list_by_statement_item_ids(ids, executor)
            .await?)
    }
}
