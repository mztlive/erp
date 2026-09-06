//! `DatabaseExt` 超特征聚合器：每域一个 `extensions/<domain>.rs` 文件。
//!
//! 本文件 P0 后冻结：新增域的访问器一律写在自己的 `extensions/<domain>.rs`，
//! 并通过把 trait 加进 supertrait 列表与本文件里的聚合 trait 生效，聚合 trait 本身不再改。

mod catalog;
mod contract;
mod cost;
mod fulfillment;
mod integration_ops;
mod inventory;
mod legacy_import;
mod payable;
mod procurement_responsibility;
mod purchase_order;
mod receivable;
mod returns;
mod sales_order;
mod sales_review;
mod supplier_api;
mod supplier_fulfillment;
mod supplier_offering;
mod supplier_settlement;
mod warehouse;

pub use catalog::CatalogExt;
pub use contract::ContractExt;
pub use cost::CostExt;
pub use fulfillment::FulfillmentExt;
pub use integration_ops::IntegrationOpsExt;
pub use inventory::InventoryExt;
pub use legacy_import::LegacyImportExt;
pub use payable::PayableExt;
pub use procurement_responsibility::ProcurementResponsibilityExt;
pub use purchase_order::PurchaseOrderExt;
pub use receivable::ReceivableExt;
pub use returns::ReturnsExt;
pub use sales_order::SalesOrderExt;
pub use sales_review::SalesReviewExt;
pub use supplier_api::SupplierApiExt;
pub use supplier_fulfillment::SupplierFulfillmentExt;
pub use supplier_offering::SupplierOfferingExt;
pub use supplier_settlement::SupplierSettlementExt;
pub use warehouse::WarehouseExt;

/// Database 的统一仓储访问入口：聚合全部 34 个域的访问器 trait。
///
/// 各域在 `extensions/<domain>.rs` 中扩展自己的访问器方法；调用点（`db.accounts()` 等）
/// 签名保持不变。
pub trait DatabaseExt:
    erp_identity::AccessControlExt
    + erp_audit::AuditExt
    + erp_workflow::ApprovalIntegrationExt
    + erp_workflow::BpmExt
    + erp_support::BulkJobExt
    + CatalogExt
    + ContractExt
    + CostExt
    + erp_customer::CustomerExt
    + erp_workflow::DocumentRegistryExt
    + erp_support::FileAssetExt
    + FulfillmentExt
    + IntegrationOpsExt
    + InventoryExt
    + LegacyImportExt
    + erp_party::PartyExt
    + PayableExt
    + ProcurementResponsibilityExt
    + PurchaseOrderExt
    + ReceivableExt
    + ReturnsExt
    + SalesOrderExt
    + SalesReviewExt
    + erp_support::SourceRegistryExt
    + erp_supplier::SupplierExt
    + SupplierApiExt
    + SupplierOfferingExt
    + SupplierFulfillmentExt
    + SupplierSettlementExt
    + WarehouseExt
    + erp_workflow::WorkItemExt
{
}

impl<
        T: erp_identity::AccessControlExt
            + erp_audit::AuditExt
            + erp_workflow::ApprovalIntegrationExt
            + erp_workflow::BpmExt
            + erp_support::BulkJobExt
            + CatalogExt
            + ContractExt
            + CostExt
            + erp_customer::CustomerExt
            + erp_workflow::DocumentRegistryExt
            + erp_support::FileAssetExt
            + FulfillmentExt
            + IntegrationOpsExt
            + InventoryExt
            + LegacyImportExt
            + erp_party::PartyExt
            + PayableExt
            + ProcurementResponsibilityExt
            + PurchaseOrderExt
            + ReceivableExt
            + ReturnsExt
            + SalesOrderExt
            + SalesReviewExt
            + erp_support::SourceRegistryExt
            + erp_supplier::SupplierExt
            + SupplierApiExt
            + SupplierOfferingExt
            + SupplierFulfillmentExt
            + SupplierSettlementExt
            + WarehouseExt
            + erp_workflow::WorkItemExt,
    > DatabaseExt for T
{
}
