//! 命令和显示复用的原子来源读取；调用者决定空输入护栏与执行顺序。
use erp_integration::repository::IntegrationOpsExt;
use erp_inventory::InventoryExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::repository::prelude::*;
use persistence_core::Executor;

use super::WorkItemFactsReader;
use crate::errors::Result;
impl WorkItemFactsReader {
    // 工作项授权或展示引用销售单时，按 ID 沿用调用方执行器查询领域仓储；不另开事务，不在本层改变空批次行为。
    //
    // 调用参数：
    // * `ids` - 销售单 ID。
    //
    // 返回仓储读到的有效销售单。
    pub(in crate::workbench) async fn read_sales_orders(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_sales::entity::sales_order::SalesOrder>> {
        Ok(self.db.sales_orders().list_active_by_ids(ids, executor).await?)
    }
    // 工作项授权或展示引用采购单时，按 ID 沿用调用方执行器查询领域仓储；不另开事务，不在本层改变空批次行为。
    //
    // 调用参数：
    // * `ids` - 采购单 ID。
    //
    // 返回仓储读到的有效采购单。
    pub(in crate::workbench) async fn read_purchase_orders(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_procurement::entity::purchase_order::PurchaseOrder>> {
        Ok(self.db.purchase_orders().list_active_by_ids(ids, executor).await?)
    }
    // 工作项授权或展示引用销售修订时，按 ID 沿用调用方执行器查询领域仓储；不另开事务，不在本层改变空批次行为。
    //
    // 调用参数：
    // * `ids` - 销售修订 ID。
    //
    // 返回仓储读到的有效销售修订。
    pub(in crate::workbench) async fn read_sales_revisions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_sales::entity::sales_order::SalesOrderRevision>> {
        Ok(self.db.sales_order_revisions().list_active_by_ids(ids, executor).await?)
    }
    // 工作项授权或展示引用采购修订时，按 ID 沿用调用方执行器查询领域仓储；不另开事务，不在本层改变空批次行为。
    //
    // 调用参数：
    // * `ids` - 采购修订 ID。
    //
    // 返回仓储读到的有效采购修订。
    pub(in crate::workbench) async fn read_purchase_revisions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_procurement::entity::purchase_order::PurchaseOrderRevision>> {
        Ok(self.db.purchase_order_revisions().list_active_by_ids(ids, executor).await?)
    }
    // 工作项授权或展示引用销售变更单时，按 ID 沿用调用方执行器查询领域仓储；不另开事务，不在本层改变空批次行为。
    //
    // 调用参数：
    // * `ids` - 销售变更单 ID。
    //
    // 返回仓储读到的有效销售变更单。
    pub(in crate::workbench) async fn read_sales_changes(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_sales::entity::sales_review::SalesChangeOrder>> {
        Ok(self.db.sales_change_orders().list_active_by_ids(ids, executor).await?)
    }
    // 工作项授权或展示引用采购变更单时，按 ID 沿用调用方执行器查询领域仓储；不另开事务，不在本层改变空批次行为。
    //
    // 调用参数：
    // * `ids` - 采购变更单 ID。
    //
    // 返回仓储读到的有效采购变更单。
    pub(in crate::workbench) async fn read_purchase_changes(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_procurement::entity::purchase_order::PurchaseChangeOrder>> {
        Ok(self.db.purchase_change_orders().list_active_by_ids(ids, executor).await?)
    }
    // 工作项授权或展示引用销售变更提交时，按 ID 沿用调用方执行器查询领域仓储；不另开事务，不在本层改变空批次行为。
    //
    // 调用参数：
    // * `ids` - 销售变更提交 ID。
    //
    // 返回仓储读到的有效销售变更提交。
    pub(in crate::workbench) async fn read_sales_change_submissions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_sales::entity::sales_review::SalesChangeSubmission>> {
        Ok(self.db.sales_change_submissions().list_active_by_ids(ids, executor).await?)
    }
    // 工作项授权或展示引用采购变更提交时，按 ID 沿用调用方执行器查询领域仓储；不另开事务，不在本层改变空批次行为。
    //
    // 调用参数：
    // * `ids` - 采购变更提交 ID。
    //
    // 返回仓储读到的有效采购变更提交。
    pub(in crate::workbench) async fn read_purchase_change_submissions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_procurement::entity::purchase_order::PurchaseChangeSubmission>> {
        Ok(self.db.purchase_change_submissions().list_active_by_ids(ids, executor).await?)
    }
    // 工作项授权或展示引用集成异常任务时，按 ID 沿用调用方执行器查询领域仓储；不另开事务，不在本层改变空批次行为。
    //
    // 调用参数：
    // * `ids` - 集成异常任务 ID。
    //
    // 返回仓储读到的有效集成异常任务。
    pub(in crate::workbench) async fn read_integration_errors(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_integration::entity::integration_ops::IntegrationErrorTask>> {
        Ok(self.db.integration_error_tasks().list_active_by_ids(ids, executor).await?)
    }
    // 工作项授权或展示引用对账差异时，按 ID 沿用调用方执行器查询领域仓储；不另开事务，不在本层改变空批次行为。
    //
    // 调用参数：
    // * `ids` - 对账差异 ID。
    //
    // 返回仓储读到的有效对账差异。
    pub(in crate::workbench) async fn read_reconciliation_differences(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_integration::entity::integration_ops::ReconciliationDifference>> {
        Ok(self.db.reconciliation_differences().list_active_by_ids(ids, executor).await?)
    }
    // 工作项授权或展示引用库存调整单时，按 ID 沿用调用方执行器查询领域仓储；不另开事务，不在本层改变空批次行为。
    //
    // 调用参数：
    // * `ids` - 库存调整单 ID。
    //
    // 返回仓储读到的有效库存调整单。
    pub(in crate::workbench) async fn read_stock_adjustments(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_inventory::StockAdjustment>> {
        Ok(self.db.stock_adjustments().list_active_by_ids(ids, executor).await?)
    }
    // 工作项授权或展示引用供应商结算单时，按 ID 沿用调用方执行器查询领域仓储；不另开事务，不在本层改变空批次行为。
    //
    // 调用参数：
    // * `ids` - 供应商结算单 ID。
    //
    // 返回仓储读到的有效结算单。
    pub(in crate::workbench) async fn read_settlement_statements(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_supply::entity::supplier_settlement::SupplierSettlementStatement>> {
        Ok(self.db.supplier_settlement_statements().list_active_by_ids(ids, executor).await?)
    }
    // 为工作项授权事实取数时，读取结算明细，保留调用者安排的差异与名称查询插槽。
    //
    // 调用参数：
    // * `ids` - 结算单 ID。
    //
    // 返回这些结算单下的结算明细。
    pub(in crate::workbench) async fn read_settlement_items(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_supply::entity::supplier_settlement::SupplierSettlementItem>> {
        Ok(self.db.supplier_settlement_items().list_by_statement_ids(ids, executor).await?)
    }
    // 为工作项授权事实取数时，读取结算差异，不增加过滤、排序或空集合护栏。
    //
    // 调用参数：
    // * `ids` - 结算明细 ID。
    //
    // 返回这些明细下的结算差异。
    pub(in crate::workbench) async fn read_settlement_differences(
        &self,
        ids: &[erp_core::ids::SupplierSettlementItemId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<erp_supply::entity::supplier_settlement::SupplierSettlementDifference>> {
        Ok(self.db.supplier_settlement_differences().list_by_statement_item_ids(ids, executor).await?)
    }
}
